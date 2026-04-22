use crate::social::types::*;

/// Trait that all platform providers must implement.
#[async_trait::async_trait]
pub trait PlatformProvider: Send + Sync {
    fn platform(&self) -> Platform;

    /// Publish a post to the platform. Returns the platform-specific post ID.
    async fn publish(&self, post: &SocialPost) -> Result<String, String>;

    /// Delete a published post.
    async fn delete(&self, platform_post_id: &str) -> Result<(), String>;

    /// Fetch analytics for a published post.
    async fn fetch_analytics(&self, platform_post_id: &str) -> Result<PostAnalytics, String>;

    /// Get character/content limits for this platform.
    fn content_limit(&self) -> usize;
}

/// Twitter/X platform provider.
pub struct TwitterProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl TwitterProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for TwitterProvider {
    fn platform(&self) -> Platform {
        Platform::Twitter
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Twitter access_token not configured")?;

        let body = serde_json::json!({ "text": post.content });

        let resp = self
            .client
            .post("https://api.twitter.com/2/tweets")
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Twitter API error: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Twitter API {status}: {text}"));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Parse error: {e}"))?;

        json.get("data")
            .and_then(|d| d.get("id"))
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No tweet ID in response".to_string())
    }

    async fn delete(&self, platform_post_id: &str) -> Result<(), String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Twitter access_token not configured")?;

        let resp = self
            .client
            .delete(&format!(
                "https://api.twitter.com/2/tweets/{platform_post_id}"
            ))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("Twitter API error: {e}"))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("Twitter delete failed: {}", resp.status()))
        }
    }

    async fn fetch_analytics(&self, _platform_post_id: &str) -> Result<PostAnalytics, String> {
        // Twitter v2 API requires elevated access for metrics
        Err("Twitter analytics requires elevated API access".to_string())
    }

    fn content_limit(&self) -> usize {
        280
    }
}

/// LinkedIn platform provider.
pub struct LinkedInProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl LinkedInProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for LinkedInProvider {
    fn platform(&self) -> Platform {
        Platform::LinkedIn
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("LinkedIn access_token not configured")?;

        let author = self
            .config
            .extra
            .get("person_id")
            .ok_or("LinkedIn person_id not configured")?;

        let body = serde_json::json!({
            "author": format!("urn:li:person:{author}"),
            "lifecycleState": "PUBLISHED",
            "specificContent": {
                "com.linkedin.ugc.ShareContent": {
                    "shareCommentary": { "text": post.content },
                    "shareMediaCategory": "NONE"
                }
            },
            "visibility": {
                "com.linkedin.ugc.MemberNetworkVisibility": "PUBLIC"
            }
        });

        let resp = self
            .client
            .post("https://api.linkedin.com/v2/ugcPosts")
            .bearer_auth(token)
            .header("X-Restli-Protocol-Version", "2.0.0")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("LinkedIn API error: {e}"))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("LinkedIn API error: {text}"));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        json.get("id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No post ID in response".to_string())
    }

    async fn delete(&self, _platform_post_id: &str) -> Result<(), String> {
        Err("LinkedIn post deletion requires UGC API access".to_string())
    }

    async fn fetch_analytics(&self, _platform_post_id: &str) -> Result<PostAnalytics, String> {
        Err("LinkedIn analytics not yet implemented".to_string())
    }

    fn content_limit(&self) -> usize {
        3000
    }
}

