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
		Task::done(Message::Net(crate::app::message::NetMessage::ConnectPressed))
	}

	pub(crate) fn upsert_identity_from_kick_blob(&mut self, raw: String) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		if let Some(parsed) = settings::parse_kick_oauth_blob(&raw) {
			let username = parsed.username.clone();
			let user_id = parsed.user_id.clone();
			let oauth_token = parsed.oauth_token.clone();

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
		Task::done(Message::Net(crate::app::message::NetMessage::ConnectPressed))
	}
}
