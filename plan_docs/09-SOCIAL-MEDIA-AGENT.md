# 09 — Social Media Manager Agent

## Objective

Build a specialized social media management agent that can manage social media profiles, draft marketing campaigns, schedule posts, generate content, and let users review and publish. This is a platform-level feature that uses the sub-agent system (doc 04) and integrates patterns from Postiz.

---

## Current State

- **Core social module implemented**: `src/social/` with types, manager, providers
- **17 platform providers implemented** with publish/delete/analytics via `SocialPlatformProvider` trait
- **Config/env wiring complete** for all 17 platforms with auto-enable on credential detection
- **Platform trait**: `SocialPlatformProvider` with `publish()`, `delete()`, `get_analytics()` — all 17 providers implement it
- **ContentType enum**: Text, Image, Video, Carousel, Story, Reel, Article, Pin, Thread
- **SocialPost expanded**: `content_type`, `title`, `extra: HashMap<String, String>` for platform-specific params

### Implemented Platforms (17/17)

| Platform | Provider | API | Status |
|----------|----------|-----|--------|
| Twitter/X | `TwitterProvider` | v2 API, OAuth 1.0a | ✅ Implemented |
| LinkedIn | `LinkedInProvider` | UGC API | ✅ Implemented |
| Bluesky | `BlueskyProvider` | AT Protocol | ✅ Implemented |
| Facebook | `FacebookProvider` | Graph API v21.0 | ✅ Implemented |
| Instagram | `InstagramProvider` | FB Graph API (container+publish) | ✅ Implemented |
| TikTok | `TikTokProvider` | Content Publishing API | ✅ Implemented |
| YouTube | `YouTubeProvider` | Data API v3 (resumable upload) | ✅ Implemented |
| Pinterest | `PinterestProvider` | API v5 | ✅ Implemented |
| Reddit | `RedditProvider` | OAuth API | ✅ Implemented |
| Threads | `ThreadsProvider` | Meta Graph API v1.0 | ✅ Implemented |
| Mastodon | `MastodonProvider` | /api/v1/statuses (instance-agnostic) | ✅ Implemented |
| Discord | `DiscordProvider` | Bot API v10 + webhook fallback | ✅ Implemented |
| Slack | `SlackProvider` | chat.postMessage | ✅ Implemented |
| Medium | `MediumProvider` | v1 API (markdown articles) | ✅ Implemented |
| Dev.to | `DevToProvider` | Forem API | ✅ Implemented |
| Hashnode | `HashnodeProvider` | GraphQL API | ✅ Implemented |
| WordPress | `WordPressProvider` | REST API v2 | ✅ Implemented |

### Not Yet Implemented
- Campaign management (planner, content calendar)
- AI content generation / variation engine
- Post scheduling with auto-publish loop
- Analytics storage and aggregation
- Media handling (upload, resize)
- OAuth2 flows per platform
- SQLite storage for posts/campaigns

---

## Reference Implementation

### Postiz (Primary — Social Media Platform)
- **Path**: `extra_repos/postiz-app-main/`
- **Features**:
  - 36+ platform integrations (Twitter/X, Instagram, LinkedIn, Facebook, TikTok, YouTube, Pinterest, Threads, Bluesky, etc.)
  - Content scheduling with calendar views
  - AI content generation (GPT-4 powered)
  - Campaign management via post grouping
  - Media library (images, video, AI generation)
  - Team collaboration (comments, approvals)
  - Analytics per-post and per-channel
  - OAuth2 for all platforms
  - Temporal-based async job orchestration
  - Auto-refresh tokens with retry logic

---

## Architecture

### Platform Integrations (Phase 1 — MVP: 5 platforms → Expanded to 17)

All 17 platforms implemented. Each implements the `SocialPlatformProvider` trait with `publish()`, `delete()`, and `get_analytics()`.