/// Bluesky (AT Protocol) provider.
pub struct BlueskyProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl BlueskyProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for BlueskyProvider {
    fn platform(&self) -> Platform {
        Platform::Bluesky
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let handle = self
            .config
            .extra
            .get("handle")
            .ok_or("Bluesky handle not configured")?;
        let password = self
            .config
            .api_key
            .as_ref()
            .ok_or("Bluesky app password not configured")?;

        // Create session
        let session: serde_json::Value = self
            .client
            .post("https://bsky.social/xrpc/com.atproto.server.createSession")
            .json(&serde_json::json!({
                "identifier": handle,
                "password": password
            }))
            .send()
            .await
            .map_err(|e| format!("Bluesky auth error: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Parse error: {e}"))?;

        let access_jwt = session
            .get("accessJwt")
            .and_then(|v| v.as_str())
            .ok_or("No accessJwt in session")?;
        let did = session
            .get("did")
            .and_then(|v| v.as_str())
            .ok_or("No DID in session")?;

        let record = serde_json::json!({
            "repo": did,
            "collection": "app.bsky.feed.post",
            "record": {
                "$type": "app.bsky.feed.post",
                "text": post.content,
                "createdAt": chrono::Utc::now().to_rfc3339()
            }
        });

        let resp: serde_json::Value = self
            .client
            .post("https://bsky.social/xrpc/com.atproto.repo.createRecord")
            .bearer_auth(access_jwt)
            .json(&record)
            .send()
            .await
            .map_err(|e| format!("Bluesky post error: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Parse error: {e}"))?;

        resp.get("uri")
            .and_then(|u| u.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No URI in Bluesky response".to_string())
    }

    async fn delete(&self, _platform_post_id: &str) -> Result<(), String> {
        Err("Bluesky delete not yet implemented".to_string())
    }

    async fn fetch_analytics(&self, _platform_post_id: &str) -> Result<PostAnalytics, String> {
        Err("Bluesky analytics not available via API".to_string())
    }

    fn content_limit(&self) -> usize {
        300
    }
}

// ── Facebook Page Provider ───────────────────────────────────────────

/// Facebook Page provider via Graph API.
pub struct FacebookProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl FacebookProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for FacebookProvider {
    fn platform(&self) -> Platform {
        Platform::Facebook
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Facebook access_token not configured")?;
        let page_id = self
            .config
            .extra
            .get("page_id")
            .ok_or("Facebook page_id not configured")?;

        let body = serde_json::json!({
            "message": post.content,
            "access_token": token,
        });

        let resp = self
            .client
            .post(format!("https://graph.facebook.com/v21.0/{page_id}/feed"))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Facebook API error: {e}"))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Facebook API error: {text}"));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        json.get("id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No post ID in Facebook response".to_string())
    }

    async fn delete(&self, platform_post_id: &str) -> Result<(), String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Facebook access_token not configured")?;

        let resp = self
            .client
            .delete(format!(
                "https://graph.facebook.com/v21.0/{platform_post_id}"
            ))
            .query(&[("access_token", token.as_str())])
            .send()
            .await
            .map_err(|e| format!("Facebook API error: {e}"))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("Facebook delete failed: {}", resp.status()))
        }
    }

    async fn fetch_analytics(&self, platform_post_id: &str) -> Result<PostAnalytics, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Facebook access_token not configured")?;

        let resp = self
            .client
            .get(format!(
                "https://graph.facebook.com/v21.0/{platform_post_id}"
            ))
            .query(&[
                ("fields", "likes.summary(true),shares,comments.summary(true)"),
                ("access_token", token),
            ])
            .send()
            .await
            .map_err(|e| format!("Facebook API error: {e}"))?;

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;

        Ok(PostAnalytics {
            post_id: platform_post_id.to_string(),
            platform: Platform::Facebook,
            likes: json
                .pointer("/likes/summary/total_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            shares: json
                .pointer("/shares/count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            comments: json
                .pointer("/comments/summary/total_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            impressions: 0,
            clicks: 0,
            fetched_at: chrono::Utc::now(),
        })
    }

    fn content_limit(&self) -> usize {
        63206
    }
}

// ── Instagram Provider (via Facebook Graph API) ──────────────────────

/// Instagram provider via Facebook Graph API (Instagram Business accounts).
pub struct InstagramProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl InstagramProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for InstagramProvider {
    fn platform(&self) -> Platform {
        Platform::Instagram
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Instagram access_token not configured")?;
        let ig_user_id = self
            .config
            .extra
            .get("ig_user_id")
            .ok_or("Instagram ig_user_id not configured")?;

        // Instagram requires media — text-only posts not supported
        if post.media_urls.is_empty() {
            return Err("Instagram requires at least one image or video URL".to_string());
        }

        // Step 1: Create media container
        let mut container_body = serde_json::json!({
            "caption": post.content,
            "access_token": token,
        });

        let media_url = &post.media_urls[0];
        // Determine if video or image
        let is_video = media_url.contains(".mp4")
            || media_url.contains(".mov")
            || media_url.contains("video");

        if is_video {
            container_body["media_type"] = serde_json::json!("REELS");
            container_body["video_url"] = serde_json::json!(media_url);
        } else {
            container_body["image_url"] = serde_json::json!(media_url);
        }

        let container_resp = self
            .client
            .post(format!(
                "https://graph.facebook.com/v21.0/{ig_user_id}/media"
            ))
            .json(&container_body)
            .send()
            .await
            .map_err(|e| format!("Instagram container error: {e}"))?;

        let container: serde_json::Value = container_resp
            .json()
            .await
            .map_err(|e| format!("Parse error: {e}"))?;

        let creation_id = container
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("No container ID in Instagram response")?;

        // Step 2: Publish the container
        let publish_body = serde_json::json!({
            "creation_id": creation_id,
            "access_token": token,
        });

        let publish_resp = self
            .client
            .post(format!(
                "https://graph.facebook.com/v21.0/{ig_user_id}/media_publish"
            ))
            .json(&publish_body)
            .send()
            .await
            .map_err(|e| format!("Instagram publish error: {e}"))?;

        if !publish_resp.status().is_success() {
            let text = publish_resp.text().await.unwrap_or_default();
            return Err(format!("Instagram publish error: {text}"));
        }

        let json: serde_json::Value = publish_resp
            .json()
            .await
            .map_err(|e| format!("Parse error: {e}"))?;

        json.get("id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No post ID in Instagram response".to_string())
    }

    async fn delete(&self, _platform_post_id: &str) -> Result<(), String> {
        Err("Instagram does not support post deletion via API".to_string())
    }

    async fn fetch_analytics(&self, platform_post_id: &str) -> Result<PostAnalytics, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Instagram access_token not configured")?;

        let resp = self
            .client
            .get(format!(
                "https://graph.facebook.com/v21.0/{platform_post_id}/insights"
            ))
            .query(&[
                ("metric", "impressions,reach,likes,comments,shares"),
                ("access_token", token),
            ])
            .send()
            .await
            .map_err(|e| format!("Instagram insights error: {e}"))?;

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;

        let mut analytics = PostAnalytics {
            post_id: platform_post_id.to_string(),
            platform: Platform::Instagram,
            likes: 0,
            shares: 0,
            comments: 0,
            impressions: 0,
            clicks: 0,
            fetched_at: chrono::Utc::now(),
        };

        if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
            for item in data {
                let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let val = item
                    .pointer("/values/0/value")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                match name {
                    "impressions" => analytics.impressions = val,
                    "likes" => analytics.likes = val,
                    "comments" => analytics.comments = val,
                    "shares" => analytics.shares = val,
                    _ => {}
                }
            }
        }

        Ok(analytics)
    }

    fn content_limit(&self) -> usize {
        2200
    }
}

