//! Terminal user interface.
//!
//! An inline-viewport TUI built on ratatui: finished transcript entries go into
//! the terminal's own scrollback, and only the live region — a streaming reply,
//! the completion popup, the composer and the status line — is redrawn.
//!
//! Replaces a rustyline REPL that printed directly to stdout, had no layout, no
//! folding, no interrupt, and prefix-only slash completion.

pub mod app;
pub mod commands;
pub mod completion;
pub mod composer;
pub mod highlight;
pub mod history;
pub mod markdown;
pub mod status;
pub mod theme;
pub mod transcript;

pub use app::App;
