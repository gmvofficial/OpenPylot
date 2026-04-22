use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::tools::{Tool, ToolDefinition, ToolResult};

/// Max files returned by glob search.
const MAX_GLOB_RESULTS: usize = 100;

/// Max matches returned by grep search.
const MAX_GREP_RESULTS: usize = 50;

/// Max file size to grep (5 MB).
const MAX_GREP_FILE_SIZE: u64 = 5 * 1024 * 1024;

// ── Glob/File Search Tool ────────────────────────────────────────────

/// Search for files by glob pattern, inspired by Claw Code's glob_search.
pub struct GlobSearchTool {
    workspace_root: PathBuf,
}

impl GlobSearchTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }
}

#[async_trait]
impl Tool for GlobSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "glob_search".to_string(),
            description: "Search for files matching a glob pattern (e.g. '**/*.rs', 'src/**/*.ts'). \
                Returns matching file paths sorted by modification time (newest first). \
                Useful for finding files by name or extension.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern to match (e.g. '**/*.rs', 'src/**/test_*.py')"
                    },
                    "path": {
                        "type": "string",
                        "description": "Base directory to search from (default: workspace root)"
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let pattern = params
            .get("pattern")
            .and_then(|p| p.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter"))?;

        let base_dir = params
            .get("path")
            .and_then(|p| p.as_str())
            .map(|p| {
                let path = Path::new(p);
                if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    self.workspace_root.join(p)
                }
            })
            .unwrap_or_else(|| self.workspace_root.clone());

        if !base_dir.is_dir() {
            return Ok(ToolResult::err(format!("Directory not found: {}", base_dir.display())));
        }

        let full_pattern = base_dir.join(pattern).to_string_lossy().to_string();

        let mut files: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();

        match glob::glob(&full_pattern) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    if entry.is_file() {
                        let mtime = entry
                            .metadata()
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        files.push((entry, mtime));
                    }
                }
            }
            Err(e) => {
                return Ok(ToolResult::err(format!("Invalid glob pattern: {}", e)));
            }
        }

        // Sort by modification time (newest first)
        files.sort_by(|a, b| b.1.cmp(&a.1));

        let truncated = files.len() > MAX_GLOB_RESULTS;
        let files: Vec<_> = files.into_iter().take(MAX_GLOB_RESULTS).collect();

        if files.is_empty() {
            return Ok(ToolResult::ok(format!(
                "No files found matching pattern '{}' in {}",
                pattern,
                base_dir.display()
            )));
        }

        let file_list: Vec<String> = files
            .iter()
            .map(|(path, _)| {
                path.strip_prefix(&self.workspace_root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_string()
            })
            .collect();

        let mut output = format!(
            "Found {} file(s) matching '{}'",
            file_list.len(),
            pattern
        );
        if truncated {
            output.push_str(&format!(" (truncated to {}, more available)", MAX_GLOB_RESULTS));
        }
        output.push_str(":\n\n");
        output.push_str(&file_list.join("\n"));

        Ok(ToolResult::ok(output))
    }
}

// ── Grep Search Tool ─────────────────────────────────────────────────

/// Search file contents by regex/text pattern, inspired by Claw Code's grep_search.
pub struct GrepSearchTool {
    workspace_root: PathBuf,
}