// ── TikTok Provider ──────────────────────────────────────────────────

/// TikTok provider via Content Publishing API.
pub struct TikTokProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl TikTokProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for TikTokProvider {
    fn platform(&self) -> Platform {
        Platform::TikTok
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("TikTok access_token not configured")?;

        if post.media_urls.is_empty() {
            return Err("TikTok requires a video URL".to_string());
        }

        // Step 1: Init upload
        let init_body = serde_json::json!({
            "post_info": {
                "title": post.content,
                "privacy_level": "SELF_ONLY",
                "disable_comment": false,
            },
            "source_info": {
                "source": "PULL_FROM_URL",
                "video_url": post.media_urls[0],
            }
        });

        let resp = self
            .client
            .post("https://open.tiktokapis.com/v2/post/publish/video/init/")
            .bearer_auth(token)
            .header("Content-Type", "application/json; charset=UTF-8")
            .json(&init_body)
            .send()
            .await
            .map_err(|e| format!("TikTok API error: {e}"))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("TikTok API error: {text}"));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        json.pointer("/data/publish_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No publish_id in TikTok response".to_string())
    }

    async fn delete(&self, _platform_post_id: &str) -> Result<(), String> {
        Err("TikTok does not support post deletion via API".to_string())
    }

    async fn fetch_analytics(&self, platform_post_id: &str) -> Result<PostAnalytics, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("TikTok access_token not configured")?;

        let body = serde_json::json!({
            "filters": { "video_ids": [platform_post_id] },
            "fields": ["like_count", "comment_count", "share_count", "view_count"]
        });

        let resp = self
            .client
            .post("https://open.tiktokapis.com/v2/video/query/")
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("TikTok API error: {e}"))?;

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        let video = json.pointer("/data/videos/0");

        Ok(PostAnalytics {
            post_id: platform_post_id.to_string(),
            platform: Platform::TikTok,
            likes: video
                .and_then(|v| v.get("like_count"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            shares: video
                .and_then(|v| v.get("share_count"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            comments: video
                .and_then(|v| v.get("comment_count"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            impressions: video
                .and_then(|v| v.get("view_count"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            clicks: 0,
            fetched_at: chrono::Utc::now(),
        })
    }

    fn content_limit(&self) -> usize {
        2200
    }
}

// ── YouTube Provider ─────────────────────────────────────────────────

/// YouTube provider via Data API v3.
pub struct YouTubeProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl YouTubeProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for YouTubeProvider {
    fn platform(&self) -> Platform {
        Platform::YouTube
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("YouTube access_token not configured")?;

        if post.media_urls.is_empty() {
            return Err("YouTube requires a video URL for upload".to_string());
        }

        // For YouTube, we create a video resource via resumable upload.
        // First: insert video metadata.
        let title = post.title.as_deref().unwrap_or("Untitled");
        let snippet = serde_json::json!({
            "snippet": {
                "title": title,
                "description": post.content,
                "tags": post.hashtags,
                "categoryId": "22" // People & Blogs
            },
            "status": {
                "privacyStatus": "private",
                "selfDeclaredMadeForKids": false
            }
        });

        let resp = self
            .client
            .post("https://www.googleapis.com/upload/youtube/v3/videos")
            .query(&[("part", "snippet,status"), ("uploadType", "resumable")])
            .bearer_auth(token)
            .json(&snippet)
            .send()
            .await
            .map_err(|e| format!("YouTube API error: {e}"))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("YouTube API error: {text}"));
        }

        // The upload URL is in the Location header for resumable uploads.
        // For a simpler flow, we return the video resource ID from metadata.
        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        json.get("id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No video ID in YouTube response".to_string())
    }

    async fn delete(&self, platform_post_id: &str) -> Result<(), String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("YouTube access_token not configured")?;

        let resp = self
            .client
            .delete("https://www.googleapis.com/youtube/v3/videos")
            .query(&[("id", platform_post_id)])
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("YouTube API error: {e}"))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("YouTube delete failed: {}", resp.status()))
        }
    }

    async fn fetch_analytics(&self, platform_post_id: &str) -> Result<PostAnalytics, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("YouTube access_token not configured")?;

        let resp = self
            .client
            .get("https://www.googleapis.com/youtube/v3/videos")
            .query(&[("part", "statistics"), ("id", platform_post_id)])
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("YouTube API error: {e}"))?;

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        let stats = json.pointer("/items/0/statistics");

        fn parse_stat(stats: Option<&serde_json::Value>, field: &str) -> u64 {
            stats
                .and_then(|s| s.get(field))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
        }

        Ok(PostAnalytics {
            post_id: platform_post_id.to_string(),
            platform: Platform::YouTube,
            likes: parse_stat(stats, "likeCount"),
            shares: 0,
            comments: parse_stat(stats, "commentCount"),
            impressions: parse_stat(stats, "viewCount"),
            clicks: 0,
            fetched_at: chrono::Utc::now(),
        })
    }

    fn content_limit(&self) -> usize {
        5000 // description limit
    }
}

