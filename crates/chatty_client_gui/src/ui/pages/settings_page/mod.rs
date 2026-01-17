#![forbid(unsafe_code)]

mod accounts;
mod diagnostics;
mod general;
mod server;
mod sidebar;

use gpui::prelude::*;
use gpui::{Entity, SharedString, Window, div};

use gpui_component::StyledExt;
use gpui_component::input::{InputEvent, InputState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::select::{SelectEvent, SelectState};
use std::hash::{Hash, Hasher};
use std::time::SystemTime;

use crate::ui::app_state::{AppState, ChatItem, SystemNoticeUi, WindowId};
use crate::ui::net::NetController;
use crate::ui::settings;
use crate::ui::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsCategory {
	General,
	Server,
	Accounts,
	Diagnostics,
}

impl SettingsCategory {
	fn all() -> [SettingsCategory; 4] {
		[
			SettingsCategory::General,
			SettingsCategory::Server,
			SettingsCategory::Accounts,
			SettingsCategory::Diagnostics,
		]
	}

	fn label(self) -> &'static str {
		match self {
			SettingsCategory::General => "General",
			SettingsCategory::Server => "Server",
			SettingsCategory::Accounts => "Accounts",
			SettingsCategory::Diagnostics => "Diagnostics",
		}
	}

	fn key(self) -> u64 {
		match self {
			SettingsCategory::General => 1,
			SettingsCategory::Server => 2,
			SettingsCategory::Accounts => 3,
			SettingsCategory::Diagnostics => 4,
		}
	}
}

/// Settings view.
pub struct SettingsPage {
	app_state: Option<Entity<AppState>>,
	bound_window: Option<WindowId>,
	net_controller: Option<NetController>,
	settings: settings::GuiSettings,
	selected_category: SettingsCategory,

	theme_select: Entity<SelectState<Vec<SharedString>>>,
	default_platform_select: Entity<SelectState<Vec<SharedString>>>,
	max_log_items_input: Entity<InputState>,
	server_endpoint_input: Entity<InputState>,
	server_auth_token_input: Entity<InputState>,
	diagnostic_room_input: Entity<InputState>,
}

impl SettingsPage {
	fn twitch_login_url() -> String {
		std::env::var("CHATTY_TWITCH_LOGIN_URL").unwrap_or_else(|_| "https://chatty.example.com/twitch".to_string())
	}

	fn kick_login_url() -> String {
		std::env::var("CHATTY_KICK_LOGIN_URL").unwrap_or_else(|_| "https://chatty.example.com/kick".to_string())
	}

	fn open_url(url: &str) {
		#[cfg(target_os = "linux")]
		let mut cmd = std::process::Command::new("xdg-open");
		#[cfg(target_os = "macos")]
		let mut cmd = std::process::Command::new("open");
		#[cfg(target_os = "windows")]
		let mut cmd = std::process::Command::new("cmd");

		#[cfg(target_os = "windows")]
		let cmd = cmd.arg("/C").arg("start").arg(url);
		#[cfg(not(target_os = "windows"))]
		let cmd = cmd.arg(url);

		let _ = cmd.spawn();
	}

	fn push_system_notice(&self, text: impl Into<String>, cx: &mut Context<Self>) {
		let Some(app) = self.app_state.clone() else {
			return;
		};
		let Some(window_id) = self.bound_window else {
			return;
		};
		let message = text.into();
		app.update(cx, |state, _cx| {
			if let Some(tab_id) = state.windows.get(&window_id).and_then(|w| w.active_tab)
				&& let Some(tab) = state.tabs.get_mut(&tab_id)
			{
				tab.log.push(ChatItem::SystemNotice(SystemNoticeUi {
					time: SystemTime::now(),
					text: message.clone(),
				}));
			}
		});
	}

	fn identity_key(id: &str) -> u64 {
		let mut hasher = std::collections::hash_map::DefaultHasher::new();
		id.hash(&mut hasher);
		hasher.finish()
	}

	fn apply_twitch_blob(&mut self, raw: String, _window: Option<&mut Window>, cx: &mut Context<Self>) {
		self.settings.twitch_oauth_blob = raw.clone();
		if let Some(parsed) = settings::parse_twitch_oauth_blob(&raw) {
			self.settings.twitch_username = parsed.username;
			self.settings.twitch_user_id = parsed.user_id;
			self.settings.twitch_client_id = parsed.client_id;
			self.settings.twitch_oauth_token = parsed.oauth_token.clone();
			self.settings.user_oauth_token = parsed.oauth_token;
			self.upsert_identity_from_settings();
		}

		self.update_settings(cx);
	}

