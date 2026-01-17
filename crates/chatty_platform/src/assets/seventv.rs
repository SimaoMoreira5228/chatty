#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, anyhow};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::{AssetBundle, AssetProvider, AssetRef, AssetScope};

use super::common::{CachedBundle, compute_bundle_etag, prune_map_cache, prune_optional_cache};

const SEVENTV_GQL_URL: &str = "https://api.7tv.app/v4/gql";
const SEVENTV_EMOTES_TTL: Duration = Duration::from_secs(300);
const SEVENTV_CHANNEL_BADGES_TTL: Duration = Duration::from_secs(600);
const SEVENTV_BADGES_TTL: Duration = Duration::from_secs(600);

static SEVENTV_EMOTES_CACHE: OnceLock<Mutex<HashMap<String, CachedBundle>>> = OnceLock::new();
static SEVENTV_CHANNEL_BADGES_CACHE: OnceLock<Mutex<HashMap<String, CachedBundle>>> = OnceLock::new();
static SEVENTV_BADGES_CACHE: OnceLock<Mutex<Option<CachedBundle>>> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub enum SevenTvPlatform {
	Twitch,
	Kick,
}

impl SevenTvPlatform {
	fn as_str(&self) -> &'static str {
		match self {
			Self::Twitch => "TWITCH",
			Self::Kick => "KICK",
		}
	}
}

pub async fn fetch_7tv_bundle(platform: SevenTvPlatform, platform_id: &str) -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_7tv_emotes(platform, platform_id) {
		return Ok(bundle);
	}

	let query = r#"
query UserEmoteSet($platform: Platform!, $platformId: String!) {
  userByConnection(platform: $platform, platformId: $platformId) {
    personalEmoteSet {
      id
      emotes {
        items {
          alias
          emote {
            id
            defaultName
            images {
              url
              width
              height
              mime
              scale
            }
          }
        }
      }
    }
  }
}
"#;

	let req_body = SevenTvQuery {
		query,
		variables: SevenTvVars {
			platform: platform.as_str().to_string(),
			platform_id: platform_id.to_string(),
		},
	};

	let resp = reqwest::Client::new()
		.post(SEVENTV_GQL_URL)
		.json(&req_body)
		.send()
		.await
		.context("7tv gql request")?
		.error_for_status()
		.context("7tv gql status")?;

	let body: SevenTvResponse = resp.json().await.context("7tv gql json")?;
	let set = body
		.data
		.and_then(|d| d.user_by_connection)
		.and_then(|u| u.personal_emote_set)
		.ok_or_else(|| anyhow!("7tv emote set not found"))?;

	let emotes: Vec<AssetRef> = set.emotes.items.into_iter().filter_map(seventv_emote_to_asset).collect();
	let etag = compute_bundle_etag(&emotes, &[]);

	let bundle = AssetBundle {
		provider: AssetProvider::SevenTv,
		scope: AssetScope::Channel,
		cache_key: format!("7tv:set:{}", set.id),
		etag: Some(etag),
		emotes,
		badges: Vec::new(),
	};

	set_cached_7tv_emotes(platform, platform_id, bundle.clone());
	Ok(bundle)
}

pub async fn fetch_7tv_badges_bundle() -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_7tv_badges() {
		return Ok(bundle);
	}

	let query = r#"
query Badges {
  badges {
    badges {
      id
      name
      images {
        url
        width
        height
        mime
        scale
      }
    }
  }
}
"#;

	let req_body = SevenTvQuery {
		query,
		variables: SevenTvVars {
			platform: "TWITCH".to_string(),
			platform_id: "".to_string(),
		},
	};

	let resp = reqwest::Client::new()
		.post(SEVENTV_GQL_URL)
		.json(&req_body)
		.send()
		.await
		.context("7tv badges gql request")?
		.error_for_status()
		.context("7tv badges gql status")?;

	let body: SevenTvBadgesResponse = resp.json().await.context("7tv badges gql json")?;
	let badges: Vec<AssetRef> = body
		.data
		.and_then(|d| d.badges)
		.map(|b| b.badges)
		.unwrap_or_default()
		.into_iter()
		.filter_map(seventv_badge_to_asset)
		.collect();
	let etag = compute_bundle_etag(&[], &badges);

	let bundle = AssetBundle {
		provider: AssetProvider::SevenTv,
		scope: AssetScope::Global,
		cache_key: "7tv:badges:global".to_string(),
		etag: Some(etag),
		emotes: Vec::new(),
		badges,
	};

	set_cached_7tv_badges(bundle.clone());
	Ok(bundle)
}

pub async fn fetch_7tv_channel_badges_bundle(platform: SevenTvPlatform, platform_id: &str) -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_7tv_channel_badges(platform, platform_id) {
		return Ok(bundle);
	}

	let query = r#"