// ── Pinterest Provider ───────────────────────────────────────────────

/// Pinterest provider via API v5.
pub struct PinterestProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl PinterestProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for PinterestProvider {
    fn platform(&self) -> Platform {
        Platform::Pinterest
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Pinterest access_token not configured")?;
        let board_id = post
            .extra
            .get("board_id")
            .or(self.config.extra.get("board_id"))
            .ok_or("Pinterest board_id not configured")?;

        if post.media_urls.is_empty() {
            return Err("Pinterest requires an image URL".to_string());
        }

        let body = serde_json::json!({
            "board_id": board_id,
            "title": post.title.as_deref().unwrap_or(""),
            "description": post.content,
            "media_source": {
                "source_type": "image_url",
                "url": post.media_urls[0]
            }
        });

        let resp = self
            .client
            .post("https://api.pinterest.com/v5/pins")
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Pinterest API error: {e}"))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Pinterest API error: {text}"));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        json.get("id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No pin ID in Pinterest response".to_string())
    }

    async fn delete(&self, platform_post_id: &str) -> Result<(), String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Pinterest access_token not configured")?;

        let resp = self
            .client
            .delete(format!(
                "https://api.pinterest.com/v5/pins/{platform_post_id}"
            ))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("Pinterest API error: {e}"))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("Pinterest delete failed: {}", resp.status()))
        }
    }

    async fn fetch_analytics(&self, platform_post_id: &str) -> Result<PostAnalytics, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Pinterest access_token not configured")?;

        let resp = self
            .client
            .get(format!(
                "https://api.pinterest.com/v5/pins/{platform_post_id}/analytics"
            ))
            .query(&[("metric_types", "IMPRESSION,PIN_CLICK,SAVE,OUTBOUND_CLICK")])
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("Pinterest API error: {e}"))?;

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;

        Ok(PostAnalytics {
            post_id: platform_post_id.to_string(),
            platform: Platform::Pinterest,
            likes: json
                .pointer("/all/lifetime/SAVE")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            shares: 0,
            comments: 0,
            impressions: json
                .pointer("/all/lifetime/IMPRESSION")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            clicks: json
                .pointer("/all/lifetime/OUTBOUND_CLICK")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            fetched_at: chrono::Utc::now(),
        })
    }

    fn content_limit(&self) -> usize {
        500
    }
}

// ── Reddit Provider ──────────────────────────────────────────────────

