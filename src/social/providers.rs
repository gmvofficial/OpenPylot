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

        let json: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {e}"))?;

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

    /// LinkedIn upload recipes. `feedshare-image` for images, `feedshare-document`
    /// for PDFs (which render as the swipeable carousel posts).
    fn recipe_for(content_type: &ContentType) -> Option<&'static str> {
        match content_type {
            ContentType::Image => Some("urn:li:digitalmediaRecipe:feedshare-image"),
            ContentType::Document => Some("urn:li:digitalmediaRecipe:feedshare-document"),
            _ => None,
        }
    }

    /// Register an upload with LinkedIn and get back an upload URL + asset URN.
    /// Step 1 of the 3-step media upload flow.
    async fn register_upload(
        &self,
        token: &str,
        owner: &str,
        recipe: &str,
    ) -> Result<(String, String), String> {
        // Spec requires `supportedUploadMechanism: ["SYNCHRONOUS_UPLOAD"]` so
        // that LinkedIn confirms ingestion of the bytes before we attempt to
        // create the post (otherwise step 3 races step 2 and fails).
        let body = serde_json::json!({
            "registerUploadRequest": {
                "recipes": [recipe],
                "owner": format!("urn:li:person:{owner}"),
                "serviceRelationships": [
                    {
                        "relationshipType": "OWNER",
                        "identifier": "urn:li:userGeneratedContent"
                    }
                ],
                "supportedUploadMechanism": ["SYNCHRONOUS_UPLOAD"]
            }
        });

        let resp = self
            .client
            .post("https://api.linkedin.com/v2/assets?action=registerUpload")
            .bearer_auth(token)
            .header("X-Restli-Protocol-Version", "2.0.0")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("LinkedIn registerUpload error: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            eprintln!("[linkedin] step 1 registerUpload failed: HTTP {status}: {text}");
            return Err(format!("LinkedIn registerUpload {status}: {text}"));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("LinkedIn registerUpload parse error: {e}"))?;

        let upload_url = json
            .pointer("/value/uploadMechanism/com.linkedin.digitalmedia.uploading.MediaUploadHttpRequest/uploadUrl")
            .and_then(|v| v.as_str())
            .ok_or("LinkedIn registerUpload: no uploadUrl in response")?
            .to_string();

        let asset = json
            .pointer("/value/asset")
            .and_then(|v| v.as_str())
            .ok_or("LinkedIn registerUpload: no asset URN in response")?
            .to_string();

        Ok((upload_url, asset))
    }

    /// Download remote media and PUT it to LinkedIn's upload URL.
    /// Step 2 of the 3-step flow. Returns Ok once LinkedIn has accepted the bytes.
    async fn upload_media_from_url(
        &self,
        token: &str,
        upload_url: &str,
        media_url: &str,
    ) -> Result<(), String> {
        // Fetch the user-supplied media. We trust the URL — it's something the
        // user (or their agent) pasted. We do *not* re-upload local files yet.
        let download = self
            .client
            .get(media_url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch media from {media_url}: {e}"))?;

        if !download.status().is_success() {
            return Err(format!(
                "Failed to fetch media from {media_url}: HTTP {}",
                download.status()
            ));
        }

        let bytes = download
            .bytes()
            .await
            .map_err(|e| format!("Failed to read media bytes: {e}"))?;

        self.upload_media_bytes(token, upload_url, bytes.to_vec())
            .await
    }

    /// PUT raw image bytes to LinkedIn's signed upload URL.
    ///
    /// Per the LinkedIn assets API spec, this request must carry **only** the
    /// `Authorization` header — no `Content-Type`, no Restli header. A
    /// successful synchronous upload returns HTTP 201.
    async fn upload_media_bytes(
        &self,
        token: &str,
        upload_url: &str,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        let resp = self
            .client
            .put(upload_url)
            .bearer_auth(token)
            .body(bytes)
            .send()
            .await
            .map_err(|e| format!("LinkedIn media upload error: {e}"))?;

        let status = resp.status();
        // Synchronous upload should return 201 Created. Some edge regions
        // return 200; accept both but reject anything else loudly.
        if status.as_u16() != 201 && status.as_u16() != 200 {
            let text = resp.text().await.unwrap_or_default();
            eprintln!("[linkedin] step 2 binary upload failed: HTTP {status}: {text}");
            return Err(format!("LinkedIn media upload {status}: {text}"));
        }
        Ok(())
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

        // LinkedIn renders the share commentary as plain text — strip any
        // markdown the user (or an LLM) may have inserted so it doesn't
        // appear literally on the timeline.
        let clean = crate::social::strip_markdown(&post.content);

        // Decide whether this is a media post (image / PDF) or text-only.
        // Media posts go through the 3-step upload flow; text posts hit
        // /v2/ugcPosts directly.
        let recipe = Self::recipe_for(&post.content_type);
        let has_media = recipe.is_some() && !post.media_urls.is_empty();

        let (share_media_category, media_array) = if has_media {
            let recipe = recipe.unwrap();
            // Upload every URL the user supplied (LinkedIn allows multiple
            // images / a single PDF) and collect their asset URNs.
            let mut media_items = Vec::with_capacity(post.media_urls.len());
            for url in &post.media_urls {
                let (upload_url, asset) = self.register_upload(token, author, recipe).await?;
                self.upload_media_from_url(token, &upload_url, url).await?;

                // Document posts require a `title` — fall back to the post
                // title or a sensible default if the user didn't set one.
                let title_text = post
                    .title
                    .clone()
                    .unwrap_or_else(|| match post.content_type {
                        ContentType::Document => "Shared document".to_string(),
                        _ => String::new(),
                    });

                media_items.push(serde_json::json!({
                    "status": "READY",
                    "media": asset,
                    "title": { "text": title_text },
                    "description": { "text": "" }
                }));

                // PDF posts only support one document attachment in the UGC
                // API; bail after the first to avoid silent truncation.
                if matches!(post.content_type, ContentType::Document) {
                    break;
                }
            }

            let category = match post.content_type {
                ContentType::Document => "NATIVE_DOCUMENT",
                _ => "IMAGE",
            };
            (category, media_items)
        } else {
            ("NONE", Vec::new())
        };

        let mut share_content = serde_json::json!({
            "shareCommentary": { "text": clean },
            "shareMediaCategory": share_media_category,
        });
        if !media_array.is_empty() {
            share_content["media"] = serde_json::Value::Array(media_array);
        }

        let body = serde_json::json!({
            "author": format!("urn:li:person:{author}"),
            "lifecycleState": "PUBLISHED",
            "specificContent": {
                "com.linkedin.ugc.ShareContent": share_content
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
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            eprintln!("[linkedin] step 3 ugcPosts failed: HTTP {status}: {text}");
            return Err(format!("LinkedIn API error {status}: {text}"));
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

/// Best-effort image MIME from a URL/path extension. Falls back to JPEG
/// because that's what Facebook's photo endpoint is most forgiving with.
fn guess_image_mime(url_or_path: &str) -> String {
    let lower = url_or_path
        .split('?')
        .next()
        .unwrap_or(url_or_path)
        .to_lowercase();
    if lower.ends_with(".png") {
        "image/png".into()
    } else if lower.ends_with(".gif") {
        "image/gif".into()
    } else if lower.ends_with(".webp") {
        "image/webp".into()
    } else {
        "image/jpeg".into()
    }
}

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

        // ── Image post ────────────────────────────────────────────────────────
        // If a single image is attached, use the /photos endpoint which
        // creates a photo post (image + caption). PDF/document is not
        // supported by the Facebook Pages API — fall back to text-only.
        //
        // IMPORTANT: We must upload the image *bytes* via multipart (`source`
        // field) rather than passing a `url` field. If we send `url`,
        // Facebook's own servers try to fetch it, which fails for any
        // non-publicly-reachable URL (localhost, 127.0.0.1, private LAN,
        // ngrok-free tunnels behind auth, etc.) with the misleading error:
        //   code 324, "Missing or invalid image file" (is_transient=true).
        // Multipart upload sidesteps that entirely.
        if matches!(post.content_type, crate::social::ContentType::Image)
            && post.media_urls.len() == 1
        {
            let image_url = &post.media_urls[0];

            // Fetch the image bytes ourselves (works for http://, https://,
            // and our own /uploads/ URLs).
            let img_resp = self
                .client
                .get(image_url)
                .send()
                .await
                .map_err(|e| format!("Failed to download image '{image_url}': {e}"))?;
            if !img_resp.status().is_success() {
                return Err(format!(
                    "Failed to download image '{image_url}': HTTP {}",
                    img_resp.status()
                ));
            }
            let img_mime = img_resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|h| h.to_str().ok())
                .map(|s| s.to_string())
                .unwrap_or_else(|| guess_image_mime(image_url));
            let img_bytes = img_resp
                .bytes()
                .await
                .map_err(|e| format!("Failed to read image bytes: {e}"))?;

            let filename = image_url
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("upload.jpg")
                .to_string();

            let part = reqwest::multipart::Part::bytes(img_bytes.to_vec())
                .file_name(filename)
                .mime_str(&img_mime)
                .map_err(|e| format!("Invalid image mime '{img_mime}': {e}"))?;

            let form = reqwest::multipart::Form::new()
                .text("caption", post.content.clone())
                .text("access_token", token.clone())
                .part("source", part);

            let resp = self
                .client
                .post(format!("https://graph.facebook.com/v22.0/{page_id}/photos"))
                .multipart(form)
                .send()
                .await
                .map_err(|e| format!("Facebook API error: {e}"))?;

            if !resp.status().is_success() {
                let text = resp.text().await.unwrap_or_default();
                return Err(format!("Facebook photo post error: {text}"));
            }

            let json: serde_json::Value =
                resp.json().await.map_err(|e| format!("Parse error: {e}"))?;
            return json
                .get("post_id")
                .or_else(|| json.get("id"))
                .and_then(|id| id.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| "No post ID in Facebook photo response".to_string());
        }

        // ── Text post (default) ───────────────────────────────────────────────
        // Send as form fields (not JSON body) — the Graph API requires
        // access_token as a query param or form field. Embedding it in a
        // JSON body causes a code-190 "could not be decrypted" error.
        let resp = self
            .client
            .post(format!("https://graph.facebook.com/v22.0/{page_id}/feed"))
            .form(&[
                ("message", post.content.as_str()),
                ("access_token", token.as_str()),
            ])
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
                "https://graph.facebook.com/v22.0/{platform_post_id}"
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
                "https://graph.facebook.com/v22.0/{platform_post_id}"
            ))
            .query(&[
                (
                    "fields",
                    "likes.summary(true),shares,comments.summary(true)",
                ),
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
        let is_video =
            media_url.contains(".mp4") || media_url.contains(".mov") || media_url.contains("video");

        if is_video {
            container_body["media_type"] = serde_json::json!("REELS");
            container_body["video_url"] = serde_json::json!(media_url);
        } else {
            container_body["image_url"] = serde_json::json!(media_url);
        }

        let container_resp = self
            .client
            .post(format!(
                "https://graph.facebook.com/v22.0/{ig_user_id}/media"
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
                "https://graph.facebook.com/v22.0/{ig_user_id}/media_publish"
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
                "https://graph.facebook.com/v22.0/{platform_post_id}/insights"
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
            .header(
                "User-Agent",
                concat!("pylot:v", env!("CARGO_PKG_VERSION"), " (by /u/pylot)"),
            )
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
            .header(
                "User-Agent",
                concat!("pylot:v", env!("CARGO_PKG_VERSION"), " (by /u/pylot)"),
            )
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
            .post(format!("https://graph.threads.net/v1.0/{user_id}/threads"))
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
            let err = json
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown");
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

        let me: serde_json::Value = me_resp
            .json()
            .await
            .map_err(|e| format!("Parse error: {e}"))?;
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
            .post(format!("https://api.medium.com/v1/users/{user_id}/posts"))
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
            .get(format!("https://dev.to/api/articles/{platform_post_id}"))
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
                let errors = json
                    .get("errors")
                    .map(|e| e.to_string())
                    .unwrap_or_default();
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
            .delete(format!("{site_url}/wp-json/wp/v2/posts/{platform_post_id}"))
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

// ---------------------------------------------------------------------------
// LinkedIn image-post helper
// ---------------------------------------------------------------------------

/// Detect the MIME type of an image from its magic bytes.
/// Returns `Some("image/jpeg" | "image/png")` for supported formats, else `None`.
fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 3 && bytes[0..3] == [0xFF, 0xD8, 0xFF] {
        Some("image/jpeg")
    } else if bytes.len() >= 8 && bytes[0..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        Some("image/png")
    } else {
        None
    }
}

/// Post an image **with text** to LinkedIn using the documented 3-step flow.
///
/// 1. `POST /v2/assets?action=registerUpload` (with `SYNCHRONOUS_UPLOAD`)
///    → returns `uploadUrl` + `asset` URN.
/// 2. `PUT {uploadUrl}` with the raw image bytes (only the `Authorization`
///    header — no `Content-Type`). Expects HTTP 201.
/// 3. `POST /v2/ugcPosts` with `shareMediaCategory: "IMAGE"` and the asset URN.
///
/// Only JPEG and PNG images are accepted. Returns the new post URN on success.
/// Each failed step prints a clear error message including the HTTP status.
pub async fn post_image_to_linkedin(
    access_token: &str,
    user_id: &str,
    image_path: &std::path::Path,
    text: &str,
) -> Result<String, String> {
    use std::io::Read;

    // ---- Validate the image up front (JPEG / PNG only) --------------------
    let mut file = std::fs::File::open(image_path)
        .map_err(|e| format!("Cannot open image at {}: {e}", image_path.display()))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read image at {}: {e}", image_path.display()))?;

    let mime = sniff_image_mime(&bytes).ok_or_else(|| {
        format!(
            "Unsupported image format at {} — only JPEG and PNG are accepted",
            image_path.display()
        )
    })?;
    println!(
        "[linkedin] image OK ({mime}, {} bytes) — starting 3-step upload",
        bytes.len()
    );

    let client = reqwest::Client::new();

    // ---- Step 1: register upload -----------------------------------------
    let register_body = serde_json::json!({
        "registerUploadRequest": {
            "recipes": ["urn:li:digitalmediaRecipe:feedshare-image"],
            "owner": format!("urn:li:person:{user_id}"),
            "serviceRelationships": [
                {
                    "relationshipType": "OWNER",
                    "identifier": "urn:li:userGeneratedContent"
                }
            ],
            "supportedUploadMechanism": ["SYNCHRONOUS_UPLOAD"]
        }
    });

    let reg_resp = client
        .post("https://api.linkedin.com/v2/assets?action=registerUpload")
        .bearer_auth(access_token)
        .header("X-Restli-Protocol-Version", "2.0.0")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&register_body)
        .send()
        .await
        .map_err(|e| format!("[linkedin] step 1 transport error: {e}"))?;

    let reg_status = reg_resp.status();
    if !reg_status.is_success() {
        let body = reg_resp.text().await.unwrap_or_default();
        let msg = format!("[linkedin] step 1 registerUpload FAILED: HTTP {reg_status}: {body}");
        eprintln!("{msg}");
        return Err(msg);
    }

    let reg_json: serde_json::Value = reg_resp
        .json()
        .await
        .map_err(|e| format!("[linkedin] step 1 parse error: {e}"))?;

    let upload_url = reg_json
        .pointer(
            "/value/uploadMechanism/com.linkedin.digitalmedia.uploading.MediaUploadHttpRequest/uploadUrl",
        )
        .and_then(|v| v.as_str())
        .ok_or_else(|| "[linkedin] step 1: missing uploadUrl in response".to_string())?
        .to_string();

    let asset_urn = reg_json
        .pointer("/value/asset")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "[linkedin] step 1: missing asset URN in response".to_string())?
        .to_string();

    println!("[linkedin] step 1 OK — asset={asset_urn}");

    // ---- Step 2: PUT the raw bytes (NO Content-Type header) --------------
    let put_resp = client
        .put(&upload_url)
        .bearer_auth(access_token)
        .body(bytes)
        .send()
        .await
        .map_err(|e| format!("[linkedin] step 2 transport error: {e}"))?;

    let put_status = put_resp.status();
    // SYNCHRONOUS_UPLOAD returns 201 Created. Tolerate 200 for parity with
    // some edge regions but reject anything else loudly.
    if put_status.as_u16() != 201 && put_status.as_u16() != 200 {
        let body = put_resp.text().await.unwrap_or_default();
        let msg = format!("[linkedin] step 2 binary upload FAILED: HTTP {put_status}: {body}");
        eprintln!("{msg}");
        return Err(msg);
    }
    println!("[linkedin] step 2 OK — bytes uploaded (HTTP {put_status})");

    // ---- Step 3: create the UGC post with shareMediaCategory=IMAGE -------
    let post_body = serde_json::json!({
        "author": format!("urn:li:person:{user_id}"),
        "lifecycleState": "PUBLISHED",
        "specificContent": {
            "com.linkedin.ugc.ShareContent": {
                "shareCommentary": { "text": text },
                "shareMediaCategory": "IMAGE",
                "media": [
                    {
                        "status": "READY",
                        "description": { "text": "" },
                        "media": asset_urn,
                        "title": { "text": "" }
                    }
                ]
            }
        },
        "visibility": {
            "com.linkedin.ugc.MemberNetworkVisibility": "PUBLIC"
        }
    });

    let post_resp = client
        .post("https://api.linkedin.com/v2/ugcPosts")
        .bearer_auth(access_token)
        .header("X-Restli-Protocol-Version", "2.0.0")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&post_body)
        .send()
        .await
        .map_err(|e| format!("[linkedin] step 3 transport error: {e}"))?;

    let post_status = post_resp.status();
    if !post_status.is_success() {
        let body = post_resp.text().await.unwrap_or_default();
        let msg = format!("[linkedin] step 3 ugcPosts FAILED: HTTP {post_status}: {body}");
        eprintln!("{msg}");
        return Err(msg);
    }

    // The post URN is returned in the `x-restli-id` header or the `id` body field.
    let header_urn = post_resp
        .headers()
        .get("x-restli-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let post_urn = match header_urn {
        Some(u) if !u.is_empty() => u,
        _ => {
            let json: serde_json::Value = post_resp
                .json()
                .await
                .map_err(|e| format!("[linkedin] step 3 parse error: {e}"))?;
            json.get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| "[linkedin] step 3: no post ID in response".to_string())?
        }
    };

    println!("[linkedin] step 3 OK — post URN={post_urn}");
    Ok(post_urn)
}

#[cfg(test)]
mod linkedin_image_tests {
    use super::*;

    #[test]
    fn sniff_image_mime_recognises_jpeg_and_png() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
        let gif = b"GIF89a...";
        assert_eq!(sniff_image_mime(&jpeg), Some("image/jpeg"));
        assert_eq!(sniff_image_mime(&png), Some("image/png"));
        assert_eq!(sniff_image_mime(gif), None);
        assert_eq!(sniff_image_mime(&[]), None);
    }

    #[tokio::test]
    async fn rejects_non_image_files() {
        let tmp = std::env::temp_dir().join("pylot_not_an_image.txt");
        std::fs::write(&tmp, b"hello world").unwrap();
        let err = post_image_to_linkedin("token", "user", &tmp, "hi")
            .await
            .unwrap_err();
        assert!(err.contains("Unsupported image format"), "got: {err}");
        let _ = std::fs::remove_file(&tmp);
    }

    /// End-to-end smoke test against the real LinkedIn API. Skipped unless
    /// `LINKEDIN_ACCESS_TOKEN`, `LINKEDIN_USER_ID` and `LINKEDIN_TEST_IMAGE`
    /// are all set — run manually with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "requires live LinkedIn credentials"]
    async fn live_post_image_to_linkedin() {
        let token = std::env::var("LINKEDIN_ACCESS_TOKEN")
            .expect("set LINKEDIN_ACCESS_TOKEN to run this test");
        let user =
            std::env::var("LINKEDIN_USER_ID").expect("set LINKEDIN_USER_ID (numeric person id)");
        let image = std::env::var("LINKEDIN_TEST_IMAGE").unwrap_or_else(|_| "test.png".to_string());
        let path = std::path::PathBuf::from(image);

        let urn = post_image_to_linkedin(
            &token,
            &user,
            &path,
            "Automated 3-step image post test from pylot 🚀",
        )
        .await
        .expect("post_image_to_linkedin failed");

        println!("posted: {urn}");
        assert!(urn.starts_with("urn:li:"));
    }
}
