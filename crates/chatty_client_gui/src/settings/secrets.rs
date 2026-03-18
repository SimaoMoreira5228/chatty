use std::path::PathBuf;

use tracing::info;

use super::persistence::settings_dir;

pub fn secrets_path() -> PathBuf {
	let mut p = settings_dir();
	p.push("secrets.json");
	p
}

fn secret_name_hash(name: &str) -> String {
	use std::collections::hash_map::DefaultHasher;
	use std::hash::{Hash, Hasher};
	let mut s = DefaultHasher::new();
	name.hash(&mut s);
	format!("{:016x}", s.finish())
}

pub fn store_secret(name: &str, value: &str) -> Result<(), String> {
	let path = secrets_path();
	let name_hash = secret_name_hash(name);
	info!("storing secret (name_hash={}, secrets_path={})", name_hash, path.display());

	if let Ok(entry) = keyring::Entry::new("chatty", name) {
		match entry.set_password(value) {
			Ok(()) => {
				info!("stored secret in system keyring");
				return Ok(());
			}
			Err(e) => {
				info!("keyring set_password failed: {}", e);
			}
		}
	} else {
		info!("keyring entry creation failed, falling back to file");
	}

	let mut map: std::collections::HashMap<String, String> = if let Ok(data) = std::fs::read_to_string(&path) {
		match serde_json::from_str(&data) {
			Ok(m) => m,
			Err(e) => {
				info!("corrupt secrets file: {}", e);
				std::collections::HashMap::new()
			}
		}
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
				#[cfg(unix)]
				{
					if let Err(e) = std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600)) {
						info!("failed to set restrictive permissions on secrets file: {}", e);
					}
				}
				Ok(())
			}
			Err(e) => {
				return Err(format!("failed to write secrets file: {}", e));
			}
		},
		Err(e) => return Err(format!("failed to serialize secrets: {}", e)),
	}
}

pub fn read_secret(name: &str) -> Option<String> {
	let path = secrets_path();
	if let Ok(entry) = keyring::Entry::new("chatty", name) {
		match entry.get_password() {
			Ok(val) => {
				info!("read secret from keyring");
				return Some(val);
			}
			Err(e) => {
				info!("keyring get_password failed: {}", e);
			}
		}
	}

	if let Ok(data) = std::fs::read_to_string(&path)
		&& let Ok(map) = serde_json::from_str::<std::collections::HashMap<String, String>>(&data)
		&& let Some(v) = map.get(name)
	{
		info!("read secret from file {}", path.display());
		return Some(v.clone());
	}

	None
}
