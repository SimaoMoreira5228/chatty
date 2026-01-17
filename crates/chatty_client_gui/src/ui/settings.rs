#![forbid(unsafe_code)]
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::ui::theme::ThemeKind;
use chatty_client_core::ClientConfigV1;
use chatty_domain::Platform;
use chatty_domain::RoomKey;
use chatty_util::endpoint::validate_quic_endpoint;

/// GUI settings persisted on disk (v1).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Identity {
	/// Unique id for this identity (opaque string).
	pub id: String,

	/// Human-friendly display name shown in the UI.
	pub display_name: String,

	/// Platform associated with this identity (twitch/kick/youtube).
	pub platform: Platform,

	/// Username/login for the platform (optional).
	pub username: String,

	/// Platform user id (optional).
	pub user_id: String,

	/// OAuth token for this identity (optional).
	pub oauth_token: String,

	/// Platform client id (optional).
	pub client_id: String,

	/// Whether this identity is enabled (checkbox in settings).
	pub enabled: bool,
}

impl Default for Identity {
	fn default() -> Self {
		Self {
			id: String::new(),
			display_name: String::new(),
			platform: Platform::Twitch,
			username: String::new(),
			user_id: String::new(),
			oauth_token: String::new(),
			client_id: String::new(),
			enabled: true,
		}
	}
}

/// Persisted group configuration (client-side only).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GroupSettings {
	/// Stable id for the group.
	pub id: u64,
	/// Display name.
	pub name: String,
	/// Rooms included in the group.
	pub rooms: Vec<RoomKey>,
}

impl Default for GroupSettings {
	fn default() -> Self {
		Self {
			id: 0,
			name: String::new(),
			rooms: Vec::new(),
		}
	}
}

/// GUI settings persisted on disk (v1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiSettings {
	/// Chosen theme kind for the GUI.
	pub theme: ThemeKind,

	/// Which platform to assume for bare channel names (e.g. "xqc" -> twitch/xqc).
	pub default_platform: Platform,

	/// Per-tab maximum chat log items (UI convenience).
	pub max_log_items: usize,

	/// Saved user identities/accounts available for 'act as'.
	pub identities: Vec<Identity>,

	/// Optional id of the currently selected active identity.
	pub active_identity: Option<String>,

	/// Client-side groups (fan-in tabs).
	#[serde(default)]
	pub groups: Vec<GroupSettings>,

	/// Server endpoint in quic://host:port form.
	pub server_endpoint_quic: String,

	/// Optional auth token (future use).
	pub server_auth_token: String,

	/// Optional user OAuth token (e.g. Twitch).
	pub user_oauth_token: String,

	/// Raw Twitch login blob (chatterino-style key/value string).
	pub twitch_oauth_blob: String,

	/// Twitch client id (from login blob).
	pub twitch_client_id: String,

	/// Twitch user id (from login blob).
	pub twitch_user_id: String,

	/// Twitch username/login (from login blob).
	pub twitch_username: String,

	/// Twitch OAuth token (from login blob).
	pub twitch_oauth_token: String,

	/// Kick OAuth token (user access token).
	#[serde(default)]
	pub kick_oauth_token: String,

	/// Raw Kick login blob (JSON string).
	#[serde(default)]
	pub kick_oauth_blob: String,

	/// Kick user id.
	#[serde(default)]
	pub kick_user_id: String,

	/// Kick username/login.
	#[serde(default)]
	pub kick_username: String,
}

#[derive(Debug, Clone)]
pub struct TwitchOAuthInfo {
	pub username: String,
	pub user_id: String,
	pub client_id: String,
	pub oauth_token: String,
}

#[derive(Debug, Clone)]
pub struct KickOAuthInfo {
	pub username: String,
	pub user_id: String,
	pub oauth_token: String,
}

pub fn parse_twitch_oauth_blob(blob: &str) -> Option<TwitchOAuthInfo> {
	let mut username = String::new();
	let mut user_id = String::new();
	let mut client_id = String::new();
	let mut oauth_token = String::new();

	for part in blob.split(';') {
		let part = part.trim();
		if part.is_empty() {
			continue;
		}
		let (k, v) = part.split_once('=')?;
		let v = v.trim();
		match k.trim() {
			"username" => username = v.to_string(),
			"user_id" => user_id = v.to_string(),
			"client_id" => client_id = v.to_string(),
			"oauth_token" => oauth_token = v.to_string(),
			_ => {}
		}
	}

	if username.is_empty() || user_id.is_empty() || client_id.is_empty() || oauth_token.is_empty() {
		return None;
	}

	Some(TwitchOAuthInfo {
		username,
		user_id,
		client_id,
		oauth_token,
	})
}

