use chatty_domain::Platform;
use iced::Task;
use rust_i18n::t;

use crate::app::types::{ClipboardTarget, PlatformChoice, ShortcutKeyChoice, SplitLayoutChoice, ThemeChoice};
use crate::app::{Chatty, Message};

impl Chatty {
	pub fn update_platform_selected(&mut self, choice: PlatformChoice) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.default_platform = choice.0;
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_max_log_items_changed(&mut self, v: String) -> Task<Message> {
		self.state.ui.max_log_items_raw = v.clone();
		if let Ok(n) = v.trim().parse::<usize>()
			&& n > 0
		{
			let mut gs = self.state.gui_settings().clone();
			gs.max_log_items = n;
			self.state.set_gui_settings(gs);
			for tab in self.state.tabs.values_mut() {
				tab.log.max_items = n;
				while tab.log.items.len() > n {
					tab.log.items.pop_front();
				}
			}
		}
		Task::none()
	}

	pub fn update_split_layout_selected(&mut self, choice: SplitLayoutChoice) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.split_layout = choice.0;
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_drag_modifier_selected(&mut self, choice: ShortcutKeyChoice) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.keybinds.drag_modifier = choice.0;
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_close_key_changed(&mut self, choice: String) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.keybinds.close_key = choice
			.chars()
			.next()
			.map(|c| c.to_ascii_lowercase().to_string())
			.unwrap_or_default();
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_new_key_changed(&mut self, choice: String) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.keybinds.new_key = choice
			.chars()
			.next()
			.map(|c| c.to_ascii_lowercase().to_string())
			.unwrap_or_default();
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_reconnect_key_changed(&mut self, choice: String) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.keybinds.reconnect_key = choice
			.chars()
			.next()
			.map(|c| c.to_ascii_lowercase().to_string())
			.unwrap_or_default();
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_vim_left_key_changed(&mut self, choice: String) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.keybinds.vim_left_key = choice
			.chars()
			.next()
			.map(|c| c.to_ascii_lowercase().to_string())
			.unwrap_or_default();
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_vim_down_key_changed(&mut self, choice: String) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.keybinds.vim_down_key = choice
			.chars()
			.next()
			.map(|c| c.to_ascii_lowercase().to_string())
			.unwrap_or_default();
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_vim_up_key_changed(&mut self, choice: String) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.keybinds.vim_up_key = choice
			.chars()
			.next()
			.map(|c| c.to_ascii_lowercase().to_string())
			.unwrap_or_default();
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_vim_right_key_changed(&mut self, choice: String) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.keybinds.vim_right_key = choice
			.chars()
			.next()
			.map(|c| c.to_ascii_lowercase().to_string())
			.unwrap_or_default();
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_vim_nav_toggled(&mut self, val: bool) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.keybinds.vim_nav = val;
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_theme_selected(&mut self, choice: ThemeChoice) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.theme = choice.0;
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_locale_selected(&mut self, locale: String) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.locale = locale.clone();
		self.state.set_gui_settings(gs);
		rust_i18n::set_locale(&locale);
		Task::none()
	}

	pub fn update_auto_connect_toggled(&mut self, val: bool) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.auto_connect_on_startup = val;
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_open_platform_login(&mut self, platform: Platform) -> Task<Message> {
		let url = match platform {
			Platform::Twitch => chatty_client_core::TWITCH_LOGIN_URL.to_string(),
			Platform::Kick => chatty_client_core::KICK_LOGIN_URL.to_string(),
			_ => String::new(),
		};

		if url.trim().is_empty() {
			return self.toast(t!("settings.no_login_url").to_string());
		}

		if let Err(e) = open::that(url) {
			return self.toast(format!("{}: {}", t!("settings.open_failed"), e));
		}

		Task::none()
	}

	pub fn update_identity_use(&mut self, id: String) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.active_identity = Some(id);
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_identity_toggle(&mut self, id: String) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		if let Some(identity) = gs.identities.iter_mut().find(|i| i.id == id) {
			identity.enabled = !identity.enabled;
			if !identity.enabled && gs.active_identity.as_deref() == Some(identity.id.as_str()) {
				gs.active_identity = None;
			}
			self.state.set_gui_settings(gs);
		}
		Task::none()
	}

	pub fn update_identity_remove(&mut self, id: String) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.identities.retain(|i| i.id != id);
		if gs.active_identity.as_deref() == Some(id.as_str()) {
			gs.active_identity = None;
		}
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_clear_identity(&mut self) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		gs.active_identity = None;
		self.state.set_gui_settings(gs);
		Task::none()
	}

	pub fn update_clipboard_read(&mut self, target: ClipboardTarget, txt: Option<String>) -> Task<Message> {
		let Some(txt) = txt.filter(|s| !s.trim().is_empty()) else {
			return self.toast(t!("clipboard_empty").to_string());
		};
		match target {
			ClipboardTarget::Twitch => self.upsert_identity_from_twitch_blob(txt),
			ClipboardTarget::Kick => self.upsert_identity_from_kick_blob(txt),
		}
	}
}
