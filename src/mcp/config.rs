//! On-disk MCP server configuration.
//!
//! # Why this exists
//!
//! The MCP client, transports and registry were all implemented, but nothing
//! ever read a server list off disk: startup built an empty [`McpRegistry`] and
//! `mcp_config_path` was parsed out of config and then never used. The result
//! was a complete, correct MCP stack that could not connect to anything, and a
//! `pylot mcp list` that printed a hardcoded "No MCP servers configured"
//! regardless of the truth.
//!
//! This module is the missing half: load, save, and edit
//! `~/.pylot/mcp-servers.json`.
//!
//! # Format
//!
//! Two shapes are accepted so configs can be copied straight from other MCP
//! hosts without rewriting them:
//!
//! ```json
//! { "mcpServers": { "dbpylot": { "command": "dbpylot", "args": ["mcp"] } } }
//! ```
//!
//! ```json
//! { "servers": [ { "name": "dbpylot", "transport": "stdio",
//!                  "command": "dbpylot", "args": ["mcp"] } ] }
//! ```
//!
//! The first is the Claude Desktop / Claude Code spelling; the second is this
//! project's own. Both round-trip to the same [`McpServerConfig`] list, and
//! saving always writes the `servers` form.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::mcp::types::{McpServerConfig, McpTransportType};

/// Default filename inside the data directory.
pub const DEFAULT_FILENAME: &str = "mcp-servers.json";

/// The whole config file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfigFile {
    /// This project's native form.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<McpServerConfig>,

    /// The Claude Desktop / Claude Code form, accepted on read so an existing
    /// config can be reused verbatim. Never written back — [`save`] normalises
    /// everything into `servers`.
    #[serde(
        default,
        rename = "mcpServers",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub mcp_servers: HashMap<String, HostStyleServer>,
}

/// One entry in the `mcpServers` object form, where the key is the name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostStyleServer {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
    /// Optional explicit transport; inferred from `command` vs `url` when absent.
    #[serde(default)]
    pub transport: Option<McpTransportType>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl McpConfigFile {
    /// Flatten both accepted shapes into one list, in a stable order.
    ///
    /// A name present in both forms keeps its `servers` entry — the explicit,
    /// native spelling wins over the imported one.
    pub fn all_servers(&self) -> Vec<McpServerConfig> {
        let mut out = self.servers.clone();
        let existing: Vec<String> = out.iter().map(|s| s.name.clone()).collect();

        let mut imported: Vec<(String, &HostStyleServer)> = self
            .mcp_servers
            .iter()
            .filter(|(name, _)| !existing.contains(name))
            .map(|(name, cfg)| (name.clone(), cfg))
            .collect();
        // HashMap iteration order is arbitrary; sort so `pylot mcp list` is stable.
        imported.sort_by(|a, b| a.0.cmp(&b.0));

        for (name, cfg) in imported {
            let transport = cfg.transport.clone().unwrap_or_else(|| {
                if cfg.url.is_some() {
                    McpTransportType::Http
                } else {
                    McpTransportType::Stdio
                }
            });
            out.push(McpServerConfig {
                name,
                transport,
                command: cfg.command.clone(),
                args: cfg.args.clone(),
                url: cfg.url.clone(),
                headers: cfg.headers.clone(),
                env: cfg.env.clone(),
                enabled: cfg.enabled,
            });
        }
        out
    }
}

/// Resolve the config path: explicit override, else `<data_dir>/mcp-servers.json`.
///
/// A leading `~` in the override is expanded, since it arrives from a TOML
/// string where the shell never got a chance to.
pub fn resolve_path(data_dir: &Path, configured: Option<&str>) -> PathBuf {
    match configured {
        Some(raw) => {
            let raw = raw.trim();
            if let Some(rest) = raw.strip_prefix("~/") {
                if let Some(home) = dirs::home_dir() {
                    return home.join(rest);
                }
            }
            PathBuf::from(raw)
        }
        None => data_dir.join(DEFAULT_FILENAME),
    }
}

