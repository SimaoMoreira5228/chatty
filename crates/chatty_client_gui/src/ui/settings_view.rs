#![forbid(unsafe_code)]

use iced::widget::{button, column, container, pick_list, row, rule, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Shadow};
use rust_i18n::t;
include!(concat!(env!("OUT_DIR"), "/locales_generated.rs"));

use crate::app::{Chatty, Message, PlatformChoice, SettingsCategory, ShortcutKeyChoice, SplitLayoutChoice, ThemeChoice};
use crate::theme;
use chatty_domain::Platform;

fn single_char(s: &str) -> String {
	s.chars()
		.next()
		.map(|c| c.to_ascii_lowercase().to_string())
		.unwrap_or_default()
}

pub fn view(app: &Chatty, palette: theme::Palette) -> Element<'_, Message> {
	let current_theme = ThemeChoice(app.state.gui_settings().theme);
	let theme_picker = pick_list(&ThemeChoice::ALL[..], Some(current_theme), Message::ThemeSelected);
	let current_split = SplitLayoutChoice(app.state.gui_settings().split_layout);
	let split_picker = pick_list(&SplitLayoutChoice::ALL[..], Some(current_split), Message::SplitLayoutSelected);
	let keybinds = &app.state.gui_settings().keybinds;
	let drag_modifier_picker = pick_list(
		&ShortcutKeyChoice::ALL[..],
		Some(ShortcutKeyChoice(keybinds.drag_modifier)),
		Message::DragModifierSelected,
	);
	let close_key_input = text_input("q", &keybinds.close_key)
		.on_input(|s: String| Message::CloseKeyChanged(single_char(&s)))
		.width(Length::FillPortion(1));
	let new_key_input = text_input("n", &keybinds.new_key)
		.on_input(|s: String| Message::NewKeyChanged(single_char(&s)))
		.width(Length::FillPortion(1));
	let reconnect_key_input = text_input("r", &keybinds.reconnect_key)
		.on_input(|s: String| Message::ReconnectKeyChanged(single_char(&s)))
		.width(Length::FillPortion(1));

	let close_invalid = keybinds.close_key.trim().is_empty();
	let new_invalid = keybinds.new_key.trim().is_empty();
	let reconnect_invalid = keybinds.reconnect_key.trim().is_empty();
	let vim_left_invalid = keybinds.vim_left_key.trim().is_empty();
	let vim_down_invalid = keybinds.vim_down_key.trim().is_empty();
	let vim_up_invalid = keybinds.vim_up_key.trim().is_empty();
	let vim_right_invalid = keybinds.vim_right_key.trim().is_empty();

	let red = iced::Color::from_rgb(0.9, 0.2, 0.2);
	let platform_picker = pick_list(
		&PlatformChoice::ALL[..],
		Some(PlatformChoice(app.state.gui_settings().default_platform)),
		Message::PlatformSelected,
	);

	let categories = {
		let mut c = column![text(t!("settings.title")).color(palette.text), rule::horizontal(1)].spacing(10);
		for cat in SettingsCategory::ALL {
			let active = cat == app.settings_category;
			let color = if active { palette.text } else { palette.text_dim };
			c = c.push(button(text(t!(cat.label_key())).color(color)).on_press(Message::SettingsCategorySelected(cat)));
		}
		c
	};

	let gs = app.state.gui_settings().clone();
	let right: Element<'_, Message> = match app.settings_category {
		SettingsCategory::General => column![
			text(t!("settings.general")).color(palette.text),
			rule::horizontal(1),
			row![text(t!("settings.theme")).color(palette.text_dim), theme_picker]
				.spacing(12)
				.align_y(Alignment::Center),
			row![text(t!("settings.default_split")).color(palette.text_dim), split_picker]
				.spacing(12)
				.align_y(Alignment::Center),
			row![text(t!("settings.default_platform")).color(palette.text_dim), platform_picker]
				.spacing(12)
				.align_y(Alignment::Center),
			row![
				text(t!("settings.max_log_items")).color(palette.text_dim),
				text_input("2000", &app.max_log_items_raw).on_input(Message::MaxLogItemsChanged),
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![
				text(t!("settings.auto_connect")).color(palette.text_dim),
				iced::widget::checkbox(gs.auto_connect_on_startup).on_toggle(Message::AutoConnectToggled),
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![text(t!("settings.locale")).color(palette.text_dim), {
				let locales: &'static [&'static str] = AVAILABLE_LOCALES;
				let selected: Option<&'static str> = locales
					.iter()
					.copied()
					.find(|s| *s == gs.locale)
					.or_else(|| locales.first().copied());
				pick_list(locales, selected, |s: &'static str| Message::LocaleSelected(s.to_string()))
			}]
			.spacing(12)
			.align_y(Alignment::Center),
		]
		.spacing(12)
		.padding(12)
		.into(),
		SettingsCategory::Keybinds => column![
			text(t!("settings.keybinds")).color(palette.text),
			rule::horizontal(1),
			text(t!("settings.pane_drag_drop")).color(palette.text_dim),
			row![
				text(t!("settings.hold_to_drag")).color(palette.text_dim),
				drag_modifier_picker
			]
			.spacing(12)
			.align_y(Alignment::Center),
			text(t!("settings.split_actions")).color(palette.text_dim),
			row![
				text(t!("settings.close_split")).color(palette.text_dim),
				container(close_key_input).padding(2).style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.surface_bg)),
					border: Border {
						color: if close_invalid { red } else { palette.border },
						width: 1.0,
						radius: 6.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				}),
				if close_invalid {
					text(t!("settings.required")).color(red)
				} else {
					text("")
				}
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![
				text(t!("settings.new_split")).color(palette.text_dim),
				container(new_key_input).padding(2).style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.surface_bg)),
					border: Border {
						color: if new_invalid { red } else { palette.border },
						width: 1.0,
						radius: 6.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				}),
				if new_invalid {
					text(t!("settings.required")).color(red)
				} else {
					text("")
				}
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![
				text(t!("settings.reconnect")).color(palette.text_dim),
				container(reconnect_key_input)
					.padding(2)
					.style(move |_theme| container::Style {
						text_color: Some(palette.text),
						background: Some(Background::Color(palette.surface_bg)),
						border: Border {
							color: if reconnect_invalid { red } else { palette.border },
							width: 1.0,
							radius: 6.0.into(),
						},
						shadow: Shadow::default(),
						snap: false,
					}),
				if reconnect_invalid {
					text(t!("settings.required")).color(red)
				} else {
					text("")
				}
			]
			.spacing(12)
			.align_y(Alignment::Center),
			text(t!("settings.shortcuts_help")).color(palette.text_muted),
			row![
				text(t!("settings.vim_navigation")).color(palette.text_dim),
				iced::widget::checkbox(keybinds.vim_nav).on_toggle(Message::VimNavToggled)
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![
				text(t!("settings.vim_left")).color(palette.text_dim),
				container(
					text_input("h", &keybinds.vim_left_key)
						.on_input(|s: String| Message::VimLeftKeyChanged(single_char(&s)))
						.width(Length::FillPortion(1))
				)
				.padding(2)
				.style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.surface_bg)),
					border: Border {
						color: if vim_left_invalid { red } else { palette.border },
						width: 1.0,
						radius: 6.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				}),
				if vim_left_invalid {
					text(t!("settings.required")).color(red)
				} else {
					text("")
				}
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![
				text(t!("settings.vim_down")).color(palette.text_dim),
				container(
					text_input("j", &keybinds.vim_down_key)
						.on_input(|s: String| Message::VimDownKeyChanged(single_char(&s)))
						.width(Length::FillPortion(1))
				)
				.padding(2)
				.style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.surface_bg)),
					border: Border {
						color: if vim_down_invalid { red } else { palette.border },
						width: 1.0,
						radius: 6.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				}),
				if vim_down_invalid {
					text(t!("settings.required")).color(red)
				} else {
					text("")
				}
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![
				text(t!("settings.vim_up")).color(palette.text_dim),
				container(
					text_input("k", &keybinds.vim_up_key)
						.on_input(|s: String| Message::VimUpKeyChanged(single_char(&s)))
						.width(Length::FillPortion(1))
				)
				.padding(2)
				.style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.surface_bg)),
					border: Border {
						color: if vim_up_invalid { red } else { palette.border },
						width: 1.0,
						radius: 6.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				}),
				if vim_up_invalid {
					text(t!("settings.required")).color(red)
				} else {
					text("")
				}
			]
			.spacing(12)
			.align_y(Alignment::Center),
			row![
				text(t!("settings.vim_right")).color(palette.text_dim),
				container(
					text_input("l", &keybinds.vim_right_key)
						.on_input(|s: String| Message::VimRightKeyChanged(single_char(&s)))
						.width(Length::FillPortion(1))
				)
				.padding(2)
				.style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.surface_bg)),
					border: Border {
						color: if vim_right_invalid { red } else { palette.border },
						width: 1.0,
						radius: 6.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				}),
				if vim_right_invalid {
					text(t!("settings.required")).color(red)
				} else {
					text("")
				}
			]
			.spacing(12)
			.align_y(Alignment::Center),
			text(t!("settings.vim_nav_help")).color(palette.text_muted),
			row![
				button(text(t!("export_layout_title"))).on_press(Message::ExportLayoutPressed),
				button(text(t!("settings.import_layout_clipboard"))).on_press(Message::ImportLayoutPressed),
				button(text(t!("settings.import_from_file"))).on_press(Message::ImportFromFilePressed),
				button(text(t!("settings.reset_layout"))).on_press(Message::ResetLayoutPressed),
			]
			.spacing(8)
			.align_y(Alignment::Center),
			text(t!("settings.export_description")).color(palette.text_muted),
		]
		.spacing(12)
		.padding(12)
		.into(),
		SettingsCategory::Server => {
			let endpoint_locked = chatty_client_core::ClientConfigV1::server_endpoint_locked();
			let default_endpoint = chatty_client_core::ClientConfigV1::default_server_endpoint_quic();

			let hmac_enabled = chatty_client_core::HMAC_ENABLED;
			let hmac_key = chatty_client_core::HMAC_KEY;
			let show_hmac_input = hmac_enabled && hmac_key.is_empty();

			let endpoint_row = if endpoint_locked {
				row![
					text(format!("{} ", t!("settings.endpoint"))).color(palette.text_dim),
					text(default_endpoint).color(palette.text).width(Length::Fill)
				]
			} else {
				row![
					text(format!("{} ", t!("settings.endpoint"))).color(palette.text_dim),
					text_input("quic://host:port", &app.server_endpoint_quic)
						.on_input(Message::ServerEndpointChanged)
						.width(Length::Fill),
				]
			};

			let hmac_row: Option<iced::Element<'_, Message>> = if show_hmac_input {
				Some(
					row![
						text(t!("settings.hmac_token")).color(palette.text_dim),
						text_input(t!("settings.optional").to_string().as_str(), &app.server_auth_token)
							.on_input(Message::ServerAuthTokenChanged)
							.width(Length::Fill),
					]
					.spacing(12)
					.align_y(Alignment::Center)
					.into(),
				)
			} else {
				None
			};

			column![
				text(t!("settings.server")).color(palette.text),
				rule::horizontal(1),
				endpoint_row,
				if let Some(h) = hmac_row { h } else { text("").into() },
				row![
					button(text(t!("settings.connect"))).on_press(Message::ConnectPressed),
					button(text(t!("settings.disconnect"))).on_press(Message::DisconnectPressed),
				]
				.spacing(10),
			]
			.spacing(12)
			.padding(12)
			.into()
		}
		SettingsCategory::Accounts => {
			let mut identities_col = column![].spacing(8);
			for identity in gs.identities.iter() {
				let enabled = if identity.enabled {
					t!("status.enabled")
				} else {
					t!("status.disabled")
				};
				let is_active = gs.active_identity.as_deref() == Some(identity.id.as_str());
				let status = if is_active {
					t!("status.active")
				} else {
					t!("status.inactive")
				};
				identities_col = identities_col.push(
					row![
						text(identity.display_name.clone()).color(palette.text),
						text(format!("{:?}", identity.platform)).color(palette.text_dim),
						text(format!("{status} / {enabled}")).color(palette.text_muted),
						button(text(t!("action.use"))).on_press(Message::IdentityUse(identity.id.clone())),
						button(text(if identity.enabled {
							t!("action.disable")
						} else {
							t!("action.enable")
						}))
						.on_press(Message::IdentityToggle(identity.id.clone())),
						button(text(t!("action.remove"))).on_press(Message::IdentityRemove(identity.id.clone())),
					]
					.spacing(10)
					.align_y(Alignment::Center),
				);
			}

			let twitch_login_cfg = chatty_client_core::TWITCH_LOGIN_URL;
			let kick_login_cfg = chatty_client_core::KICK_LOGIN_URL;

			let mut body_col = column![
				text(t!("settings.accounts")).color(palette.text),
				rule::horizontal(1),
				row![
					text(t!("settings.active_identity")).color(palette.text_dim),
					text(app.active_identity_label()).color(palette.text),
					button(text(t!("action.clear"))).on_press(Message::ClearIdentity),
				]
				.spacing(12)
				.align_y(Alignment::Center),
				rule::horizontal(1),
				text(t!("settings.paste_login_blob_help")).color(palette.text_muted),
			]
			.spacing(12);

			if !twitch_login_cfg.is_empty() {
				body_col = body_col.push(
					row![
						text(t!("settings.twitch_login")).color(palette.text_dim).width(Length::Fill),
						button(text(t!("settings.open_login_page"))).on_press(Message::OpenPlatformLogin(Platform::Twitch)),
						button(text(t!("settings.paste_login_blob"))).on_press(Message::PasteTwitchBlob),
					]
					.spacing(10)
					.align_y(Alignment::Center),
				);
			}

			if !kick_login_cfg.is_empty() {
				body_col = body_col.push(
					row![
						text(t!("settings.kick_login")).color(palette.text_dim).width(Length::Fill),
						button(text(t!("settings.open_login_page"))).on_press(Message::OpenPlatformLogin(Platform::Kick)),
						button(text(t!("settings.paste_login_blob"))).on_press(Message::PasteKickBlob),
					]
					.spacing(10)
					.align_y(Alignment::Center),
				);
			}

			body_col = body_col
				.push(rule::horizontal(1))
				.push(text(t!("settings.identities")).color(palette.text_dim))
				.push(identities_col);

			scrollable(body_col.padding(12)).into()
		}
		SettingsCategory::Diagnostics => {
			let status_text = match &app.state.connection {
				chatty_client_ui::app_state::ConnectionStatus::Disconnected { .. } => t!("status.disconnected").to_string(),
				chatty_client_ui::app_state::ConnectionStatus::Connecting => t!("status.connecting").to_string(),
				chatty_client_ui::app_state::ConnectionStatus::Reconnecting {
					attempt,
					next_retry_in_ms,
				} => format!("{} (attempt {attempt}, {next_retry_in_ms}ms)", t!("status.reconnecting")),
				chatty_client_ui::app_state::ConnectionStatus::Connected { server } => {
					format!("{}: {server}", t!("status.connected"))
				}
			};

			let last_error = app.pending_error.as_ref().cloned().or_else(|| match &app.state.connection {
				chatty_client_ui::app_state::ConnectionStatus::Disconnected { reason } => reason.clone(),
				_ => None,
			});

			let mut notifs = column![].spacing(4);
			for n in app.state.notifications.iter().rev().take(20).rev() {
				notifs = notifs.push(text(format!("{:?}: {}", n.kind, n.message)).color(palette.text_dim));
			}

			scrollable(
				column![
					text(t!("settings.diagnostics")).color(palette.text),
					rule::horizontal(1),
					row![
						text(format!("{} ", t!("settings.endpoint"))).color(palette.text_dim),
						text(app.server_endpoint_quic.clone()).color(palette.text)
					],
					row![
						text(format!("{} ", t!("settings.connection_status"))).color(palette.text_dim),
						text(status_text).color(palette.text)
					],
					row![
						text(format!("{} ", t!("settings.last_error"))).color(palette.text_dim),
						text(last_error.unwrap_or_else(|| "(none)".to_string())).color(palette.text_dim)
					],
					rule::horizontal(1),
					text(t!("settings.recent_notifications")).color(palette.text_dim),
					notifs,
				]
				.spacing(12)
				.padding(12),
			)
			.into()
		}
	};

	row![
		container(categories).width(180).style(move |_theme| container::Style {
			text_color: Some(palette.text),
			background: Some(Background::Color(palette.panel_bg_2)),
			border: Border::default(),
			shadow: Shadow::default(),
			snap: false,
		}),
		container(right)
			.width(Length::Fill)
			.height(Length::Fill)
			.style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(palette.panel_bg)),
				border: Border::default(),
				shadow: Shadow::default(),
				snap: false,
			}),
	]
	.spacing(10)
	.padding(12)
	.into()
}