	fn apply_kick_blob(&mut self, raw: String, _window: Option<&mut Window>, cx: &mut Context<Self>) {
		self.settings.kick_oauth_blob = raw.clone();
		if let Some(parsed) = settings::parse_kick_oauth_blob(&raw) {
			self.settings.kick_username = parsed.username;
			self.settings.kick_user_id = parsed.user_id;
			self.settings.kick_oauth_token = parsed.oauth_token;
			self.upsert_kick_identity_from_settings();
		}

		self.update_settings(cx);
	}

	fn upsert_identity_from_settings(&mut self) {
		let username = self.settings.twitch_username.trim().to_string();
		let user_id = self.settings.twitch_user_id.trim().to_string();
		let client_id = self.settings.twitch_client_id.trim().to_string();
		let oauth_token = self.settings.twitch_oauth_token.trim().to_string();
		if username.is_empty() || oauth_token.is_empty() {
			return;
		}
		let id = if !user_id.is_empty() {
			format!("twitch:{}", user_id)
		} else {
			format!("twitch:{}", username)
		};
		let identity = settings::Identity {
			id: id.clone(),
			display_name: username.clone(),
			platform: chatty_domain::Platform::Twitch,
			username,
			user_id,
			oauth_token,
			client_id,
			enabled: true,
		};
		if let Some(existing) = self.settings.identities.iter_mut().find(|i| i.id == id) {
			*existing = identity;
		} else {
			self.settings.identities.push(identity);
		}
		self.settings.active_identity = Some(id);
	}

	fn upsert_kick_identity_from_settings(&mut self) {
		let username = self.settings.kick_username.trim().to_string();
		let user_id = self.settings.kick_user_id.trim().to_string();
		let oauth_token = self.settings.kick_oauth_token.trim().to_string();
		if username.is_empty() || oauth_token.is_empty() {
			return;
		}
		let id = if !user_id.is_empty() {
			format!("kick:{}", user_id)
		} else {
			format!("kick:{}", username)
		};
		let identity = settings::Identity {
			id: id.clone(),
			display_name: username.clone(),
			platform: chatty_domain::Platform::Kick,
			username,
			user_id,
			oauth_token,
			client_id: String::new(),
			enabled: true,
		};
		if let Some(existing) = self.settings.identities.iter_mut().find(|i| i.id == id) {
			*existing = identity;
		} else {
			self.settings.identities.push(identity);
		}
		self.settings.active_identity = Some(id);
	}

	fn active_identity_label(&self) -> String {
		if let Some(id) = self.settings.active_identity.as_ref()
			&& let Some(identity) = self.settings.identities.iter().find(|i| &i.id == id)
		{
			return format!("{} ({})", identity.display_name, identity.platform);
		}
		"None".to_string()
	}

	fn parse_rooms_list(&self, raw: &str) -> Vec<chatty_domain::RoomKey> {
		let mut rooms = Vec::new();
		for entry in raw.split([',', '\n']) {
			let item = entry.trim();
			if item.is_empty() {
				continue;
			}
			if let Ok(room) = chatty_domain::RoomKey::parse(item) {
				rooms.push(room);
				continue;
			}
			let default_platform = self.settings.default_platform;
			if let Ok(room_id) = chatty_domain::RoomId::new(item.to_string()) {
				rooms.push(chatty_domain::RoomKey::new(default_platform, room_id));
			}
		}
		rooms
	}

	fn sync_groups_into_state(&self, cx: &mut Context<Self>) {
		let Some(app) = self.app_state.clone() else {
			return;
		};
		let groups = self.settings.groups.clone();
		app.update(cx, |state, _cx| {
			state.settings.groups = groups;
			state.sync_groups_from_settings();
		});
	}

