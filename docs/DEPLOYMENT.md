# Deployment

How to run OpenPylot in production-like environments: Docker, systemd, launchd, behind a reverse proxy. For local install see [GETTING-STARTED.md](./GETTING-STARTED.md).

---

## Topology

OpenPylot runs as a single binary that hosts:

- the REST + WebSocket API on a single port (default **3001**),
- the background scheduler (cron jobs),
- optional static serving of the built Next.js frontend,
- optional Telegram long-polling bot.

```
              ┌─────────────────────────────┐
   Internet ──▶  Reverse proxy (nginx /     │
              │  Caddy / Traefik) — TLS,    │
              │  auth, rate limit           │
              └────────────┬────────────────┘
                           │
                   :3001 (HTTP/WS)
                           │
                ┌──────────▼──────────┐
                │  pylot serve        │
                │  ├─ API + WS        │
                │  ├─ Scheduler       │
                │  └─ Frontend (opt.) │
                └──────────┬──────────┘
                           │
                ┌──────────▼──────────┐
                │  ~/.pylot/          │
                │  ├─ secrets.enc     │
                │  ├─ config.toml     │
                │  └─ data/*.db,*.json│
                └─────────────────────┘
```

Everything is local state on disk. There is **no external database requirement** — SQLite is used for smart memory.

---

## Docker

A `Dockerfile` and `docker-compose.yml` are provided at the repo root.

### docker-compose (recommended)

```bash
docker compose up -d
docker compose logs -f pylot
```

The compose file mounts `./pylot-data` → `/home/pylot/.pylot` so credentials and SQLite data survive container restarts.

Override env vars in a `.env` next to `docker-compose.yml`:

```env
OPENAI_API_KEY=sk-...
PYLOT_API_PORT=3001
RUST_LOG=info
```

### Plain Docker

```bash
docker build -t pylot .

docker run -d --name pylot \
  -p 3001:3001 \
  -v $HOME/.pylot:/home/pylot/.pylot \
  -e OPENAI_API_KEY=sk-... \
  -e RUST_LOG=info \
  pylot
```

### Healthcheck

```yaml
healthcheck:
  test: ['CMD', 'curl', '-fsS', 'http://localhost:3001/api/status']
  interval: 30s
  timeout: 5s
  retries: 3
```

---

## systemd (Linux)

`pylot serve install` generates the unit file for you, but here it is for reference (`/etc/systemd/system/pylot.service`):

```ini
[Unit]
Description=OpenPylot Agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=pylot
Environment=RUST_LOG=info
Environment=PYLOT_API_PORT=3001
ExecStart=/usr/local/bin/pylot serve
Restart=on-failure
RestartSec=5
WorkingDirectory=/home/pylot

[Install]
WantedBy=multi-user.target
```

