//! Companion apps: sibling Pylot tools that OpenPylot can host.
//!
//! # What a companion is
//!
//! MCP already covers the part of a companion that is *tools* — once
//! `mcp-servers.json` lists `dbpylot`, its six tools appear in the agent's
//! registry and both the terminal and the web chat can call them.
//!
//! What MCP cannot carry is a companion's **own web interface**. OpenDbPylot
//! ships a full SQL workbench — schema training, result tables, Plotly charts —
//! that is a web app, not a tool call. A companion registry is how that app
//! gets reachable from inside OpenPylot instead of being a second program the
//! user has to start, find a port for, and trust separately.
//!
//! # How it works
//!
//! ```text
//!   browser ──▶ /companions/dbpylot/…  ──▶  127.0.0.1:<ephemeral>
//!               (behind OpenPylot's        (dbpylot serve, loopback only,
//!                access token)              started and supervised by us)
//! ```
//!
//! The child binds loopback on a port we pick, so it is never independently
//! reachable; the only way in is through OpenPylot's authenticated proxy. The
//! proxy rewrites HTML on the way out to inject a shim that re-points the
//! companion's absolute `/api/…` requests at the mounted prefix, which is what
//! lets an app written to live at `/` work unmodified under a sub-path.

pub mod proxy;
pub mod registry;

pub use registry::{Companion, CompanionRegistry, CompanionState};

/// URL prefix every companion is mounted under.
pub const MOUNT_PREFIX: &str = "/companions";

/// The companions OpenPylot knows how to host.
///
/// Kept as a fixed catalogue rather than something user-editable: hosting means
/// launching a child process, and the set of binaries worth launching that way
/// is small, known, and part of this project's own family.
pub fn catalogue() -> Vec<Companion> {
    vec![Companion {
        name: "dbpylot".to_string(),
        title: "Database".to_string(),
        description: "Ask your database questions in plain English — OpenDbPylot.".to_string(),
        binary: "dbpylot".to_string(),
        serve_args: vec!["serve".to_string(), "--headless".to_string()],
        // `dbpylot serve --port` lets the registry assign an ephemeral port, so
        // several instances never collide on a fixed 8080.
        port_flag: Some("--port".to_string()),
        health_path: "/".to_string(),
    }]
}

/// Look up a companion by name.
pub fn find(name: &str) -> Option<Companion> {
    catalogue().into_iter().find(|c| c.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_is_not_empty() {
        assert!(!catalogue().is_empty());
    }

    #[test]
    fn every_companion_has_the_fields_the_proxy_needs() {
        for c in catalogue() {
            assert!(!c.name.is_empty());
            assert!(!c.binary.is_empty());
            assert!(!c.title.is_empty());
            assert!(
                c.health_path.starts_with('/'),
                "{}: health path must be absolute",
                c.name
            );
        }
    }

    #[test]
    fn names_are_url_safe_since_they_become_path_segments() {
        for c in catalogue() {
            assert!(
                c.name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-'),
                "{} would need escaping in a URL path",
                c.name
            );
        }
    }

    #[test]
    fn names_are_unique() {
        let mut names: Vec<String> = catalogue().into_iter().map(|c| c.name).collect();
        let before = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), before);
    }

    #[test]
    fn lookup_finds_a_known_companion_and_rejects_others() {
        assert!(find("dbpylot").is_some());
        assert!(find("not-a-companion").is_none());
        assert!(find("").is_none());
    }

    #[test]
    fn dbpylot_is_launched_headless_so_it_does_not_steal_a_browser_tab() {
        // Without --headless the child opens its own window, which is confusing
        // when it is meant to render inside OpenPylot.
        let dbpylot = find("dbpylot").unwrap();
        assert!(dbpylot.serve_args.contains(&"--headless".to_string()));
    }
}
