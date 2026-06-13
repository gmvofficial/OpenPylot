# OpenPylot — Configuration Reference

Complete reference for all configuration options. OpenPylot uses a layered configuration system where higher-priority sources override lower ones.

> For installation see [INSTALLATION.md](./INSTALLATION.md). For how secrets are encrypted at rest see [SECURITY.md](./SECURITY.md). For per-platform social-media credential setup see [SOCIAL-PLATFORMS.md](./SOCIAL-PLATFORMS.md).

## Configuration Priority

| Priority    | Source                  | Notes                                            |
| ----------- | ----------------------- | ------------------------------------------------ |
| 1 (highest) | Environment variables   | Best for CI/CD and Docker                        |
| 2           | Encrypted secrets vault | `~/.pylot/secrets.enc` — for API keys and tokens |
| 3           | TOML config files       | `config/default.toml` or `~/.pylot/config.toml`  |
| 4 (lowest)  | Built-in defaults       | Hardcoded in `config.rs`                         |

---

## TOML Configuration

The default config file is `config/default.toml` in the project directory. For user-level overrides, create `~/.pylot/config.toml`.

### Agent

```toml
[agent]
name = "Pylot"                    # Agent display name
persona = "helpful, concise, professional"  # System persona description
max_context_messages = 50         # Max messages kept in conversation context
max_tool_iterations = 15          # Max tool call loops per turn
```

### LLM

```toml
[llm]
provider = "openai"       # "openai" or "anthropic"
model = "gpt-4o"          # Model name
max_tokens = 4096          # Max output tokens
temperature = 0.6          # Sampling temperature (0.0 – 2.0)
```

### Storage

```toml
[storage]
data_dir = ""   # Data directory (default: ~/.pylot/data)
```

### Google Calendar

```toml
[google_calendar]
enabled = true
redirect_port = 8085   # Local OAuth redirect port
scopes = ["https://www.googleapis.com/auth/calendar"]
```

### Gmail

```toml
[gmail]
enabled = false
```

### Telegram

```toml
[telegram]
enabled = true
```

### WhatsApp

```toml
[whatsapp]
enabled = false
```

### Scheduler

```toml
[scheduler]
enabled = false
```

### Memory

```toml
[memory]
enabled = true
db_name = "smart_memory.db"               # SQLite database filename
embedding_model = "text-embedding-3-small" # OpenAI embedding model
auto_extract = true                        # Auto-extract facts from conversations
extraction_interval = 5                    # Extract every N conversation turns
similarity_threshold = 0.35               # Minimum cosine similarity for search results
max_memory_context = 10                    # Max memory facts injected into context
max_knowledge_context = 10                 # Max knowledge chunks injected into context
chunk_size = 500                           # Document chunk size (characters)
chunk_overlap = 50                         # Chunk overlap (characters)
```

### Social Media

```toml
[social]
twitter_enabled = false
linkedin_enabled = false
bluesky_enabled = false
facebook_enabled = false
instagram_enabled = false
tiktok_enabled = false
youtube_enabled = false
pinterest_enabled = false
reddit_enabled = false
threads_enabled = false
mastodon_enabled = false
discord_enabled = false
slack_enabled = false
medium_enabled = false
devto_enabled = false
hashnode_enabled = false
wordpress_enabled = false
```

### MCP (Model Context Protocol)

```toml
[mcp]
enabled = false
# config_path = "~/.pylot/mcp-servers.json"
```

### Learning

```toml
[learning]
enabled = true
auto_score = false       # LLM-as-judge auto-scoring (uses extra LLM calls)
judge_votes = 3          # Number of judge votes per evaluation
skill_evolution = false  # Auto-generate skills from failure patterns
```

### Marketing

```toml
[marketing]
enabled = false
```

---

## Environment Variables

All configuration values can be overridden via environment variables.

### Core

