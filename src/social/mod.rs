pub mod manager;
pub mod providers;
pub mod text;
pub mod types;

pub use manager::SocialManager;
pub use providers::{
    post_image_to_linkedin, BlueskyProvider, FacebookProvider, LinkedInProvider, PlatformProvider,
    TwitterProvider,
};
pub use text::{linkedin_post_url, strip_markdown};
pub use types::{
    Campaign, CampaignStatus, ContentType, Platform, PlatformConfig, PostAnalytics, PostStatus,
    SocialPost,
};
