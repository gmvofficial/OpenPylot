//! Detecting, launching and supervising companion processes.

use std::collections::HashMap;
use std::net::TcpListener;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// A companion app OpenPylot can host.
#[derive(Debug, Clone)]
pub struct Companion {
    /// Stable id; becomes the URL segment under `/companions/`.
    pub name: String,
    /// Human-readable name for the UI.
    pub title: String,
    pub description: String,
    /// Executable to look for on `PATH`.
    pub binary: String,
    /// Arguments that start its web server.
    pub serve_args: Vec<String>,
    /// Flag used to pass a port, if the binary supports one.
    pub port_flag: Option<String>,
    /// Path to poll to decide the child is up.
    pub health_path: String,
}

/// What a companion is doing, as reported to the UI.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CompanionState {
    /// The binary is not on `PATH`.
    NotInstalled,
    /// Installed but not started.
    Stopped,
    /// Starting; not yet answering health checks.
    Starting,
    /// Up and proxied.
    Running { port: u16 },
    /// Tried to start and could not.
    Failed { error: String },
}

impl CompanionState {
    pub fn is_running(&self) -> bool {
        matches!(self, CompanionState::Running { .. })
    }

    pub fn port(&self) -> Option<u16> {
        match self {
            CompanionState::Running { port } => Some(*port),
            _ => None,
        }
    }
}

/// A companion as the API reports it.
#[derive(Debug, Clone, Serialize)]
pub struct CompanionStatus {
    pub name: String,
    pub title: String,
    pub description: String,
    #[serde(flatten)]
    pub state: CompanionState,
    /// Where the UI should point an iframe, when it is running.
    pub url: Option<String>,
}

/// A running child plus the port it was given.
struct Running {
    child: Child,
    port: u16,
}

impl Drop for Running {
    fn drop(&mut self) {
        // A companion outliving its parent would hold its port and keep serving
        // with nothing in front of it. `start_kill` is the non-async kill that
        // is safe to call from Drop.
        let _ = self.child.start_kill();
    }
}

/// Tracks which companions are installed and which are up.
#[derive(Clone)]
pub struct CompanionRegistry {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    companions: Vec<Companion>,
    running: HashMap<String, Running>,
    states: HashMap<String, CompanionState>,
}

impl CompanionRegistry {
    pub fn new(companions: Vec<Companion>) -> Self {
        let states = companions
            .iter()
            .map(|c| {
                let state = if which::which(&c.binary).is_ok() {
                    CompanionState::Stopped
                } else {
                    CompanionState::NotInstalled
                };
                (c.name.clone(), state)
            })
            .collect();

        Self {
            inner: Arc::new(Mutex::new(Inner {
                companions,
                running: HashMap::new(),
                states,
            })),
        }
    }

    /// Every companion and its current state.
    pub async fn list(&self) -> Vec<CompanionStatus> {
        let inner = self.inner.lock().await;
        inner
            .companions
            .iter()
            .map(|c| {
                let state = inner
                    .states
                    .get(&c.name)
                    .cloned()
                    .unwrap_or(CompanionState::Stopped);
                CompanionStatus {
                    name: c.name.clone(),
                    title: c.title.clone(),
                    description: c.description.clone(),
                    url: state
                        .is_running()
                        .then(|| format!("{}/{}/", super::MOUNT_PREFIX, c.name)),
                    state,
                }
            })
            .collect()
    }

    pub async fn state(&self, name: &str) -> Option<CompanionState> {
        self.inner.lock().await.states.get(name).cloned()
    }

    /// The loopback port a running companion is on, for the proxy.
    pub async fn port(&self, name: &str) -> Option<u16> {
        self.inner.lock().await.states.get(name)?.port()
    }

