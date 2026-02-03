use std::fs;
use std::path::PathBuf;

use chatty_client_core::ClientConfigV1;
use chatty_domain::Platform;
use chatty_util::endpoint::validate_quic_endpoint;
use tracing::{error, info};

use super::secrets::{read_secret, store_secret};
use super::types::{CURRENT_SETTINGS_VERSION, GuiSettings};

pub fn settings_dir() -> PathBuf {
	if let Some(cfg) = dirs::config_dir() {
		let mut dir = cfg;
		dir.push("chatty");
		return dir;
	}

	if let Some(home) = dirs::home_dir() {
		let mut dir = home.join(".config");
		dir.push("chatty");
		return dir;
	}

	let mut dir = PathBuf::from(".");
	dir.push("chatty");
	dir
}

pub fn settings_path() -> PathBuf {
	let mut p = settings_dir();
	p.push("gui-settings.toml");
	p
}

pub fn migrate_settings_toml(mut v: toml::Value) -> toml::Value {
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

pub fn load_from_disk() -> Option<GuiSettings> {
	let path = settings_path();
	info!("loading settings from {}", path.display());
	let data = match fs::read_to_string(&path) {
		Ok(d) => d,
		Err(e) => {
			info!("no settings file at {}: {}", path.display(), e);
			return None;
		}
	};

	let mut settings: GuiSettings;
	let v = match toml::from_str::<toml::Value>(&data) {
		Ok(v) => v,
		Err(e) => {
			error!("failed to parse settings TOML: {}", e);
			return None;
		}
	};

	let v = migrate_settings_toml(v);

	let pretty = match toml::to_string_pretty(&v) {
		Ok(s) => s,
		Err(e) => {
			error!("failed to serialize migrated toml::Value to TOML: {}", e);
			return None;
		}
	};

	settings = match toml::from_str::<GuiSettings>(&pretty) {
		Ok(s) => s,
		Err(e) => {
			error!("failed to deserialize migrated settings into GuiSettings: {}", e);
			return None;
		}
	};

	if ClientConfigV1::server_endpoint_locked() {
		settings.server_endpoint_quic = String::new();
	}

	if let Some(tok) = read_secret("server_auth_token") {
		settings.server_auth_token = tok;
	}

	for identity in settings.identities.iter_mut() {
		let key = format!("identity:{}:oauth", identity.id);
		if let Some(val) = read_secret(&key) {
			identity.oauth_token = val;
		}
		let refresh_key = format!("identity:{}:refresh", identity.id);
		if let Some(val) = read_secret(&refresh_key) {
			identity.refresh_token = val;
		}
	}

	Some(settings)
}

pub fn persist_to_disk(cfg: &GuiSettings) {
	let mut to_persist = cfg.clone();
	if ClientConfigV1::server_endpoint_locked() {
		to_persist.server_endpoint_quic = String::new();
	}

	if !cfg.server_auth_token.trim().is_empty() {
		let _ = store_secret("server_auth_token", cfg.server_auth_token.trim());
		to_persist.server_auth_token = String::new();
	}

	for id in cfg.identities.iter() {
		if !id.oauth_token.trim().is_empty() {
			let key = format!("identity:{}:oauth", id.id);
			let _ = store_secret(&key, id.oauth_token.trim());
		}
		if !id.refresh_token.trim().is_empty() {
			let key = format!("identity:{}:refresh", id.id);
			let _ = store_secret(&key, id.refresh_token.trim());
		}
	}

	let path = settings_path();
	info!("persisting settings to {}", path.display());
	if let Some(parent) = path.parent()
		&& let Err(e) = fs::create_dir_all(parent)
	{
		error!("failed to create settings dir {}: {}", parent.display(), e);
	}

	match toml::to_string_pretty(&to_persist) {
		Ok(data) => {
			let len = data.len();
			match fs::write(&path, data) {
				Ok(()) => info!("wrote settings file {} ({} bytes)", path.display(), len),
				Err(e) => error!("failed to write settings file {}: {}", path.display(), e),
			}
		}
		Err(e) => {
			error!("failed to serialize settings for writing: {}", e);
		}
	}
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

	let mut twitch_identity = None;
	let mut kick_identity = None;
	if let Some(active_id) = settings.active_identity.as_deref()
		&& let Some(active) = settings
			.identities
			.iter()
			.find(|identity| identity.enabled && !identity.oauth_token.trim().is_empty() && identity.id == active_id)
	{
		match active.platform {
			Platform::Twitch => twitch_identity = Some(active),
			Platform::Kick => kick_identity = Some(active),
			_ => {}
		}
	}

	for identity in settings.identities.iter().rev() {
		if !identity.enabled || identity.oauth_token.trim().is_empty() {
			continue;
		}

		match identity.platform {
			Platform::Twitch => {
				if twitch_identity.is_none() {
					twitch_identity = Some(identity);
				}
			}
			Platform::Kick => {
				if kick_identity.is_none() {
					kick_identity = Some(identity);
				}
			}
			_ => {}
		}
	}

	if let Some(identity) = twitch_identity {
		let token = identity.oauth_token.trim();
		if !token.is_empty() {
			cfg.user_oauth_token = Some(token.to_string());
		}

		let refresh = identity.refresh_token.trim();
		if !refresh.is_empty() {
			cfg.twitch_refresh_token = Some(refresh.to_string());
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

	if let Some(identity) = kick_identity {
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

	Ok(cfg)
}

#[cfg(test)]
mod tests {

	use super::*;

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
}
