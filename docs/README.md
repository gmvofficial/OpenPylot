# OpenPylot — Documentation

Welcome to the OpenPylot backend documentation. OpenPylot is a Rust-powered personal AI assistant that ships as a single binary exposing a CLI, REST API, WebSocket streams, and SDKs for Python and Node.js.

This index points you to the right document depending on what you want to do.

## I want to…

| Goal                           | Read                                         |
| ------------------------------ | -------------------------------------------- |
| Try it in 5 minutes            | [GETTING-STARTED.md](./GETTING-STARTED.md)   |
| Install on my machine          | [INSTALLATION.md](./INSTALLATION.md)         |
| Understand the system          | [ARCHITECTURE.md](./ARCHITECTURE.md)         |
| Configure the agent            | [CONFIGURATION.md](./CONFIGURATION.md)       |
| Call the HTTP / WebSocket API  | [API.md](./API.md)                           |
| Deploy to production           | [DEPLOYMENT.md](./DEPLOYMENT.md)             |
| Build from source / contribute | [DEVELOPMENT.md](./DEVELOPMENT.md)           |
| Review the security model      | [SECURITY.md](./SECURITY.md)                 |
| Connect social platforms       | [SOCIAL-PLATFORMS.md](./SOCIAL-PLATFORMS.md) |
| Use sub-agents                 | [AGENTS.md](./AGENTS.md)                     |
| Add custom skills / agents     | [PLUGINS.md](./PLUGINS.md)                   |

## Documentation map

```
docs/
├── README.md            # You are here
├── GETTING-STARTED.md   # Quickstart (install → first chat)
├── INSTALLATION.md      # All install methods (binary, brew, docker, source)
├── CONFIGURATION.md     # TOML, env vars, secrets vault
├── ARCHITECTURE.md      # Modules, data flow, design decisions
├── API.md               # REST + WebSocket endpoint reference
├── DEPLOYMENT.md        # Docker, systemd, launchd, reverse proxy
├── DEVELOPMENT.md       # Build, test, project layout, contributing
├── SECURITY.md          # Vault, encryption, OAuth, threat model
├── AGENTS.md            # Sub-agent orchestration
├── PLUGINS.md           # Plug-and-play skills & agent presets
└── SOCIAL-PLATFORMS.md  # Per-platform setup (17 platforms)
```

## Project at a glance

- **Language:** Rust (1.75+)
- **Binary name:** `pylot`
- **Default API port:** `3001`
- **Data directory:** `~/.pylot/`
- **License:** MIT
- **Repository:** <https://github.com/globalmindventures/OpenPylot>

For a high-level feature overview see the top-level [`README.md`](../README.md) at the repo root.
