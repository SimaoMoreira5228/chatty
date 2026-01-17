#![forbid(unsafe_code)]

use std::sync::OnceLock;
use std::time::Duration;

use super::{bttv, ffz, kick, seventv, twitch};

static ASSET_CACHE_PRUNER: OnceLock<()> = OnceLock::new();

pub fn ensure_asset_cache_pruner() {
	ASSET_CACHE_PRUNER.get_or_init(|| {
		tokio::spawn(async {
			let mut interval = tokio::time::interval(Duration::from_secs(300));
			loop {
				interval.tick().await;
				prune_asset_caches();
			}
		});
	});
}

fn prune_asset_caches() {
	ffz::prune_caches();
	seventv::prune_caches();
	bttv::prune_caches();
	twitch::prune_caches();
	kick::prune_caches();
}
