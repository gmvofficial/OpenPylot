//! # Skill Status Report
//!
//! Generates a detailed status report for each loaded skill — used by the
//! frontend skills dashboard and CLI `pylot skills` command.
//! Mirrors OpenClaw's `SkillStatusReport` / `SkillStatusEntry`.

use serde::Serialize;
use super::config::SkillEntryConfig;
use super::scanner::{scan_skill_directory, ScanSummary};
use super::types::{Skill, SkillInstaller, SkillSource};

/// Status of a single skill — everything the frontend needs to render.
#[derive(Debug, Clone, Serialize)]
pub struct SkillStatusEntry {
    pub name: String,
    pub description: String,
    pub version: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub source: String,
    pub source_path: String,

    // Status flags
    pub enabled: bool,
    pub disabled: bool,
    pub eligible: bool,

    // Requirements
    pub required_bins: Vec<String>,
    pub required_env: Vec<String>,
    pub missing_bins: Vec<String>,
    pub missing_env: Vec<String>,
    pub os_filter: Vec<String>,

    // Install options
    pub install_options: Vec<InstallOption>,

    // Security
    pub security: SecurityStatus,

    // Config
    pub has_api_key: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallOption {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub bins: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecurityStatus {
    pub scanned: bool,
    pub safe: bool,
    pub critical_count: usize,
    pub warn_count: usize,
}

/// Full status report for all skills.
#[derive(Debug, Clone, Serialize)]
pub struct SkillStatusReport {
    pub skills: Vec<SkillStatusEntry>,
    pub total: usize,
    pub ready: usize,
    pub needs_setup: usize,
    pub disabled: usize,
}

/// Build a status report for a list of skills.
pub fn build_status_report(
    skills: &[&Skill],
    entry_configs: &std::collections::HashMap<String, SkillEntryConfig>,
) -> SkillStatusReport {
    let entries: Vec<SkillStatusEntry> = skills
        .iter()
        .map(|skill| build_entry(skill, entry_configs.get(&skill.meta.name)))
        .collect();

    let ready = entries.iter().filter(|e| !e.disabled && e.eligible).count();
    let needs_setup = entries.iter().filter(|e| !e.disabled && !e.eligible).count();
    let disabled = entries.iter().filter(|e| e.disabled).count();
    let total = entries.len();

    SkillStatusReport {
        skills: entries,
        total,
        ready,
        needs_setup,
        disabled,
    }
}

fn build_entry(skill: &Skill, config: Option<&SkillEntryConfig>) -> SkillStatusEntry {
    let disabled = config.map(|c| c.is_disabled()).unwrap_or(false);

    // Check missing binaries
    let required_bins = skill.meta.requires.as_ref()
        .map(|r| r.bins.clone())
        .unwrap_or_default();
    let missing_bins: Vec<String> = required_bins.iter()
        .filter(|bin| which::which(bin).is_err())
        .cloned()
        .collect();

    // Check missing env vars
    let required_env = skill.meta.requires.as_ref()
        .map(|r| r.env.clone())
        .unwrap_or_default();
    let missing_env: Vec<String> = required_env.iter()
        .filter(|e| std::env::var(e).is_err())
        .cloned()
        .collect();

    // OS check
    let os_ok = if skill.meta.os.is_empty() {
        true
    } else {
        let current = if cfg!(target_os = "macos") { "macos" }
            else if cfg!(target_os = "linux") { "linux" }
            else if cfg!(target_os = "windows") { "windows" }
            else { "unknown" };
        skill.meta.os.iter().any(|os| {
            let lower = os.to_lowercase();
            lower == current || (lower == "darwin" && current == "macos")
        })
    };

    let eligible = !disabled && os_ok && missing_bins.is_empty() && missing_env.is_empty();

    // Install options
    let install_options: Vec<InstallOption> = skill.meta.install.iter().map(|inst| {
        InstallOption {
            id: inst.id.clone(),
            kind: inst.kind.clone(),
            label: inst.label.clone().unwrap_or_else(|| format!("Install via {}", inst.kind)),
            bins: inst.bins.clone(),
        }
    }).collect();

    // Security scan (only scan scripts/ subdirectory)
    let scripts_dir = skill.source_path.parent()
        .map(|p| p.join("scripts"));
    let scan = scripts_dir
        .filter(|d| d.exists())
        .map(|d| scan_skill_directory(&d));
    let security = match scan {
        Some(summary) => SecurityStatus {
            scanned: true,
            safe: summary.is_safe(),
            critical_count: summary.critical,
            warn_count: summary.warn,
        },
        None => SecurityStatus {
            scanned: false,
            safe: true,
            critical_count: 0,
            warn_count: 0,
        },
    };

    SkillStatusEntry {
        name: skill.meta.name.clone(),
        description: skill.meta.description.clone(),
        version: skill.meta.version.clone(),
        category: skill.meta.category.clone(),
        tags: skill.meta.tags.clone(),
        source: format!("{:?}", skill.source),
        source_path: skill.source_path.display().to_string(),
        enabled: !disabled,
        disabled,
        eligible,
        required_bins,
        required_env,
        missing_bins,
        missing_env,
        os_filter: skill.meta.os.clone(),
        install_options,
        security,
        has_api_key: config.and_then(|c| c.api_key.as_ref()).is_some(),
    }
}
