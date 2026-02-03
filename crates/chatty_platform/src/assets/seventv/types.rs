#![forbid(unsafe_code)]

#[derive(Debug, Clone, Copy)]
pub enum SevenTvPlatform {
	Twitch,
	Kick,
}

impl SevenTvPlatform {
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::Twitch => "TWITCH",
			Self::Kick => "KICK",
		}
	}
}

#[derive(Debug, Clone, Default)]
pub struct SevenTvUserEmoteSets {
	pub active_emote_set_id: Option<String>,
	pub personal_emote_set_id: Option<String>,
}

impl SevenTvUserEmoteSets {
	pub fn primary_set_id(&self) -> Option<&str> {
		if let Some(id) = self.active_emote_set_id.as_deref() {
			Some(id)
		} else {
			self.personal_emote_set_id.as_deref()
		}
	}

	pub fn set_ids(&self) -> Vec<String> {
		let mut ids = Vec::new();
		if let Some(active) = self.active_emote_set_id.as_ref() {
			ids.push(active.clone());
		}
		if let Some(personal) = self.personal_emote_set_id.as_ref()
			&& !ids.iter().any(|id| id == personal)
		{
			ids.push(personal.clone());
		}
		ids
	}
}