pub fn parse_kick_oauth_blob(blob: &str) -> Option<KickOAuthInfo> {
	let value: JsonValue = serde_json::from_str(blob).ok()?;
	let username = value.get("username")?.as_str()?.trim().to_string();
	let user_id = value.get("user_id")?.as_str()?.trim().to_string();
	let oauth_token = value.get("oauth_token")?.as_str()?.trim().to_string();
	if username.is_empty() || user_id.is_empty() || oauth_token.is_empty() {
		return None;
	}
	Some(KickOAuthInfo {
		username,
		user_id,
		oauth_token,
	})
}

pub fn build_client_config(settings: &GuiSettings) -> Result<ClientConfigV1, String> {
	let mut cfg = if ClientConfigV1::server_endpoint_locked() {
		let endpoint = ClientConfigV1::default_server_endpoint_quic();
		validate_quic_endpoint(endpoint).map_err(|err| format!("Invalid build-time server endpoint: {err}"))?;
		ClientConfigV1::from_quic_endpoint(endpoint).map_err(|err| format!("Invalid build-time server endpoint: {err}"))?
	} else {
		let endpoint = settings.server_endpoint_quic.trim();
		if endpoint.is_empty() {
			ClientConfigV1::default()
		} else {
			validate_quic_endpoint(endpoint).map_err(|err| format!("Invalid server endpoint: {err}"))?;
			ClientConfigV1::from_quic_endpoint(endpoint).map_err(|err| format!("Invalid server endpoint: {err}"))?
		}
	};

	let token = settings.server_auth_token.trim();
	if !token.is_empty() {
		cfg.auth_token = Some(token.to_string());
	}

	let active_identity = settings
		.active_identity
		.as_ref()
		.and_then(|active_id| settings.identities.iter().find(|id| id.id == *active_id && id.enabled));

	if let Some(identity) = active_identity {
		match identity.platform {
			Platform::Twitch => {
				let token = identity.oauth_token.trim();
				if !token.is_empty() {
					cfg.user_oauth_token = Some(token.to_string());
				}

				let client_id = identity.client_id.trim();
				if !client_id.is_empty() {
					cfg.twitch_client_id = Some(client_id.to_string());
				}

				let user_id = identity.user_id.trim();
				if !user_id.is_empty() {
					cfg.twitch_user_id = Some(user_id.to_string());
				}

				let username = identity.username.trim();
				if !username.is_empty() {
					cfg.twitch_username = Some(username.to_string());
				}
			}
			Platform::Kick => {
				let token = identity.oauth_token.trim();
				if !token.is_empty() {
					cfg.kick_user_oauth_token = Some(token.to_string());
				}

				let user_id = identity.user_id.trim();
				if !user_id.is_empty() {
					cfg.kick_user_id = Some(user_id.to_string());
				}

				let username = identity.username.trim();
				if !username.is_empty() {
					cfg.kick_username = Some(username.to_string());
				}
			}
			Platform::YouTube => {}
		}
	} else {
		let user_token = settings.user_oauth_token.trim();
		if !user_token.is_empty() {
			cfg.user_oauth_token = Some(user_token.to_string());
		}

		let twitch_token = settings.twitch_oauth_token.trim();
		if !twitch_token.is_empty() {
			cfg.user_oauth_token = Some(twitch_token.to_string());
		}

		let twitch_client_id = settings.twitch_client_id.trim();
		if !twitch_client_id.is_empty() {
			cfg.twitch_client_id = Some(twitch_client_id.to_string());
		}

		let twitch_user_id = settings.twitch_user_id.trim();
		if !twitch_user_id.is_empty() {
			cfg.twitch_user_id = Some(twitch_user_id.to_string());
		}

		let twitch_username = settings.twitch_username.trim();
		if !twitch_username.is_empty() {
			cfg.twitch_username = Some(twitch_username.to_string());
		}

		let kick_token = settings.kick_oauth_token.trim();
		if !kick_token.is_empty() {
			cfg.kick_user_oauth_token = Some(kick_token.to_string());
		}

		let kick_user_id = settings.kick_user_id.trim();
		if !kick_user_id.is_empty() {
			cfg.kick_user_id = Some(kick_user_id.to_string());
		}

		let kick_username = settings.kick_username.trim();
		if !kick_username.is_empty() {
			cfg.kick_username = Some(kick_username.to_string());
		}
	}

	Ok(cfg)
}