	fn paste_twitch_blob(&mut self, window: &mut Window, cx: &mut Context<Self>) {
		if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
			self.apply_twitch_blob(text.to_string(), Some(window), cx);
		}
	}

	fn paste_kick_blob(&mut self, window: &mut Window, cx: &mut Context<Self>) {
		if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
			self.apply_kick_blob(text.to_string(), Some(window), cx);
		}
	}

	fn connect_now(&mut self, cx: &mut Context<Self>) {
		self.push_system_notice("Diagnostics: connect requested", cx);
		let Some(net) = self.net_controller.clone() else {
			return;
		};
		let app_state = self.app_state.clone();
		let settings_snapshot = self.settings.clone();
		cx.spawn(async move |_, cx| {
			let cfg = match settings::build_client_config(&settings_snapshot) {
				Ok(cfg) => cfg,
				Err(err) => {
					if let Some(app) = app_state.clone() {
						app.update(cx, |state, _cx| {
							state.push_notification(crate::ui::app_state::UiNotificationKind::Error, err);
						});
					}
					return;
				}
			};
			if let Err(err) = net.connect(cfg).await
				&& let Some(app) = app_state
			{
				app.update(cx, |state, _cx| {
					state.push_notification(crate::ui::app_state::UiNotificationKind::Error, err);
				});
			}
		})
		.detach();
	}

	fn disconnect_now(&mut self, cx: &mut Context<Self>) {
		self.push_system_notice("Diagnostics: disconnect requested", cx);
		let Some(net) = self.net_controller.clone() else {
			return;
		};
		let app_state = self.app_state.clone();
		cx.spawn(async move |_, cx| {
			if let Err(err) = net.disconnect("user disconnect").await
				&& let Some(app) = app_state
			{
				app.update(cx, |state, _cx| {
					state.push_notification(crate::ui::app_state::UiNotificationKind::Error, err);
				});
			}
		})
		.detach();
	}

	fn inject_lag_marker(&mut self, cx: &mut Context<Self>) {
		let Some(app) = self.app_state.clone() else {
			return;
		};
		let raw = self.diagnostic_room_input.read(cx).value().to_string();
		let rooms = self.parse_rooms_list(&raw);
		let dropped = 128;
		if rooms.is_empty() {
			self.push_system_notice("Diagnostics: no rooms specified for lag marker", cx);
		}
		app.update(cx, |state, _cx| {
			for room in &rooms {
				state.push_lagged(room, dropped, Some("diagnostic lag marker".to_string()));
			}
		});
	}

	pub fn new(
		window: &mut Window,
		cx: &mut Context<Self>,
		app_state: Option<Entity<AppState>>,
		bound_window: Option<WindowId>,
		net_controller: Option<NetController>,
	) -> Self {
		let current_settings = settings::get_cloned();

		let theme_select = cx.new(|cx| {
			let mut state = SelectState::new(
				vec![
					"dark".into(),
					"light".into(),
					"solar".into(),
					"high".into(),
					"ocean".into(),
					"dracula".into(),
					"gruvbox".into(),
					"nord".into(),
					"synthwave".into(),
					"dark amethyst".into(),
				],
				None,
				window,
				cx,
			);
			match current_settings.theme {
				theme::ThemeKind::Dark => state.set_selected_value(&"dark".into(), window, cx),
				theme::ThemeKind::Light => state.set_selected_value(&"light".into(), window, cx),
				theme::ThemeKind::Solarized => state.set_selected_value(&"solar".into(), window, cx),
				theme::ThemeKind::Gruvbox => state.set_selected_value(&"gruvbox".into(), window, cx),
				theme::ThemeKind::HighContrast => state.set_selected_value(&"high".into(), window, cx),
				theme::ThemeKind::Ocean => state.set_selected_value(&"ocean".into(), window, cx),
				theme::ThemeKind::Dracula => state.set_selected_value(&"dracula".into(), window, cx),
				theme::ThemeKind::Nord => state.set_selected_value(&"nord".into(), window, cx),
				theme::ThemeKind::Synthwave => state.set_selected_value(&"synthwave".into(), window, cx),
				theme::ThemeKind::DarkAmethyst => state.set_selected_value(&"dark amethyst".into(), window, cx),
			}
			state
		});

		let default_platform_select = cx.new(|cx| {
			let mut state = SelectState::new(vec!["twitch".into(), "kick".into()], None, window, cx);
			match current_settings.default_platform {
				chatty_domain::Platform::Twitch => state.set_selected_value(&"twitch".into(), window, cx),
				chatty_domain::Platform::Kick => state.set_selected_value(&"kick".into(), window, cx),
				chatty_domain::Platform::YouTube => state.set_selected_value(&"youtube".into(), window, cx),
			}
			state
		});

		let max_log_items_input =
			cx.new(|cx| InputState::new(window, cx).default_value(current_settings.max_log_items.to_string()));

		let server_endpoint_input = cx.new(|cx| {
			InputState::new(window, cx)
				.default_value(current_settings.server_endpoint_quic.clone())
				.placeholder("quic://host:port")
		});
		let server_auth_token_input = cx.new(|cx| {
			InputState::new(window, cx)
				.default_value(current_settings.server_auth_token.clone())
				.placeholder("auth token (optional)")
		});
		let diagnostic_room_input = cx.new(|cx| InputState::new(window, cx).placeholder("room:twitch/demo or twitch:demo"));

		Self {
			app_state,
			bound_window,
			net_controller,
			settings: current_settings,
			selected_category: SettingsCategory::General,
			theme_select,
			default_platform_select,
			max_log_items_input,
			server_endpoint_input,
			server_auth_token_input,
			diagnostic_room_input,
		}
	}

	fn update_settings(&mut self, cx: &mut Context<Self>) {
		settings::set_and_persist(self.settings.clone());
		self.sync_groups_into_state(cx);
		cx.notify();
	}
}