/// Reddit provider via OAuth API.
pub struct RedditProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl RedditProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for RedditProvider {
    fn platform(&self) -> Platform {
        Platform::Reddit
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Reddit access_token not configured")?;
        let subreddit = post
            .extra
            .get("subreddit")
            .or(self.config.extra.get("subreddit"))
            .ok_or("Reddit subreddit not specified")?;

        let title = post.title.as_deref().unwrap_or("Post");

        let mut form = vec![
            ("sr", subreddit.as_str()),
            ("title", title),
            ("kind", "self"),
            ("text", &post.content),
        ];

        // If there's a URL, make it a link post
        if !post.media_urls.is_empty() {
            form.retain(|&(k, _)| k != "kind" && k != "text");
            form.push(("kind", "link"));
            form.push(("url", &post.media_urls[0]));
        }

        let resp = self
            .client
            .post("https://oauth.reddit.com/api/submit")
            .bearer_auth(token)
            .header("User-Agent", "pylot:v0.3.0 (by /u/pylot)")
            .form(&form)
            .send()
            .await
            .map_err(|e| format!("Reddit API error: {e}"))?;

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;

        if let Some(errors) = json.pointer("/json/errors").and_then(|e| e.as_array()) {
            if !errors.is_empty() {
                return Err(format!("Reddit errors: {:?}", errors));
            }
        }

        json.pointer("/json/data/id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No post ID in Reddit response".to_string())
    }

    async fn delete(&self, platform_post_id: &str) -> Result<(), String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Reddit access_token not configured")?;

        let resp = self
            .client
            .post("https://oauth.reddit.com/api/del")
            .bearer_auth(token)
            .header("User-Agent", "pylot:v0.3.0 (by /u/pylot)")
            .form(&[("id", format!("t3_{platform_post_id}"))])
            .send()
            .await
            .map_err(|e| format!("Reddit API error: {e}"))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("Reddit delete failed: {}", resp.status()))
        }
    }

    async fn fetch_analytics(&self, _platform_post_id: &str) -> Result<PostAnalytics, String> {
        Err("Reddit does not provide per-post analytics via API".to_string())
    }

    fn content_limit(&self) -> usize {
        40000
    }
}

// ── Threads Provider (Meta) ──────────────────────────────────────────

/// Threads provider via Meta Graph API.
pub struct ThreadsProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl ThreadsProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for ThreadsProvider {
    fn platform(&self) -> Platform {
        Platform::Threads
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Threads access_token not configured")?;
        let user_id = self
            .config
            .extra
            .get("user_id")
            .ok_or("Threads user_id not configured")?;

        // Step 1: Create container
        let mut body = serde_json::json!({
            "text": post.content,
            "media_type": "TEXT",
            "access_token": token,
        });

        if !post.media_urls.is_empty() {
            let media_url = &post.media_urls[0];
            let is_video = media_url.contains(".mp4") || media_url.contains("video");
            if is_video {
                body["media_type"] = serde_json::json!("VIDEO");
                body["video_url"] = serde_json::json!(media_url);
            } else {
                body["media_type"] = serde_json::json!("IMAGE");
                body["image_url"] = serde_json::json!(media_url);
            }
        }

        let container_resp = self
            .client
            .post(format!(
                "https://graph.threads.net/v1.0/{user_id}/threads"
            ))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Threads API error: {e}"))?;

        let container: serde_json::Value = container_resp
            .json()
            .await
            .map_err(|e| format!("Parse error: {e}"))?;

        let creation_id = container
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("No container ID in Threads response")?;

        // Step 2: Publish
        let publish_body = serde_json::json!({
            "creation_id": creation_id,
            "access_token": token,
        });

        let publish_resp = self
            .client
            .post(format!(
                "https://graph.threads.net/v1.0/{user_id}/threads_publish"
            ))
            .json(&publish_body)
            .send()
            .await
            .map_err(|e| format!("Threads publish error: {e}"))?;

        let json: serde_json::Value = publish_resp
            .json()
            .await
            .map_err(|e| format!("Parse error: {e}"))?;

        json.get("id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No post ID in Threads response".to_string())
    }

    async fn delete(&self, _platform_post_id: &str) -> Result<(), String> {
        Err("Threads does not support post deletion via API".to_string())
    }

    async fn fetch_analytics(&self, platform_post_id: &str) -> Result<PostAnalytics, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Threads access_token not configured")?;

        let resp = self
            .client
            .get(format!(
                "https://graph.threads.net/v1.0/{platform_post_id}/insights"
            ))
            .query(&[
                ("metric", "views,likes,replies,reposts"),
                ("access_token", token),
            ])
            .send()
            .await
            .map_err(|e| format!("Threads insights error: {e}"))?;

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;