| Platform | API | Auth | Features |
|----------|-----|------|----------|
| Twitter/X | v2 API | OAuth 1.0a (4 keys) | Post, thread, analytics (likes/retweets/replies) |
| LinkedIn | UGC API | OAuth 2.0 | Post, article, analytics (likes/comments/shares) |
| Bluesky | AT Protocol | App password | Post, thread |
| Facebook | Graph API v21.0 | Page access token | Page post, delete, analytics (likes/shares/comments) |
| Instagram | FB Graph API | OAuth 2.0 (via Facebook) | Image/reel via container+publish, analytics (likes/comments/reach) |
| TikTok | Content Publishing API | OAuth 2.0 | Video upload (init+publish), analytics (views/likes/shares) |
| YouTube | Data API v3 | OAuth 2.0 | Resumable video upload, delete, statistics (views/likes/comments) |
| Pinterest | API v5 | OAuth 2.0 | Pin creation with board targeting, analytics (impressions/saves/clicks) |
| Reddit | OAuth2 API | OAuth 2.0 | Self/link posts to subreddit, delete |
| Threads | Meta Graph API v1.0 | OAuth 2.0 | Text/image/video via container+publish, insights |
| Mastodon | /api/v1/statuses | App token (instance-agnostic) | Post, delete, analytics (reblogs/favourites/replies) |
| Discord | Bot API v10 | Bot token or webhook URL | Channel messages, delete |
| Slack | Web API | Bot token | chat.postMessage, chat.delete |
| Medium | v1 API | Integration token | Markdown articles (published as draft) |
| Dev.to | Forem API | API key | Articles, unpublish, page_views analytics |
| Hashnode | GraphQL | API key | PublishPost mutation, publication targeting |
| WordPress | REST API v2 | Basic auth (app password) | Posts, delete, site_url configurable |

### Module Structure

```
src/social/
├── mod.rs                  -- Public API, SocialMediaManager
├── types.rs                -- Post, Campaign, Platform, Schedule, Analytics
├── platforms/
│   ├── mod.rs             -- Platform trait, registry
│   ├── twitter.rs         -- Twitter/X API
│   ├── linkedin.rs        -- LinkedIn API
│   ├── instagram.rs       -- Instagram Graph API
│   ├── facebook.rs        -- Facebook Graph API
│   └── bluesky.rs         -- Bluesky AT Protocol
├── campaign.rs             -- Campaign management (draft, schedule, review)
├── content.rs              -- AI content generation
├── scheduler.rs            -- Post scheduling + auto-publish
├── analytics.rs            -- Analytics collection
├── media.rs                -- Media handling (upload, resize)
└── tools.rs                -- Social media tools for agent
```

### Data Structures

```rust
// File: src/social/types.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Platform {
    Twitter,
    LinkedIn,
    Instagram,
    Facebook,
    Bluesky,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PostStatus {
    Draft,
    Pending,        // Awaiting user review/approval
    Scheduled,      // Approved, waiting for publish time
    Published,      // Successfully posted
    Failed,         // Publish failed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialPost {
    pub id: String,
    pub platform: Platform,
    pub content: String,
    pub media: Vec<MediaAttachment>,
    pub status: PostStatus,
    pub scheduled_at: Option<String>,       // ISO timestamp
    pub published_at: Option<String>,
    pub campaign_id: Option<String>,
    pub hashtags: Vec<String>,
    pub mentions: Vec<String>,
    pub link: Option<String>,
    pub thread_parent: Option<String>,      // For threads
    pub analytics: Option<PostAnalytics>,
    pub platform_post_id: Option<String>,   // ID from platform after publishing
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAttachment {
    pub id: String,
    pub url: String,            // Local path or URL
    pub media_type: String,     // image/jpeg, video/mp4, etc.
    pub alt_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Campaign {
    pub id: String,
    pub name: String,
    pub description: String,
    pub platforms: Vec<Platform>,
    pub posts: Vec<String>,         // Post IDs
    pub status: CampaignStatus,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub goals: Vec<String>,         // Campaign goals/KPIs
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CampaignStatus {
    Planning,
    Draft,
    Review,
    Active,
    Completed,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostAnalytics {
    pub impressions: Option<u64>,
    pub likes: Option<u64>,
    pub shares: Option<u64>,
    pub comments: Option<u64>,
    pub clicks: Option<u64>,
    pub reach: Option<u64>,
    pub fetched_at: String,
}
```