    /// Start a companion, returning the port it came up on.
    ///
    /// Starting one that is already running is a no-op that returns the existing
    /// port, so a UI can call this on every page load without spawning a second
    /// copy.
    pub async fn start(&self, name: &str) -> Result<u16, String> {
        {
            let inner = self.inner.lock().await;
            if let Some(CompanionState::Running { port }) = inner.states.get(name) {
                return Ok(*port);
            }
        }

        let companion = {
            let inner = self.inner.lock().await;
            inner
                .companions
                .iter()
                .find(|c| c.name == name)
                .cloned()
                .ok_or_else(|| format!("No companion named '{name}'"))?
        };

        if which::which(&companion.binary).is_err() {
            let error = format!(
                "'{}' is not installed or not on PATH. Install it, then try again.",
                companion.binary
            );
            self.set_state(name, CompanionState::NotInstalled).await;
            return Err(error);
        }

        self.set_state(name, CompanionState::Starting).await;

        let port = free_port().map_err(|e| format!("Could not find a free port: {e}"))?;
        let mut command = Command::new(&companion.binary);
        command.args(&companion.serve_args);
        if let Some(flag) = &companion.port_flag {
            command.arg(flag).arg(port.to_string());
        }
        // The child's own logs would corrupt the TUI and are not ours to show.
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        let child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                let error = format!("Could not start {}: {e}", companion.binary);
                self.set_state(name, CompanionState::Failed { error: error.clone() })
                    .await;
                return Err(error);
            }
        };

        {
            let mut inner = self.inner.lock().await;
            inner
                .running
                .insert(name.to_string(), Running { child, port });
        }

        match wait_until_healthy(port, &companion.health_path, Duration::from_secs(20)).await {
            Ok(()) => {
                self.set_state(name, CompanionState::Running { port }).await;
                tracing::info!("Companion '{name}' running on 127.0.0.1:{port}");
                Ok(port)
            }
            Err(e) => {
                // A child that never answered is worse than none: it holds a
                // port and would be proxied to a dead socket.
                self.stop(name).await;
                let error = format!("{} started but never answered: {e}", companion.binary);
                self.set_state(name, CompanionState::Failed { error: error.clone() })
                    .await;
                Err(error)
            }
        }
    }

    /// Stop a companion. Stopping one that is not running is a no-op.
    pub async fn stop(&self, name: &str) {
        let mut inner = self.inner.lock().await;
        if let Some(mut running) = inner.running.remove(name) {
            let _ = running.child.kill().await;
        }
        // Preserve "not installed" — stopping does not uninstall anything.
        let next = match inner.states.get(name) {
            Some(CompanionState::NotInstalled) => CompanionState::NotInstalled,
            _ => CompanionState::Stopped,
        };
        inner.states.insert(name.to_string(), next);
    }

    /// Stop every companion. Called on shutdown.
    pub async fn stop_all(&self) {
        let names: Vec<String> = {
            let inner = self.inner.lock().await;
            inner.running.keys().cloned().collect()
        };
        for name in names {
            self.stop(&name).await;
        }
    }

    async fn set_state(&self, name: &str, state: CompanionState) {
        self.inner
            .lock()
            .await
            .states
            .insert(name.to_string(), state);
    }
}

