#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::Context;
use parking_lot::Mutex;
use serde::Deserialize;

use super::common::{CachedBundle, compute_bundle_etag, prune_map_cache, prune_optional_cache};
use crate::{AssetBundle, AssetImage, AssetProvider, AssetRef, AssetScale, AssetScope};

const TWITCH_BADGES_TTL: Duration = Duration::from_secs(600);
const TWITCH_EMOTES_TTL: Duration = Duration::from_secs(600);

static TWITCH_BADGES_CACHE: OnceLock<Mutex<Option<CachedBundle>>> = OnceLock::new();
static TWITCH_CHANNEL_BADGES_CACHE: OnceLock<Mutex<HashMap<String, CachedBundle>>> = OnceLock::new();
static TWITCH_GLOBAL_EMOTES_CACHE: OnceLock<Mutex<Option<CachedBundle>>> = OnceLock::new();
static TWITCH_CHANNEL_EMOTES_CACHE: OnceLock<Mutex<HashMap<String, CachedBundle>>> = OnceLock::new();

pub async fn fetch_twitch_global_emotes_bundle(client_id: &str, bearer_token: &str) -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_twitch_global_emotes() {
		return Ok(bundle);
	}

	let url = "https://api.twitch.tv/helix/chat/emotes/global";
	let resp = reqwest::Client::new()
		.get(url)
		.header("Client-Id", client_id)
		.header("Authorization", format!("Bearer {bearer_token}"))
		.send()
		.await
		.context("twitch global emotes request")?
		.error_for_status()
		.context("twitch global emotes status")?;

	let body: TwitchEmotesResponse = resp.json().await.context("twitch global emotes json")?;
	let emotes: Vec<AssetRef> = body.data.into_iter().filter_map(twitch_emote_to_asset).collect();
	let etag = compute_bundle_etag(&emotes, &[]);

	let bundle = AssetBundle {
		provider: AssetProvider::Twitch,
		scope: AssetScope::Global,
		cache_key: "twitch:emotes:global".to_string(),
		etag: Some(etag),
		emotes,
		badges: Vec::new(),
	};

	set_cached_twitch_global_emotes(bundle.clone());
	Ok(bundle)
}

pub async fn fetch_twitch_channel_emotes_bundle(
	client_id: &str,
	bearer_token: &str,
	broadcaster_id: &str,
) -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_twitch_channel_emotes(broadcaster_id) {
		return Ok(bundle);
	}

	let url = format!("https://api.twitch.tv/helix/chat/emotes?broadcaster_id={broadcaster_id}");
	let resp = reqwest::Client::new()
		.get(url)
		.header("Client-Id", client_id)
		.header("Authorization", format!("Bearer {bearer_token}"))
		.send()
		.await
		.context("twitch channel emotes request")?
		.error_for_status()
		.context("twitch channel emotes status")?;

	let body: TwitchEmotesResponse = resp.json().await.context("twitch channel emotes json")?;
	let emotes: Vec<AssetRef> = body.data.into_iter().filter_map(twitch_emote_to_asset).collect();
	let etag = compute_bundle_etag(&emotes, &[]);

	let bundle = AssetBundle {
		provider: AssetProvider::Twitch,
		scope: AssetScope::Channel,
		cache_key: format!("twitch:emotes:channel:{broadcaster_id}"),
		etag: Some(etag),
		emotes,
		badges: Vec::new(),
	};

	set_cached_twitch_channel_emotes(broadcaster_id, bundle.clone());
	Ok(bundle)
}

pub async fn fetch_twitch_badges_bundle(client_id: &str, bearer_token: &str) -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_twitch_badges() {
		return Ok(bundle);
	}

	let url = "https://api.twitch.tv/helix/chat/badges/global";
	let resp = reqwest::Client::new()
		.get(url)
		.header("Client-Id", client_id)
		.header("Authorization", format!("Bearer {bearer_token}"))
		.send()
		.await
		.context("twitch badges request")?
		.error_for_status()
		.context("twitch badges status")?;

	let body: TwitchBadgesResponse = resp.json().await.context("twitch badges json")?;
	let mut badges = Vec::new();
	for set in body.data {
		let set_id = set.set_id;
		for ver in set.versions {
			if let Some(asset) = twitch_badge_to_asset(&set_id, ver) {
				badges.push(asset);
			}
		}
	}

	let etag = compute_bundle_etag(&[], &badges);

	let bundle = AssetBundle {
		provider: AssetProvider::Twitch,
		scope: AssetScope::Global,
		cache_key: "twitch:badges:global".to_string(),
		etag: Some(etag),
		emotes: Vec::new(),
		badges,
	};

	set_cached_twitch_badges(bundle.clone());
	Ok(bundle)
}

