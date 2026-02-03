#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use parking_lot::Mutex;
use tracing::info;

use super::common::{CachedBundle, compute_bundle_etag, prune_map_cache, prune_optional_cache};
use crate::{AssetBundle, AssetImage, AssetProvider, AssetRef, AssetScale, AssetScope};

mod event_api;
mod gql;
mod types;

pub use event_api::{DispatchPayload, DispatchType, SevenTvSubscription, ensure_seventv_event_api};
use gql::{SevenTvBadge, SevenTvEmoteSetEmote, SevenTvGqlClient, SevenTvImage, SevenTvUserBadges};
pub use types::{SevenTvPlatform, SevenTvUserEmoteSets};

const SEVENTV_EMOTE_SET_TTL: Duration = Duration::from_secs(300);
const SEVENTV_USER_SETS_TTL: Duration = Duration::from_secs(300);
const SEVENTV_CHANNEL_BADGES_TTL: Duration = Duration::from_secs(600);
const SEVENTV_BADGES_TTL: Duration = Duration::from_secs(600);

static SEVENTV_USER_SETS_CACHE: OnceLock<Mutex<HashMap<String, CachedUserEmoteSets>>> = OnceLock::new();
static SEVENTV_EMOTE_SET_CACHE: OnceLock<Mutex<HashMap<String, CachedBundle>>> = OnceLock::new();
static SEVENTV_CHANNEL_BADGES_CACHE: OnceLock<Mutex<HashMap<String, CachedBundle>>> = OnceLock::new();
static SEVENTV_BADGES_CACHE: OnceLock<Mutex<Option<CachedBundle>>> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub enum SevenTvCacheMode {
	UseCache,
	Refresh,
}

pub async fn fetch_7tv_bundle(platform: SevenTvPlatform, platform_id: &str) -> anyhow::Result<AssetBundle> {
	let (bundle, _) = fetch_7tv_bundle_with_sets(platform, platform_id, SevenTvCacheMode::UseCache).await?;
	Ok(bundle)
}

pub async fn fetch_7tv_bundle_with_sets(
	platform: SevenTvPlatform,
	platform_id: &str,
	cache_mode: SevenTvCacheMode,
) -> anyhow::Result<(AssetBundle, SevenTvUserEmoteSets)> {
	let client = SevenTvGqlClient::new();

	let sets = if matches!(cache_mode, SevenTvCacheMode::UseCache) {
		get_cached_7tv_user_emote_sets(platform, platform_id)
	} else {
		None
	};

	let sets = match sets {
		Some(sets) => sets,
		None => {
			let sets = client
				.user_emote_sets(platform, platform_id)
				.await
				.with_context(|| format!("7tv user emote sets ({})", platform.as_str()))?;
			set_cached_7tv_user_emote_sets(platform, platform_id, sets.clone());
			sets
		}
	};

	let mut emotes_map: HashMap<String, AssetRef> = HashMap::new();
	for set_id in sets.set_ids() {
		let bundle = fetch_7tv_emote_set_bundle(&client, &set_id, cache_mode).await?;
		for asset in bundle.emotes {
			emotes_map.entry(asset.id.clone()).or_insert(asset);
		}
	}

	let primary_set_id = sets.primary_set_id().ok_or_else(|| anyhow!("7tv emote set not found"))?;
	if emotes_map.is_empty() {
		return Err(anyhow!("7tv emote set not found"));
	}

	let emotes: Vec<AssetRef> = emotes_map.into_values().collect();
	let etag = compute_bundle_etag(&emotes, &[]);

	let bundle = AssetBundle {
		provider: AssetProvider::SevenTv,
		scope: AssetScope::Channel,
		cache_key: format!("7tv:set:{}", primary_set_id),
		etag: Some(etag),
		emotes,
		badges: Vec::new(),
	};

	info!(platform=%platform.as_str(), platform_id=%platform_id, cache_key=%bundle.cache_key, emote_count=bundle.emotes.len(), "7tv channel emote bundle fetched");

	Ok((bundle, sets))
}

