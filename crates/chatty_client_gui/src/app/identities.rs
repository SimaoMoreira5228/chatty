#![forbid(unsafe_code)]

use chatty_domain::Platform;
use iced::Task;

use crate::app::message::Message;
use crate::app::model::Chatty;
use crate::settings;

impl Chatty {
	pub(crate) fn upsert_identity_from_twitch_blob(&mut self, raw: String) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		if let Some(parsed) = settings::parse_twitch_oauth_blob(&raw) {
			let username = parsed.username.clone();
			let user_id = parsed.user_id.clone();
			let client_id = parsed.client_id.clone();
			let oauth_token = parsed.oauth_token.clone();
			let refresh_token = parsed.refresh_token.clone();

			if !oauth_token.is_empty() && (!username.is_empty() || !user_id.is_empty()) {
				let id = if !user_id.is_empty() {
					format!("twitch:{}", user_id)
				} else if !username.is_empty() {
					format!("twitch:{}", username)
				} else {
					"twitch:unknown".to_string()
				};
				let display_name = if !username.is_empty() {
					username.clone()
				} else if !user_id.is_empty() {
					user_id.clone()
				} else {
					"Twitch".to_string()
				};
				let identity = settings::Identity {
					id: id.clone(),
					display_name,
					platform: Platform::Twitch,
					username,
					user_id,
					oauth_token,
					refresh_token,
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
		Task::done(Message::Net(Box::new(crate::app::message::NetMessage::ConnectPressed)))
	}

	pub(crate) fn upsert_identity_from_kick_blob(&mut self, raw: String) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		if let Some(parsed) = settings::parse_kick_oauth_blob(&raw) {
			let username = parsed.username.clone();
			let user_id = parsed.user_id.clone();
			let oauth_token = parsed.oauth_token.clone();

			if !oauth_token.is_empty() && (!username.is_empty() || !user_id.is_empty()) {
				let id = if !user_id.is_empty() {
					format!("kick:{}", user_id)
				} else if !username.is_empty() {
					format!("kick:{}", username)
				} else {
					"kick:unknown".to_string()
				};
				let display_name = if !username.is_empty() {
					username.clone()
				} else if !user_id.is_empty() {
					user_id.clone()
				} else {
					"Kick".to_string()
				};
				let identity = settings::Identity {
					id: id.clone(),
					display_name,
					platform: Platform::Kick,
					username,
					user_id,
					oauth_token,
					refresh_token: String::new(),
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
		Task::done(Message::Net(Box::new(crate::app::message::NetMessage::ConnectPressed)))
	}
}
