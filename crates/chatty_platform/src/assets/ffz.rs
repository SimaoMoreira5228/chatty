#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, anyhow};
use parking_lot::Mutex;
use serde::Deserialize;
use tracing::info;

use super::common::{CachedBundle, compute_bundle_etag, guess_format, prune_map_cache, prune_optional_cache};
use crate::{AssetBundle, AssetImage, AssetProvider, AssetRef, AssetScale, AssetScope};

const FFZ_BASE_URL: &str = "https://api.frankerfacez.com";
const FFZ_EMOTES_TTL: Duration = Duration::from_secs(300);
const FFZ_BADGES_TTL: Duration = Duration::from_secs(600);
const FFZ_GLOBAL_EMOTES_TTL: Duration = Duration::from_secs(600);

static FFZ_EMOTES_CACHE: OnceLock<Mutex<HashMap<String, CachedBundle>>> = OnceLock::new();
static FFZ_BADGES_CACHE: OnceLock<Mutex<Option<CachedBundle>>> = OnceLock::new();
static FFZ_GLOBAL_EMOTES_CACHE: OnceLock<Mutex<Option<CachedBundle>>> = OnceLock::new();

pub async fn fetch_ffz_bundle(room_login: &str) -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_ffz_emotes(room_login) {
		return Ok(bundle);
	}

	let url = format!("{FFZ_BASE_URL}/v1/room/{room_login}");
	let resp = reqwest::Client::new()
		.get(url)
		.send()
		.await
		.context("ffz room request")?
		.error_for_status()
		.context("ffz room status")?;

	let body: FfzRoomResponse = resp.json().await.context("ffz room json")?;
	let set_id = body.room.set;
	let set = body
		.sets
		.get(&set_id.to_string())
		.ok_or_else(|| anyhow!("ffz set not found"))?;

	let emotes: Vec<AssetRef> = set.emoticons.iter().filter_map(ffz_emote_to_asset).collect();
	let badges = ffz_room_badges(&body.room);
	let etag = compute_bundle_etag(&emotes, &badges);

	let bundle = AssetBundle {
		provider: AssetProvider::Ffz,
		scope: AssetScope::Channel,
		cache_key: format!("ffz:set:{set_id}"),
		etag: Some(etag),
		emotes,
		badges,
	};

	info!(room=%room_login, cache_key=%bundle.cache_key, emote_count=bundle.emotes.len(), badge_count=bundle.badges.len(), "ffz channel bundle fetched");

	set_cached_ffz_emotes(room_login, bundle.clone());
	Ok(bundle)
}

pub async fn fetch_ffz_badges_bundle() -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_ffz_badges() {
		return Ok(bundle);
	}

	let url = format!("{FFZ_BASE_URL}/v1/badges");
	let resp = reqwest::Client::new()
		.get(url)
		.send()
		.await
		.context("ffz badges request")?
		.error_for_status()
		.context("ffz badges status")?;

	let body: FfzBadgesResponse = resp.json().await.context("ffz badges json")?;
	let badges: Vec<AssetRef> = body.badges.into_iter().filter_map(ffz_badge_to_asset).collect();
	let etag = compute_bundle_etag(&[], &badges);

	let bundle = AssetBundle {
		provider: AssetProvider::Ffz,
		scope: AssetScope::Global,
		cache_key: "ffz:badges:global".to_string(),
		etag: Some(etag),
		emotes: Vec::new(),
		badges,
	};

	info!(cache_key=%bundle.cache_key, badge_count=bundle.badges.len(), "ffz global badges bundle fetched");

	set_cached_ffz_badges(bundle.clone());
	Ok(bundle)
}

