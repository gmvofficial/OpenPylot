use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Permission modes matching Claw Code's permission tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Read-only: only allow read tools (grep, glob, read_file, list_dir).
    ReadOnly,
    /// Workspace-write: allow writes within the workspace directory.
    WorkspaceWrite,
    /// Full access: allow all operations including shell commands.
    FullAccess,
}

impl Default for PermissionMode {
    fn default() -> Self {
        Self::WorkspaceWrite
    }
}

/// Per-tool permission override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPermission {
    /// Always allow this tool.
    Allow,
    /// Always deny this tool.
    Deny,
    /// Prompt the user before executing.
    Prompt,
}

/// Policy engine that checks permissions for tool calls.
#[derive(Debug, Clone)]
pub struct PermissionPolicy {
    mode: PermissionMode,
    /// Per-tool overrides: tool_name → permission.
    overrides: HashMap<String, ToolPermission>,
    /// Workspace root for path-based checks.
    workspace_root: Option<String>,
}

impl PermissionPolicy {
    pub fn new(mode: PermissionMode) -> Self {
        Self {
            mode,
            overrides: HashMap::new(),
            workspace_root: None,
        }
    }

    pub fn with_workspace_root(mut self, root: String) -> Self {
        self.workspace_root = Some(root);
        self
    }

    pub fn set_override(&mut self, tool_name: &str, perm: ToolPermission) {
        self.overrides.insert(tool_name.to_string(), perm);
    }

    pub fn mode(&self) -> PermissionMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: PermissionMode) {
        self.mode = mode;
    }

    /// Check whether a tool call is allowed under current policy.
    pub fn check(&self, tool_name: &str, args: &serde_json::Value) -> PermissionOutcome {
        // Check per-tool overrides first
        if let Some(perm) = self.overrides.get(tool_name) {
            return match perm {
                ToolPermission::Allow => PermissionOutcome::Allow,
                ToolPermission::Deny => PermissionOutcome::Deny(format!(
                    "Tool '{}' is blocked by permission override",
                    tool_name
                )),
                ToolPermission::Prompt => PermissionOutcome::NeedsApproval(format!(
                    "Tool '{}' requires user approval",
                    tool_name
                )),
            };
        }

        match self.mode {
            PermissionMode::FullAccess => PermissionOutcome::Allow,
            PermissionMode::ReadOnly => {
                if Self::is_read_only_tool(tool_name) {
                    PermissionOutcome::Allow
                } else {
                    PermissionOutcome::Deny(format!(
                        "Tool '{}' is not allowed in read-only mode",
                        tool_name
                    ))
                }
            }
            PermissionMode::WorkspaceWrite => {
                if Self::is_read_only_tool(tool_name) {
                    PermissionOutcome::Allow
                } else if Self::is_write_tool(tool_name) {
                    // Check if the path is within workspace
                    if self.check_workspace_path(args) {
                        PermissionOutcome::Allow
                    } else {
                        PermissionOutcome::NeedsApproval(format!(
                            "Tool '{}' targets path outside workspace",
                            tool_name
                        ))
                    }
                } else if Self::is_dangerous_tool(tool_name) {
                    PermissionOutcome::NeedsApproval(format!(
                        "Tool '{}' requires approval in workspace-write mode",
                        tool_name
                    ))
                } else {
                    PermissionOutcome::Allow
                }
            }
        }
    }

    fn is_read_only_tool(name: &str) -> bool {
        matches!(
            name,
            "read_file" | "grep_search" | "glob_search" | "list_directory"
                | "web_search" | "web_extract"
                | "recall_memories" | "search_knowledge"
                | "list_notes" | "search_notes"
                | "list_reminders"
                | "list_calendar_events"
                | "gmail_search" | "gmail_get" | "gmail_draft_list" | "gmail_draft_get"
                | "get_telegram_updates"
                | "document_loader"
        )
    }

    fn is_write_tool(name: &str) -> bool {
        matches!(
            name,
            "write_file" | "edit_file" | "create_note" | "delete_note"
        )
    }

    fn is_dangerous_tool(name: &str) -> bool {
        matches!(name, "bash" | "shell_execute")
    }

    /// Check if tool arguments reference a path within the workspace.
    fn check_workspace_path(&self, args: &serde_json::Value) -> bool {
        let workspace = match &self.workspace_root {
            Some(w) => w,
            None => return true, // No workspace constraint
        };

        // Check common path parameter names
        for key in &["path", "file_path", "directory"] {
            if let Some(path_str) = args.get(*key).and_then(|v| v.as_str()) {
                let path = std::path::Path::new(path_str);
                if path.is_absolute() && !path_str.starts_with(workspace.as_str()) {
                    return false;
                }
            }
        }

        true
    }
}

/// Result of a permission check.
#[derive(Debug, Clone)]
pub enum PermissionOutcome {
    /// Tool call is allowed.
    Allow,
    /// Tool call is denied (with reason).
    Deny(String),
    /// Tool call needs user approval (with reason).
    NeedsApproval(String),
}