        let mut analytics = PostAnalytics {
            post_id: platform_post_id.to_string(),
            platform: Platform::Threads,
            likes: 0,
            shares: 0,
            comments: 0,
            impressions: 0,
            clicks: 0,
            fetched_at: chrono::Utc::now(),
        };

        if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
            for item in data {
                let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let val = item
                    .pointer("/values/0/value")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                match name {
                    "views" => analytics.impressions = val,
                    "likes" => analytics.likes = val,
                    "replies" => analytics.comments = val,
                    "reposts" => analytics.shares = val,
                    _ => {}
                }
            }
        }

        Ok(analytics)
    }

    fn content_limit(&self) -> usize {
        500
    }
}

// ── Mastodon Provider ────────────────────────────────────────────────

/// Mastodon provider — works with any Mastodon-compatible instance.
pub struct MastodonProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl MastodonProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for MastodonProvider {
    fn platform(&self) -> Platform {
        Platform::Mastodon
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Mastodon access_token not configured")?;
        let instance = self
            .config
            .extra
            .get("instance")
            .ok_or("Mastodon instance URL not configured")?;

        let body = serde_json::json!({
            "status": post.content,
        });

        let resp = self
            .client
            .post(format!("{instance}/api/v1/statuses"))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Mastodon API error: {e}"))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Mastodon API error: {text}"));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        json.get("id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No status ID in Mastodon response".to_string())
    }

    async fn delete(&self, platform_post_id: &str) -> Result<(), String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Mastodon access_token not configured")?;
        let instance = self
            .config
            .extra
            .get("instance")
            .ok_or("Mastodon instance URL not configured")?;

        let resp = self
            .client
            .delete(format!("{instance}/api/v1/statuses/{platform_post_id}"))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("Mastodon API error: {e}"))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("Mastodon delete failed: {}", resp.status()))
        }
    }

    async fn fetch_analytics(&self, _platform_post_id: &str) -> Result<PostAnalytics, String> {
        Err("Mastodon does not provide post analytics via API".to_string())
    }

    fn content_limit(&self) -> usize {
        500 // default; instances can configure higher
    }
}

// ── Discord Provider ─────────────────────────────────────────────────

/// Discord provider — posts to a channel via Bot token or webhook.
pub struct DiscordProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl DiscordProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for DiscordProvider {
    fn platform(&self) -> Platform {
        Platform::Discord
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        // Check for webhook URL first (simpler), then bot token
        if let Some(webhook_url) = self.config.extra.get("webhook_url") {
            let body = serde_json::json!({ "content": post.content });
            let resp = self
                .client
                .post(webhook_url)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Discord webhook error: {e}"))?;

            if !resp.status().is_success() {
                let text = resp.text().await.unwrap_or_default();
                return Err(format!("Discord webhook error: {text}"));
            }

            let json: serde_json::Value =
                resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
            return json
                .get("id")
                .and_then(|id| id.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| "No message ID in Discord response".to_string());
        }

        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Discord bot token not configured")?;
        let channel_id = post
            .extra
            .get("channel_id")
            .or(self.config.extra.get("channel_id"))
            .ok_or("Discord channel_id not specified")?;

        let body = serde_json::json!({ "content": post.content });

        let resp = self
            .client
            .post(format!(
                "https://discord.com/api/v10/channels/{channel_id}/messages"
            ))
            .header("Authorization", format!("Bot {token}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Discord API error: {e}"))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Discord API error: {text}"));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        json.get("id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No message ID in Discord response".to_string())
    }

    async fn delete(&self, platform_post_id: &str) -> Result<(), String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Discord bot token not configured")?;
        let channel_id = self
            .config
            .extra
            .get("channel_id")
            .ok_or("Discord channel_id not configured")?;

        let resp = self
            .client
            .delete(format!(
                "https://discord.com/api/v10/channels/{channel_id}/messages/{platform_post_id}"
            ))
            .header("Authorization", format!("Bot {token}"))
            .send()
            .await
            .map_err(|e| format!("Discord API error: {e}"))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("Discord delete failed: {}", resp.status()))
        }
    }

    async fn fetch_analytics(&self, _platform_post_id: &str) -> Result<PostAnalytics, String> {
        Err("Discord does not provide message analytics".to_string())
    }

    fn content_limit(&self) -> usize {
        2000
    }
}

// ── Slack Provider ───────────────────────────────────────────────────