pub async fn fetch_ffz_global_emotes_bundle() -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_ffz_global_emotes() {
		return Ok(bundle);
	}

	let ids_url = format!("{FFZ_BASE_URL}/v1/set/global/ids");
	let resp = reqwest::Client::new()
		.get(ids_url)
		.send()
		.await
		.context("ffz global emotes ids request")?
		.error_for_status()
		.context("ffz global emotes ids status")?;
	let ids_body: FfzGlobalSetIdsResponse = resp.json().await.context("ffz global emotes ids json")?;

	let global_id = ids_body
		.default_sets
		.first()
		.ok_or_else(|| anyhow!("ffz global emote set ids missing"))?;

	let set_url = format!("{FFZ_BASE_URL}/v1/set/{global_id}");
	let resp = reqwest::Client::new()
		.get(set_url)
		.send()
		.await
		.context("ffz global emote set request")?
		.error_for_status()
		.context("ffz global emote set status")?;
	let set_body: FfzGlobalSetResponse = resp.json().await.context("ffz global emote set json")?;

	let emotes: Vec<AssetRef> = set_body.set.emoticons.iter().filter_map(ffz_emote_to_asset).collect();
	let etag = compute_bundle_etag(&emotes, &[]);

	let bundle = AssetBundle {
		provider: AssetProvider::Ffz,
		scope: AssetScope::Global,
		cache_key: format!("ffz:emotes:global:{global_id}"),
		etag: Some(etag),
		emotes,
		badges: Vec::new(),
	};

	info!(cache_key=%bundle.cache_key, emote_count=bundle.emotes.len(), "ffz global emotes bundle fetched");

	set_cached_ffz_global_emotes(bundle.clone());
	Ok(bundle)
}

pub(crate) fn prune_caches() {
	if let Some(cache) = FFZ_EMOTES_CACHE.get() {
		prune_map_cache(cache, FFZ_EMOTES_TTL);
	}

	if let Some(cache) = FFZ_BADGES_CACHE.get() {
		prune_optional_cache(cache, FFZ_BADGES_TTL);
	}

	if let Some(cache) = FFZ_GLOBAL_EMOTES_CACHE.get() {
		prune_optional_cache(cache, FFZ_GLOBAL_EMOTES_TTL);
	}
}

fn ffz_emote_to_asset(emote: &FfzEmote) -> Option<AssetRef> {
	fn scale_from_str(scale: &str) -> Option<AssetScale> {
		match scale {
			"1" => Some(AssetScale::One),
			"2" => Some(AssetScale::Two),
			"3" => Some(AssetScale::Three),
			"4" => Some(AssetScale::Four),
			_ => None,
		}
	}

	let mut images = Vec::new();
	let animated = emote.animated.as_ref();
	for (scale, url) in emote.urls.iter() {
		let Some(scale) = scale_from_str(scale.as_str()) else {
			continue;
		};
		let scale_key = scale.as_u8().to_string();
		let chosen_url = animated
			.and_then(|m| m.get(scale_key.as_str()))
			.cloned()
			.unwrap_or_else(|| url.clone());
		let format = guess_format(&chosen_url);
		images.push(AssetImage {
			scale,
			url: chosen_url,
			format,
			width: emote.width.unwrap_or(0.0).round() as u32,
			height: emote.height.unwrap_or(0.0).round() as u32,
		});
	}

	if images.is_empty() {
		return None;
	}

	images.sort_by_key(|img| img.scale.as_u8());

	Some(AssetRef {
		id: emote.id.to_string(),
		name: emote.name.clone(),
		images,
	})
}

fn ffz_room_badges(room: &FfzRoom) -> Vec<AssetRef> {
	let mut badges = Vec::new();

	if let Some(urls) = room.vip_badge.as_ref()
		&& let Some(badge) = ffz_badge_from_urls("ffz:vip", "VIP", urls)
	{
		badges.push(badge);
	}

	if let Some(urls) = room.mod_urls.as_ref()
		&& let Some(badge) = ffz_badge_from_urls("ffz:moderator", "Moderator", urls)
	{
		badges.push(badge);
	}

	badges
}

fn ffz_badge_from_urls(id: &str, name: &str, urls: &HashMap<String, String>) -> Option<AssetRef> {
	let mut images = Vec::new();
	for (scale, url) in urls {
		let scale = match scale.as_str() {
			"1" => Some(AssetScale::One),
			"2" => Some(AssetScale::Two),
			"3" => Some(AssetScale::Three),
			"4" => Some(AssetScale::Four),
			_ => None,
		};
		let Some(scale) = scale else {
			continue;
		};
		images.push(AssetImage {
			scale,
			url: url.clone(),
			format: guess_format(url),
			width: 0,
			height: 0,
		});
	}

	if images.is_empty() {
		return None;
	}

	images.sort_by_key(|img| img.scale.as_u8());

	Some(AssetRef {
		id: id.to_string(),
		name: name.to_string(),
		images,
	})
}