impl GrepSearchTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    /// Walk directory tree and collect matching files.
    fn walk_and_grep(
        &self,
        base_dir: &Path,
        regex: &regex::Regex,
        file_ext_filter: Option<&str>,
        context_lines: usize,
        max_results: usize,
    ) -> Result<Vec<GrepMatch>> {
        let mut matches = Vec::new();
        let walker = walkdir::WalkDir::new(base_dir)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                // Skip hidden dirs and common non-source dirs
                !name.starts_with('.')
                    && name != "node_modules"
                    && name != "target"
                    && name != "__pycache__"
                    && name != ".git"
                    && name != "dist"
                    && name != "build"
                    && name != "vendor"
            });

        for entry in walker.flatten() {
            if matches.len() >= max_results {
                break;
            }

            if !entry.file_type().is_file() {
                continue;
            }

            // Check file extension filter
            if let Some(ext) = file_ext_filter {
                if entry.path().extension().and_then(|e| e.to_str()) != Some(ext) {
                    continue;
                }
            }

            // Skip large files
            if let Ok(meta) = entry.metadata() {
                if meta.len() > MAX_GREP_FILE_SIZE {
                    continue;
                }
            }

            // Skip binary files (check first 512 bytes)
            if let Ok(content) = std::fs::read(entry.path()) {
                let check_len = content.len().min(512);
                if content[..check_len].contains(&0) {
                    continue; // Binary file
                }
            }

            let content = match std::fs::read_to_string(entry.path()) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let lines: Vec<&str> = content.lines().collect();
            let mut file_matches = Vec::new();

            for (line_num, line) in lines.iter().enumerate() {
                if regex.is_match(line) {
                    // Collect context
                    let start = line_num.saturating_sub(context_lines);
                    let end = (line_num + context_lines + 1).min(lines.len());
                    let context: Vec<String> = lines[start..end]
                        .iter()
                        .enumerate()
                        .map(|(i, l)| {
                            let actual_line = start + i + 1;
                            let marker = if actual_line == line_num + 1 { ">" } else { " " };
                            format!("{}{:>4} | {}", marker, actual_line, l)
                        })
                        .collect();

                    file_matches.push(LineMatch {
                        line_number: line_num + 1,
                        context: context.join("\n"),
                    });
                }
            }

            if !file_matches.is_empty() {
                let rel_path = entry
                    .path()
                    .strip_prefix(&self.workspace_root)
                    .unwrap_or(entry.path())
                    .to_string_lossy()
                    .to_string();

                matches.push(GrepMatch {
                    file: rel_path,
                    matches: file_matches,
                });

                if matches.len() >= max_results {
                    break;
                }
            }
        }

        Ok(matches)
    }
}

struct GrepMatch {
    file: String,
    matches: Vec<LineMatch>,
}

struct LineMatch {
    line_number: usize,
    context: String,
}

#[async_trait]
impl Tool for GrepSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "grep_search".to_string(),
            description: "Search file contents using a regex pattern. Returns matching lines with \
                context. Automatically skips binary files, hidden directories, node_modules, \
                and build output. Useful for finding code patterns, function definitions, and usages.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for (e.g. 'fn main', 'TODO|FIXME', 'class\\s+\\w+')"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search in (default: workspace root)"
                    },
                    "file_type": {
                        "type": "string",
                        "description": "File extension filter (e.g. 'rs', 'py', 'ts')"
                    },
                    "context": {
                        "type": "integer",
                        "description": "Number of context lines before/after each match (default: 2)"
                    },
                    "case_insensitive": {
                        "type": "boolean",
                        "description": "Case-insensitive matching (default: true)"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of files to return (default: 20)"
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let pattern = params
            .get("pattern")
            .and_then(|p| p.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter"))?;

        let base_dir = params
            .get("path")
            .and_then(|p| p.as_str())
            .map(|p| {
                let path = Path::new(p);
                if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    self.workspace_root.join(p)
                }
            })
            .unwrap_or_else(|| self.workspace_root.clone());

        let file_type = params.get("file_type").and_then(|f| f.as_str());
        let context_lines = params.get("context").and_then(|c| c.as_u64()).unwrap_or(2) as usize;
        let case_insensitive = params.get("case_insensitive").and_then(|c| c.as_bool()).unwrap_or(true);
        let max_results = params.get("max_results").and_then(|m| m.as_u64()).unwrap_or(20) as usize;

        let regex = regex::RegexBuilder::new(pattern)
            .case_insensitive(case_insensitive)
            .build()
            .map_err(|e| anyhow::anyhow!("Invalid regex pattern: {}", e))?;

        let matches = self.walk_and_grep(
            &base_dir,
            &regex,
            file_type,
            context_lines,
            max_results.min(MAX_GREP_RESULTS),
        )?;

        if matches.is_empty() {
            return Ok(ToolResult::ok(format!(
                "No matches found for pattern '{}' in {}",
                pattern,
                base_dir.display()
            )));
        }

        let total_matches: usize = matches.iter().map(|m| m.matches.len()).sum();

        let mut output = format!(
            "Found {} match(es) in {} file(s) for pattern '{}':\n",
            total_matches,
            matches.len(),
            pattern
        );

        for grep_match in &matches {
            output.push_str(&format!("\n── {} ──\n", grep_match.file));
            for line_match in &grep_match.matches {
                output.push_str(&format!("Line {}:\n{}\n", line_match.line_number, line_match.context));
            }
        }

        Ok(ToolResult::ok(output))
    }
}