Then:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now pylot
sudo systemctl status pylot
journalctl -u pylot -f
```

---

## launchd (macOS)

`pylot serve install` writes `~/Library/LaunchAgents/com.openpylot.pylot.plist` and loads it. Manage with:

```bash
launchctl load   ~/Library/LaunchAgents/com.openpylot.pylot.plist
launchctl unload ~/Library/LaunchAgents/com.openpylot.pylot.plist
pylot serve uninstall   # removes the plist
```

Logs go to `~/.pylot/logs/pylot.log`.

---

## Reverse proxy + TLS

The API has **no built-in authentication** and CORS is open by default. Any public deployment must put `pylot` behind a reverse proxy.

### nginx

```nginx
server {
  listen 443 ssl http2;
  server_name pylot.example.com;

  ssl_certificate     /etc/letsencrypt/live/pylot.example.com/fullchain.pem;
  ssl_certificate_key /etc/letsencrypt/live/pylot.example.com/privkey.pem;

  # Basic auth (or replace with your SSO/JWT layer)
  auth_basic           "OpenPylot";
  auth_basic_user_file /etc/nginx/.htpasswd;

  client_max_body_size 100M;   # match API upload limit

  location / {
    proxy_pass http://127.0.0.1:3001;
    proxy_set_header Host              $host;
    proxy_set_header X-Forwarded-For   $remote_addr;
    proxy_set_header X-Forwarded-Proto $scheme;

    # WebSocket support (/ws/*)
    proxy_http_version 1.1;
    proxy_set_header Upgrade    $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_read_timeout 3600s;
  }
}
```

### Caddy

```caddyfile
pylot.example.com {
  basicauth { admin <bcrypt-hash> }
  reverse_proxy 127.0.0.1:3001
  request_body { max_size 100MB }
}
```

---

## Environment

Recommended environment variables for production:

| Variable               | Example             | Purpose                                           |
| ---------------------- | ------------------- | ------------------------------------------------- |
| `RUST_LOG`             | `info`              | Log level (use `debug` only when troubleshooting) |
| `PYLOT_API_PORT`       | `3001`              | API listen port                                   |
| `PYLOT_DATA_DIR`       | `/var/lib/pylot`    | Override `~/.pylot`                               |
| `OPENAI_API_KEY`       | `sk-…`              | LLM credential (or use vault)                     |
| `ANTHROPIC_API_KEY`    | `sk-ant-…`          | LLM credential (or use vault)                     |
| `GOOGLE_REDIRECT_PORT` | `8085`              | Local OAuth callback port                         |
| `PYLOT_VAULT_PASSWORD` | _(set by operator)_ | Unattended vault unlock                           |

The full list lives in [CONFIGURATION.md](./CONFIGURATION.md).

---

## Backups

Back up the entire data directory:

```bash
tar czf pylot-backup-$(date +%F).tar.gz -C $HOME .pylot
```

Critical files:

- `secrets.enc` — encrypted credentials (lose this = re-add every integration)
- `data/smart_memory.db` — semantic memory
- `data/*.json` — notes, reminders, knowledge metadata, OAuth tokens
- `config.toml` — user overrides

> Backups should be encrypted at rest — `secrets.enc` is already encrypted but the SQLite DB and JSON files are not.

---

## Upgrading

Binary install:

```bash
curl -fsSL https://raw.githubusercontent.com/gmvofficial/OpenPylot/main/install.sh | bash
pylot --version
```

Docker:

```bash
docker compose pull
docker compose up -d
```

The data layout is forward-compatible across `0.x` releases; the release notes ([CHANGELOG.md](../CHANGELOG.md)) call out any one-off migrations.

---

## Sizing

A typical single-user instance is comfortable with:

- **1 vCPU**, **512 MB RAM**, **5 GB disk**
- Outbound HTTPS to OpenAI/Anthropic, Google APIs, etc.
- Inbound only on the reverse-proxy TLS port

Memory grows mostly with the SQLite semantic-memory database; expect ~1 MB per 10k stored facts.

---

## Observability

- **Logs:** stdout (JSON when `RUST_LOG_FORMAT=json`), also tailed via `pylot logs`.
- **Healthcheck:** `GET /api/status` returns `200` once the agent is ready.
- **Metrics:** not yet exposed natively — scrape `/api/status` and `/api/jobs` for now.

---

## Hardening checklist

- [ ] Run as a dedicated non-root user (`pylot`).
- [ ] Bind the binary to `127.0.0.1` and only expose via reverse proxy.
- [ ] Terminate TLS at the proxy and force HTTPS.
- [ ] Add basic auth, mTLS, or an SSO layer in the proxy.
- [ ] Set `PYLOT_VAULT_PASSWORD` via your secret manager — never commit it.
- [ ] Restrict CORS at the proxy if the API is consumed only by your dashboard.
- [ ] Schedule encrypted backups of `~/.pylot/`.
- [ ] Set `RUST_LOG=info` (avoid `debug` in prod — verbose and may log payloads).

See [SECURITY.md](./SECURITY.md) for the full security model.
