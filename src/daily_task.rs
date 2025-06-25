// daily_task.rs
use once_cell::sync::Lazy;
use rand::{seq::SliceRandom, Rng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SocialPlatform {
    YouTube,
    Twitter,
    LinkedIn,
    Instagram,
    Telegram,
    Discord,
    Facebook,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Links {
    pub url: String,
    pub platform: SocialPlatform,
    pub visited: bool,
}

// Global mutable container of all available links
static LINK_STORAGE: Lazy<Mutex<Vec<Links>>> = Lazy::new(|| {
    Mutex::new(vec![
        Links {
            url: "https://youtube.com/campaign1".into(),
            platform: SocialPlatform::YouTube,
            visited: false,
        },
        Links {
            url: "https://twitter.com/promo123".into(),
            platform: SocialPlatform::Twitter,
            visited: false,
        },
        Links {
            url: "https://linkedin.com/offer".into(),
            platform: SocialPlatform::LinkedIn,
            visited: false,
        },
        Links {
            url: "https://discord.gg/community".into(),
            platform: SocialPlatform::Discord,
            visited: false,
        },
    ])
});

/// Fetch X random links from the global pool
pub fn get_random_links(count: usize) -> Vec<Links> {
    let storage = LINK_STORAGE.lock().unwrap();
    let mut rng = rand::thread_rng();
    storage.choose_multiple(&mut rng, count).cloned().collect()
}

/// Insert a new link into the global storage
pub fn insert_link(new_link: Links) {
    let mut storage = LINK_STORAGE.lock().unwrap();
    storage.push(new_link);
}

/// Delete a link by URL (exact match)
pub fn delete_link_by_url(url: &str) -> bool {
    let mut storage = LINK_STORAGE.lock().unwrap();
    let original_len = storage.len();
    storage.retain(|link| link.url != url);
    storage.len() != original_len
}

/// View all links (debug/test only)
pub fn list_all_links() -> Vec<Links> {
    LINK_STORAGE.lock().unwrap().clone()
}