### Platform Trait

```rust
// File: src/social/platforms/mod.rs

#[async_trait]
pub trait SocialPlatform: Send + Sync {
    fn platform(&self) -> Platform;
    fn name(&self) -> &str;

    /// Authenticate / refresh token
    async fn authenticate(&self) -> Result<()>;

    /// Check if auth is valid
    async fn is_authenticated(&self) -> bool;

    /// Publish a post
    async fn publish(&self, post: &SocialPost) -> Result<PublishResult>;

    /// Delete a published post
    async fn delete(&self, platform_post_id: &str) -> Result<()>;

    /// Get analytics for a post
    async fn get_analytics(&self, platform_post_id: &str) -> Result<PostAnalytics>;

    /// Get profile info
    async fn get_profile(&self) -> Result<ProfileInfo>;

    /// Validate post content (character limits, media requirements)
    fn validate(&self, post: &SocialPost) -> Result<Vec<ValidationWarning>>;

    /// Platform-specific content constraints
    fn constraints(&self) -> PlatformConstraints;
}

pub struct PlatformConstraints {
    pub max_chars: usize,           // 280 for Twitter, 3000 for LinkedIn, etc.
    pub max_images: usize,
    pub max_video_size_mb: usize,
    pub supports_threads: bool,
    pub supports_scheduling: bool,
    pub supports_analytics: bool,
}
```

---

## Implementation Steps

### Step 1: Define types and platform trait (Day 1)

Files: `src/social/types.rs`, `src/social/platforms/mod.rs` — as defined above.

### Step 2: Implement Twitter/X platform (Day 1-2)

**File**: `src/social/platforms/twitter.rs`

```rust
pub struct TwitterPlatform {
    client: reqwest::Client,
    oauth_token: String,
    api_base: String,   // https://api.twitter.com/2
}

impl TwitterPlatform {
    pub async fn new(secrets: &SecretsVault) -> Result<Self> {
        let token = secrets.get("twitter_oauth_token").await?;
        Ok(Self {
            client: reqwest::Client::new(),
            oauth_token: token,
            api_base: "https://api.twitter.com/2".to_string(),
        })
    }
}

#[async_trait]
impl SocialPlatform for TwitterPlatform {
    fn platform(&self) -> Platform { Platform::Twitter }
    fn name(&self) -> &str { "Twitter/X" }

    async fn publish(&self, post: &SocialPost) -> Result<PublishResult> {
        // POST /2/tweets
        let mut body = json!({ "text": post.content });

        // Handle media
        if !post.media.is_empty() {
            let media_ids = self.upload_media(&post.media).await?;
            body["media"] = json!({ "media_ids": media_ids });
        }

        // Handle thread (reply to parent)
        if let Some(parent_id) = &post.thread_parent {
            body["reply"] = json!({ "in_reply_to_tweet_id": parent_id });
        }

        let resp = self.client.post(&format!("{}/tweets", self.api_base))
            .bearer_auth(&self.oauth_token)
            .json(&body)
            .send().await?;

        let result: Value = resp.json().await?;
        Ok(PublishResult {
            platform_post_id: result["data"]["id"].as_str().unwrap().to_string(),
            url: format!("https://x.com/i/status/{}", result["data"]["id"]),
        })
    }

    fn constraints(&self) -> PlatformConstraints {
        PlatformConstraints {
            max_chars: 280,
            max_images: 4,
            max_video_size_mb: 512,
            supports_threads: true,
            supports_scheduling: false,  // Via our scheduler, not Twitter's
            supports_analytics: true,
        }
    }

    fn validate(&self, post: &SocialPost) -> Result<Vec<ValidationWarning>> {
        let mut warnings = vec![];
        if post.content.len() > 280 {
            warnings.push(ValidationWarning::ContentTooLong {
                max: 280, actual: post.content.len()
            });
        }
        Ok(warnings)
    }
    // ... other methods
}
```

