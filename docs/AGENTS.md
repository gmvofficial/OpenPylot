# Sub-Agent System

OpenPylot supports spawning autonomous sub-agents that run in parallel, each with their own LLM context and tool access.

## Architecture

The **AgentOrchestrator** manages sub-agent lifecycle:

- **spawn** — Create a new sub-agent with a name, task, and optional model override
- **cancel** — Abort a running sub-agent
- **status** — Check whether a sub-agent is Running, Completed, Failed, TimedOut, or Cancelled
- **list** — View all sub-agents and their states
- **wait_for** — Block until a sub-agent finishes

Sub-agents run as isolated `Agent` instances in separate Tokio tasks, with their own system prompt and tool registry. They share the same LLM provider configuration but maintain independent conversation contexts.

## Usage

### Via Chat

```
Spawn a sub-agent named "researcher" to find the latest AI news
```

```
What's the status of the researcher agent?
```

```
Cancel the researcher agent
```

### Via API

```bash
# List all sub-agents
curl http://localhost:3001/api/agents

# List available presets (coder, researcher, writer, marketer, …)
curl http://localhost:3001/api/agents/presets

# Spawn a sub-agent from a preset
curl -X POST http://localhost:3001/api/agents \
  -H 'Content-Type: application/json' \
  -d '{"name":"researcher","task":"Find the latest AI news","preset":"researcher"}'

# Get sub-agent status
curl http://localhost:3001/api/agents/{id}

# Cancel a running sub-agent
curl -X DELETE http://localhost:3001/api/agents/{id}

# List / clear run history
curl http://localhost:3001/api/agents/{id}/runs
curl -X DELETE http://localhost:3001/api/agents/{id}/runs
```

Full route list: [API.md › Sub-agents](./API.md#sub-agents).

### Via CLI

```bash
pylot agents list                                # active sub-agents
pylot agents presets                             # available presets
pylot agents show <preset>                       # preset details
pylot agents spawn --preset coder "Add JWT auth" # spawn from preset
```

### Via Web UI

Navigate to the **Sub-Agents** page in the sidebar to monitor active and completed agents.

## Sub-Agent Types

| Type         | Description                                                |
| ------------ | ---------------------------------------------------------- |
| `Task`       | One-shot agent that completes a task and returns a result  |
| `Background` | Long-running agent that monitors or processes continuously |
| `Specialist` | Agent with a specific system prompt for domain expertise   |

## Configuration

The orchestrator limits concurrent sub-agents to **4** by default. Each sub-agent has:

- A **50-message** context window (vs. the main agent's configurable limit)
- A configurable **timeout** (default: 5 minutes)
- Access to all registered tools
- No smart memory (to keep sub-agents lightweight)

## Limitations

- Sub-agents cannot spawn their own sub-agents (no recursive spawning).
- Run history is persisted to disk per sub-agent, but live in-flight state is in memory and resets on restart.

## See also

- [PLUGINS.md](./PLUGINS.md) — how to add your own agent presets (no Rust required).
- [API.md](./API.md#sub-agents) — full HTTP API for sub-agent management.
- [ARCHITECTURE.md](./ARCHITECTURE.md) — where sub-agents fit in the overall system.