| Variable            | Description                                           | Default                          |
| ------------------- | ----------------------------------------------------- | -------------------------------- |
| `OPENAI_API_KEY`    | OpenAI API key                                        | —                                |
| `ANTHROPIC_API_KEY` | Anthropic API key                                     | —                                |
| `LLM_PROVIDER`      | LLM provider (`openai` / `anthropic`)                 | `openai`                         |
| `LLM_MODEL`         | Model name                                            | `gpt-4o`                         |
| `LLM_MAX_TOKENS`    | Max output tokens                                     | `4096`                           |
| `LLM_TEMPERATURE`   | Sampling temperature                                  | `0.6`                            |
| `AGENT_NAME`        | Agent display name                                    | `Pylot`                          |
| `AGENT_PERSONA`     | System persona description                            | `helpful, concise, professional` |
| `DATA_DIR`          | Data directory                                        | `~/.pylot/data`                  |
| `RUST_LOG`          | Log level (`error`, `warn`, `info`, `debug`, `trace`) | `info`                           |

### Google Calendar & Gmail

| Variable               | Description                               |
| ---------------------- | ----------------------------------------- |
| `GOOGLE_CLIENT_ID`     | Google OAuth client ID                    |
| `GOOGLE_CLIENT_SECRET` | Google OAuth client secret                |
| `GOOGLE_REDIRECT_PORT` | Local redirect port (default: `8085`)     |
| `GMAIL_ENABLED`        | Enable Gmail integration (`true`/`false`) |

### Telegram

| Variable             | Description                        |
| -------------------- | ---------------------------------- |
| `TELEGRAM_BOT_TOKEN` | Telegram bot token from @BotFather |
| `TELEGRAM_CHAT_ID`   | Default chat ID for notifications  |

### WhatsApp (Twilio)

| Variable               | Description                   |
| ---------------------- | ----------------------------- |
| `TWILIO_ACCOUNT_SID`   | Twilio account SID            |
| `TWILIO_AUTH_TOKEN`    | Twilio auth token             |
| `TWILIO_WHATSAPP_FROM` | Twilio WhatsApp sender number |
| `WHATSAPP_TO`          | Default recipient number      |

### Social Media Platforms

#### Twitter/X

| Variable                      | Description                          |
| ----------------------------- | ------------------------------------ |
| `TWITTER_API_KEY`             | Twitter API key (consumer key)       |
| `TWITTER_API_SECRET`          | Twitter API secret (consumer secret) |
| `TWITTER_ACCESS_TOKEN`        | OAuth access token                   |
| `TWITTER_ACCESS_TOKEN_SECRET` | OAuth access token secret            |

#### LinkedIn

| Variable                | Description                                        |
| ----------------------- | -------------------------------------------------- |
| `LINKEDIN_ACCESS_TOKEN` | LinkedIn OAuth 2.0 access token                    |
| `LINKEDIN_PERSON_ID`    | LinkedIn person URN (e.g., `urn:li:person:ABC123`) |

#### Bluesky

| Variable               | Description                               |
| ---------------------- | ----------------------------------------- |
| `BLUESKY_HANDLE`       | Bluesky handle (e.g., `user.bsky.social`) |
| `BLUESKY_APP_PASSWORD` | Bluesky app password                      |

#### Facebook

| Variable                | Description                |
| ----------------------- | -------------------------- |
| `FACEBOOK_ACCESS_TOKEN` | Facebook page access token |
| `FACEBOOK_PAGE_ID`      | Facebook page ID           |

#### Instagram

| Variable                 | Description                                         |
| ------------------------ | --------------------------------------------------- |
| `INSTAGRAM_ACCESS_TOKEN` | Instagram API access token (via Facebook Graph API) |
| `INSTAGRAM_USER_ID`      | Instagram business account user ID                  |

#### TikTok

| Variable              | Description                   |
| --------------------- | ----------------------------- |
| `TIKTOK_ACCESS_TOKEN` | TikTok OAuth 2.0 access token |

#### YouTube

| Variable               | Description                    |
| ---------------------- | ------------------------------ |
| `YOUTUBE_ACCESS_TOKEN` | YouTube OAuth 2.0 access token |

#### Pinterest

| Variable                 | Description                       |
| ------------------------ | --------------------------------- |
| `PINTEREST_ACCESS_TOKEN` | Pinterest OAuth 2.0 access token  |
| `PINTEREST_BOARD_ID`     | Pinterest board ID for publishing |