pub async fn fetch_twitch_channel_badges_bundle(
	client_id: &str,
	bearer_token: &str,
	broadcaster_id: &str,
) -> anyhow::Result<AssetBundle> {
	if let Some(bundle) = get_cached_twitch_channel_badges(broadcaster_id) {
		return Ok(bundle);
	}

	let url = format!("https://api.twitch.tv/helix/chat/badges?broadcaster_id={broadcaster_id}");
	let resp = reqwest::Client::new()
		.get(url)
		.header("Client-Id", client_id)
		.header("Authorization", format!("Bearer {bearer_token}"))
		.send()
		.await
		.context("twitch channel badges request")?
		.error_for_status()
		.context("twitch channel badges status")?;

	let body: TwitchBadgesResponse = resp.json().await.context("twitch channel badges json")?;
	let mut badges = Vec::new();
	for set in body.data {
		let set_id = set.set_id;
		for ver in set.versions {
			if let Some(asset) = twitch_badge_to_asset(&set_id, ver) {
				badges.push(asset);
			}
		}
	}
	let etag = compute_bundle_etag(&[], &badges);

	let bundle = AssetBundle {
		provider: AssetProvider::Twitch,
		scope: AssetScope::Channel,
		cache_key: format!("twitch:badges:channel:{broadcaster_id}"),
		etag: Some(etag),
		emotes: Vec::new(),
		badges,
	};

	set_cached_twitch_channel_badges(broadcaster_id, bundle.clone());
	Ok(bundle)
}

pub(crate) fn prune_caches() {
	if let Some(cache) = TWITCH_BADGES_CACHE.get() {
		prune_optional_cache(cache, TWITCH_BADGES_TTL);
	}
	if let Some(cache) = TWITCH_CHANNEL_BADGES_CACHE.get() {
		prune_map_cache(cache, TWITCH_BADGES_TTL);
	}
	if let Some(cache) = TWITCH_GLOBAL_EMOTES_CACHE.get() {
		prune_optional_cache(cache, TWITCH_EMOTES_TTL);
	}
	if let Some(cache) = TWITCH_CHANNEL_EMOTES_CACHE.get() {
		prune_map_cache(cache, TWITCH_EMOTES_TTL);
	}
}

fn twitch_badge_to_asset(set_id: &str, badge: TwitchBadgeVersion) -> Option<AssetRef> {
	let mut images = Vec::new();
	if !badge.image_url_1x.is_empty() {
		images.push(AssetImage {
			scale: AssetScale::One,
			url: badge.image_url_1x.clone(),
			format: "png".to_string(),
			width: 0,
			height: 0,
		});
	}
	if let Some(url) = badge.image_url_2x.clone() {
		images.push(AssetImage {
			scale: AssetScale::Two,
			url,
			format: "png".to_string(),
			width: 0,
			height: 0,
		});
	}
	if let Some(url) = badge.image_url_4x.clone() {
		images.push(AssetImage {
			scale: AssetScale::Four,
			url,
			format: "png".to_string(),
			width: 0,
			height: 0,
		});
	}

	if images.is_empty() {
		return None;
	}

	Some(AssetRef {
		id: format!("twitch:{set_id}:{}", badge.id),
		name: badge.title.unwrap_or_else(|| format!("{set_id}:{}", badge.id)),
		images,
	})
}

fn get_cached_twitch_badges() -> Option<AssetBundle> {
	let cache = TWITCH_BADGES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	let entry = guard.as_ref()?;
	if entry.fetched_at.elapsed() <= TWITCH_BADGES_TTL {
		Some(entry.bundle.clone())
	} else {
		*guard = None;
		None
	}
}

fn set_cached_twitch_badges(bundle: AssetBundle) {
	let cache = TWITCH_BADGES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	*guard = Some(CachedBundle {
		fetched_at: std::time::Instant::now(),
		bundle,
	});
}

fn get_cached_twitch_channel_badges(broadcaster_id: &str) -> Option<AssetBundle> {
	let cache = TWITCH_CHANNEL_BADGES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	if let Some(entry) = guard.get(broadcaster_id) {
		if entry.fetched_at.elapsed() <= TWITCH_BADGES_TTL {
			Some(entry.bundle.clone())
		} else {
			guard.remove(broadcaster_id);
			None
		}
	} else {
		None
	}
}

