use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::tools::{Tool, ToolDefinition, ToolResult};

/// Maximum file size for read operations (10 MB).
const MAX_READ_SIZE: u64 = 10 * 1024 * 1024;

/// Maximum file size for write operations (5 MB).
const MAX_WRITE_SIZE: usize = 5 * 1024 * 1024;

/// Blocked paths — prevent reading/writing sensitive system files.
const BLOCKED_PATH_PREFIXES: &[&str] = &[
    "/dev/", "/proc/", "/sys/", "/boot/",
];

/// Blocked file patterns — prevent reading/writing credentials.
const BLOCKED_FILE_PATTERNS: &[&str] = &[
    ".ssh/id_", ".ssh/authorized_keys",
    ".gnupg/", ".aws/credentials",
    "secrets.enc", ".env.local",
];

/// Normalize and validate a file path.
/// Returns the canonical absolute path, or an error if blocked.
fn normalize_and_validate(path_str: &str, workspace_root: &Path) -> Result<PathBuf> {
    let path = Path::new(path_str);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };

    // Canonicalize if the file/parent exists
    let canonical = if absolute.exists() {
        absolute.canonicalize()?
    } else if let Some(parent) = absolute.parent() {
        if parent.exists() {
            parent.canonicalize()?.join(absolute.file_name().unwrap_or_default())
        } else {
            absolute
        }
    } else {
        absolute
    };

    let path_str = canonical.to_string_lossy().to_string();

    // Block sensitive paths
    for prefix in BLOCKED_PATH_PREFIXES {
        if path_str.starts_with(prefix) {
            anyhow::bail!("Access denied: path '{}' is in a protected directory", prefix);
        }
    }

    for pattern in BLOCKED_FILE_PATTERNS {
        if path_str.contains(pattern) {
            anyhow::bail!("Access denied: path matches protected pattern '{}'", pattern);
        }
    }

    Ok(canonical)
}

// ── Read File Tool ───────────────────────────────────────────────────

/// Read file with line-based slicing, inspired by Claw Code's read_file.
pub struct ReadFileTool {
    workspace_root: PathBuf,
}

impl ReadFileTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".to_string(),
            description: "Read the contents of a file. Supports line-offset and line-limit \
                for reading specific portions of large files. Returns the content with line numbers.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The file path (relative to workspace root, or absolute)"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Zero-indexed line offset to start reading from (default: 0)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of lines to read (default: entire file)"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path_str = params
            .get("path")
            .and_then(|p| p.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;

        let offset = params.get("offset").and_then(|o| o.as_u64()).unwrap_or(0) as usize;
        let limit = params.get("limit").and_then(|l| l.as_u64()).map(|l| l as usize);

        let path = normalize_and_validate(path_str, &self.workspace_root)?;

        if !path.exists() {
            return Ok(ToolResult::err(format!("File not found: {}", path.display())));
        }

        if !path.is_file() {
            return Ok(ToolResult::err(format!("Not a file: {}", path.display())));
        }

        // Check file size
        let metadata = std::fs::metadata(&path)?;
        if metadata.len() > MAX_READ_SIZE {
            return Ok(ToolResult::err(format!(
                "File too large ({} bytes, max {} bytes). Use offset/limit to read portions.",
                metadata.len(), MAX_READ_SIZE
            )));
        }

        let content = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = offset.min(total_lines);
        let end = limit.map_or(total_lines, |l| (start + l).min(total_lines));
        let selected: Vec<String> = lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:>4} | {}", start + i + 1, line))
            .collect();

        let output = format!(
            "File: {} ({} total lines, showing lines {}-{})\n\n{}",
            path.display(),
            total_lines,
            start + 1,
            end,
            selected.join("\n")
        );

        Ok(ToolResult::ok(output))
    }
}

// ── Write File Tool ──────────────────────────────────────────────────

/// Write/create file, inspired by Claw Code's write_file.
pub struct WriteFileTool {
    workspace_root: PathBuf,
}

impl WriteFileTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write_file".to_string(),
            description: "Create or overwrite a file with the given content. \
                Parent directories are created automatically. Use edit_file for partial edits.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The file path (relative to workspace root, or absolute)"
                    },
                    "content": {
                        "type": "string",
                        "description": "The full content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path_str = params
            .get("path")
            .and_then(|p| p.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;

        let content = params
            .get("content")
            .and_then(|c| c.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'content' parameter"))?;

        if content.len() > MAX_WRITE_SIZE {
            return Ok(ToolResult::err(format!(
                "Content too large ({} bytes, max {} bytes)",
                content.len(), MAX_WRITE_SIZE
            )));
        }

        let path = normalize_and_validate(path_str, &self.workspace_root)?;

        // Record if it's a new file or update
        let is_new = !path.exists();

        // Create parent directories
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&path, content)?;

        let action = if is_new { "Created" } else { "Updated" };
        let line_count = content.lines().count();

        Ok(ToolResult::ok(format!(
            "{} {} ({} lines, {} bytes)",
            action,
            path.display(),
            line_count,
            content.len()
        )))
    }
}

// ── Edit File Tool ───────────────────────────────────────────────────

/// Edit file by string replacement, inspired by Claw Code's edit_file.
pub struct EditFileTool {
    workspace_root: PathBuf,
}

impl EditFileTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }
}

#[async_trait]
impl Tool for EditFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "edit_file".to_string(),
            description: "Edit a file by replacing an exact string with new content. \
                The old_string must appear in the file. Use replace_all=true to replace all occurrences.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The file path (relative to workspace root, or absolute)"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "The exact string to find and replace"
                    },
                    "new_string": {
                        "type": "string",
                        "description": "The replacement string"
                    },
                    "replace_all": {
                        "type": "boolean",
                        "description": "Replace all occurrences (default: false, replaces first only)"
                    }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let path_str = params
            .get("path")
            .and_then(|p| p.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;

        let old_string = params
            .get("old_string")
            .and_then(|o| o.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'old_string' parameter"))?;

        let new_string = params
            .get("new_string")
            .and_then(|n| n.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'new_string' parameter"))?;

        let replace_all = params
            .get("replace_all")
            .and_then(|r| r.as_bool())
            .unwrap_or(false);

        if old_string == new_string {
            return Ok(ToolResult::err("old_string and new_string are identical — nothing to change"));
        }

        let path = normalize_and_validate(path_str, &self.workspace_root)?;

        if !path.exists() {
            return Ok(ToolResult::err(format!("File not found: {}", path.display())));
        }

        let original = std::fs::read_to_string(&path)?;

        if !original.contains(old_string) {
            return Ok(ToolResult::err(format!(
                "old_string not found in {}. Make sure the string matches exactly (including whitespace).",
                path.display()
            )));
        }

        let count = original.matches(old_string).count();

        let updated = if replace_all {
            original.replace(old_string, new_string)
        } else {
            original.replacen(old_string, new_string, 1)
        };

        std::fs::write(&path, &updated)?;

        let replaced = if replace_all { count } else { 1 };

        Ok(ToolResult::ok(format!(
            "Edited {}: replaced {} occurrence(s) ({} total found)",
            path.display(),
            replaced,
            count
        )))
    }
}
