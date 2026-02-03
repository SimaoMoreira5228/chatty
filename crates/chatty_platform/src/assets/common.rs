#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::{AssetBundle, AssetRef};

pub(crate) struct CachedBundle {
	pub(crate) fetched_at: Instant,
	pub(crate) bundle: AssetBundle,
}

pub(crate) fn prune_map_cache(cache: &Mutex<HashMap<String, CachedBundle>>, ttl: Duration) {
	let mut guard = cache.lock();
	guard.retain(|_, entry| entry.fetched_at.elapsed() <= ttl);
}

pub(crate) fn prune_optional_cache(cache: &Mutex<Option<CachedBundle>>, ttl: Duration) {
	let mut guard = cache.lock();
	if let Some(entry) = guard.as_ref()
		&& entry.fetched_at.elapsed() > ttl
	{
		*guard = None;
	}
}

pub(crate) fn guess_format(url: &str) -> String {
	let lower = url.to_ascii_lowercase();
	if lower.ends_with(".gif") {
		"gif".to_string()
	} else if lower.ends_with(".png") {
		"png".to_string()
	} else if lower.ends_with(".webp") {
		"webp".to_string()
	} else if lower.ends_with(".avif") {
		"avif".to_string()
	} else if lower.ends_with(".svg") {
		"svg".to_string()
	} else {
		"png".to_string()
	}
}

pub(crate) fn compute_bundle_etag(emotes: &[AssetRef], badges: &[AssetRef]) -> String {
	let mut keys = Vec::with_capacity(emotes.len().saturating_add(badges.len()));
	for emote in emotes {
		let mut images = emote.images.clone();
		images.sort_by_key(|img| img.scale.as_u8());
		let image_key = images
			.iter()
			.map(|img| {
				format!(
					"{}:{}:{}:{}:{}",
					img.scale.as_u8(),
					img.url,
					img.format,
					img.width,
					img.height
				)
			})
			.collect::<Vec<_>>()
			.join("|");
		keys.push(format!("e:{}:{}:{}", emote.id, emote.name, image_key));
	}

	for badge in badges {
		let mut images = badge.images.clone();
		images.sort_by_key(|img| img.scale.as_u8());
		let image_key = images
			.iter()
			.map(|img| {
				format!(
					"{}:{}:{}:{}:{}",
					img.scale.as_u8(),
					img.url,
					img.format,
					img.width,
					img.height
				)
			})
			.collect::<Vec<_>>()
			.join("|");
		keys.push(format!("b:{}:{}:{}", badge.id, badge.name, image_key));
	}

	keys.sort();
	let mut hasher = DefaultHasher::new();
	for key in keys {
		key.hash(&mut hasher);
	}

	format!("{:016x}", hasher.finish())
}
