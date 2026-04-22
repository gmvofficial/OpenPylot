//! # Sub-Agent System
//!
//! Multi-agent orchestration: spawn isolated sub-agents for parallel tasks,
//! background research, and specialized work.

pub mod manifest;
pub mod orchestrator;
pub mod store;
pub mod types;

pub use manifest::{AgentManifest, AgentManifestRegistry, ManifestSource};
pub use orchestrator::AgentOrchestrator;
pub use store::SubAgentStore;
pub use types::*;