### Step 3: Implement other platforms (Day 2-3)

Repeat the pattern for LinkedIn, Instagram, Facebook, Bluesky. Each implements the `SocialPlatform` trait with platform-specific API calls.

Key differences:
- **LinkedIn**: POST to `/v2/ugcPosts`, supports articles, longer text (3000 chars)
- **Instagram**: Requires media (text-only posts not supported), uses Graph API
- **Facebook**: Page posts via Graph API, supports link previews
- **Bluesky**: AT Protocol, app password auth, supports rich text facets

### Step 4: AI Content Generation (Day 3)

**File**: `src/social/content.rs`

```rust
pub struct ContentGenerator {
    llm: Arc<dyn LlmProvider>,
}

impl ContentGenerator {
    /// Generate platform-specific content from a topic/brief
    pub async fn generate_post(
        &self,
        topic: &str,
        platform: Platform,
        tone: Option<&str>,
        constraints: &PlatformConstraints,
    ) -> Result<GeneratedContent> {
        let prompt = format!(
            "Generate a social media post for {}.\n\n\
             Topic: {}\n\
             Tone: {}\n\
             Character limit: {}\n\n\
             Include:\n\
             - Engaging opening hook\n\
             - Key message\n\
             - Call to action\n\
             - 3-5 relevant hashtags\n\n\
             Return as JSON: {{\"content\": \"...\", \"hashtags\": [...], \"suggested_media\": \"...\"}}",
            platform_name(platform),
            topic,
            tone.unwrap_or("professional and engaging"),
            constraints.max_chars,
        );

        let response = self.llm.chat_simple(&prompt).await?;
        serde_json::from_str(&response)
    }

    /// Adapt content from one platform to another
    pub async fn adapt_for_platform(
        &self,
        content: &str,
        from: Platform,
        to: Platform,
    ) -> Result<GeneratedContent> {
        let prompt = format!(
            "Adapt this {} post for {}:\n\n{}\n\n\
             Respect the target platform's style, character limits, and best practices.\n\
             Return as JSON: {{\"content\": \"...\", \"hashtags\": [...]}}",
            platform_name(from), platform_name(to), content
        );
        // ...
    }

    /// Generate a full campaign (multiple posts across platforms)
    pub async fn generate_campaign(
        &self,
        brief: &CampaignBrief,
    ) -> Result<Vec<SocialPost>> {
        let prompt = format!(
            "Create a social media campaign:\n\n\
             Name: {}\n\
             Goal: {}\n\
             Platforms: {:?}\n\
             Duration: {} days\n\
             Posts per platform: {}\n\
             Tone: {}\n\
             Key messages: {:?}\n\n\
             Generate a complete campaign schedule with posts for each platform.\n\
             Space posts optimally over the duration.\n\
             Each post should be platform-appropriate.\n\
             Return as JSON array of posts with: platform, content, hashtags, suggested_date, suggested_time",
            brief.name, brief.goal, brief.platforms,
            brief.duration_days, brief.posts_per_platform,
            brief.tone, brief.key_messages,
        );
        // ...
    }
}
```

### Step 5: Campaign Management (Day 4)

**File**: `src/social/campaign.rs`

```rust
pub struct CampaignManager {
    store: Arc<SocialStore>,     // SQLite storage for posts/campaigns
    platforms: HashMap<Platform, Box<dyn SocialPlatform>>,
    content_gen: Arc<ContentGenerator>,
}

impl CampaignManager {
    /// Create a new campaign from brief
    pub async fn create_campaign(&self, brief: &CampaignBrief) -> Result<Campaign>;

    /// Generate all posts for a campaign (AI-powered)
    pub async fn generate_campaign_content(&self, campaign_id: &str) -> Result<Vec<SocialPost>>;

    /// Get campaign with all posts for review
    pub async fn get_campaign_for_review(&self, campaign_id: &str) -> Result<CampaignReview>;

    /// Approve a post (moves to Scheduled)
    pub async fn approve_post(&self, post_id: &str) -> Result<()>;

    /// Approve all posts in campaign
    pub async fn approve_campaign(&self, campaign_id: &str) -> Result<()>;

    /// Edit a draft post
    pub async fn edit_post(&self, post_id: &str, content: &str) -> Result<()>;

    /// Get campaign analytics
    pub async fn get_campaign_analytics(&self, campaign_id: &str) -> Result<CampaignAnalytics>;
}
```

