# OpenPylot — Social Media Platform Setup

Detailed setup guides for all 17 supported social media platforms.

Each platform auto-enables when valid credentials are detected (via environment variables or the secrets vault). You can also explicitly enable/disable platforms in `config/default.toml` under `[social]`.

---

## Twitter / X

**Auth**: OAuth 1.0a

### Setup

1. Go to the [Twitter Developer Portal](https://developer.twitter.com/en/portal/dashboard)
2. Create a project and app
3. Under **Keys and Tokens**, generate:
   - API Key (Consumer Key)
   - API Key Secret (Consumer Secret)
   - Access Token
   - Access Token Secret
4. Enable **Read and Write** permissions

### Environment Variables

```bash
TWITTER_API_KEY=your_api_key
TWITTER_API_SECRET=your_api_secret
TWITTER_ACCESS_TOKEN=your_access_token
TWITTER_ACCESS_TOKEN_SECRET=your_access_token_secret
```

### TOML

```toml
[social]
twitter_enabled = true
```

### Supported Actions

- Publish text tweets
- Delete tweets
- Get analytics (likes, retweets, replies)

---

## LinkedIn

**Auth**: OAuth 2.0

### Setup

1. Go to [LinkedIn Developer Portal](https://www.linkedin.com/developers/apps)
2. Create an app
3. Under **Products**, request access to **Share on LinkedIn** and **Sign In with LinkedIn**
4. Under **Auth**, note your Client ID and Client Secret
5. Generate an access token via the OAuth 2.0 flow
6. Find your Person ID: `GET https://api.linkedin.com/v2/me` → the `id` field

### Environment Variables

```bash
LINKEDIN_ACCESS_TOKEN=your_access_token
LINKEDIN_PERSON_ID=urn:li:person:YOUR_ID
```

### Supported Actions

- Publish text posts
- Delete posts
- Get engagement analytics

---

## Bluesky

**Auth**: App password

### Setup

1. Go to [Bluesky Settings → App Passwords](https://bsky.app/settings/app-passwords)
2. Create a new app password

### Environment Variables

```bash
BLUESKY_HANDLE=yourhandle.bsky.social
BLUESKY_APP_PASSWORD=your_app_password
```

### Supported Actions

- Publish text posts
- Delete posts

---

## Facebook

**Auth**: Page access token

### Setup

1. Go to [Meta for Developers](https://developers.facebook.com/)
2. Create an app (type: Business)
3. Add the **Pages** product
4. Under **Tools → Graph API Explorer**, select your page and generate a Page Access Token
5. For long-lived tokens, exchange via: `GET /oauth/access_token?grant_type=fb_exchange_token&client_id=APP_ID&client_secret=APP_SECRET&fb_exchange_token=SHORT_TOKEN`

### Environment Variables

```bash
FACEBOOK_ACCESS_TOKEN=your_page_access_token
FACEBOOK_PAGE_ID=your_page_id
```

### Supported Actions

- Publish posts to pages
- Delete posts
- Get post insights

---

## Instagram

**Auth**: Facebook Graph API (Instagram Business Account)

### Setup

1. Set up a [Facebook Business Page](https://business.facebook.com/) and connect your Instagram account
2. In [Meta for Developers](https://developers.facebook.com/), create an app
3. Add the **Instagram Graph API** product
4. Generate a user access token with `instagram_basic`, `instagram_content_publish` permissions
5. Get your Instagram User ID: `GET /me/accounts` → page ID → `GET /{page-id}?fields=instagram_business_account`

### Environment Variables

```bash
INSTAGRAM_ACCESS_TOKEN=your_access_token
INSTAGRAM_USER_ID=your_instagram_user_id
```

### Supported Actions

- Publish photo/video posts (requires media URL)
- Delete posts
- Get post insights

---

## TikTok

**Auth**: OAuth 2.0

### Setup

1. Go to [TikTok for Developers](https://developers.tiktok.com/)
2. Create an app
3. Request access to **Content Posting API**
4. Implement OAuth 2.0 to obtain an access token

### Environment Variables

```bash
TIKTOK_ACCESS_TOKEN=your_access_token
```

### Supported Actions

- Publish videos (upload initiation)
- Delete posts

### Notes

- TikTok requires video content; text-only posts are not supported
- Video must be uploaded separately before publishing

---

## YouTube

**Auth**: OAuth 2.0

### Setup

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Enable the **YouTube Data API v3**
3. Create OAuth 2.0 credentials
4. Authorize with `youtube.upload` and `youtube.force-ssl` scopes

### Environment Variables

```bash
YOUTUBE_ACCESS_TOKEN=your_access_token
```

### Supported Actions

- Upload videos with title, description, tags
- Delete videos
- Get video analytics

### Notes

- YouTube only supports video content
- Videos are uploaded as "unlisted" by default

---

## Pinterest

**Auth**: OAuth 2.0

### Setup

1. Go to [Pinterest for Developers](https://developers.pinterest.com/)
2. Create an app
3. Get an access token with `pins:read`, `pins:write`, `boards:read` scopes
4. Find your board ID: `GET /v5/boards` → select the target board

### Environment Variables

```bash
PINTEREST_ACCESS_TOKEN=your_access_token
PINTEREST_BOARD_ID=your_board_id
```

### Supported Actions

- Create pins (image + description + link)
- Delete pins

---

## Reddit

**Auth**: OAuth 2.0

### Setup

1. Go to [Reddit App Preferences](https://www.reddit.com/prefs/apps)
2. Create a **script** or **web app**
3. Get an access token via OAuth 2.0
4. Choose a default subreddit for posting

### Environment Variables

```bash
REDDIT_ACCESS_TOKEN=your_access_token
REDDIT_SUBREDDIT=your_subreddit
```

### Supported Actions

- Submit text posts and links
- Delete posts
- Get post score/comments

### Notes

- Respect subreddit rules and Reddit's API rate limits
- Self-promotional content may be flagged by moderators

---

## Threads

**Auth**: Meta Graph API

### Setup

1. Go to [Meta for Developers](https://developers.facebook.com/)
2. Create or use an existing app
3. Add the **Threads API** product
4. Generate an access token with `threads_basic`, `threads_content_publish` permissions
5. Get your Threads User ID from the API

### Environment Variables

```bash
THREADS_ACCESS_TOKEN=your_access_token
THREADS_USER_ID=your_threads_user_id
```

### Supported Actions

- Publish text posts
- Delete posts

---

## Mastodon

**Auth**: Application access token

### Setup

1. On your Mastodon instance, go to **Preferences → Development → New application**
2. Set scopes: `read`, `write`, `follow`
3. Copy the access token

### Environment Variables

```bash
MASTODON_ACCESS_TOKEN=your_access_token
MASTODON_INSTANCE=https://mastodon.social
```

### Supported Actions

- Publish toots (text, with optional media)
- Delete toots
- Get toot stats

### Notes

- Set `MASTODON_INSTANCE` to your specific instance URL

---

## Discord

**Auth**: Bot token

### Setup

1. Go to [Discord Developer Portal](https://discord.com/developers/applications)
2. Create an application → **Bot** → copy the bot token
3. Under **OAuth2 → URL Generator**, select `bot` scope with `Send Messages` permission
4. Use the generated URL to invite the bot to your server
5. Right-click the target channel → **Copy Channel ID** (enable Developer Mode in settings)

### Environment Variables

```bash
DISCORD_BOT_TOKEN=your_bot_token
DISCORD_CHANNEL_ID=your_channel_id
```

### Supported Actions

- Send messages to a channel
- Delete messages

---

## Slack

**Auth**: Bot token

### Setup

1. Go to [Slack API](https://api.slack.com/apps) → **Create New App** → From scratch
2. Under **OAuth & Permissions**, add scopes: `chat:write`, `channels:read`
3. Install to workspace
4. Copy the **Bot User OAuth Token** (`xoxb-...`)

### Environment Variables

```bash
SLACK_BOT_TOKEN=xoxb-your-bot-token
SLACK_CHANNEL=general
```

### Supported Actions

- Send messages to channels
- Delete messages

---

## Medium

**Auth**: Integration token

### Setup

1. Go to [Medium Settings → Integration tokens](https://medium.com/me/settings)
2. Generate a new integration token

### Environment Variables

```bash
MEDIUM_TOKEN=your_integration_token
```

### Supported Actions

- Publish articles (title, content in Markdown or HTML)

### Notes

- Medium API is read/write but has limited functionality
- Articles are published as drafts by default

---

## Dev.to

**Auth**: API key

### Setup

1. Go to [Dev.to Settings → Extensions](https://dev.to/settings/extensions)
2. Under **DEV Community API Keys**, generate a new key

### Environment Variables

```bash
DEVTO_API_KEY=your_api_key
```

### Supported Actions

- Publish articles (title, body in Markdown, tags)
- Delete articles

---

## Hashnode

**Auth**: API key

### Setup

1. Go to [Hashnode Settings → Developer](https://hashnode.com/settings/developer)
2. Generate a Personal Access Token
3. Find your publication ID from your blog dashboard URL or the Hashnode API

### Environment Variables

```bash
HASHNODE_API_KEY=your_api_key
HASHNODE_PUBLICATION_ID=your_publication_id
```

### Supported Actions

- Publish articles (title, content in Markdown)
- Delete articles

---

## WordPress

**Auth**: Application password (Basic auth)

### Setup

1. In WordPress admin, go to **Users → Profile → Application Passwords**
2. Enter a name for the application and click **Add New Application Password**
3. Copy the generated password (shown only once)

### Environment Variables

```bash
WORDPRESS_SITE_URL=https://your-site.com
WORDPRESS_USERNAME=your_username
WORDPRESS_APP_PASSWORD=your_application_password
```

### Supported Actions

- Publish posts (title, content, categories, tags)
- Delete posts
- Get post stats

### Notes

- Requires WordPress 5.6+ (Application Passwords are built-in)
- For self-hosted sites, ensure the REST API is enabled (`/wp-json/wp/v2/posts`)
- The site URL should not include a trailing slash
