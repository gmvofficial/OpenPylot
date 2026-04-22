pub mod manager;
pub mod providers;
pub mod types;

pub use manager::SocialManager;
pub use providers::{BlueskyProvider, LinkedInProvider, PlatformProvider, TwitterProvider};
pub use types::{
    Campaign, CampaignStatus, Platform, PlatformConfig, PostAnalytics, PostStatus, SocialPost,
};
