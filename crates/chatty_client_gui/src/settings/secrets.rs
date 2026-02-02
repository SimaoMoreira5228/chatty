use std::path::PathBuf;

use tracing::info;

use super::persistence::settings_dir;

pub fn secrets_path() -> PathBuf {
	let mut p = settings_dir();
	p.push("secrets.json");
	p
}

pub fn store_secret(name: &str, value: &str) -> Result<(), String> {
	let path = secrets_path();
	info!("storing secret '{}' (secrets path={})", name, path.display());

	if let Ok(entry) = keyring::Entry::new("chatty", name) {
		match entry.set_password(value) {
			Ok(()) => {
				info!("stored secret '{}' in system keyring", name);
				return Ok(());
			}
			Err(e) => {
				info!("keyring set_password failed for '{}': {}", name, e);
			}
		}
	} else {
		info!("keyring entry creation failed for '{}', falling back to file", name);
	}

	let mut map: std::collections::HashMap<String, String> = if let Ok(data) = std::fs::read_to_string(&path) {
		serde_json::from_str(&data).unwrap_or_default()
	} else {
		std::collections::HashMap::new()
	};

	map.insert(name.to_string(), value.to_string());
	if let Some(parent) = path.parent() {
		let _ = std::fs::create_dir_all(parent);
	}

	match serde_json::to_string_pretty(&map) {
		Ok(s) => match std::fs::write(&path, s) {
			Ok(()) => {
				info!("wrote secrets file {}", path.display());
			}
			Err(e) => {
				return Err(format!("failed to write secrets file: {}", e));
			}
		},
		Err(e) => return Err(format!("failed to serialize secrets: {}", e)),
	}
	Ok(())
}

pub fn read_secret(name: &str) -> Option<String> {
	let path = secrets_path();
	if let Ok(entry) = keyring::Entry::new("chatty", name) {
		match entry.get_password() {
			Ok(val) => {
				info!("read secret '{}' from keyring", name);
				return Some(val);
			}
			Err(e) => {
				info!("keyring get_password failed for '{}': {}", name, e);
			}
		}
	}

	if let Ok(data) = std::fs::read_to_string(&path)
		&& let Ok(map) = serde_json::from_str::<std::collections::HashMap<String, String>>(&data)
		&& let Some(v) = map.get(name)
	{
		info!("read secret '{}' from file {}", name, path.display());
		return Some(v.clone());
	}

	None
}