/// Slack provider via Bot OAuth token.
pub struct SlackProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl SlackProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for SlackProvider {
    fn platform(&self) -> Platform {
        Platform::Slack
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Slack bot token not configured")?;
        let channel = post
            .extra
            .get("channel")
            .or(self.config.extra.get("channel"))
            .ok_or("Slack channel not specified")?;

        let body = serde_json::json!({
            "channel": channel,
            "text": post.content,
        });

        let resp = self
            .client
            .post("https://slack.com/api/chat.postMessage")
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Slack API error: {e}"))?;

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;

        if json.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let err = json.get("error").and_then(|e| e.as_str()).unwrap_or("unknown");
            return Err(format!("Slack error: {err}"));
        }

        json.get("ts")
            .and_then(|ts| ts.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No timestamp in Slack response".to_string())
    }

    async fn delete(&self, platform_post_id: &str) -> Result<(), String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Slack bot token not configured")?;
        let channel = self
            .config
            .extra
            .get("channel")
            .ok_or("Slack channel not configured")?;

        let body = serde_json::json!({
            "channel": channel,
            "ts": platform_post_id,
        });

        let resp = self
            .client
            .post("https://slack.com/api/chat.delete")
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Slack API error: {e}"))?;

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        if json.get("ok").and_then(|v| v.as_bool()) == Some(true) {
            Ok(())
        } else {
            Err(format!("Slack delete failed"))
        }
    }

    async fn fetch_analytics(&self, _platform_post_id: &str) -> Result<PostAnalytics, String> {
        Err("Slack does not provide message analytics".to_string())
    }

    fn content_limit(&self) -> usize {
        40000
    }
}

// ── Medium Provider ──────────────────────────────────────────────────

/// Medium provider via API (integration token).
pub struct MediumProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl MediumProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for MediumProvider {
    fn platform(&self) -> Platform {
        Platform::Medium
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let token = self
            .config
            .access_token
            .as_ref()
            .ok_or("Medium integration token not configured")?;

        // Get user ID first
        let me_resp = self
            .client
            .get("https://api.medium.com/v1/me")
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("Medium API error: {e}"))?;

        let me: serde_json::Value = me_resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        let user_id = me
            .pointer("/data/id")
            .and_then(|v| v.as_str())
            .ok_or("Could not get Medium user ID")?;

        let title = post.title.as_deref().unwrap_or("Untitled");
        let body = serde_json::json!({
            "title": title,
            "contentFormat": "markdown",
            "content": post.content,
            "tags": post.hashtags,
            "publishStatus": "draft"
        });

        let resp = self
            .client
            .post(format!(
                "https://api.medium.com/v1/users/{user_id}/posts"
            ))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Medium API error: {e}"))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Medium API error: {text}"));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        json.pointer("/data/id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "No post ID in Medium response".to_string())
    }

    async fn delete(&self, _platform_post_id: &str) -> Result<(), String> {
        Err("Medium does not support post deletion via API".to_string())
    }

    async fn fetch_analytics(&self, _platform_post_id: &str) -> Result<PostAnalytics, String> {
        Err("Medium does not provide analytics via API".to_string())
    }

    fn content_limit(&self) -> usize {
        100_000 // practical limit for articles
    }
}

// ── Dev.to Provider ──────────────────────────────────────────────────

