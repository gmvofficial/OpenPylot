// Library crate exposing modules for integration tests and external use.

pub mod agent;
pub mod api;
pub mod config;
pub mod context;
pub mod jobs;
pub mod llm;
pub mod memory;
pub mod oauth;
pub mod scheduler;
pub mod secrets;
pub mod tools;
pub mod webhooks;

// Note: init, terminal, telegram_bot are not re-exported
// as they depend on the binary crate's internal wiring.
