use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Supported social media platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    // Tier 1 — Major social networks
    Twitter,
    LinkedIn,
    Instagram,
    Facebook,
    Bluesky,
    TikTok,
    YouTube,
    Pinterest,
    Reddit,
    Threads,
    // Tier 2 — Community & messaging
    Mastodon,
    Discord,
    Slack,
    // Tier 2 — Blogging
    Medium,
    #[serde(rename = "devto")]
    DevTo,
    Hashnode,
    WordPress,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Twitter => write!(f, "twitter"),
            Platform::LinkedIn => write!(f, "linkedin"),
            Platform::Instagram => write!(f, "instagram"),
            Platform::Facebook => write!(f, "facebook"),
            Platform::Bluesky => write!(f, "bluesky"),
            Platform::TikTok => write!(f, "tiktok"),
            Platform::YouTube => write!(f, "youtube"),
            Platform::Pinterest => write!(f, "pinterest"),
            Platform::Reddit => write!(f, "reddit"),
            Platform::Threads => write!(f, "threads"),
            Platform::Mastodon => write!(f, "mastodon"),
            Platform::Discord => write!(f, "discord"),
            Platform::Slack => write!(f, "slack"),
            Platform::Medium => write!(f, "medium"),
            Platform::DevTo => write!(f, "devto"),
            Platform::Hashnode => write!(f, "hashnode"),
            Platform::WordPress => write!(f, "wordpress"),
        }
    }
}

impl Platform {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "twitter" | "x" => Some(Platform::Twitter),
            "linkedin" => Some(Platform::LinkedIn),
            "instagram" => Some(Platform::Instagram),
            "facebook" | "fb" => Some(Platform::Facebook),
            "bluesky" | "bsky" => Some(Platform::Bluesky),
            "tiktok" => Some(Platform::TikTok),
            "youtube" | "yt" => Some(Platform::YouTube),
            "pinterest" => Some(Platform::Pinterest),
            "reddit" => Some(Platform::Reddit),
            "threads" => Some(Platform::Threads),
            "mastodon" => Some(Platform::Mastodon),
            "discord" => Some(Platform::Discord),
            "slack" => Some(Platform::Slack),
            "medium" => Some(Platform::Medium),
            "devto" | "dev.to" => Some(Platform::DevTo),
            "hashnode" => Some(Platform::Hashnode),
            "wordpress" | "wp" => Some(Platform::WordPress),
            _ => None,
        }
    }
}

/// Type of content being posted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Text,
    Image,
    Video,
    Carousel,
    Story,
    Reel,
    Article,
    Pin,
    Thread,
}

/// A social media post.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialPost {
    pub id: String,
    pub platform: Platform,
    pub content: String,
    pub content_type: ContentType,
    pub title: Option<String>,
    pub media_urls: Vec<String>,
    pub hashtags: Vec<String>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub status: PostStatus,
    pub campaign_id: Option<String>,
    pub platform_post_id: Option<String>,
    /// Platform-specific extra fields (e.g., subreddit, board_id, channel_id).
    pub extra: HashMap<String, String>,
}

/// Post lifecycle status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostStatus {
    Draft,
    Scheduled,
    Published,
    Failed,
}

/// Analytics for a published post.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostAnalytics {
    pub post_id: String,
    pub platform: Platform,
    pub likes: u64,
    pub shares: u64,
    pub comments: u64,
    pub impressions: u64,
    pub clicks: u64,
    pub fetched_at: DateTime<Utc>,
}

/// A campaign grouping multiple posts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Campaign {
    pub id: String,
    pub name: String,
    pub description: String,
    pub platforms: Vec<Platform>,
    pub posts: Vec<String>, // post IDs
    pub created_at: DateTime<Utc>,
    pub status: CampaignStatus,
}

/// Campaign status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignStatus {
    Planning,
    Active,
    Completed,
    Paused,
}

/// Configuration for a platform provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    pub platform: Platform,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub extra: HashMap<String, String>,
}