// ── List Directory Tool ──────────────────────────────────────────────

/// List directory contents as a tree, inspired by Claw Code's file_tree.
pub struct ListDirectoryTool {
    workspace_root: PathBuf,
}

impl ListDirectoryTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    fn build_tree(dir: &Path, prefix: &str, depth: usize, max_depth: usize) -> Result<String> {
        if depth >= max_depth {
            return Ok(format!("{}...\n", prefix));
        }

        let mut entries: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .collect();

        // Sort: directories first, then alphabetical
        entries.sort_by(|a, b| {
            let a_is_dir = a.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            let b_is_dir = b.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.file_name().cmp(&b.file_name()),
            }
        });

        // Filter common non-useful dirs
        let entries: Vec<_> = entries
            .into_iter()
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                !matches!(
                    name.as_str(),
                    ".git" | "node_modules" | "target" | "__pycache__"
                        | ".next" | ".tox" | ".mypy_cache" | ".pytest_cache"
                        | "dist" | ".DS_Store"
                )
            })
            .collect();

        let mut output = String::new();
        let count = entries.len();

        for (i, entry) in entries.iter().enumerate() {
            let is_last = i == count - 1;
            let connector = if is_last { "└── " } else { "├── " };
            let child_prefix = if is_last { "    " } else { "│   " };

            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);

            if is_dir {
                output.push_str(&format!("{}{}{}/\n", prefix, connector, name));
                let subtree = Self::build_tree(
                    &entry.path(),
                    &format!("{}{}", prefix, child_prefix),
                    depth + 1,
                    max_depth,
                )?;
                output.push_str(&subtree);
            } else {
                output.push_str(&format!("{}{}{}\n", prefix, connector, name));
            }
        }

        Ok(output)
    }
}

#[async_trait]
impl Tool for ListDirectoryTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_directory".to_string(),
            description: "List directory contents as a tree structure. Shows files and subdirectories \
                with visual tree connectors. Skips hidden dirs, node_modules, target, etc.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path to list (default: workspace root)"
                    },
                    "depth": {
                        "type": "integer",
                        "description": "Maximum recursion depth (default: 3)"
                    }
                }
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let dir_path = params
            .get("path")
            .and_then(|p| p.as_str())
            .map(|p| {
                let path = Path::new(p);
                if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    self.workspace_root.join(p)
                }
            })
            .unwrap_or_else(|| self.workspace_root.clone());

        let max_depth = params.get("depth").and_then(|d| d.as_u64()).unwrap_or(3) as usize;

        if !dir_path.is_dir() {
            return Ok(ToolResult::err(format!("Not a directory: {}", dir_path.display())));
        }

        let rel_path = dir_path
            .strip_prefix(&self.workspace_root)
            .unwrap_or(&dir_path)
            .to_string_lossy()
            .to_string();

        let header = if rel_path.is_empty() || rel_path == "." {
            "./\n".to_string()
        } else {
            format!("{}/\n", rel_path)
        };

        let tree = Self::build_tree(&dir_path, "", 0, max_depth)?;

        Ok(ToolResult::ok(format!("{}{}", header, tree)))
    }
}