query ChannelBadges($platform: Platform!, $platformId: String!) {
  userByConnection(platform: $platform, platformId: $platformId) {
    style {
      activeBadge {
        id
        name
        images {
          url
          width
          height
          mime
          scale
        }
      }
    }
    inventory {
      badges {
        to {
          badge {
            id
            name
            images {
              url
              width
              height
              mime
              scale
            }
          }
        }
      }
    }
  }
}
"#;

	let req_body = SevenTvQuery {
		query,
		variables: SevenTvVars {
			platform: platform.as_str().to_string(),
			platform_id: platform_id.to_string(),
		},
	};

	let resp = reqwest::Client::new()
		.post(SEVENTV_GQL_URL)
		.json(&req_body)
		.send()
		.await
		.context("7tv channel badges gql request")?
		.error_for_status()
		.context("7tv channel badges gql status")?;

	let body: SevenTvChannelBadgesResponse = resp.json().await.context("7tv channel badges gql json")?;
	let user = body
		.data
		.and_then(|d| d.user_by_connection)
		.ok_or_else(|| anyhow!("7tv user not found"))?;

	let mut dedupe: HashMap<String, AssetRef> = HashMap::new();
	if let Some(style) = user.style {
		if let Some(badge) = style.active_badge {
			if let Some(asset) = seventv_badge_to_asset(badge) {
				dedupe.entry(asset.id.clone()).or_insert(asset);
			}
		}
	}

	if let Some(inventory) = user.inventory {
		for edge in inventory.badges {
			if let Some(node) = edge.to {
				if let Some(badge) = node.badge {
					if let Some(asset) = seventv_badge_to_asset(badge) {
						dedupe.entry(asset.id.clone()).or_insert(asset);
					}
				}
			}
		}
	}

	let badges: Vec<AssetRef> = dedupe.into_values().collect();
	let etag = compute_bundle_etag(&[], &badges);
	let bundle = AssetBundle {
		provider: AssetProvider::SevenTv,
		scope: AssetScope::Channel,
		cache_key: format!("7tv:badges:channel:{}:{}", platform.as_str(), platform_id),
		etag: Some(etag),
		emotes: Vec::new(),
		badges,
	};

	set_cached_7tv_channel_badges(platform, platform_id, bundle.clone());
	Ok(bundle)
}

pub(crate) fn prune_caches() {
	if let Some(cache) = SEVENTV_EMOTES_CACHE.get() {
		prune_map_cache(cache, SEVENTV_EMOTES_TTL);
	}
	if let Some(cache) = SEVENTV_CHANNEL_BADGES_CACHE.get() {
		prune_map_cache(cache, SEVENTV_CHANNEL_BADGES_TTL);
	}
	if let Some(cache) = SEVENTV_BADGES_CACHE.get() {
		prune_optional_cache(cache, SEVENTV_BADGES_TTL);
	}
}

fn seventv_emote_to_asset(item: SevenTvEmoteSetEmote) -> Option<AssetRef> {
	let image = item
		.emote
		.images
		.iter()
		.find(|img| img.scale == 1)
		.or_else(|| item.emote.images.first())?;

	let name = if item.alias.trim().is_empty() {
		item.emote.default_name.clone()
	} else {
		item.alias.clone()
	};

	Some(AssetRef {
		id: item.emote.id,
		name,
		image_url: image.url.clone(),
		image_format: image.mime.split('/').last().unwrap_or("webp").to_string(),
		width: image.width as u32,
		height: image.height as u32,
	})
}

fn seventv_badge_to_asset(badge: SevenTvBadge) -> Option<AssetRef> {
	let image = badge
		.images
		.iter()
		.find(|img| img.scale == 1)
		.or_else(|| badge.images.first())?;

	Some(AssetRef {
		id: badge.id,
		name: badge.name,
		image_url: image.url.clone(),
		image_format: image.mime.split('/').last().unwrap_or("png").to_string(),
		width: image.width as u32,
		height: image.height as u32,
	})
}

fn get_cached_7tv_emotes(platform: SevenTvPlatform, platform_id: &str) -> Option<AssetBundle> {
	let cache = SEVENTV_EMOTES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let key = format!("{}:{}", platform.as_str(), platform_id);
	let mut guard = cache.lock();
	if let Some(entry) = guard.get(&key) {
		if entry.fetched_at.elapsed() <= SEVENTV_EMOTES_TTL {
			Some(entry.bundle.clone())
		} else {
			guard.remove(&key);
			None
		}
	} else {
		None
	}
}

fn get_cached_7tv_channel_badges(platform: SevenTvPlatform, platform_id: &str) -> Option<AssetBundle> {
	let cache = SEVENTV_CHANNEL_BADGES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let key = format!("{}:{}", platform.as_str(), platform_id);
	let mut guard = cache.lock();
	if let Some(entry) = guard.get(&key) {
		if entry.fetched_at.elapsed() <= SEVENTV_CHANNEL_BADGES_TTL {
			Some(entry.bundle.clone())
		} else {
			guard.remove(&key);
			None
		}
	} else {
		None
	}
}

