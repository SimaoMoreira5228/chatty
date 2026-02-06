#![forbid(unsafe_code)]

use iced::Task;

use crate::app::message::Message;
use crate::app::model::Chatty;
use crate::app::types::{
	ClipboardTarget, PlatformChoice, SettingsCategory, ShortcutKeyChoice, SplitLayoutChoice, ThemeChoice,
};

#[derive(Debug, Clone)]
pub enum SettingsMessage {
	CategorySelected(SettingsCategory),
	ServerEndpointChanged(String),
	ServerAuthTokenChanged(String),
	MaxLogItemsChanged(String),
	ThemeSelected(ThemeChoice),
	PlatformSelected(PlatformChoice),
	SplitLayoutSelected(SplitLayoutChoice),
	DragModifierSelected(ShortcutKeyChoice),
	CloseKeyChanged(String),
	NewKeyChanged(String),
	ReconnectKeyChanged(String),
	VimNavToggled(bool),
	VimLeftKeyChanged(String),
	VimDownKeyChanged(String),
	VimUpKeyChanged(String),
	VimRightKeyChanged(String),
	LocaleSelected(String),
	AutoConnectToggled(bool),
	IdentityRemove(String),
	OpenPlatformLogin(chatty_domain::Platform),
	PasteTwitchBlob,
	PasteKickBlob,
	IdentityClipboardRead(ClipboardTarget, Option<String>),
	ExportLayoutPressed,
	ImportLayoutPressed,
	ImportFromFilePressed,
	ResetLayoutPressed,
}

#[derive(Debug, Clone)]
pub struct SettingsView {
	pub category: SettingsCategory,
}

impl SettingsView {
	pub fn new(category: SettingsCategory) -> Self {
		Self { category }
	}

	pub fn update(&mut self, app: &mut Chatty, message: SettingsMessage) -> Task<Message> {
		match message {
			SettingsMessage::CategorySelected(cat) => {
				self.category = cat;
				Task::none()
			}
			SettingsMessage::ServerEndpointChanged(v) => app.update_server_endpoint_changed(v),
			SettingsMessage::ServerAuthTokenChanged(v) => app.update_server_auth_token_changed(v),
			SettingsMessage::MaxLogItemsChanged(v) => app.update_max_log_items_changed(v),
			SettingsMessage::ThemeSelected(choice) => app.update_theme_selected(choice),
			SettingsMessage::PlatformSelected(choice) => app.update_platform_selected(choice),
			SettingsMessage::SplitLayoutSelected(choice) => app.update_split_layout_selected(choice),
			SettingsMessage::DragModifierSelected(choice) => app.update_drag_modifier_selected(choice),
			SettingsMessage::CloseKeyChanged(v) => app.update_close_key_changed(v),
			SettingsMessage::NewKeyChanged(v) => app.update_new_key_changed(v),
			SettingsMessage::ReconnectKeyChanged(v) => app.update_reconnect_key_changed(v),
			SettingsMessage::VimNavToggled(v) => app.update_vim_nav_toggled(v),
			SettingsMessage::VimLeftKeyChanged(v) => app.update_vim_left_key_changed(v),
			SettingsMessage::VimDownKeyChanged(v) => app.update_vim_down_key_changed(v),
			SettingsMessage::VimUpKeyChanged(v) => app.update_vim_up_key_changed(v),
			SettingsMessage::VimRightKeyChanged(v) => app.update_vim_right_key_changed(v),
			SettingsMessage::LocaleSelected(v) => app.update_locale_selected(v),
			SettingsMessage::AutoConnectToggled(v) => app.update_auto_connect_toggled(v),
			SettingsMessage::IdentityRemove(id) => app.update_identity_remove(id),
			SettingsMessage::OpenPlatformLogin(p) => app.update_open_platform_login(p),
			SettingsMessage::PasteTwitchBlob => iced::clipboard::read()
				.map(|txt| Message::Settings(SettingsMessage::IdentityClipboardRead(ClipboardTarget::Twitch, txt))),
			SettingsMessage::PasteKickBlob => iced::clipboard::read()
				.map(|txt| Message::Settings(SettingsMessage::IdentityClipboardRead(ClipboardTarget::Kick, txt))),
			SettingsMessage::IdentityClipboardRead(target, txt) => app.update_clipboard_read(target, txt),
			SettingsMessage::ExportLayoutPressed => app.update_export_layout_pressed(),
			SettingsMessage::ImportLayoutPressed => app.update_import_layout_pressed(),
			SettingsMessage::ImportFromFilePressed => app.update_import_from_file_pressed(),
			SettingsMessage::ResetLayoutPressed => app.update_reset_layout_pressed(),
		}
	}
}
