# Security Guide

This document covers OpenPylot's security architecture and best practices.

## Secrets Management

### Vault

All sensitive credentials are stored in an **AES-256-GCM encrypted vault** at `~/.pylot/secrets.vault`:

- Master key derived via **Argon2id** from a user-provided password
- Vault is encrypted at rest — never stored in plaintext
- Credentials are decrypted only when needed at runtime

### Environment Variables

As an alternative to the vault, credentials can be provided via environment variables:

```bash
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
TWITTER_API_KEY=...
```

Environment variables take precedence over vault values.

## OAuth Security

### PKCE (Proof Key for Code Exchange)

OAuth flows for public clients (like Twitter/X) use **PKCE with S256 challenge method**:

1. A random `code_verifier` is generated for each flow
2. A SHA-256 `code_challenge` is sent with the authorization request
3. The original `code_verifier` is sent during token exchange
4. The provider verifies the challenge before issuing tokens

This prevents authorization code interception attacks.

### CSRF Protection

All OAuth flows include a random `state` parameter that is validated on callback to prevent cross-site request forgery.

### Token Storage

- Access tokens and refresh tokens are stored in the encrypted vault
- Tokens are never logged or exposed in API responses
- Token refresh happens automatically before expiry via the scheduler

## API Security

### Local-Only by Default

The API server binds to `localhost:3001` by default — not accessible from the network.

### CORS

CORS is configured to allow all origins for development. In production, restrict this via configuration.

### Input Validation

- All API inputs are deserialized through typed Rust structs (serde)
- Path parameters are validated before use
- SQL queries use parameterized statements (never string interpolation)

## Dangerous Command Protection

The agent has a built-in safety system for dangerous operations:

- Commands matching dangerous patterns (rm -rf, DROP TABLE, etc.) require explicit approval
- Secrets in tool output are automatically redacted
- Approval can be required for all tool calls via `--approval` mode

### Dangerous Patterns

The following patterns trigger approval prompts:

- `rm -rf`, `rm -r /`
- `DROP TABLE`, `DROP DATABASE`
- `chmod 777`, `chmod -R 777`
- `> /dev/sda`, `mkfs`
- `:(){ :|:& };:` (fork bomb)
- `curl | sh`, `wget | sh`

## Social Media Security

### Credential Isolation

Each social platform's credentials are stored independently:

- API keys and secrets stay server-side only
- OAuth tokens are per-user and encrypted at rest
- No credentials are embedded in the application binary

### Rate Limiting

Social media API calls respect platform rate limits. The SocialManager tracks API usage per platform.

## Best Practices

1. **Use the vault** — Don't store credentials in config files
2. **Rotate tokens** — Use `pylot init` to refresh OAuth tokens periodically
3. **Review tool calls** — Enable `--approval` mode for sensitive environments
4. **Keep updated** — Run `pylot update` to get security patches
5. **Limit network access** — Keep the API server on localhost unless behind a reverse proxy