fn get_cached_7tv_badges() -> Option<AssetBundle> {
	let cache = SEVENTV_BADGES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	let entry = guard.as_ref()?;
	if entry.fetched_at.elapsed() <= SEVENTV_BADGES_TTL {
		Some(entry.bundle.clone())
	} else {
		*guard = None;
		None
	}
}

fn set_cached_7tv_badges(bundle: AssetBundle) {
	let cache = SEVENTV_BADGES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	*guard = Some(CachedBundle {
		fetched_at: std::time::Instant::now(),
		bundle,
	});
}

fn set_cached_7tv_channel_badges(platform: SevenTvPlatform, platform_id: &str, bundle: AssetBundle) {
	let cache = SEVENTV_CHANNEL_BADGES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	let key = format!("{}:{}", platform.as_str(), platform_id);
	guard.insert(
		key,
		CachedBundle {
			fetched_at: std::time::Instant::now(),
			bundle,
		},
	);
}

fn set_cached_7tv_emotes(platform: SevenTvPlatform, platform_id: &str, bundle: AssetBundle) {
	let cache = SEVENTV_EMOTES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	let key = format!("{}:{}", platform.as_str(), platform_id);
	guard.insert(
		key,
		CachedBundle {
			fetched_at: std::time::Instant::now(),
			bundle,
		},
	);
}

#[derive(Debug, Serialize)]
struct SevenTvQuery<'a> {
	query: &'a str,
	variables: SevenTvVars,
}

#[derive(Debug, Serialize)]
struct SevenTvVars {
	platform: String,
	#[serde(rename = "platformId")]
	platform_id: String,
}

#[derive(Debug, Deserialize)]
struct SevenTvResponse {
	data: Option<SevenTvData>,
}

#[derive(Debug, Deserialize)]
struct SevenTvData {
	#[serde(rename = "userByConnection")]
	user_by_connection: Option<SevenTvUser>,
}

#[derive(Debug, Deserialize)]
struct SevenTvBadgesResponse {
	data: Option<SevenTvBadgesData>,
}

#[derive(Debug, Deserialize)]
struct SevenTvChannelBadgesResponse {
	data: Option<SevenTvChannelBadgesData>,
}

#[derive(Debug, Deserialize)]
struct SevenTvChannelBadgesData {
	#[serde(rename = "userByConnection")]
	user_by_connection: Option<SevenTvChannelBadgesUser>,
}

#[derive(Debug, Deserialize)]
struct SevenTvChannelBadgesUser {
	#[serde(default)]
	inventory: Option<SevenTvInventory>,
	#[serde(default)]
	style: Option<SevenTvUserStyle>,
}

#[derive(Debug, Deserialize)]
struct SevenTvInventory {
	#[serde(default)]
	badges: Vec<SevenTvEntitlementEdgeBadge>,
}

#[derive(Debug, Deserialize)]
struct SevenTvEntitlementEdgeBadge {
	#[serde(default)]
	to: Option<SevenTvEntitlementNodeBadge>,
}

#[derive(Debug, Deserialize)]
struct SevenTvEntitlementNodeBadge {
	#[serde(default)]
	badge: Option<SevenTvBadge>,
}

#[derive(Debug, Deserialize)]
struct SevenTvUserStyle {
	#[serde(rename = "activeBadge")]
	active_badge: Option<SevenTvBadge>,
}

#[derive(Debug, Deserialize)]
struct SevenTvBadgesData {
	badges: Option<SevenTvBadges>,
}

#[derive(Debug, Deserialize)]
struct SevenTvBadges {
	#[serde(default)]
	badges: Vec<SevenTvBadge>,
}

#[derive(Debug, Deserialize)]
struct SevenTvUser {
	#[serde(rename = "personalEmoteSet")]
	personal_emote_set: Option<SevenTvEmoteSet>,
}

#[derive(Debug, Deserialize)]
struct SevenTvEmoteSet {
	id: String,
	emotes: SevenTvEmoteSetEmoteSearch,
}

#[derive(Debug, Deserialize)]
struct SevenTvEmoteSetEmoteSearch {
	items: Vec<SevenTvEmoteSetEmote>,
}

#[derive(Debug, Deserialize)]
struct SevenTvEmoteSetEmote {
	alias: String,
	emote: SevenTvEmote,
}

#[derive(Debug, Deserialize)]
struct SevenTvEmote {
	id: String,
	#[serde(rename = "defaultName")]
	default_name: String,
	images: Vec<SevenTvImage>,
}

#[derive(Debug, Deserialize)]
struct SevenTvImage {
	url: String,
	width: i32,
	height: i32,
	mime: String,
	scale: i32,
}

#[derive(Debug, Deserialize)]
struct SevenTvBadge {
	id: String,
	name: String,
	#[serde(default)]
	images: Vec<SevenTvImage>,
}