### Step 6: Post Scheduler (Day 4)

**File**: `src/social/scheduler.rs`

```rust
pub struct SocialScheduler {
    store: Arc<SocialStore>,
    platforms: HashMap<Platform, Box<dyn SocialPlatform>>,
}

impl SocialScheduler {
    /// Start the scheduler loop (runs as background task)
    pub async fn run(&self) {
        loop {
            // Check for posts due for publishing
            let due_posts = self.store.get_scheduled_due().await.unwrap_or_default();

            for post in due_posts {
                if let Some(platform) = self.platforms.get(&post.platform) {
                    match platform.publish(&post).await {
                        Ok(result) => {
                            self.store.mark_published(&post.id, &result.platform_post_id).await.ok();
                            log::info!("Published post {} to {:?}", post.id, post.platform);
                        }
                        Err(e) => {
                            self.store.mark_failed(&post.id, &e.to_string()).await.ok();
                            log::error!("Failed to publish post {}: {}", post.id, e);
                            // Retry logic: reschedule with backoff
                        }
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }
}
```

### Step 7: Agent Tools (Day 5)

**File**: `src/social/tools.rs`

Register these as tools for the agent:

```rust
/// Tool: social_create_campaign
/// Creates a marketing campaign with AI-generated content
// Input: { "name": "...", "goal": "...", "platforms": ["twitter", "linkedin"],
//          "duration_days": 14, "posts_per_platform": 5, "tone": "professional" }
// Output: Campaign with all generated posts in "pending" status

/// Tool: social_generate_post
/// Generates a single social media post
// Input: { "topic": "...", "platform": "twitter", "tone": "casual" }
// Output: Generated post content with hashtags

/// Tool: social_list_campaigns
/// Lists all campaigns with status
// Output: [{ id, name, status, post_count }]

/// Tool: social_get_posts
/// Gets posts for review (optionally filtered by campaign)
// Input: { "campaign_id": "optional", "status": "pending" }
// Output: [{ id, platform, content, status, scheduled_at }]

/// Tool: social_approve_post
/// Approves a pending post for publishing
// Input: { "post_id": "..." }

/// Tool: social_schedule_post
/// Schedules a post for a specific time
// Input: { "post_id": "...", "scheduled_at": "2024-01-15T10:00:00Z" }

/// Tool: social_get_analytics
/// Gets analytics for a campaign or post
// Input: { "campaign_id": "...", "post_id": "optional" }

/// Tool: social_list_platforms
/// Lists connected social media platforms
// Output: [{ platform, authenticated, profile }]

/// Tool: social_connect_platform
/// Start OAuth flow to connect a social media platform
// Input: { "platform": "twitter" }
```

### Step 8: Storage (Day 5)

**File**: `src/social/store.rs` — SQLite tables for posts, campaigns, analytics

```sql
CREATE TABLE social_posts (
    id TEXT PRIMARY KEY,
    platform TEXT NOT NULL,
    content TEXT NOT NULL,
    media TEXT DEFAULT '[]',         -- JSON array
    status TEXT NOT NULL DEFAULT 'draft',
    scheduled_at TEXT,
    published_at TEXT,
    campaign_id TEXT,
    hashtags TEXT DEFAULT '[]',      -- JSON array
    platform_post_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (campaign_id) REFERENCES social_campaigns(id)
);

CREATE TABLE social_campaigns (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    platforms TEXT DEFAULT '[]',  -- JSON array
    status TEXT NOT NULL DEFAULT 'planning',
    start_date TEXT,
    end_date TEXT,
    goals TEXT DEFAULT '[]',     -- JSON array
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE social_analytics (
    id TEXT PRIMARY KEY,
    post_id TEXT NOT NULL,
    impressions INTEGER,
    likes INTEGER,
    shares INTEGER,
    comments INTEGER,
    clicks INTEGER,
    reach INTEGER,
    fetched_at TEXT NOT NULL,
    FOREIGN KEY (post_id) REFERENCES social_posts(id)
);
```