pub async fn fetch_7tv_badges_bundle() -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_7tv_badges() {
		return Ok(bundle);
	}

	let client = SevenTvGqlClient::new();
	let badges = client.global_badges().await.context("7tv badges gql request")?;
	let badges: Vec<AssetRef> = badges.into_iter().filter_map(seventv_badge_to_asset).collect();
	let etag = compute_bundle_etag(&[], &badges);

	let bundle = AssetBundle {
		provider: AssetProvider::SevenTv,
		scope: AssetScope::Global,
		cache_key: "7tv:badges:global".to_string(),
		etag: Some(etag),
		emotes: Vec::new(),
		badges,
	};

	info!(cache_key=%bundle.cache_key, badge_count=bundle.badges.len(), "7tv global badges bundle fetched");

	set_cached_7tv_badges(bundle.clone());
	Ok(bundle)
}

pub async fn fetch_7tv_channel_badges_bundle(platform: SevenTvPlatform, platform_id: &str) -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_7tv_channel_badges(platform, platform_id) {
		return Ok(bundle);
	}

	let client = SevenTvGqlClient::new();
	let user_badges = client
		.channel_badges(platform, platform_id)
		.await
		.context("7tv channel badges gql request")?;

	let mut dedupe: HashMap<String, AssetRef> = HashMap::new();
	merge_user_badges(&mut dedupe, user_badges);

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

	info!(platform=%platform.as_str(), platform_id=%platform_id, cache_key=%bundle.cache_key, badge_count=bundle.badges.len(), "7tv channel badges bundle fetched");

	set_cached_7tv_channel_badges(platform, platform_id, bundle.clone());
	Ok(bundle)
}

pub(crate) fn prune_caches() {
	if let Some(cache) = SEVENTV_USER_SETS_CACHE.get() {
		prune_user_sets_cache(cache, SEVENTV_USER_SETS_TTL);
	}
	if let Some(cache) = SEVENTV_EMOTE_SET_CACHE.get() {
		prune_map_cache(cache, SEVENTV_EMOTE_SET_TTL);
	}
	if let Some(cache) = SEVENTV_CHANNEL_BADGES_CACHE.get() {
		prune_map_cache(cache, SEVENTV_CHANNEL_BADGES_TTL);
	}
	if let Some(cache) = SEVENTV_BADGES_CACHE.get() {
		prune_optional_cache(cache, SEVENTV_BADGES_TTL);
	}
}

fn prune_user_sets_cache(cache: &Mutex<HashMap<String, CachedUserEmoteSets>>, ttl: Duration) {
	let mut guard = cache.lock();
	guard.retain(|_, entry| entry.fetched_at.elapsed() <= ttl);
}

fn merge_user_badges(dedupe: &mut HashMap<String, AssetRef>, user_badges: SevenTvUserBadges) {
	if let Some(badge) = user_badges.active_badge
		&& let Some(asset) = seventv_badge_to_asset(badge)
	{
		dedupe.entry(asset.id.clone()).or_insert(asset);
	}

	for badge in user_badges.inventory_badges {
		if let Some(asset) = seventv_badge_to_asset(badge) {
			dedupe.entry(asset.id.clone()).or_insert(asset);
		}
	}
}

async fn fetch_7tv_emote_set_bundle(
	client: &SevenTvGqlClient,
	set_id: &str,
	cache_mode: SevenTvCacheMode,
) -> anyhow::Result<AssetBundle> {
	if matches!(cache_mode, SevenTvCacheMode::UseCache)
		&& let Some(bundle) = get_cached_7tv_emote_set(set_id)
	{
		return Ok(bundle);
	}

	let set = client
		.emote_set(set_id)
		.await
		.with_context(|| format!("7tv emote set {set_id}"))?;
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

	set_cached_7tv_emote_set(&set.id, bundle.clone());
	Ok(bundle)
}