pub fn modal(app: &Chatty, palette: theme::Palette) -> Option<Element<'_, Message>> {
	if app.pending_export_root.is_some() {
		let path_s = app
			.pending_export_path
			.as_ref()
			.map(|p| p.display().to_string())
			.unwrap_or_else(|| "(no path chosen)".to_string());
		let inner = container(column![
			text(t!("export_layout_title")).color(palette.text).size(16),
			rule::horizontal(1),
			text(format!("{} {}", t!("path_colon"), path_s)).color(palette.text_dim),
			row![
				button(text(t!("choose_path"))).on_press(Message::ChooseExportPathPressed),
				if app.pending_export_path.is_some() {
					button(text(t!("confirm_label"))).on_press(Message::ConfirmExport)
				} else {
					button(text(t!("confirm_label")).color(palette.text_muted))
				},
				button(text(t!("cancel_label"))).on_press(Message::CancelExport),
			]
			.spacing(8)
			.align_y(Alignment::Center),
		])
		.padding(12)
		.style(move |_theme| container::Style {
			text_color: Some(palette.text),
			background: Some(Background::Color(palette.tooltip_bg)),
			border: Border::default(),
			shadow: Shadow::default(),
			snap: false,
		})
		.width(Length::Shrink)
		.height(Length::Shrink);

		let overlay = container(container(inner).center_x(Length::Fill).center_y(Length::Fill))
			.width(Length::Fill)
			.height(Length::Fill)
			.style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.45))),
				border: Border::default(),
				shadow: Shadow::default(),
				snap: false,
			});

		return Some(overlay.into());
	}

	if app.pending_import_root.is_some() {
		let pr = app.pending_import_root.as_ref().unwrap();
		let tabs = pr.tabs.len();
		fn leaf_count(node: &crate::ui::layout::UiNode) -> usize {
			match node {
				crate::ui::layout::UiNode::Leaf(_) => 1,
				crate::ui::layout::UiNode::Split { first, second, .. } => leaf_count(first) + leaf_count(second),
			}
		}

		let leaves = leaf_count(&pr.root);
		let inner = container(column![
			text(t!("import_layout_title")).color(palette.text).size(16),
			rule::horizontal(1),
			text(format!("{} {} leaves, {} tabs", t!("parsed_stats"), leaves, tabs)).color(palette.text_dim),
			row![
				button(text(t!("apply_label"))).on_press(Message::ConfirmImport),
				button(text(t!("cancel_label"))).on_press(Message::CancelImport)
			]
			.spacing(8)
			.align_y(Alignment::Center),
		])
		.padding(12)
		.style(move |_theme| container::Style {
			text_color: Some(palette.text),
			background: Some(Background::Color(palette.tooltip_bg)),
			border: Border::default(),
			shadow: Shadow::default(),
			snap: false,
		})
		.width(Length::Shrink)
		.height(Length::Shrink);

		let overlay = container(container(inner).center_x(Length::Fill).center_y(Length::Fill))
			.width(Length::Fill)
			.height(Length::Fill)
			.style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.45))),
				border: Border::default(),
				shadow: Shadow::default(),
				snap: false,
			});

		return Some(overlay.into());
	}

	if app.pending_reset {
		let inner = container(column![
			text(t!("reset_layout_title")).color(palette.text).size(16),
			rule::horizontal(1),
			text(t!("reset_description")).color(palette.text_dim),
			row![
				button(text(t!("reset_label"))).on_press(Message::ConfirmReset),
				button(text(t!("cancel_label"))).on_press(Message::CancelReset)
			]
			.spacing(8)
			.align_y(Alignment::Center),
		])
		.padding(12)
		.style(move |_theme| container::Style {
			text_color: Some(palette.text),
			background: Some(Background::Color(palette.tooltip_bg)),
			border: Border::default(),
			shadow: Shadow::default(),
			snap: false,
		})
		.width(Length::Shrink)
		.height(Length::Shrink);

		let overlay = container(container(inner).center_x(Length::Fill).center_y(Length::Fill))
			.width(Length::Fill)
			.height(Length::Fill)
			.style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.45))),
				border: Border::default(),
				shadow: Shadow::default(),
				snap: false,
			});

		return Some(overlay.into());
	}

	if let Some(msg) = app.pending_error.as_ref() {
		let inner = container(column![
			text(t!("error_modal_title")).color(palette.text).size(16),
			rule::horizontal(1),
			text(msg.clone()).color(palette.text_dim),
			row![
				button(text(t!("retry_label"))).on_press(Message::ConnectPressed),
				button(text(t!("dismiss_label"))).on_press(Message::CancelError),
			]
			.spacing(8)
			.align_y(Alignment::Center),
		])
		.padding(12)
		.style(move |_theme| container::Style {
			text_color: Some(palette.text),
			background: Some(Background::Color(palette.tooltip_bg)),
			border: Border::default(),
			shadow: Shadow::default(),
			snap: false,
		})
		.width(Length::Shrink)
		.height(Length::Shrink);

		let overlay = container(container(inner).center_x(Length::Fill).center_y(Length::Fill))
			.width(Length::Fill)
			.height(Length::Fill)
			.style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.45))),
				border: Border::default(),
				shadow: Shadow::default(),
				snap: false,
			});

		return Some(overlay.into());
	}

	None
}