/// Load the config, returning an empty one when the file does not exist.
///
/// A malformed file is an error rather than a silent empty list — quietly
/// dropping every configured server because of a stray comma is exactly the
/// failure mode this module exists to end.
pub fn load(path: &Path) -> Result<McpConfigFile> {
    if !path.exists() {
        return Ok(McpConfigFile::default());
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read MCP config at {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(McpConfigFile::default());
    }
    serde_json::from_str(&text)
        .with_context(|| format!("Failed to parse MCP config at {}", path.display()))
}

/// Write the config, normalising everything into the `servers` form.
pub fn save(path: &Path, servers: &[McpServerConfig]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = McpConfigFile {
        servers: servers.to_vec(),
        mcp_servers: HashMap::new(),
    };
    let json = serde_json::to_string_pretty(&file)?;

    // Write-then-rename so an interrupted save cannot truncate a good config.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json)
        .with_context(|| format!("Failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("Failed to replace {}", path.display()))?;
    Ok(())
}

/// Add or replace a server by name. Returns whether an existing entry was replaced.
pub fn upsert(path: &Path, server: McpServerConfig) -> Result<bool> {
    let mut servers = load(path)?.all_servers();
    let replaced = match servers.iter_mut().find(|s| s.name == server.name) {
        Some(existing) => {
            *existing = server;
            true
        }
        None => {
            servers.push(server);
            false
        }
    };
    save(path, &servers)?;
    Ok(replaced)
}

/// Remove a server by name. Returns whether anything was removed.
pub fn remove(path: &Path, name: &str) -> Result<bool> {
    let mut servers = load(path)?.all_servers();
    let before = servers.len();
    servers.retain(|s| s.name != name);
    let removed = servers.len() != before;
    if removed {
        save(path, &servers)?;
    }
    Ok(removed)
}

/// Enable or disable a server by name. Returns whether the server was found.
pub fn set_enabled(path: &Path, name: &str, enabled: bool) -> Result<bool> {
    let mut servers = load(path)?.all_servers();
    let Some(server) = servers.iter_mut().find(|s| s.name == name) else {
        return Ok(false);
    };
    server.enabled = enabled;
    save(path, &servers)?;
    Ok(true)
}

/// The config OpenDbPylot needs, for `pylot mcp add dbpylot`.
///
/// OpenDbPylot is the sibling project in this family and its MCP tool names are
/// a documented, stable contract, so wiring it up should not require the user
/// to remember any of this.
pub fn opendbpylot_preset() -> McpServerConfig {
    McpServerConfig {
        name: "dbpylot".to_string(),
        transport: McpTransportType::Stdio,
        command: Some("dbpylot".to_string()),
        args: Some(vec!["mcp".to_string()]),
        url: None,
        headers: None,
        env: None,
        enabled: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DEFAULT_FILENAME);
        (dir, path)
    }

    #[test]
    fn a_missing_file_is_an_empty_config_not_an_error() {
        let (_d, path) = tmp();
        let cfg = load(&path).unwrap();
        assert!(cfg.all_servers().is_empty());
    }

    #[test]
    fn a_malformed_file_is_an_error_not_a_silent_empty_list() {
        let (_d, path) = tmp();
        std::fs::write(&path, "{ not json").unwrap();
        assert!(
            load(&path).is_err(),
            "silently dropping every server on a typo is the bug this module fixes"
        );
    }

    #[test]
    fn reads_the_native_servers_array() {
        let (_d, path) = tmp();
        std::fs::write(
            &path,
            r#"{"servers":[{"name":"dbpylot","transport":"stdio","command":"dbpylot","args":["mcp"]}]}"#,
        )
        .unwrap();

        let servers = load(&path).unwrap().all_servers();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "dbpylot");
        assert_eq!(servers[0].command.as_deref(), Some("dbpylot"));
        assert!(servers[0].enabled, "enabled should default to true");
    }

    #[test]
    fn reads_the_claude_desktop_object_form() {
        let (_d, path) = tmp();
        std::fs::write(
            &path,
            r#"{"mcpServers":{"dbpylot":{"command":"dbpylot","args":["mcp"]}}}"#,
        )
        .unwrap();

        let servers = load(&path).unwrap().all_servers();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "dbpylot");
        assert!(matches!(servers[0].transport, McpTransportType::Stdio));
    }

    #[test]
    fn infers_http_transport_from_a_url() {
        let (_d, path) = tmp();
        std::fs::write(
            &path,
            r#"{"mcpServers":{"remote":{"url":"https://example.com/mcp"}}}"#,
        )
        .unwrap();

        let servers = load(&path).unwrap().all_servers();
        assert!(matches!(servers[0].transport, McpTransportType::Http));
    }

    #[test]
    fn both_forms_merge_with_the_native_one_winning_on_a_name_clash() {
        let (_d, path) = tmp();
        std::fs::write(
            &path,
            r#"{
                "servers":[{"name":"dbpylot","transport":"stdio","command":"native"}],
                "mcpServers":{"dbpylot":{"command":"imported"},"other":{"command":"x"}}
            }"#,
        )
        .unwrap();

        let servers = load(&path).unwrap().all_servers();
        assert_eq!(servers.len(), 2);
        let dbpylot = servers.iter().find(|s| s.name == "dbpylot").unwrap();
        assert_eq!(dbpylot.command.as_deref(), Some("native"));
    }

    #[test]
    fn upsert_adds_then_replaces() {
        let (_d, path) = tmp();

        assert!(!upsert(&path, opendbpylot_preset()).unwrap(), "first add");
        assert_eq!(load(&path).unwrap().all_servers().len(), 1);

        let mut changed = opendbpylot_preset();
        changed.args = Some(vec!["mcp".into(), "--verbose".into()]);
        assert!(upsert(&path, changed).unwrap(), "second add replaces");

        let servers = load(&path).unwrap().all_servers();
        assert_eq!(servers.len(), 1, "no duplicate entry");
        assert_eq!(servers[0].args.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn remove_reports_whether_it_did_anything() {
        let (_d, path) = tmp();
        upsert(&path, opendbpylot_preset()).unwrap();

        assert!(remove(&path, "dbpylot").unwrap());
        assert!(!remove(&path, "dbpylot").unwrap(), "second removal is a no-op");
        assert!(load(&path).unwrap().all_servers().is_empty());
    }

    #[test]
    fn set_enabled_toggles_and_persists() {
        let (_d, path) = tmp();
        upsert(&path, opendbpylot_preset()).unwrap();

        assert!(set_enabled(&path, "dbpylot", false).unwrap());
        assert!(!load(&path).unwrap().all_servers()[0].enabled);

        assert!(set_enabled(&path, "dbpylot", true).unwrap());
        assert!(load(&path).unwrap().all_servers()[0].enabled);

        assert!(!set_enabled(&path, "nope", true).unwrap());
    }

    #[test]
    fn saving_normalises_the_imported_form_into_servers() {
        let (_d, path) = tmp();
        std::fs::write(&path, r#"{"mcpServers":{"a":{"command":"x"}}}"#).unwrap();

        // Any mutation rewrites the file.
        upsert(&path, opendbpylot_preset()).unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"servers\""));
        assert!(
            !raw.contains("mcpServers"),
            "save should emit one canonical shape, got: {raw}"
        );
        assert_eq!(load(&path).unwrap().all_servers().len(), 2);
    }

    #[test]
    fn resolve_path_defaults_into_the_data_dir() {
        let path = resolve_path(Path::new("/data"), None);
        assert_eq!(path, Path::new("/data").join(DEFAULT_FILENAME));
    }

    #[test]
    fn resolve_path_expands_a_leading_tilde() {
        let path = resolve_path(Path::new("/data"), Some("~/custom/mcp.json"));
        assert!(
            !path.to_string_lossy().starts_with('~'),
            "a literal ~ directory would silently never be found: {}",
            path.display()
        );
        assert!(path.ends_with("custom/mcp.json"));
    }

    #[test]
    fn the_opendbpylot_preset_is_a_stdio_server() {
        let preset = opendbpylot_preset();
        assert_eq!(preset.name, "dbpylot");
        assert!(matches!(preset.transport, McpTransportType::Stdio));
        assert_eq!(preset.args.as_deref(), Some(&["mcp".to_string()][..]));
    }
}