#### Reddit

| Variable              | Description                   |
| --------------------- | ----------------------------- |
| `REDDIT_ACCESS_TOKEN` | Reddit OAuth 2.0 access token |
| `REDDIT_SUBREDDIT`    | Default subreddit for posting |

#### Threads

| Variable               | Description                               |
| ---------------------- | ----------------------------------------- |
| `THREADS_ACCESS_TOKEN` | Threads API access token (Meta Graph API) |
| `THREADS_USER_ID`      | Threads user ID                           |

#### Mastodon

| Variable                | Description                                             |
| ----------------------- | ------------------------------------------------------- |
| `MASTODON_ACCESS_TOKEN` | Mastodon application access token                       |
| `MASTODON_INSTANCE`     | Mastodon instance URL (e.g., `https://mastodon.social`) |

#### Discord

| Variable             | Description                |
| -------------------- | -------------------------- |
| `DISCORD_BOT_TOKEN`  | Discord bot token          |
| `DISCORD_CHANNEL_ID` | Default Discord channel ID |

#### Slack

| Variable          | Description                  |
| ----------------- | ---------------------------- |
| `SLACK_BOT_TOKEN` | Slack bot token (`xoxb-...`) |
| `SLACK_CHANNEL`   | Default Slack channel        |

#### Medium

| Variable       | Description              |
| -------------- | ------------------------ |
| `MEDIUM_TOKEN` | Medium integration token |

#### Dev.to

| Variable        | Description    |
| --------------- | -------------- |
| `DEVTO_API_KEY` | Dev.to API key |

#### Hashnode

| Variable                  | Description             |
| ------------------------- | ----------------------- |
| `HASHNODE_API_KEY`        | Hashnode API key        |
| `HASHNODE_PUBLICATION_ID` | Hashnode publication ID |

#### WordPress

| Variable                 | Description                                      |
| ------------------------ | ------------------------------------------------ |
| `WORDPRESS_SITE_URL`     | WordPress site URL (e.g., `https://example.com`) |
| `WORDPRESS_USERNAME`     | WordPress username                               |
| `WORDPRESS_APP_PASSWORD` | WordPress application password                   |

### MCP

| Variable          | Description                    |
| ----------------- | ------------------------------ |
| `MCP_ENABLED`     | Enable MCP (`true`/`false`)    |
| `MCP_CONFIG_PATH` | Path to MCP server config JSON |

### Learning

| Variable                   | Description                             |
| -------------------------- | --------------------------------------- |
| `LEARNING_ENABLED`         | Enable learning engine (`true`/`false`) |
| `LEARNING_AUTO_SCORE`      | Enable LLM-as-judge auto-scoring        |
| `LEARNING_JUDGE_VOTES`     | Number of judge votes                   |
| `LEARNING_SKILL_EVOLUTION` | Enable skill evolution from failures    |

### Marketing

| Variable            | Description                             |
| ------------------- | --------------------------------------- |
| `MARKETING_ENABLED` | Enable marketing agent (`true`/`false`) |

---

## Secrets Vault

The secrets vault (`~/.pylot/secrets.enc`) stores sensitive credentials encrypted at rest.

### How It Works

- Encryption: AES-256-GCM
- Key derivation: Argon2id with machine-specific salt
- Machine-bound: the vault is tied to the machine it was created on

### Managing Secrets

```bash
# Interactive setup (stores secrets in vault)
pylot init

# Add a specific integration
pylot add google-calendar   # Prompts for OAuth credentials
pylot add telegram           # Prompts for bot token

# View configured integrations
pylot status

# Reset all secrets
pylot init --reset
```

### Vault vs Environment Variables

| Approach | When to Use                               |
| -------- | ----------------------------------------- |
| Vault    | Personal machines, long-lived credentials |
| Env vars | CI/CD, Docker, ephemeral environments     |
| Both     | Env vars override vault values            |

When both are present, environment variables take precedence over vault values. This lets you override specific credentials in Docker or CI without modifying the vault.