fn twitch_emote_to_asset(emote: TwitchEmote) -> Option<AssetRef> {
	let mut images = Vec::new();
	if emote.format.iter().any(|f| f == "animated") {
		let base = format!("https://static-cdn.jtvnw.net/emoticons/v2/{}/animated/dark", emote.id);
		images.push(AssetImage {
			scale: AssetScale::One,
			url: format!("{base}/1.0"),
			format: "gif".to_string(),
			width: 0,
			height: 0,
		});
		images.push(AssetImage {
			scale: AssetScale::Two,
			url: format!("{base}/2.0"),
			format: "gif".to_string(),
			width: 0,
			height: 0,
		});
		images.push(AssetImage {
			scale: AssetScale::Three,
			url: format!("{base}/3.0"),
			format: "gif".to_string(),
			width: 0,
			height: 0,
		});
	} else {
		if !emote.images.url_1x.is_empty() {
			images.push(AssetImage {
				scale: AssetScale::One,
				url: emote.images.url_1x.clone(),
				format: "png".to_string(),
				width: 0,
				height: 0,
			});
		}
		if let Some(url) = emote.images.url_2x.clone() {
			images.push(AssetImage {
				scale: AssetScale::Two,
				url,
				format: "png".to_string(),
				width: 0,
				height: 0,
			});
		}
		if let Some(url) = emote.images.url_4x.clone() {
			images.push(AssetImage {
				scale: AssetScale::Four,
				url,
				format: "png".to_string(),
				width: 0,
				height: 0,
			});
		}
	}

	if images.is_empty() {
		return None;
	}

	Some(AssetRef {
		id: format!("twitch:emote:{}", emote.id),
		name: emote.name,
		images,
	})
}

fn get_cached_twitch_global_emotes() -> Option<AssetBundle> {
	let cache = TWITCH_GLOBAL_EMOTES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	let entry = guard.as_ref()?;
	if entry.fetched_at.elapsed() <= TWITCH_EMOTES_TTL {
		Some(entry.bundle.clone())
	} else {
		*guard = None;
		None
	}
}

fn set_cached_twitch_global_emotes(bundle: AssetBundle) {
	let cache = TWITCH_GLOBAL_EMOTES_CACHE.get_or_init(|| Mutex::new(None));
	let mut guard = cache.lock();
	*guard = Some(CachedBundle {
		fetched_at: std::time::Instant::now(),
		bundle,
	});
}

fn get_cached_twitch_channel_emotes(broadcaster_id: &str) -> Option<AssetBundle> {
	let cache = TWITCH_CHANNEL_EMOTES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	if let Some(entry) = guard.get(broadcaster_id) {
		if entry.fetched_at.elapsed() <= TWITCH_EMOTES_TTL {
			Some(entry.bundle.clone())
		} else {
			guard.remove(broadcaster_id);
			None
		}
	} else {
		None
	}
}

fn set_cached_twitch_channel_emotes(broadcaster_id: &str, bundle: AssetBundle) {
	let cache = TWITCH_CHANNEL_EMOTES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	guard.insert(
		broadcaster_id.to_string(),
		CachedBundle {
			fetched_at: std::time::Instant::now(),
			bundle,
		},
	);
}

fn set_cached_twitch_channel_badges(broadcaster_id: &str, bundle: AssetBundle) {
	let cache = TWITCH_CHANNEL_BADGES_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
	let mut guard = cache.lock();
	guard.insert(
		broadcaster_id.to_string(),
		CachedBundle {
			fetched_at: std::time::Instant::now(),
			bundle,
		},
	);
}

#[derive(Debug, Deserialize)]
struct TwitchBadgesResponse {
	data: Vec<TwitchBadgeSet>,
}

#[derive(Debug, Deserialize)]
struct TwitchBadgeSet {
	#[serde(rename = "set_id")]
	set_id: String,
	versions: Vec<TwitchBadgeVersion>,
}

#[derive(Debug, Deserialize)]
struct TwitchBadgeVersion {
	id: String,
	#[serde(default)]
	title: Option<String>,
	#[serde(rename = "image_url_1x")]
	image_url_1x: String,
	#[serde(rename = "image_url_2x")]
	#[serde(default)]
	image_url_2x: Option<String>,
	#[serde(rename = "image_url_4x")]
	#[serde(default)]
	image_url_4x: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TwitchEmotesResponse {
	data: Vec<TwitchEmote>,
}

#[derive(Debug, Deserialize)]
struct TwitchEmote {
	id: String,
	name: String,
	images: TwitchEmoteImages,
	#[serde(default)]
	format: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TwitchEmoteImages {
	#[serde(rename = "url_1x")]
	url_1x: String,
	#[serde(rename = "url_2x")]
	#[serde(default)]
	url_2x: Option<String>,
	#[serde(rename = "url_4x")]
	#[serde(default)]
	url_4x: Option<String>,
}
