# Plugins — Agents & Skills

OpenPylot is plug-and-play. You can add **new sub-agents** and **new skills** without editing Rust — just drop files in the right directory and they are picked up on the next run.

- Agent presets live in `agents/*.toml`
- Skills live in `skills/<category>/<name>/SKILL.md`

Both have a **precedence chain** (workspace > user > bundled) so you can override bundled defaults locally.

---

## 1. Agent Presets (Plug-and-play sub-agents)

An agent preset is a TOML manifest that describes a reusable sub-agent configuration: its persona, the model it prefers, which tools it may call, and execution limits.

### Locations (in precedence order)

1. **Workspace** — `./agents/*.toml` in the directory you run `pylot` from
2. **User** — `~/.pylot/agents/*.toml`
3. **Bundled** — `agents/*.toml` shipped with OpenPylot

If two manifests share the same `name`, the higher-precedence one wins.

### Manifest schema

```toml
# agents/coder.toml
name         = "coder"
description  = "Expert software engineer. Writes clean, tested, idiomatic code."
agent_type   = "Specialist"        # Task | Background | Specialist
system_prompt = """
You are a senior software engineer. Follow the repository's conventions.
Read existing code before editing. Prefer small, reversible changes.
"""
model_override = "claude-3-5-sonnet-20241022"   # optional
allowed_tools  = [                              # optional — omit = inherit parent
  "read_file", "write_file", "edit_file",
  "list_dir", "grep_search",
  "run_bash",
  "search_web", "extract_web",
]
timeout_secs   = 900   # default 300
max_iterations = 30    # default 10
```

### Using a preset

**CLI:**

```bash
pylot agents presets                       # list all presets
pylot agents show coder                    # pretty-print one
pylot agents path                          # show ~/.pylot/agents
pylot agents spawn --preset coder "Implement JWT auth in src/auth.rs"
```

**From the web UI:** the `Sub-Agents` page's spawn form shows a preset picker that prefills model + tools from any manifest.

**From the HTTP API:**

```http
GET  /api/agents/presets            → { presets: [...], user_dir: "..." }
GET  /api/agents/presets/{name}     → full manifest incl. system_prompt
POST /api/agents                    → spawn (pass model/tools from preset)
```

### Bundled presets

OpenPylot ships with four presets out of the box:

| Name         | Type        | Purpose                                                  |
| ------------ | ----------- | -------------------------------------------------------- |
| `coder`      | Specialist  | Reads/edits code, runs tests, follows repo conventions   |
| `researcher` | Specialist  | Web search + extract, synthesizes cited summaries        |
| `writer`     | Specialist  | Drafts long-form content, tight prose, on-brand voice    |
| `marketer`   | Specialist  | Campaign plans + posts, wires social scheduling tools    |

### Adding your own

```bash
mkdir -p ~/.pylot/agents
$EDITOR ~/.pylot/agents/sre.toml
# paste the schema above, tweak, save

pylot agents presets            # should now include "sre"
pylot agents spawn --preset sre "Triage PagerDuty incident PAG-1234"
```

No rebuild required.

---

## 2. Skills (SKILL.md files)

Skills are declarative instruction files the agent surfaces to the LLM when the user's intent matches.

### Locations (in precedence order)

1. **Workspace** — `./skills/**/SKILL.md`
2. **User** — `~/.pylot/skills/**/SKILL.md`
3. **Bundled** — `skills/**/SKILL.md`

### Schema

```markdown
---
name: pdf-summarizer
category: productivity
description: Summarize a PDF file into an executive-brief style bulleted summary.
triggers:
  - summarize pdf
  - tl;dr pdf
  - pdf summary
examples:
  - "Summarize ~/Downloads/report.pdf"
  - "Give me a tl;dr of this PDF"
tools:
  - read_file
  - extract_pdf
os: [linux, macos, windows]
---

# PDF Summarizer

## Procedure

1. Resolve the PDF path. If the user mentions a filename only, search the
   current workspace and `~/Downloads`.
2. Extract text via `extract_pdf`.
3. Produce:
   - 3-sentence TL;DR at the top.
   - Bulleted section headers with 2–4 bullets each.
   - "Action items" at the bottom if any appear.

## Style

- Keep it under 400 words unless the source is >50 pages.
- No filler. No "In this document, we will..." openers.
```

The YAML frontmatter is required; the body is the natural-language guidance injected into the system prompt when the skill matches.

### Matching

Skills are matched to user input via keyword/trigger overlap plus semantic similarity against the description and examples. The top matches are included in the system prompt for that turn only, keeping context small.

### Bundled skills

OpenPylot ships with 30+ skills across:

- `productivity/` — email, calendar, PDF, notes
- `coding/` — review, test generation, refactor
- `communication/` — drafting, social posts, reply etiquette
- `research/` — web research, citations, summarization
- `media/` — image gen, transcription
- `system/` — file ops, shell, git

Many are ported from [OpenClaw](https://github.com/openclaw/openclaw) and rewritten to match our flat YAML schema.

### Adding your own

```bash
mkdir -p ~/.pylot/skills/productivity/my-skill
$EDITOR ~/.pylot/skills/productivity/my-skill/SKILL.md
# paste the schema above, tweak, save
```

List them:

```bash
pylot skills list
pylot skills show my-skill
```

---

## 3. Precedence cheat sheet

| Source      | Path                          | Overrides               |
| ----------- | ----------------------------- | ----------------------- |
| Workspace   | `./agents/`, `./skills/`      | Everything              |
| User        | `~/.pylot/agents/`, `…skills` | Bundled                 |
| Bundled     | `agents/`, `skills/`          | —                       |

Same name = higher precedence wins. Different names = all are loaded.

## 4. See also

- `docs/ARCHITECTURE.md` — how the agent loop dispatches to sub-agents
- `docs/AGENTS.md` — sub-agent runtime model, timeouts, isolation
- `docs/CONFIGURATION.md` — global config, models, API keys