/// Ask the OS for an unused port by binding `:0` and immediately releasing it.
///
/// Inherently racy — another process could take the port in between — but it is
/// the standard approach, and a collision surfaces as a clean start failure
/// rather than silent breakage.
fn free_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Poll until the child answers, or give up.
async fn wait_until_healthy(port: u16, path: &str, timeout: Duration) -> Result<(), String> {
    let url = format!("http://127.0.0.1:{port}{path}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| e.to_string())?;

    let deadline = Instant::now() + timeout;
    let mut last = String::from("no response");

    while Instant::now() < deadline {
        match client.get(&url).send().await {
            // Any HTTP answer proves the server is listening; a 404 on the
            // health path still means the process is up.
            Ok(_) => return Ok(()),
            Err(e) => last = e.to_string(),
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(name: &str, binary: &str) -> Companion {
        Companion {
            name: name.to_string(),
            title: name.to_string(),
            description: String::new(),
            binary: binary.to_string(),
            serve_args: vec![],
            port_flag: None,
            health_path: "/".to_string(),
        }
    }

    #[test]
    fn free_port_returns_something_bindable() {
        let port = free_port().unwrap();
        assert!(port > 0);
        // Proving it is actually free: we can bind it right back.
        let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
        drop(listener);
    }

    #[test]
    fn free_port_does_not_repeat_immediately() {
        let a = free_port().unwrap();
        let b = free_port().unwrap();
        // Not strictly guaranteed by the OS, but a registry that handed the same
        // port to two companions would be a real bug worth noticing.
        assert_ne!(a, b);
    }

    #[tokio::test]
    async fn a_missing_binary_is_reported_as_not_installed() {
        let registry = CompanionRegistry::new(vec![fake("ghost", "openpylot-definitely-missing")]);
        assert_eq!(
            registry.state("ghost").await,
            Some(CompanionState::NotInstalled)
        );
    }

    #[tokio::test]
    async fn an_installed_binary_starts_out_stopped() {
        // `sh` exists everywhere this runs.
        let registry = CompanionRegistry::new(vec![fake("shell", "sh")]);
        assert_eq!(registry.state("shell").await, Some(CompanionState::Stopped));
    }

    #[tokio::test]
    async fn starting_a_missing_binary_fails_with_a_useful_message() {
        let registry = CompanionRegistry::new(vec![fake("ghost", "openpylot-definitely-missing")]);
        let err = registry.start("ghost").await.unwrap_err();
        assert!(err.contains("not installed"), "{err}");
        assert!(err.contains("PATH"), "the message should say how to fix it: {err}");
    }

    #[tokio::test]
    async fn starting_an_unknown_companion_fails() {
        let registry = CompanionRegistry::new(vec![]);
        assert!(registry.start("nope").await.is_err());
    }

    #[tokio::test]
    async fn a_binary_that_exits_immediately_is_reported_as_failed() {
        // `true` starts fine and exits at once, so it never answers a health
        // check — the case that would otherwise leave the proxy pointing at a
        // dead socket.
        let registry = CompanionRegistry::new(vec![Companion {
            health_path: "/".into(),
            ..fake("quitter", "true")
        }]);

        let result = tokio::time::timeout(
            Duration::from_secs(25),
            registry.start("quitter"),
        )
        .await
        .expect("start should give up rather than hang");

        assert!(result.is_err());
        assert!(matches!(
            registry.state("quitter").await,
            Some(CompanionState::Failed { .. })
        ));
    }

    #[tokio::test]
    async fn stopping_something_that_is_not_running_is_a_no_op() {
        let registry = CompanionRegistry::new(vec![fake("shell", "sh")]);
        registry.stop("shell").await;
        assert_eq!(registry.state("shell").await, Some(CompanionState::Stopped));
    }

    #[tokio::test]
    async fn stopping_does_not_claim_an_uninstalled_companion_is_installed() {
        let registry = CompanionRegistry::new(vec![fake("ghost", "openpylot-definitely-missing")]);
        registry.stop("ghost").await;
        assert_eq!(
            registry.state("ghost").await,
            Some(CompanionState::NotInstalled),
            "stopping must not upgrade NotInstalled to Stopped"
        );
    }

    #[tokio::test]
    async fn the_listing_carries_a_url_only_while_running() {
        let registry = CompanionRegistry::new(vec![fake("shell", "sh")]);
        let listed = registry.list().await;
        assert_eq!(listed.len(), 1);
        assert!(listed[0].url.is_none(), "a stopped companion has nowhere to point");
    }

    #[test]
    fn state_reports_its_port_only_when_running() {
        assert_eq!(CompanionState::Running { port: 1234 }.port(), Some(1234));
        assert_eq!(CompanionState::Stopped.port(), None);
        assert_eq!(CompanionState::NotInstalled.port(), None);
        assert_eq!(
            CompanionState::Failed { error: "x".into() }.port(),
            None
        );
    }

    #[tokio::test]
    async fn health_check_gives_up_rather_than_hanging_forever() {
        let port = free_port().unwrap();
        let started = Instant::now();
        let result = wait_until_healthy(port, "/", Duration::from_millis(500)).await;
        assert!(result.is_err());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the timeout must be honoured"
        );
    }
}
