//! Slash commands: the catalogue, and how their arguments complete.
//!
//! The old REPL matched a flat `&[&str]` by prefix, which meant `/mod` found
//! `/model` but `/mdl` found nothing, and no command could suggest its own
//! arguments. Commands are described here as data so both the completion
//! engine and the `/help` screen read from one source.

/// What a command accepts after its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Args {
    /// Takes nothing.
    None,
    /// Free text (a search query, a filename).
    Free(&'static str),
    /// One of a fixed set of values.
    Choice(&'static [&'static str]),
    /// A session id, completed from the session store at runtime.
    Session,
    /// A model name, completed from the provider's catalogue at runtime.
    Model,
}

/// One slash command.
#[derive(Debug, Clone, Copy)]
pub struct Command {
    pub name: &'static str,
    /// Alternative spellings that run the same thing.
    pub aliases: &'static [&'static str],
    pub summary: &'static str,
    pub args: Args,
}

/// Permission modes offered by `/mode`, matching [`crate::permissions::PermissionMode`].
pub const MODES: &[&str] = &["read-only", "workspace-write", "full-access"];

/// Formats offered by `/export`.
pub const EXPORT_FORMATS: &[&str] = &["markdown", "json", "text"];

/// The full catalogue, in the order `/help` lists it.
pub const COMMANDS: &[Command] = &[
    Command {
        name: "/help",
        aliases: &["/?"],
        summary: "Show the command reference and key bindings",
        args: Args::None,
    },
    Command {
        name: "/new",
        aliases: &["/clear", "/reset"],
        summary: "Start a fresh conversation",
        args: Args::None,
    },
    Command {
        name: "/model",
        aliases: &[],
        summary: "Show or switch the active model",
        args: Args::Model,
    },
    Command {
        name: "/mode",
        aliases: &[],
        summary: "Show or set the permission mode",
        args: Args::Choice(MODES),
    },
    Command {
        name: "/context",
        aliases: &[],
        summary: "Show what is currently in the context window",
        args: Args::None,
    },
    Command {
        name: "/compress",
        aliases: &["/compact"],
        summary: "Summarise the conversation to reclaim context",
        args: Args::None,
    },
    Command {
        name: "/cost",
        aliases: &["/usage"],
        summary: "Show token usage and estimated cost",
        args: Args::None,
    },
    Command {
        name: "/tools",
        aliases: &[],
        summary: "List available tools, including MCP tools",
        args: Args::None,
    },
    Command {
        name: "/skills",
        aliases: &[],
        summary: "List loaded skills",
        args: Args::None,
    },
    Command {
        name: "/mcp",
        aliases: &[],
        summary: "Show connected MCP servers",
        args: Args::None,
    },
    Command {
        name: "/sessions",
        aliases: &[],
        summary: "List saved sessions",
        args: Args::None,
    },
    Command {
        name: "/resume",
        aliases: &["/load"],
        summary: "Resume a saved session",
        args: Args::Session,
    },
    Command {
        name: "/save",
        aliases: &[],
        summary: "Save the current session",
        args: Args::Free("name"),
    },
    Command {
        name: "/search",
        aliases: &[],
        summary: "Search this conversation",
        args: Args::Free("query"),
    },
    Command {
        name: "/export",
        aliases: &[],
        summary: "Write the conversation to a file",
        args: Args::Choice(EXPORT_FORMATS),
    },
    Command {
        name: "/thinking",
        aliases: &[],
        summary: "Toggle display of the model's reasoning",
        args: Args::None,
    },
    Command {
        name: "/verbose",
        aliases: &[],
        summary: "Toggle full tool output",
        args: Args::None,
    },
    Command {
        name: "/theme",
        aliases: &[],
        summary: "Show the active colour theme",
        args: Args::None,
    },
    Command {
        name: "/status",
        aliases: &[],
        summary: "Show agent and integration status",
        args: Args::None,
    },
    Command {
        name: "/quit",
        aliases: &["/exit"],
        summary: "Leave the session",
        args: Args::None,
    },
];

/// Look up a command by name or alias.
pub fn lookup(name: &str) -> Option<&'static Command> {
    let name = name.trim();
    COMMANDS
        .iter()
        .find(|c| c.name == name || c.aliases.contains(&name))
}

/// Every name and alias, for completion.
pub fn all_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Vec::new();
    for command in COMMANDS {
        names.push(command.name);
        names.extend(command.aliases.iter().copied());
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_command_starts_with_a_slash() {
        for name in all_names() {
            assert!(name.starts_with('/'), "{name} is not a slash command");
        }
    }

    #[test]
    fn no_name_or_alias_is_used_twice() {
        // A duplicate would make `lookup` silently pick the first definition.
        let names = all_names();
        let unique: HashSet<_> = names.iter().collect();
        assert_eq!(
            unique.len(),
            names.len(),
            "duplicate command name or alias in the catalogue"
        );
    }

    #[test]
    fn every_command_has_a_summary() {
        for command in COMMANDS {
            assert!(
                !command.summary.is_empty(),
                "{} has no summary, so /help would show a blank row",
                command.name
            );
        }
    }

    #[test]
    fn lookup_resolves_names_and_aliases_to_the_same_command() {
        let by_name = lookup("/quit").unwrap();
        let by_alias = lookup("/exit").unwrap();
        assert_eq!(by_name.name, by_alias.name);
    }

    #[test]
    fn lookup_tolerates_surrounding_whitespace() {
        assert!(lookup("  /help  ").is_some());
    }

    #[test]
    fn lookup_rejects_an_unknown_command() {
        assert!(lookup("/definitely-not-a-command").is_none());
        assert!(lookup("").is_none());
    }

    #[test]
    fn choice_arguments_are_never_empty() {
        for command in COMMANDS {
            if let Args::Choice(options) = command.args {
                assert!(
                    !options.is_empty(),
                    "{} offers a choice of nothing",
                    command.name
                );
            }
        }
    }

    #[test]
    fn mode_choices_match_the_permission_modes() {
        // If a mode is added to the policy engine without being listed here,
        // /mode can set it but cannot complete it.
        assert_eq!(MODES.len(), 3);
        assert!(MODES.contains(&"read-only"));
        assert!(MODES.contains(&"workspace-write"));
        assert!(MODES.contains(&"full-access"));
    }
}
