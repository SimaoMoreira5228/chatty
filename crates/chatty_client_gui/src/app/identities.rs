#![forbid(unsafe_code)]

use chatty_client_ui::settings;
use chatty_domain::Platform;

use crate::app::Chatty;

impl Chatty {
	pub(crate) fn upsert_identity_from_twitch_blob(&mut self, raw: String) {
		let mut gs = self.state.gui_settings().clone();
		gs.twitch_oauth_blob = raw.clone();
		if let Some(parsed) = settings::parse_twitch_oauth_blob(&raw) {
			gs.twitch_username = parsed.username.clone();
			gs.twitch_user_id = parsed.user_id.clone();
			gs.twitch_client_id = parsed.client_id.clone();
			gs.twitch_oauth_token = parsed.oauth_token.clone();
			gs.user_oauth_token = parsed.oauth_token;

			let username = gs.twitch_username.trim().to_string();
			let user_id = gs.twitch_user_id.trim().to_string();
			let client_id = gs.twitch_client_id.trim().to_string();
			let oauth_token = gs.twitch_oauth_token.trim().to_string();
			if !username.is_empty() && !oauth_token.is_empty() {
				let id = if !user_id.is_empty() {
					format!("twitch:{}", user_id)
				} else {
					format!("twitch:{}", username)
				};
				let identity = settings::Identity {
					id: id.clone(),
					display_name: username.clone(),
					platform: Platform::Twitch,
					username,
					user_id,
					oauth_token,
					client_id,
					enabled: true,
				};
				if let Some(existing) = gs.identities.iter_mut().find(|i| i.id == id) {
					*existing = identity;
				} else {
					gs.identities.push(identity);
				}
				gs.active_identity = Some(id);
			}
		}
		self.state.set_gui_settings(gs);
	}

	pub(crate) fn upsert_identity_from_kick_blob(&mut self, raw: String) {
		let mut gs = self.state.gui_settings().clone();
		gs.kick_oauth_blob = raw.clone();
		if let Some(parsed) = settings::parse_kick_oauth_blob(&raw) {
			gs.kick_username = parsed.username.clone();
			gs.kick_user_id = parsed.user_id.clone();
			gs.kick_oauth_token = parsed.oauth_token.clone();

			let username = gs.kick_username.trim().to_string();
			let user_id = gs.kick_user_id.trim().to_string();
			let oauth_token = gs.kick_oauth_token.trim().to_string();
			if !username.is_empty() && !oauth_token.is_empty() {
				let id = if !user_id.is_empty() {
					format!("kick:{}", user_id)
				} else {
					format!("kick:{}", username)
				};
				let identity = settings::Identity {
					id: id.clone(),
					display_name: username.clone(),
					platform: Platform::Kick,
					username,
					user_id,
					oauth_token,
					client_id: String::new(),
					enabled: true,
				};
				if let Some(existing) = gs.identities.iter_mut().find(|i| i.id == id) {
					*existing = identity;
				} else {
					gs.identities.push(identity);
				}
				gs.active_identity = Some(id);
			}
		}
		self.state.set_gui_settings(gs);
	}
}
