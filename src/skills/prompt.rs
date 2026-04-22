//! # Skill Prompt Formatting
//!
//! Formats skills into structured XML for injection into the system prompt.
//! Mirrors OpenClaw's `formatSkillsForPrompt()` — the LLM uses the `read`
//! tool to load full SKILL.md content when it matches a skill description.
//!
//! ## Progressive Disclosure (3 levels):
//! 1. **Metadata** (always in prompt) — name + description + path (~100 words)
//! 2. **SKILL.md body** (on-demand) — loaded via read tool when triggered
//! 3. **Bundled resources** (as needed) — scripts/, references/, assets/

use super::types::Skill;

/// Format skills as structured XML for the system prompt.
/// This is the Level 1 (metadata-only) injection — compact and precise.
///
/// The LLM sees all available skill names/descriptions and can decide
/// to read the full SKILL.md when a task matches.
pub fn format_skills_xml(skills: &[&Skill], compact_home: bool) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let home = if compact_home {
        dirs::home_dir().map(|h| h.display().to_string())
    } else {
        None
    };

    let mut lines = Vec::new();
    lines.push(String::new());
    lines.push(
        "The following skills provide specialized instructions for specific tasks.".to_string(),
    );
    lines.push(
        "Use the read tool to load a skill's file when the task matches its description."
            .to_string(),
    );
    lines.push(
        "When a skill file references a relative path, resolve it against the skill directory."
            .to_string(),
    );
    lines.push(String::new());
    lines.push("<available_skills>".to_string());

    for skill in skills {
        let path = compact_path(&skill.source_path.display().to_string(), home.as_deref());
        lines.push("  <skill>".to_string());
        lines.push(format!("    <name>{}</name>", escape_xml(&skill.meta.name)));
        lines.push(format!(
            "    <description>{}</description>",
            escape_xml(&skill.meta.description)
        ));
        lines.push(format!("    <location>{}</location>", escape_xml(&path)));
        if let Some(ref cat) = skill.meta.category {
            lines.push(format!("    <category>{}</category>", escape_xml(cat)));
        }
        lines.push("  </skill>".to_string());
    }

    lines.push("</available_skills>".to_string());
    lines.join("\n")
}

/// Format matched skills as detailed instructions for the current request.
/// This is a Level 2 injection — the full markdown body is included.
pub fn format_matched_skills(skills: &[&Skill], max_chars: usize) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut section = String::from("\n\n## Active Skill Instructions\n\n");
    section.push_str("Follow these skill-specific instructions for this request:\n\n");

    let mut total_chars = section.len();
    for skill in skills {
        let skill_dir = skill
            .source_path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let block = format!(
            "### Skill: {}\n**Skill directory:** {}\n\n{}\n\n",
            skill.meta.name, skill_dir, skill.content
        );
        if total_chars + block.len() > max_chars {
            section.push_str("\n_(Additional skills truncated due to context budget)_\n");
            break;
        }
        section.push_str(&block);
        total_chars += block.len();
    }

    section
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn compact_path(path: &str, home: Option<&str>) -> String {
    if let Some(home) = home {
        if path.starts_with(home) {
            return format!("~{}", &path[home.len()..]);
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::types::{Skill, SkillMeta, SkillSource};
    use std::path::PathBuf;

    fn test_skill(name: &str, desc: &str) -> Skill {
        Skill {
            meta: SkillMeta {
                name: name.to_string(),
                description: desc.to_string(),
                version: "1.0.0".to_string(),
                author: None,
                category: Some("coding".to_string()),
                tags: vec![],
                os: vec![],
                requires: None,
                install: vec![],
                examples: vec![],
            },
            content: format!("Instructions for {}", name),
            source_path: PathBuf::from(format!("skills/{}/SKILL.md", name)),
            source: SkillSource::Bundled,
        }
    }

    #[test]
    fn test_xml_format_empty() {
        let skills: Vec<&Skill> = vec![];
        assert_eq!(format_skills_xml(&skills, false), "");
    }

    #[test]
    fn test_xml_format_single_skill() {
        let skill = test_skill("debug", "Debug systematically");
        let xml = format_skills_xml(&[&skill], false);
        assert!(xml.contains("<available_skills>"));
        assert!(xml.contains("<name>debug</name>"));
        assert!(xml.contains("<description>Debug systematically</description>"));
        assert!(xml.contains("</available_skills>"));
    }

    #[test]
    fn test_xml_escaping() {
        let skill = test_skill("test", "Handle <html> & \"quotes\"");
        let xml = format_skills_xml(&[&skill], false);
        assert!(xml.contains("&lt;html&gt;"));
        assert!(xml.contains("&amp;"));
        assert!(xml.contains("&quot;quotes&quot;"));
    }

    #[test]
    fn test_matched_truncation() {
        let s1 = test_skill("a", "desc a");
        let s2 = test_skill("b", "desc b");
        let result = format_matched_skills(&[&s1, &s2], 100);
        assert!(result.contains("truncated"));
    }
}
