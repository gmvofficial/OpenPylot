use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing;

use super::types::{Skill, SkillMeta, SkillSource};

/// Loads and parses SKILL.md files from the filesystem.
pub struct SkillLoader;

impl SkillLoader {
    /// Parse a single SKILL.md file into a `Skill`.
    pub fn parse_skill_file(path: &Path, source: SkillSource) -> Result<Skill> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read skill file: {}", path.display()))?;

        let (frontmatter, body) = Self::split_frontmatter(&content)
            .with_context(|| format!("Invalid SKILL.md format: {}", path.display()))?;

        let meta: SkillMeta = serde_yaml::from_str(&frontmatter)
            .with_context(|| format!("Failed to parse YAML frontmatter in {}", path.display()))?;

        Ok(Skill {
            meta,
            content: body.trim().to_string(),
            source_path: path.to_path_buf(),
            source,
        })
    }

    /// Split `---\nyaml\n---\nmarkdown` into `(yaml_str, markdown_str)`.
    fn split_frontmatter(content: &str) -> Result<(String, String)> {
        let trimmed = content.trim_start();

        if !trimmed.starts_with("---") {
            anyhow::bail!("SKILL.md must start with YAML frontmatter (---)");
        }

        // Find the closing "---"
        let after_first = &trimmed[3..];
        let closing_pos = after_first.find("\n---")
            .ok_or_else(|| anyhow::anyhow!("No closing --- found for YAML frontmatter"))?;

        let yaml_str = after_first[..closing_pos].trim().to_string();
        let body_start = closing_pos + 4; // skip "\n---"
        let body = if body_start < after_first.len() {
            after_first[body_start..].to_string()
        } else {
            String::new()
        };

        Ok((yaml_str, body))
    }

    /// Recursively scan a directory for SKILL.md files.
    /// Looks for files named `SKILL.md` (case-insensitive) in all subdirectories.
    pub fn scan_directory(dir: &Path, source: SkillSource) -> Vec<Skill> {
        let mut skills = Vec::new();

        if !dir.exists() || !dir.is_dir() {
            return skills;
        }

        Self::scan_recursive(dir, &source, &mut skills);
        skills
    }

    fn scan_recursive(dir: &Path, source: &SkillSource, skills: &mut Vec<Skill>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Cannot read directory {}: {}", dir.display(), e);
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                Self::scan_recursive(&path, source, skills);
            } else if Self::is_skill_file(&path) {
                match Self::parse_skill_file(&path, source.clone()) {
                    Ok(skill) => {
                        tracing::debug!(
                            "Loaded skill '{}' from {}",
                            skill.meta.name,
                            path.display()
                        );
                        skills.push(skill);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse {}: {}", path.display(), e);
                    }
                }
            }
        }
    }

    fn is_skill_file(path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case("SKILL.md"))
            .unwrap_or(false)
    }

    /// Resolve bundled skills directory: next to executable, or `skills/` in cwd.
    pub fn bundled_skills_dir() -> Option<PathBuf> {
        // 1. Try next to executable
        if let Ok(exe) = std::env::current_exe() {
            let dir = exe.parent().map(|p| p.join("skills"));
            if let Some(ref d) = dir {
                if d.exists() {
                    return dir;
                }
            }
        }
        // 2. Try cwd/skills/
        let cwd_skills = PathBuf::from("skills");
        if cwd_skills.exists() {
            return Some(cwd_skills);
        }
        None
    }

    /// Local skills directory: `~/.pylot/skills/`
    pub fn local_skills_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".pylot").join("skills"))
    }

    /// Workspace skills directory: `./skills/` relative to a workspace root.
    pub fn workspace_skills_dir(workspace: Option<&Path>) -> Option<PathBuf> {
        workspace.map(|ws| ws.join("skills"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_frontmatter_valid() {
        let content = "---\nname: test\ndescription: A test\n---\n# Body\nSome instructions";
        let (yaml, body) = SkillLoader::split_frontmatter(content).unwrap();
        assert!(yaml.contains("name: test"));
        assert!(body.contains("# Body"));
    }

    #[test]
    fn test_split_frontmatter_no_body() {
        let content = "---\nname: test\ndescription: A test\n---\n";
        let (yaml, body) = SkillLoader::split_frontmatter(content).unwrap();
        assert!(yaml.contains("name: test"));
        assert!(body.trim().is_empty());
    }

    #[test]
    fn test_split_frontmatter_missing_close() {
        let content = "---\nname: test\nno closing";
        let result = SkillLoader::split_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_split_frontmatter_no_start() {
        let content = "no frontmatter here";
        let result = SkillLoader::split_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_youtube_transcripts_skill() {
        // Integration test: load the actual youtube-transcripts SKILL.md from disk
        let skill_path = PathBuf::from("skills/research/youtube-transcripts/SKILL.md");
        if !skill_path.exists() {
            eprintln!("Skipping: SKILL.md not found at {:?}", skill_path);
            return;
        }
        let skill = SkillLoader::parse_skill_file(&skill_path, SkillSource::Bundled).unwrap();
        assert_eq!(skill.meta.name, "youtube-transcripts");
        assert_eq!(skill.meta.category.as_deref(), Some("research"));
        assert!(skill.meta.description.contains("YouTube"));
        assert!(skill.meta.tags.contains(&"youtube".to_string()));
        assert!(skill.content.contains("## Step 1"));
        // Check requires
        let reqs = skill.meta.requires.as_ref().expect("should have requires");
        assert!(reqs.bins.contains(&"python3".to_string()));
        println!("✅ youtube-transcripts skill loaded successfully");
        println!("   Name: {}", skill.meta.name);
        println!("   Desc: {}", &skill.meta.description[..60]);
        println!("   Tags: {:?}", skill.meta.tags);
        println!("   Body: {} chars", skill.content.len());
    }

    #[test]
    fn test_scan_skills_directory() {
        // Integration test: scan the entire skills/ dir and verify youtube-transcripts is found
        let skills_dir = PathBuf::from("skills");
        if !skills_dir.exists() {
            eprintln!("Skipping: skills/ directory not found");
            return;
        }
        let skills = SkillLoader::scan_directory(&skills_dir, SkillSource::Bundled);
        assert!(!skills.is_empty(), "Should find at least one skill");
        let yt = skills.iter().find(|s| s.meta.name == "youtube-transcripts");
        assert!(yt.is_some(), "youtube-transcripts skill should be discovered");
        println!("✅ Found {} skills in skills/ directory:", skills.len());
        for s in &skills {
            println!("   - {} ({})", s.meta.name, s.meta.category.as_deref().unwrap_or("none"));
        }
    }
}
