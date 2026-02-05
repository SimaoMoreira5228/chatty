use serde_json::Value as JsonValue;

#[derive(Debug, Clone)]
pub struct TwitchOAuthInfo {
	pub username: String,
	pub user_id: String,
	pub client_id: String,
	pub oauth_token: String,
	pub refresh_token: String,
}

#[derive(Debug, Clone)]
pub struct KickOAuthInfo {
	pub username: String,
	pub user_id: String,
	pub oauth_token: String,
	pub refresh_token: String,
}

pub fn parse_twitch_oauth_blob(blob: &str) -> Option<TwitchOAuthInfo> {
	let mut username = String::new();
	let mut user_id = String::new();
	let mut client_id = String::new();
	let mut oauth_token = String::new();
	let mut refresh_token = String::new();

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
			"refresh_token" => refresh_token = v.to_string(),
			_ => {}
		}
	}

	if oauth_token.is_empty() {
		return None;
	}

	if username.is_empty() && user_id.is_empty() {
		return None;
	}

	Some(TwitchOAuthInfo {
		username,
		user_id,
		client_id,
		oauth_token,
		refresh_token,
	})
}

pub fn parse_kick_oauth_blob(blob: &str) -> Option<KickOAuthInfo> {
	let value: JsonValue = serde_json::from_str(blob).ok()?;
	let username = value
		.get("username")
		.and_then(|v| v.as_str())
		.unwrap_or_default()
		.trim()
		.to_string();
	let user_id = value
		.get("user_id")
		.and_then(|v| v.as_str())
		.unwrap_or_default()
		.trim()
		.to_string();
	let oauth_token = value
		.get("oauth_token")
		.and_then(|v| v.as_str())
		.unwrap_or_default()
		.trim()
		.to_string();
	let refresh_token = value
		.get("refresh_token")
		.and_then(|v| v.as_str())
		.unwrap_or_default()
		.trim()
		.to_string();
	if oauth_token.is_empty() {
		return None;
	}
	if username.is_empty() && user_id.is_empty() {
		return None;
	}
	Some(KickOAuthInfo {
		username,
		user_id,
		oauth_token,
		refresh_token,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn it_parses_twitch_blob() {
		let blob = "username=foo;user_id=123;client_id=cid;oauth_token=tok";
		let p = parse_twitch_oauth_blob(blob).unwrap();
		assert_eq!(p.username, "foo");
		assert_eq!(p.user_id, "123");
		assert_eq!(p.client_id, "cid");
		assert_eq!(p.oauth_token, "tok");
	}
}