/// Persisted world state (all windows).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiRootState {
	pub windows: Vec<UiWindow>,
}

/// A persisted window configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiWindow {
	pub id: String,
	pub is_primary: bool,
	pub x: f32,
	pub y: f32,
	pub width: f32,
	pub height: f32,
	pub active_layout_id: Option<String>,
	pub layouts: Vec<UiLayout>,
}

/// A persisted layout definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiLayout {
	pub id: String,
	pub title: String,
	#[serde(default)]
	pub pinned: bool,
	pub splits: Vec<UiSplit>,
	pub active_split_id: Option<String>,
}

/// A persisted split configuration within a layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSplit {
	pub id: String,
	pub width: f32,
	pub active_tab_id: Option<String>,
	pub tabs: Vec<UiTab>,
}

/// A persisted tab definition within a split.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTab {
	pub id: String,
	pub title: String,
	#[serde(default)]
	pub room: Option<RoomKey>,
	#[serde(default)]
	pub group_id: Option<u64>,
	#[serde(default)]
	pub pinned: bool,
}

impl Default for GuiSettings {
	fn default() -> Self {
		Self {
			theme: ThemeKind::DarkAmethyst,
			default_platform: Platform::Twitch,
			max_log_items: 2_000,
			identities: Vec::new(),
			active_identity: None,
			groups: Vec::new(),
			server_endpoint_quic: ClientConfigV1::default_server_endpoint_quic().to_string(),
			server_auth_token: String::new(),
			user_oauth_token: String::new(),
			twitch_oauth_blob: String::new(),
			twitch_client_id: String::new(),
			twitch_user_id: String::new(),
			twitch_username: String::new(),
			twitch_oauth_token: String::new(),
			kick_oauth_token: String::new(),
			kick_oauth_blob: String::new(),
			kick_user_id: String::new(),
			kick_username: String::new(),
		}
	}
}

/// Path for GUI settings (`~/.chatty/gui_settings.toml`).
fn settings_path() -> Option<PathBuf> {
	if let Some(home) = dirs::home_dir() {
		Some(home.join(".chatty").join("gui_settings.toml"))
	} else {
		None
	}
}

/// Path for UI layout (`~/.chatty/ui_layout.json`).
fn ui_layout_path() -> Option<PathBuf> {
	if let Some(home) = dirs::home_dir() {
		Some(home.join(".chatty").join("ui_layout.json"))
	} else {
		None
	}
}

/// Best-effort load from disk.
fn load_from_disk() -> Option<GuiSettings> {
	let p = settings_path()?;
	let s = fs::read_to_string(&p).ok()?;
	toml::from_str::<GuiSettings>(&s).ok()
}

/// Best-effort save to disk.
fn save_to_disk(cfg: &GuiSettings) {
	if let Some(p) = settings_path() {
		if let Some(parent) = p.parent() {
			let _ = fs::create_dir_all(parent);
		}
		if let Ok(toml_s) = toml::to_string_pretty(cfg) {
			let _ = fs::write(p, toml_s);
		}
	}
}

/// Best-effort load of UI layout.
pub fn load_ui_layout() -> Option<UiRootState> {
	let p = ui_layout_path()?;
	let s = fs::read_to_string(&p).ok()?;
	serde_json::from_str::<UiRootState>(&s).ok()
}

/// Best-effort save of UI layout.
pub fn save_ui_layout(layout: &UiRootState) {
	if let Some(p) = ui_layout_path() {
		if let Some(parent) = p.parent() {
			let _ = fs::create_dir_all(parent);
		}
		if let Ok(json_s) = serde_json::to_string_pretty(layout) {
			let _ = fs::write(p, json_s);
		}
	}
}

/// In-memory settings store.
pub struct SettingsStore {
	inner: Mutex<GuiSettings>,
}

impl SettingsStore {
	/// Create a store primed from disk (or defaults).
	fn new() -> Self {
		let cfg = load_from_disk().unwrap_or_default();
		Self { inner: Mutex::new(cfg) }
	}

	/// Get a clone of the current settings.
	fn get_cloned(&self) -> GuiSettings {
		let guard = self.inner.lock().expect("settings mutex poisoned");
		guard.clone()
	}

	/// Replace settings and persist.
	fn set_and_persist(&self, cfg: GuiSettings) {
		{
			let mut guard = self.inner.lock().expect("settings mutex poisoned");
			*guard = cfg.clone();
		}
		// Persist outside the lock.
		save_to_disk(&cfg);
	}

