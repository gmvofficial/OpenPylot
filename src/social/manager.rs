use crate::social::providers::PlatformProvider;
use crate::social::types::*;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

/// Manages social media posts, campaigns, and multi-platform publishing.
pub struct SocialManager {
    providers: HashMap<Platform, Box<dyn PlatformProvider>>,
    posts: Vec<SocialPost>,
    campaigns: Vec<Campaign>,
    db: Option<Arc<StdMutex<rusqlite::Connection>>>,
}

const SOCIAL_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS social_posts (
    id TEXT PRIMARY KEY,
    platform TEXT NOT NULL,
    content TEXT NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'text',
    title TEXT,
    hashtags TEXT DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'draft',
    campaign_id TEXT,
    platform_post_id TEXT,
    scheduled_at TEXT,
    published_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS social_campaigns (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    platforms TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'planning',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

impl SocialManager {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            posts: Vec::new(),
            campaigns: Vec::new(),
            db: None,
        }
    }

    /// Create a SocialManager with SQLite persistence.
    pub fn with_db(db_path: &std::path::Path) -> Result<Self, String> {
        let conn = rusqlite::Connection::open(db_path)
            .map_err(|e| format!("Failed to open social DB: {e}"))?;
        conn.execute_batch(SOCIAL_SCHEMA)
            .map_err(|e| format!("Failed to create social schema: {e}"))?;

        let mut mgr = Self {
            providers: HashMap::new(),
            posts: Vec::new(),
            campaigns: Vec::new(),
            db: Some(Arc::new(StdMutex::new(conn))),
        };

        // Load existing posts and campaigns from DB
        mgr.load_from_db();
        Ok(mgr)
    }

    fn load_from_db(&mut self) {
        if let Some(ref db) = self.db {
            if let Ok(conn) = db.lock() {
                // Load posts
                if let Ok(mut stmt) = conn.prepare(
                    "SELECT id, platform, content, content_type, title, hashtags, status, campaign_id, platform_post_id, scheduled_at, published_at FROM social_posts ORDER BY created_at DESC"
                ) {
                    if let Ok(rows) = stmt.query_map([], |row| {
                        let platform_str: String = row.get(1)?;
                        let hashtags_json: String = row.get(5)?;
                        let status_str: String = row.get(6)?;
                        Ok(SocialPost {
                            id: row.get(0)?,
                            platform: Platform::from_str(&platform_str).unwrap_or(Platform::Twitter),
                            content: row.get(2)?,
                            content_type: ContentType::Text,
                            title: row.get(4)?,
                            media_urls: vec![],
                            hashtags: serde_json::from_str(&hashtags_json).unwrap_or_default(),
                            scheduled_at: row.get::<_, Option<String>>(9)?.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
                            published_at: row.get::<_, Option<String>>(10)?.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))),
                            status: match status_str.as_str() {
                                "published" => PostStatus::Published,
                                "scheduled" => PostStatus::Scheduled,
                                "failed" => PostStatus::Failed,
                                _ => PostStatus::Draft,
                            },
                            campaign_id: row.get(7)?,
                            platform_post_id: row.get(8)?,
                            extra: HashMap::new(),
                        })
                    }) {
                        self.posts = rows.filter_map(|r| r.ok()).collect();
                    }
                }

                // Load campaigns
                if let Ok(mut stmt) = conn.prepare(
                    "SELECT id, name, description, platforms, status FROM social_campaigns ORDER BY created_at DESC"
                ) {
                    if let Ok(rows) = stmt.query_map([], |row| {
                        let platforms_json: String = row.get(3)?;
                        let status_str: String = row.get(4)?;
                        Ok(Campaign {
                            id: row.get(0)?,
                            name: row.get(1)?,
                            description: row.get(2)?,
                            platforms: serde_json::from_str::<Vec<String>>(&platforms_json)
                                .unwrap_or_default()
                                .iter()
                                .filter_map(|s| Platform::from_str(s))
                                .collect(),
                            posts: vec![],
                            created_at: Utc::now(),
                            status: match status_str.as_str() {
                                "active" => CampaignStatus::Active,
                                "completed" => CampaignStatus::Completed,
                                "paused" => CampaignStatus::Paused,
                                _ => CampaignStatus::Planning,
                            },
                        })
                    }) {
                        self.campaigns = rows.filter_map(|r| r.ok()).collect();
                    }
                }
            }
        }
    }

    fn persist_post(&self, post: &SocialPost) {
        if let Some(ref db) = self.db {
            if let Ok(conn) = db.lock() {
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO social_posts (id, platform, content, hashtags, status, campaign_id, platform_post_id, published_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        post.id,
                        post.platform.to_string(),
                        post.content,
                        serde_json::to_string(&post.hashtags).unwrap_or_default(),
                        format!("{:?}", post.status).to_lowercase(),
                        post.campaign_id,
                        post.platform_post_id,
                        post.published_at.map(|d| d.to_rfc3339()),
                    ],
                );
            }
        }
    }

    fn persist_campaign(&self, campaign: &Campaign) {
        if let Some(ref db) = self.db {
            if let Ok(conn) = db.lock() {
                let platforms: Vec<String> = campaign.platforms.iter().map(|p| p.to_string()).collect();
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO social_campaigns (id, name, description, platforms, status) VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        campaign.id,
                        campaign.name,
                        campaign.description,
                        serde_json::to_string(&platforms).unwrap_or_default(),
                        format!("{:?}", campaign.status).to_lowercase(),
                    ],
                );
            }
        }
    }

    /// Register a platform provider.
    pub fn add_provider(&mut self, provider: Box<dyn PlatformProvider>) {
        let platform = provider.platform();
        self.providers.insert(platform, provider);
    }

    /// Get list of connected platforms.
    pub fn connected_platforms(&self) -> Vec<Platform> {
        self.providers.keys().copied().collect()
    }

    /// Create a draft post.
    pub fn create_post(
        &mut self,
        platform: Platform,
        content: &str,
        hashtags: Vec<String>,
        campaign_id: Option<String>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();

        // Enforce content limit if provider exists
        let content = if let Some(provider) = self.providers.get(&platform) {
            let limit = provider.content_limit();
            if content.len() > limit {
                &content[..limit]
            } else {
                content
            }
        } else {
            content
        };

        let post = SocialPost {
            id: id.clone(),
            platform,
            content: content.to_string(),
            content_type: ContentType::Text,
            title: None,
            media_urls: vec![],
            hashtags,
            scheduled_at: None,
            published_at: None,
            status: PostStatus::Draft,
            campaign_id,
            platform_post_id: None,
            extra: std::collections::HashMap::new(),
        };
        self.posts.push(post);
        self.persist_post(self.posts.last().unwrap());
        id
    }

    /// Publish a post immediately.
    pub async fn publish_post(&mut self, post_id: &str) -> Result<String, String> {
        let post = self
            .posts
            .iter()
            .find(|p| p.id == post_id)
            .ok_or("Post not found")?
            .clone();

        let provider = self
            .providers
            .get(&post.platform)
            .ok_or_else(|| format!("No provider for {}", post.platform))?;

        match provider.publish(&post).await {
            Ok(platform_id) => {
                if let Some(p) = self.posts.iter_mut().find(|p| p.id == post_id) {
                    p.status = PostStatus::Published;
                    p.published_at = Some(Utc::now());
                    p.platform_post_id = Some(platform_id.clone());
                }
                // Persist after mutable borrow is released
                if let Some(p) = self.posts.iter().find(|p| p.id == post_id) {
                    self.persist_post(p);
                }
                Ok(platform_id)
            }
            Err(e) => {
                if let Some(p) = self.posts.iter_mut().find(|p| p.id == post_id) {
                    p.status = PostStatus::Failed;
                }
                if let Some(p) = self.posts.iter().find(|p| p.id == post_id) {
                    self.persist_post(p);
                }
                Err(e)
            }
        }
    }

    /// Create a campaign.
    pub fn create_campaign(
        &mut self,
        name: &str,
        description: &str,
        platforms: Vec<Platform>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let campaign = Campaign {
            id: id.clone(),
            name: name.to_string(),
            description: description.to_string(),
            platforms,
            posts: vec![],
            created_at: Utc::now(),
            status: CampaignStatus::Planning,
        };
        self.campaigns.push(campaign);
        self.persist_campaign(self.campaigns.last().unwrap());
        id
    }

    /// Get all posts.
    pub fn list_posts(&self) -> &[SocialPost] {
        &self.posts
    }

    /// Get all campaigns.
    pub fn list_campaigns(&self) -> &[Campaign] {
        &self.campaigns
    }

    /// Get posts for a campaign.
    pub fn campaign_posts(&self, campaign_id: &str) -> Vec<&SocialPost> {
        self.posts
            .iter()
            .filter(|p| p.campaign_id.as_deref() == Some(campaign_id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_post_and_campaign() {
        let mut mgr = SocialManager::new();
        let cid = mgr.create_campaign("Launch", "Product launch", vec![Platform::Twitter]);
        let pid = mgr.create_post(
            Platform::Twitter,
            "Hello world!",
            vec!["#launch".into()],
            Some(cid.clone()),
        );
        assert_eq!(mgr.list_posts().len(), 1);
        assert_eq!(mgr.campaign_posts(&cid).len(), 1);
        assert_eq!(mgr.campaign_posts(&cid)[0].id, pid);
    }
}