fn ffz_badge_to_asset(badge: FfzBadge) -> Option<AssetRef> {
	let mut images = Vec::new();
	for (scale, url) in badge.urls.iter() {
		let scale = match scale.as_str() {
			"1" => Some(AssetScale::One),
			"2" => Some(AssetScale::Two),
			"3" => Some(AssetScale::Three),
			"4" => Some(AssetScale::Four),
			_ => None,
		};
		let Some(scale) = scale else {
			continue;
		};
		images.push(AssetImage {
			scale,
			url: url.clone(),
			format: guess_format(url),
			width: 0,
			height: 0,
		});
	}

	if images.is_empty() {
		return None;
	}

	images.sort_by_key(|img| img.scale.as_u8());

	Some(AssetRef {
		id: badge.id.to_string(),
		name: badge.name,
		images,
	})
}

fn get_cached_ffz_badges() -> Option<AssetBundle> {
	let cache = FFZ_BADGES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	let entry = guard.as_ref()?;
	if entry.fetched_at.elapsed() <= FFZ_BADGES_TTL {
		Some(entry.bundle.clone())
	} else {
		*guard = None;
		None
	}
}

fn set_cached_ffz_badges(bundle: AssetBundle) {
	let cache = FFZ_BADGES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	*guard = Some(CachedBundle {
		fetched_at: std::time::Instant::now(),
		bundle,
	});
}

fn get_cached_ffz_global_emotes() -> Option<AssetBundle> {
	let cache = FFZ_GLOBAL_EMOTES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	let entry = guard.as_ref()?;
	if entry.fetched_at.elapsed() <= FFZ_GLOBAL_EMOTES_TTL {
		Some(entry.bundle.clone())
	} else {
		*guard = None;
		None
	}
}

fn set_cached_ffz_global_emotes(bundle: AssetBundle) {
	let cache = FFZ_GLOBAL_EMOTES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	*guard = Some(CachedBundle {
		fetched_at: std::time::Instant::now(),
		bundle,
	});
}

fn get_cached_ffz_emotes(room_login: &str) -> Option<AssetBundle> {
	let cache = FFZ_EMOTES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	if let Some(entry) = guard.get(room_login) {
		if entry.fetched_at.elapsed() <= FFZ_EMOTES_TTL {
			Some(entry.bundle.clone())
		} else {
			guard.remove(room_login);
			None
		}
	} else {
		None
	}
}

fn set_cached_ffz_emotes(room_login: &str, bundle: AssetBundle) {
	let cache = FFZ_EMOTES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	guard.insert(
		room_login.to_string(),
		CachedBundle {
			fetched_at: std::time::Instant::now(),
			bundle,
		},
	);
}

#[derive(Debug, Deserialize)]
struct FfzRoomResponse {
	room: FfzRoom,
	sets: HashMap<String, FfzEmoteSet>,
}

#[derive(Debug, Deserialize)]
struct FfzRoom {
	set: u64,
	#[serde(default)]
	vip_badge: Option<HashMap<String, String>>,
	#[serde(default)]
	mod_urls: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct FfzEmoteSet {
	#[serde(default)]
	emoticons: Vec<FfzEmote>,
}

#[derive(Debug, Deserialize)]
struct FfzEmote {
	id: u64,
	name: String,
	width: Option<f64>,
	height: Option<f64>,
	urls: HashMap<String, String>,
	animated: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct FfzBadgesResponse {
	badges: Vec<FfzBadge>,
}

#[derive(Debug, Deserialize)]
struct FfzGlobalSetIdsResponse {
	#[serde(default)]
	default_sets: Vec<u64>,
}

#[derive(Debug, Deserialize)]
struct FfzGlobalSetResponse {
	set: FfzEmoteSet,
}

#[derive(Debug, Deserialize)]
struct FfzBadge {
	id: u64,
	name: String,
	urls: HashMap<String, String>,
}