	/// Update settings in-place and persist.
	fn update_and_persist<F>(&self, mut f: F)
	where
		F: FnMut(&mut GuiSettings),
	{
		let cfg = {
			let mut guard = self.inner.lock().expect("settings mutex poisoned");
			let mut cloned = guard.clone();
			f(&mut cloned);
			*guard = cloned.clone();
			cloned
		};
		save_to_disk(&cfg);
	}
}

// Global settings store.
static SETTINGS: OnceLock<SettingsStore> = OnceLock::new();

/// Initialize the global settings store.
pub fn init() -> &'static SettingsStore {
	SETTINGS.get_or_init(|| SettingsStore::new())
}

/// Get a cloned copy of the current GUI settings.
pub fn get_cloned() -> GuiSettings {
	init().get_cloned()
}

/// Replace settings and persist.
pub fn set_and_persist(cfg: GuiSettings) {
	init().set_and_persist(cfg)
}

/// Update settings in-place and persist.
pub fn update_and_persist<F>(f: F)
where
	F: FnMut(&mut GuiSettings),
{
	init().update_and_persist(f)
}

/// Set theme kind and persist.
pub fn set_theme(kind: ThemeKind) {
	update_and_persist(|s| s.theme = kind);
}

/// Get current theme kind.
pub fn theme_kind() -> ThemeKind {
	get_cloned().theme
}

/// Add an identity and persist.
pub fn add_identity(identity: Identity) {
	init().update_and_persist(|s| s.identities.push(identity.clone()));
}

/// Remove an identity by id and persist.
pub fn remove_identity_by_id(id: &str) {
	update_and_persist(|s| {
		s.identities.retain(|i| i.id != id);
		if s.active_identity.as_deref() == Some(id) {
			s.active_identity = None;
		}
	});
}

/// Update an identity by id.
pub fn update_identity_by_id<F>(id: &str, mut f: F)
where
	F: FnMut(&mut Identity),
{
	update_and_persist(|s| {
		if let Some(i) = s.identities.iter_mut().find(|i| i.id == id) {
			f(i);
		}
	});
}

/// Set the active identity id.
pub fn set_active_identity(id: Option<String>) {
	update_and_persist(|s| s.active_identity = id.clone());
}

#[cfg(test)]
mod tests {
	use super::*;

	// Serialize tests that mutate global settings.
	static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

	fn acquire_test_lock() -> std::sync::MutexGuard<'static, ()> {
		// Initialize the lock on first use and keep the guard while mutating.
		TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
	}

	#[test]
	fn init_and_read_defaults() {
		let _ = init();
		let cfg = get_cloned();

		let _ = cfg.theme;
	}

	#[test]
	fn set_and_get_theme_roundtrip() {
		let _lock = acquire_test_lock();

		let store = init();
		let orig = get_cloned();
		store.set_and_persist(GuiSettings {
			theme: ThemeKind::Solarized,
			..orig.clone()
		});
		let after = get_cloned();
		assert_eq!(after.theme, ThemeKind::Solarized);

		store.set_and_persist(orig);
	}

	#[test]
	fn add_remove_identity_roundtrip() {
		let _lock = acquire_test_lock();

		let store = init();
		let orig = get_cloned();

		let identity = Identity {
			id: "test-id-1".to_string(),
			display_name: "Test Identity".to_string(),
			platform: Platform::Twitch,
			username: "tester".to_string(),
			user_id: String::new(),
			oauth_token: String::new(),
			client_id: String::new(),
			enabled: true,
		};

		add_identity(identity.clone());
		let after = get_cloned();
		assert!(after.identities.iter().any(|i| i.id == "test-id-1"));

		remove_identity_by_id("test-id-1");
		let after2 = get_cloned();
		assert!(!after2.identities.iter().any(|i| i.id == "test-id-1"));

		store.set_and_persist(orig);
	}

	#[test]
	fn set_active_identity_roundtrip() {
		let _lock = acquire_test_lock();

		let store = init();
		let orig = get_cloned();

		let identity = Identity {
			id: "test-id-2".to_string(),
			display_name: "Active Identity".to_string(),
			platform: Platform::Kick,
			username: "active".to_string(),
			user_id: String::new(),
			oauth_token: String::new(),
			client_id: String::new(),
			enabled: true,
		};
		add_identity(identity.clone());
		set_active_identity(Some("test-id-2".to_string()));
		let after = get_cloned();
		assert_eq!(after.active_identity, Some("test-id-2".to_string()));

		remove_identity_by_id("test-id-2");
		store.set_and_persist(orig);
	}
}
