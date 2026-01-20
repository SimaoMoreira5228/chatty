use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use toml;

use chatty_client_core::ClientConfigV1;
use chatty_domain::{Platform, RoomKey};
use chatty_util::endpoint::validate_quic_endpoint;

use keyring;
use serde_json;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ShortcutKey {
	#[default]
	Alt,
	Control,
	Shift,
	Logo,
	Always,
	None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Keybinds {
	pub drag_modifier: ShortcutKey,
	pub close_key: String,
	pub new_key: String,
	pub reconnect_key: String,
	pub vim_nav: bool,
	pub vim_left_key: String,
	pub vim_down_key: String,
	pub vim_up_key: String,
	pub vim_right_key: String,
}

impl Default for Keybinds {
	fn default() -> Self {
		Self {
			drag_modifier: ShortcutKey::Alt,
			close_key: "q".to_string(),
			new_key: "n".to_string(),
			reconnect_key: "r".to_string(),
			vim_nav: true,
			vim_left_key: "h".to_string(),
			vim_down_key: "j".to_string(),
			vim_up_key: "k".to_string(),
			vim_right_key: "l".to_string(),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeKind {
	Dark,
	Light,
	Solarized,
	HighContrast,
	Ocean,
	Dracula,
	Gruvbox,
	Nord,
	Synthwave,
	#[default]
	DarkAmethyst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SplitLayoutKind {
	Spiral,
	#[default]
	Masonry,
	Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DragModifier {
	#[default]
	Alt,
	Control,
	Shift,
	Logo,
	Always,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Identity {
	pub id: String,
	pub display_name: String,
	pub platform: Platform,
	pub username: String,
	pub user_id: String,
	pub oauth_token: String,
	pub client_id: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GroupSettings {
	pub id: u64,
	pub name: String,
	pub rooms: Vec<RoomKey>,
}

const CURRENT_SETTINGS_VERSION: u32 = 1;

fn default_settings_version() -> u32 {
	CURRENT_SETTINGS_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GuiSettings {
	pub settings_version: u32,

	pub theme: ThemeKind,
	pub default_platform: Platform,
	pub max_log_items: usize,
	pub split_layout: SplitLayoutKind,
	pub identities: Vec<Identity>,
	pub active_identity: Option<String>,
	pub groups: Vec<GroupSettings>,
	pub server_endpoint_quic: String,
	pub server_auth_token: String,
	pub user_oauth_token: String,
	pub twitch_oauth_blob: String,
	pub twitch_client_id: String,
	pub twitch_user_id: String,
	pub twitch_username: String,
	pub twitch_oauth_token: String,
	pub kick_oauth_token: String,
	pub kick_oauth_blob: String,
	pub kick_user_id: String,
	pub kick_username: String,
	pub auto_connect_on_startup: bool,
	pub locale: String,
	pub keybinds: Keybinds,
}

impl Default for GuiSettings {
	fn default() -> Self {
		Self {
			settings_version: default_settings_version(),
			theme: ThemeKind::DarkAmethyst,
			default_platform: Platform::Twitch,
			max_log_items: 2000,
			split_layout: SplitLayoutKind::Masonry,
			identities: Vec::new(),
			active_identity: None,
			groups: Vec::new(),
			server_endpoint_quic: String::new(),
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
			auto_connect_on_startup: false,
			locale: "en-US".to_string(),
			keybinds: Keybinds::default(),
		}
	}
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
	}

	Ok(cfg)
}

fn settings_dir() -> PathBuf {
	let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
	dir.push("chatty");
	dir
}

fn settings_path() -> PathBuf {
	let mut p = settings_dir();
	p.push("gui-settings.toml");
	p
}

fn migrate_settings_toml(mut v: toml::Value) -> toml::Value {
	let version = v.get("settings_version").and_then(|x| x.as_integer()).unwrap_or(0) as u32;
	if version < CURRENT_SETTINGS_VERSION {
		if let Some(table) = v.as_table_mut() {
			table.insert(
				"settings_version".to_string(),
				toml::Value::Integer(CURRENT_SETTINGS_VERSION as i64),
			);
		} else {
			let mut tbl = toml::map::Map::new();
			tbl.insert(
				"settings_version".to_string(),
				toml::Value::Integer(CURRENT_SETTINGS_VERSION as i64),
			);
			return toml::Value::Table(tbl);
		}
	}
	v
}

fn load_from_disk() -> Option<GuiSettings> {
	let path = settings_path();
	let data = fs::read_to_string(path).ok()?;

	let v = toml::from_str::<toml::Value>(&data).ok()?;
	let v = migrate_settings_toml(v);
	let mut settings = toml::from_str::<GuiSettings>(&v.to_string()).ok()?;

	if ClientConfigV1::server_endpoint_locked() {
		settings.server_endpoint_quic = String::new();
	}

	if let Some(tok) = read_secret("server_auth_token") {
		settings.server_auth_token = tok;
	}
	if let Some(tok) = read_secret("twitch_oauth_token") {
		settings.twitch_oauth_token = tok;
	}
	if let Some(tok) = read_secret("kick_oauth_token") {
		settings.kick_oauth_token = tok;
	}

	for identity in settings.identities.iter_mut() {
		let key = format!("identity:{}:oauth", identity.id);
		if let Some(val) = read_secret(&key) {
			identity.oauth_token = val;
		}
	}

	Some(settings)
}

fn persist_to_disk(cfg: &GuiSettings) {
	let mut to_persist = cfg.clone();
	if ClientConfigV1::server_endpoint_locked() {
		to_persist.server_endpoint_quic = String::new();
	}

	if !cfg.server_auth_token.trim().is_empty() {
		let _ = store_secret("server_auth_token", cfg.server_auth_token.trim());
		to_persist.server_auth_token = String::new();
	}

	if !cfg.twitch_oauth_token.trim().is_empty() {
		let _ = store_secret("twitch_oauth_token", cfg.twitch_oauth_token.trim());
		to_persist.twitch_oauth_token = String::new();
	}

	if !cfg.kick_oauth_token.trim().is_empty() {
		let _ = store_secret("kick_oauth_token", cfg.kick_oauth_token.trim());
		to_persist.kick_oauth_token = String::new();
	}

	for id in cfg.identities.iter() {
		if !id.oauth_token.trim().is_empty() {
			let key = format!("identity:{}:oauth", id.id);
			let _ = store_secret(&key, id.oauth_token.trim());
		}
	}

	let path = settings_path();
	if let Some(parent) = path.parent() {
		let _ = fs::create_dir_all(parent);
	}
	if let Ok(data) = toml::to_string_pretty(&to_persist) {
		let _ = fs::write(path, data);
	}
}

#[cfg(test)]
fn persist_to_disk_at(cfg: &GuiSettings, base: &std::path::Path) {
	let mut to_persist = cfg.clone();
	if ClientConfigV1::server_endpoint_locked() {
		to_persist.server_endpoint_quic = String::new();
	}

	if !cfg.server_auth_token.trim().is_empty() {
		let _ = store_secret("server_auth_token", cfg.server_auth_token.trim());
	}

	let mut dir = base.to_path_buf();
	dir.push("chatty");
	if let Some(parent) = dir.parent() {
		let _ = std::fs::create_dir_all(parent);
	}
	let path = dir.join("gui-settings.toml");
	if let Ok(data) = toml::to_string_pretty(&to_persist) {
		let _ = std::fs::write(path, data);
	}
}

static SETTINGS: OnceLock<Mutex<GuiSettings>> = OnceLock::new();

pub fn get_cloned() -> GuiSettings {
	let lock = SETTINGS.get_or_init(|| Mutex::new(load_from_disk().unwrap_or_default()));
	lock.lock().expect("settings lock").clone()
}

pub fn set_and_persist(cfg: GuiSettings) {
	let lock = SETTINGS.get_or_init(|| Mutex::new(load_from_disk().unwrap_or_default()));
	*lock.lock().expect("settings lock") = cfg.clone();
	persist_to_disk(&cfg);
}

fn secrets_path() -> PathBuf {
	let mut p = settings_dir();
	p.push("secrets.json");
	p
}

fn store_secret(name: &str, value: &str) -> Result<(), String> {
	if let Ok(entry) = keyring::Entry::new("chatty", name)
		&& entry.set_password(value).is_ok()
	{
		return Ok(());
	}

	let path = secrets_path();
	let mut map: std::collections::HashMap<String, String> = if let Ok(data) = std::fs::read_to_string(&path) {
		serde_json::from_str(&data).unwrap_or_default()
	} else {
		std::collections::HashMap::new()
	};
	map.insert(name.to_string(), value.to_string());
	if let Some(parent) = path.parent() {
		let _ = std::fs::create_dir_all(parent);
	}
	if let Ok(s) = serde_json::to_string_pretty(&map)
		&& let Err(e) = std::fs::write(&path, s)
	{
		return Err(format!("failed to write secrets file: {}", e));
	}
	Ok(())
}

fn read_secret(name: &str) -> Option<String> {
	if let Ok(entry) = keyring::Entry::new("chatty", name)
		&& let Ok(val) = entry.get_password()
	{
		return Some(val);
	}

	let path = secrets_path();
	if let Ok(data) = std::fs::read_to_string(&path)
		&& let Ok(map) = serde_json::from_str::<std::collections::HashMap<String, String>>(&data)
	{
		return map.get(name).cloned();
	}
	None
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::tempdir;

	#[test]
	fn it_parses_twitch_blob() {
		let blob = "username=foo;user_id=123;client_id=cid;oauth_token=tok";
		let p = parse_twitch_oauth_blob(blob).unwrap();
		assert_eq!(p.username, "foo");
		assert_eq!(p.user_id, "123");
		assert_eq!(p.client_id, "cid");
		assert_eq!(p.oauth_token, "tok");
	}

	#[test]
	fn migrate_adds_version_when_missing() {
		let raw = "theme = 'DarkAmethyst'\n";
		let v = toml::from_str::<toml::Value>(raw).unwrap();
		let v = migrate_settings_toml(v);
		assert_eq!(
			v.get("settings_version").and_then(|x| x.as_integer()),
			Some(CURRENT_SETTINGS_VERSION as i64)
		);
	}

	#[test]
	fn locked_endpoint_not_written_when_locked() {
		if ClientConfigV1::server_endpoint_locked() {
			let td = tempdir().expect("tempdir");

			let cfg = GuiSettings {
				server_endpoint_quic: "quic://should-not-persist:1234".to_string(),
				..Default::default()
			};
			persist_to_disk_at(&cfg, td.path());

			let mut path = td.path().to_path_buf();
			path.push("chatty");
			path.push("gui-settings.toml");
			let s = std::fs::read_to_string(path).expect("read settings");
			assert!(!s.contains("should-not-persist"), "locked endpoint must not be persisted");
		}
	}
}