/// Dev.to provider via API (API key).
pub struct DevToProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl DevToProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for DevToProvider {
    fn platform(&self) -> Platform {
        Platform::DevTo
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or("Dev.to API key not configured")?;

        let title = post.title.as_deref().unwrap_or("Untitled");
        let body = serde_json::json!({
            "article": {
                "title": title,
                "body_markdown": post.content,
                "published": false,
                "tags": post.hashtags.iter().take(4).collect::<Vec<_>>(),
            }
        });

        let resp = self
            .client
            .post("https://dev.to/api/articles")
            .header("api-key", api_key.as_str())
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Dev.to API error: {e}"))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Dev.to API error: {text}"));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        json.get("id")
            .and_then(|id| id.as_u64())
            .map(|id| id.to_string())
            .ok_or_else(|| "No article ID in Dev.to response".to_string())
    }

    async fn delete(&self, platform_post_id: &str) -> Result<(), String> {
        // Dev.to uses "unpublish" rather than delete
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or("Dev.to API key not configured")?;

        let body = serde_json::json!({
            "article": { "published": false }
        });

        let resp = self
            .client
            .put(format!("https://dev.to/api/articles/{platform_post_id}"))
            .header("api-key", api_key.as_str())
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Dev.to API error: {e}"))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("Dev.to unpublish failed: {}", resp.status()))
        }
    }

    async fn fetch_analytics(&self, platform_post_id: &str) -> Result<PostAnalytics, String> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or("Dev.to API key not configured")?;

        let resp = self
            .client
            .get(format!(
                "https://dev.to/api/articles/{platform_post_id}"
            ))
            .header("api-key", api_key.as_str())
            .send()
            .await
            .map_err(|e| format!("Dev.to API error: {e}"))?;

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;

        Ok(PostAnalytics {
            post_id: platform_post_id.to_string(),
            platform: Platform::DevTo,
            likes: json
                .get("positive_reactions_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            shares: 0,
            comments: json
                .get("comments_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            impressions: json
                .get("page_views_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            clicks: 0,
            fetched_at: chrono::Utc::now(),
        })
    }

    fn content_limit(&self) -> usize {
        100_000
    }
}

// ── Hashnode Provider ────────────────────────────────────────────────

/// Hashnode provider via GraphQL API.
pub struct HashnodeProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl HashnodeProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for HashnodeProvider {
    fn platform(&self) -> Platform {
        Platform::Hashnode
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let token = self
            .config
            .api_key
            .as_ref()
            .ok_or("Hashnode API key not configured")?;
        let publication_id = self
            .config
            .extra
            .get("publication_id")
            .ok_or("Hashnode publication_id not configured")?;

        let title = post.title.as_deref().unwrap_or("Untitled");

        let query = serde_json::json!({
            "query": "mutation PublishPost($input: PublishPostInput!) { publishPost(input: $input) { post { id url } } }",
            "variables": {
                "input": {
                    "title": title,
                    "contentMarkdown": post.content,
                    "publicationId": publication_id,
                    "tags": post.hashtags.iter().map(|t| serde_json::json!({"name": t})).collect::<Vec<_>>(),
                }
            }
        });

        let resp = self
            .client
            .post("https://gql.hashnode.com")
            .header("Authorization", token.as_str())
            .json(&query)
            .send()
            .await
            .map_err(|e| format!("Hashnode API error: {e}"))?;

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;

        json.pointer("/data/publishPost/post/id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                let errors = json.get("errors").map(|e| e.to_string()).unwrap_or_default();
                format!("Hashnode publish failed: {errors}")
            })
    }

    async fn delete(&self, _platform_post_id: &str) -> Result<(), String> {
        Err("Hashnode post deletion requires GraphQL mutation — not yet implemented".to_string())
    }

    async fn fetch_analytics(&self, _platform_post_id: &str) -> Result<PostAnalytics, String> {
        Err("Hashnode analytics not available via API".to_string())
    }

    fn content_limit(&self) -> usize {
        100_000
    }
}

// ── WordPress Provider ───────────────────────────────────────────────

/// WordPress provider via REST API (self-hosted or WP.com).
pub struct WordPressProvider {
    pub config: PlatformConfig,
    client: reqwest::Client,
}

impl WordPressProvider {
    pub fn new(config: PlatformConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl PlatformProvider for WordPressProvider {
    fn platform(&self) -> Platform {
        Platform::WordPress
    }

    async fn publish(&self, post: &SocialPost) -> Result<String, String> {
        let site_url = self
            .config
            .extra
            .get("site_url")
            .ok_or("WordPress site_url not configured")?;
        let username = self
            .config
            .extra
            .get("username")
            .ok_or("WordPress username not configured")?;
        let password = self
            .config
            .api_key
            .as_ref()
            .ok_or("WordPress application password not configured")?;

        let title = post.title.as_deref().unwrap_or("Untitled");
        let body = serde_json::json!({
            "title": title,
            "content": post.content,
            "status": "draft",
            "tags": post.hashtags,
        });

        let resp = self
            .client
            .post(format!("{site_url}/wp-json/wp/v2/posts"))
            .basic_auth(username, Some(password))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("WordPress API error: {e}"))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("WordPress API error: {text}"));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
        json.get("id")
            .and_then(|id| id.as_u64())
            .map(|id| id.to_string())
            .ok_or_else(|| "No post ID in WordPress response".to_string())
    }

    async fn delete(&self, platform_post_id: &str) -> Result<(), String> {
        let site_url = self
            .config
            .extra
            .get("site_url")
            .ok_or("WordPress site_url not configured")?;
        let username = self
            .config
            .extra
            .get("username")
            .ok_or("WordPress username not configured")?;
        let password = self
            .config
            .api_key
            .as_ref()
            .ok_or("WordPress application password not configured")?;

        let resp = self
            .client
            .delete(format!(
                "{site_url}/wp-json/wp/v2/posts/{platform_post_id}"
            ))
            .basic_auth(username, Some(password))
            .send()
            .await
            .map_err(|e| format!("WordPress API error: {e}"))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("WordPress delete failed: {}", resp.status()))
        }
    }

    async fn fetch_analytics(&self, _platform_post_id: &str) -> Result<PostAnalytics, String> {
        Err("WordPress analytics not available via REST API".to_string())
    }

    fn content_limit(&self) -> usize {
        100_000
    }
}