fn parse_confirm_theme(event: &SelectEvent<Vec<SharedString>>) -> Option<theme::ThemeKind> {
	let SelectEvent::Confirm(value_opt) = event;
	if let Some(v) = value_opt {
		let kind = match v.as_ref() {
			"dark" => theme::ThemeKind::Dark,
			"light" => theme::ThemeKind::Light,
			"solar" => theme::ThemeKind::Solarized,
			"high" => theme::ThemeKind::HighContrast,
			"ocean" => theme::ThemeKind::Ocean,
			"dracula" => theme::ThemeKind::Dracula,
			"gruvbox" => theme::ThemeKind::Gruvbox,
			"nord" => theme::ThemeKind::Nord,
			"synthwave" => theme::ThemeKind::Synthwave,
			"dark amethyst" => theme::ThemeKind::DarkAmethyst,
			_ => theme::ThemeKind::DarkAmethyst,
		};
		return Some(kind);
	}
	None
}

fn parse_confirm_platform(event: &SelectEvent<Vec<SharedString>>) -> Option<chatty_domain::Platform> {
	let SelectEvent::Confirm(value_opt) = event;
	if let Some(v) = value_opt {
		let platform = match v.as_ref() {
			"twitch" => chatty_domain::Platform::Twitch,
			"kick" => chatty_domain::Platform::Kick,
			"youtube" => chatty_domain::Platform::YouTube,
			_ => chatty_domain::Platform::Twitch,
		};
		return Some(platform);
	}
	None
}

impl gpui::Render for SettingsPage {
	fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
		let t = theme::theme();
		let preview_t = theme::theme_with_kind(self.settings.theme);

		{
			let ent = self.theme_select.clone();
			cx.subscribe_in(&ent, window, |this, _state, event, _window, cx| {
				if let Some(kind) = parse_confirm_theme(event) {
					this.settings.theme = kind;
					this.update_settings(cx);
					theme::sync_component_theme(cx, kind);
				}
			})
			.detach();
		}

		{
			let ent = self.default_platform_select.clone();
			cx.subscribe_in(&ent, window, |this, _state, event, _window, cx| {
				if let Some(platform) = parse_confirm_platform(event) {
					this.settings.default_platform = platform;
					this.update_settings(cx);
				}
			})
			.detach();
		}

		{
			let ent = self.max_log_items_input.clone();
			cx.subscribe_in(&ent, window, |this, state: &Entity<InputState>, event, _window, cx| {
				if let InputEvent::Change = event {
					let raw = state.read(cx).value().to_string();
					if let Ok(value) = raw.trim().parse::<usize>() {
						this.settings.max_log_items = value;
						this.update_settings(cx);
					}
				}
			})
			.detach();
		}

		{
			let ent = self.server_endpoint_input.clone();
			cx.subscribe_in(&ent, window, |this, state: &Entity<InputState>, event, _window, cx| {
				if let InputEvent::Change = event {
					if chatty_client_core::ClientConfigV1::server_endpoint_locked() {
						return;
					}
					this.settings.server_endpoint_quic = state.read(cx).value().to_string();
					this.update_settings(cx);
				}
			})
			.detach();
		}

		{
			let ent = self.server_auth_token_input.clone();
			cx.subscribe_in(&ent, window, |this, state: &Entity<InputState>, event, _window, cx| {
				if let InputEvent::Change = event {
					this.settings.server_auth_token = state.read(cx).value().to_string();
					this.update_settings(cx);
				}
			})
			.detach();
		}

		let endpoint_locked = chatty_client_core::ClientConfigV1::server_endpoint_locked();
		let selected_category = self.selected_category;

		let content: gpui::Div = match selected_category {
			SettingsCategory::General => general::render(self, &t, &preview_t),
			SettingsCategory::Server => server::render(self, &t, endpoint_locked, cx),
			SettingsCategory::Accounts => accounts::render(self, &t, cx),
			SettingsCategory::Diagnostics => diagnostics::render(self, &t, window, cx),
		};

		let sidebar = sidebar::render(self, &t, window, cx);

		let content = div()
			.id("settings-page-content")
			.p_4()
			.flex()
			.flex_col()
			.gap_4()
			.child(
				div()
					.flex()
					.flex_row()
					.items_center()
					.justify_between()
					.child(div().text_lg().font_semibold().child("Settings"))
					.child(div().text_sm().text_color(t.text_dim).child(selected_category.label())),
			)
			.child(content);

		div().id("settings-page").size_full().bg(t.app_bg).text_color(t.text).child(
			div()
				.size_full()
				.flex()
				.flex_row()
				.child(sidebar)
				.child(div().flex_1().overflow_y_scrollbar().child(content)),
		)
	}
}
