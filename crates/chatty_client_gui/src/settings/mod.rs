use std::env;
use std::sync::{Mutex, OnceLock};

use chatty_util::endpoint::validate_quic_endpoint;
use tracing::info;

pub mod oauth;
pub mod persistence;
pub mod secrets;
pub mod types;

pub use oauth::{parse_kick_oauth_blob, parse_twitch_oauth_blob};
pub use persistence::{build_client_config, load_from_disk, persist_to_disk};
pub use types::{GuiSettings, Identity, ShortcutKey, SplitLayoutKind};

static SETTINGS: OnceLock<Mutex<GuiSettings>> = OnceLock::new();

pub fn get_cloned() -> GuiSettings {
	let lock = SETTINGS.get_or_init(|| {
		let initial = load_from_disk().unwrap_or_default();
		info!(
			"initialized settings: auto_connect={} identities={}",
			initial.auto_connect_on_startup,
			initial.identities.len()
		);

		let mut initial = initial;
		if !chatty_client_core::ClientConfigV1::server_endpoint_locked()
			&& let Ok(ep) = env::var("CHATTY_SERVER_ENDPOINT")
		{
			let ep = ep.trim().to_string();
			if !ep.is_empty() {
				if validate_quic_endpoint(&ep).is_ok() {
					info!(endpoint = %ep, "overriding server endpoint from CHATTY_SERVER_ENDPOINT env var");
					initial.server_endpoint_quic = ep;
				} else {
					info!(endpoint = %ep, "CHATTY_SERVER_ENDPOINT present but invalid; ignoring");
				}
			}
		}

		Mutex::new(initial)
	});
	let s = lock.lock().expect("settings lock").clone();
	info!(
		"get_cloned -> auto_connect={} identities={}",
		s.auto_connect_on_startup,
		s.identities.len()
	);
	s
}

pub fn set_and_persist(cfg: GuiSettings) {
	let lock = SETTINGS.get_or_init(|| Mutex::new(load_from_disk().unwrap_or_default()));
	*lock.lock().expect("settings lock") = cfg.clone();
	persist_to_disk(&cfg);
}