---

## User Workflow (How it works end-to-end)

```
User: "Create a marketing campaign for our new product launch on Twitter and LinkedIn.
       The product is an AI code assistant. Run it for 2 weeks."

Agent: 
1. Calls social_create_campaign with brief
2. AI generates 10 posts (5 per platform), each tailored
3. Returns campaign summary with all posts in "pending" status

User: "Show me the posts for review"

Agent:
1. Calls social_get_posts filtered by campaign
2. Displays all posts with platform, content, scheduled times

User: "Post 3 needs more emphasis on the pricing. Edit it."

Agent:
1. Calls social_generate_post with refined prompt
2. Updates post 3 with new content

User: "Looks good, approve all posts"

Agent:
1. Calls social_approve_post for each post
2. Posts move to "scheduled" status
3. Scheduler auto-publishes at scheduled times

User: "How's the campaign performing?"

Agent:
1. Calls social_get_analytics for the campaign
2. Fetches latest metrics from each platform
3. Returns combined analytics report
```

---

## OAuth Setup for Platforms

Each platform needs OAuth credentials stored in the secrets vault:

```toml
[social]
enabled = true

[social.twitter]
client_id = "${TWITTER_CLIENT_ID}"
client_secret = "${TWITTER_CLIENT_SECRET}"

[social.linkedin]
client_id = "${LINKEDIN_CLIENT_ID}"
client_secret = "${LINKEDIN_CLIENT_SECRET}"

[social.instagram]
client_id = "${INSTAGRAM_CLIENT_ID}"  # Same as Facebook app
client_secret = "${INSTAGRAM_CLIENT_SECRET}"

[social.facebook]
client_id = "${FACEBOOK_CLIENT_ID}"
client_secret = "${FACEBOOK_CLIENT_SECRET}"

[social.bluesky]
handle = "${BLUESKY_HANDLE}"
app_password = "${BLUESKY_APP_PASSWORD}"
```

The existing OAuth2 flow in `src/oauth.rs` can be extended to handle these platforms.

---

## Testing

- `test_post_creation` — Create post with validation
- `test_campaign_generation` — AI generates campaign posts
- `test_platform_validation` — Character limits enforced
- `test_scheduling` — Posts scheduled and picked up by scheduler
- `test_publish_twitter` — Mock Twitter API publish
- `test_content_adaptation` — Adapt content across platforms
- `test_analytics_fetch` — Mock analytics retrieval
- `test_oauth_flow` — Platform authentication

---

## Acceptance Criteria

- [x] 17 platforms implemented with publish/delete/analytics (expanded from original 5)
- [x] Platform enum with 17 variants + Display impl
- [x] ContentType enum (Text, Image, Video, Carousel, Story, Reel, Article, Pin, Thread)
- [x] SocialPost with content_type, title, extra fields for platform-specific params
- [x] SocialPlatformProvider trait with publish/delete/get_analytics
- [x] Config/env wiring for all 17 platforms with auto-enable logic
- [x] SocialManager with create_post, list_posts management
- [ ] AI generates platform-specific content
- [ ] Campaign creation with multi-platform posts
- [ ] Posts in pending status for user review
- [ ] Approve/edit workflow before publishing
- [ ] Scheduler auto-publishes at scheduled times
- [ ] Analytics collection from platforms
- [ ] OAuth2 authentication for each platform
- [ ] All social media tools available to agent
- [ ] Content respects platform character limits
- [ ] Thread support for Twitter