fn seventv_emote_to_asset(item: SevenTvEmoteSetEmote) -> Option<AssetRef> {
	let images = seventv_images_to_assets(&item.emote.images);
	if images.is_empty() {
		return None;
	}

	let name = if item.alias.trim().is_empty() {
		item.emote.default_name.clone()
	} else {
		item.alias.clone()
	};

	Some(AssetRef {
		id: item.emote.id,
		name,
		images,
	})
}

fn seventv_badge_to_asset(badge: SevenTvBadge) -> Option<AssetRef> {
	let images = seventv_images_to_assets(&badge.images);
	if images.is_empty() {
		return None;
	}

	Some(AssetRef {
		id: badge.id,
		name: badge.name,
		images,
	})
}

fn seventv_images_to_assets(images: &[SevenTvImage]) -> Vec<AssetImage> {
	fn format_priority(format: &str) -> u8 {
		match format {
			"gif" => 0,
			"webp" => 1,
			"avif" => 2,
			"png" => 3,
			"svg" => 4,
			_ => 5,
		}
	}

	let mut by_scale: HashMap<AssetScale, AssetImage> = HashMap::new();
	for img in images {
		let scale = match img.scale {
			1 => AssetScale::One,
			2 => AssetScale::Two,
			3 => AssetScale::Three,
			4 => AssetScale::Four,
			_ => continue,
		};

		let format = img.mime.split('/').next_back().unwrap_or("png").to_ascii_lowercase();
		let candidate = AssetImage {
			scale,
			url: img.url.clone(),
			format,
			width: img.width as u32,
			height: img.height as u32,
		};

		let replace = match by_scale.get(&scale) {
			Some(existing) => format_priority(candidate.format.as_str()) < format_priority(existing.format.as_str()),
			None => true,
		};
		if replace {
			by_scale.insert(scale, candidate);
		}
	}

	let mut out: Vec<AssetImage> = by_scale.into_values().collect();
	out.sort_by_key(|img| img.scale.as_u8());
	out
}

fn get_cached_7tv_user_emote_sets(platform: SevenTvPlatform, platform_id: &str) -> Option<SevenTvUserEmoteSets> {
	let cache = SEVENTV_USER_SETS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let key = format!("{}:{}", platform.as_str(), platform_id);
	let mut guard = cache.lock();
	if let Some(entry) = guard.get(&key) {
		if entry.fetched_at.elapsed() <= SEVENTV_USER_SETS_TTL {
			Some(entry.sets.clone())
		} else {
			guard.remove(&key);
			None
		}
	} else {
		None
	}
}

fn set_cached_7tv_user_emote_sets(platform: SevenTvPlatform, platform_id: &str, sets: SevenTvUserEmoteSets) {
	let cache = SEVENTV_USER_SETS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	let key = format!("{}:{}", platform.as_str(), platform_id);
	guard.insert(
		key,
		CachedUserEmoteSets {
			fetched_at: Instant::now(),
			sets,
		},
	);
}

fn get_cached_7tv_emote_set(set_id: &str) -> Option<AssetBundle> {
	let cache = SEVENTV_EMOTE_SET_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	if let Some(entry) = guard.get(set_id) {
		if entry.fetched_at.elapsed() <= SEVENTV_EMOTE_SET_TTL {
			Some(entry.bundle.clone())
		} else {
			guard.remove(set_id);
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

fn set_cached_7tv_emote_set(set_id: &str, bundle: AssetBundle) {
	let cache = SEVENTV_EMOTE_SET_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	guard.insert(
		set_id.to_string(),
		CachedBundle {
			fetched_at: Instant::now(),
			bundle,
		},
	);
}

#[derive(Debug)]
struct CachedUserEmoteSets {
	fetched_at: Instant,
	sets: SevenTvUserEmoteSets,
}
