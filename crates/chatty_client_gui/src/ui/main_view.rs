#![forbid(unsafe_code)]

use chatty_client_ui::app_state::{ChatItem, ConnectionStatus, TabTarget};
use iced::widget::pane_grid::PaneGrid;
use iced::widget::{
	button, column, container, image, mouse_area, opaque, pane_grid, row, rule, scrollable, space, stack, text, text_editor,
	text_input,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow};

use crate::app::{Chatty, InsertTarget, Message, PaneState, PendingCommand};
use crate::theme;

fn join_row(
	pane: pane_grid::Pane,
	pane_state: &PaneState,
	is_active: bool,
	palette: theme::Palette,
) -> Element<'static, Message> {
	let join_placeholder = t!("main.join_placeholder").to_string();
	let mut join = text_input(&join_placeholder, &pane_state.join_raw)
		.on_input(move |v| Message::PaneJoinChanged(pane, v))
		.width(Length::Fill)
		.id(format!("join-{:?}", pane));
	join = join.on_submit(Message::PaneJoinPressed(pane));

	let input = row![join].spacing(4).align_y(Alignment::Center);
	let input = container(input).style(move |_theme| container::Style {
		text_color: Some(palette.text),
		background: Some(Background::Color(if is_active {
			palette.panel_bg_2
		} else {
			palette.chat_bg
		})),
		border: Border {
			color: if is_active { palette.accent_blue } else { palette.border },
			width: 1.0,
			radius: 6.0.into(),
		},
		shadow: Shadow::default(),
		snap: false,
	});

	row![
		input,
		button(text(t!("main.join_button"))).on_press(Message::PaneJoinPressed(pane))
	]
	.spacing(8)
	.align_y(Alignment::Center)
	.into()
}

pub fn view(app: &Chatty, palette: theme::Palette) -> Element<'_, Message> {
	let panes = {
		let view = |pane: pane_grid::Pane, pane_state: &PaneState, _is_focused: bool| {
			let is_focused = pane == app.focused_pane;
			let title = pane_state
				.tab_id
				.and_then(|tid| app.state.tabs.get(&tid).map(|t| t.title.clone()))
				.unwrap_or_else(|| t!("main.welcome").to_string());

			let title_color = if is_focused { palette.text } else { palette.text_dim };
			let title_bar = pane_grid::TitleBar::new(text(title).color(title_color)).padding(6);

			let body: Element<'_, Message> = match pane_state.tab_id.and_then(|tid| app.state.tabs.get(&tid)) {
				Some(tab) => {
					let room_key = match &tab.target {
						TabTarget::Room(room) => Some(room.clone()),
						_ => None,
					};
					let can_send = room_key
						.as_ref()
						.and_then(|rk| app.state.room_permissions.get(rk))
						.map(|p| p.can_send)
						.unwrap_or(true);
					let room_state = room_key.as_ref().and_then(|rk| app.state.room_states.get(rk));
					let connected = matches!(app.state.connection, ConnectionStatus::Connected { .. });
					let can_compose = connected && room_key.is_some() && can_send;

					let mut col = column![].spacing(4);
					let anim_elapsed = app.animation_clock.duration_since(app.animation_start);
					let start_index = tab.log.items.len().saturating_sub(300);

					for (_idx, item) in tab.log.items.iter().enumerate().skip(start_index) {
						match item {
							ChatItem::ChatMessage(m) => {
								let name = m.user_display.clone().unwrap_or_else(|| m.user_login.clone());

								let mut msg_row = row![].spacing(6).align_y(Alignment::Center);
								if !m.badge_ids.is_empty() {
									let mut rendered_badges: Vec<String> = Vec::new();

									if let Some(keys) = app.state.room_asset_cache_keys.get(&m.room) {
										for cache_key in keys {
											if let Some(bundle) = app.state.asset_bundles.get(cache_key) {
												for badge in &bundle.badges {
													if m.badge_ids.iter().any(|id| id == &badge.id) {
														rendered_badges.push(badge.id.clone());
														let animated = {
															let mut cache = app.animated_cache.lock().unwrap();
															cache
																.get(&badge.image_url)
																.and_then(|anim| anim.frame_at(anim_elapsed).cloned())
														};
														if let Some(handle) = animated {
															msg_row = msg_row.push(image(handle).width(18).height(18));
														} else {
															let mut icache = app.image_cache.lock().unwrap();
															if let Some(handle) = icache.get(&badge.image_url) {
																msg_row =
																	msg_row.push(image(handle.clone()).width(18).height(18));
															} else {
																let loading = app
																	.image_loading
																	.lock()
																	.unwrap()
																	.contains(&badge.image_url);
																let failed = app
																	.image_failed
																	.lock()
																	.unwrap()
																	.contains(&badge.image_url);
																drop(icache);
																if loading {
																	msg_row =
																		msg_row.push(text("◌").color(palette.text_muted));
																} else if failed {
																	msg_row =
																		msg_row.push(text("[x]").color(palette.system_text));
																} else {
																	let _ = app
																		.image_fetch_sender
																		.try_send(badge.image_url.clone());
																	msg_row = msg_row.push(
																		text(format!("[{}]", badge.name))
																			.color(palette.text_dim),
																	);
																}
															}
														}
													}
												}
											}
										}
									}

									for cache_key in &app.state.global_asset_cache_keys {
										if let Some(bundle) = app.state.asset_bundles.get(cache_key) {
											for badge in &bundle.badges {
												if rendered_badges.contains(&badge.id) {
													continue;
												}
												if m.badge_ids.iter().any(|id| id == &badge.id) {
													let animated = {
														let mut cache = app.animated_cache.lock().unwrap();
														cache
															.get(&badge.image_url)
															.and_then(|anim| anim.frame_at(anim_elapsed).cloned())
													};
													if let Some(handle) = animated {
														msg_row = msg_row.push(image(handle).width(18).height(18));
													} else {
														let mut icache = app.image_cache.lock().unwrap();
														if let Some(handle) = icache.get(&badge.image_url) {
															msg_row =
																msg_row.push(image(handle.clone()).width(18).height(18));
														} else {
															let loading =
																app.image_loading.lock().unwrap().contains(&badge.image_url);
															let failed =
																app.image_failed.lock().unwrap().contains(&badge.image_url);
															drop(icache);
															if loading {
																msg_row = msg_row.push(text("◌").color(palette.text_muted));
															} else if failed {
																msg_row =
																	msg_row.push(text("[x]").color(palette.system_text));
															} else {
																let _ =
																	app.image_fetch_sender.try_send(badge.image_url.clone());
																msg_row = msg_row.push(
																	text(format!("[{}]", badge.name))
																		.color(palette.text_dim),
																);
															}
														}
													}
												}
											}
										}
									}
								}

								let name_txt = text(name).color(palette.chat_nick);
								let mut content_row = row![].spacing(4).align_y(Alignment::Center);
								let inline_emote = |token: &str| m.emotes.iter().find(|emote| emote.name == token);
								let tokens: Vec<&str> = m.text.split_whitespace().collect();
								for (i, token) in tokens.iter().enumerate() {
									if i > 0 {
										content_row = content_row.push(text(" ").color(if is_focused {
											palette.text
										} else {
											palette.text_dim
										}));
									}

									let mut found_emote: Option<chatty_client_ui::app_state::AssetRefUi> =
										inline_emote(token).cloned();
									if let Some(keys) = app.state.room_asset_cache_keys.get(&m.room) {
										for cache_key in keys {
											if let Some(bundle) = app.state.asset_bundles.get(cache_key) {
												for em in &bundle.emotes {
													if em.name == *token {
														found_emote = Some(em.clone());
														break;
													}
												}
											}
											if found_emote.is_some() {
												break;
											}
										}
									}

									if found_emote.is_none() {
										for cache_key in &app.state.global_asset_cache_keys {
											if let Some(bundle) = app.state.asset_bundles.get(cache_key) {
												for em in &bundle.emotes {
													if em.name == *token {
														found_emote = Some(em.clone());
														break;
													}
												}
											}
											if found_emote.is_some() {
												break;
											}
										}
									}

									let token_el: Element<'_, Message> = if let Some(emote) = found_emote {
										let animated = {
											let mut cache = app.animated_cache.lock().unwrap();
											cache
												.get(&emote.image_url)
												.and_then(|anim| anim.frame_at(anim_elapsed).cloned())
										};

										if let Some(handle) = animated {
											image(handle).width(20).height(20).into()
										} else {
											let mut icache = app.image_cache.lock().unwrap();
											if let Some(handle) = icache.get(&emote.image_url) {
												image(handle.clone()).width(20).height(20).into()
											} else {
												let loading = app.image_loading.lock().unwrap().contains(&emote.image_url);
												let failed = app.image_failed.lock().unwrap().contains(&emote.image_url);
												drop(icache);
												if loading {
													text("◌").color(palette.text_muted).into()
												} else if failed {
													text(format!("[{}]", emote.name)).color(palette.system_text).into()
												} else {
													let _ = app.image_fetch_sender.try_send(emote.image_url.clone());
													text(format!("[{}]", emote.name)).color(palette.text_dim).into()
												}
											}
										}
									} else {
										text(*token)
											.color(if is_focused { palette.text } else { palette.text_dim })
											.into()
									};

									content_row = content_row.push(token_el);
								}

								let content_row = content_row.width(Length::Fill).wrap();
								let message_key = Chatty::message_key(m);
								let content_block: Element<'_, Message> =
									if let Some(content) = app.message_text_editors.get(&message_key) {
										let key = message_key.clone();
										let overlay = text_editor(content)
											.on_action(move |action| Message::MessageTextEdit(key.clone(), action))
											.style(move |_theme, _status| text_editor::Style {
												background: Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.0)),
												border: Border::default(),
												placeholder: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
												value: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
												selection: Color::from_rgba(0.3, 0.6, 1.0, 0.35),
											});

										let overlay = container(overlay).width(Length::Fill).height(Length::Shrink);
										stack([content_row.into(), overlay.into()]).width(Length::Fill).into()
									} else {
										content_row.into()
									};

								msg_row = msg_row
									.push(name_txt)
									.push(text(": ").color(if is_focused { palette.text } else { palette.text_dim }))
									.push(content_block)
									.width(Length::Fill);
								let is_pending = app.pending_commands.iter().any(|pc| match pc {
									PendingCommand::Delete {
										room: r,
										server_message_id: s,
										platform_message_id: p,
									} => {
										(r == &m.room)
											&& s.as_ref() == m.server_message_id.as_ref()
											&& p.as_ref() == m.platform_message_id.as_ref()
									}
									_ => false,
								});
								if is_pending {
									msg_row = msg_row.push(text(" ⏳").color(palette.text_muted));
								}

								let msg_block = mouse_area(msg_row).on_right_press(Message::MessageActionButtonPressed(
									m.room.clone(),
									m.server_message_id.clone(),
									m.platform_message_id.clone(),
									m.author_id.clone(),
								));
								col = col.push(msg_block);
							}
							ChatItem::SystemNotice(n) => {
								col = col
									.push(text(format!("{} {}", t!("log.system_label"), n.text)).color(palette.system_text));
							}
							ChatItem::Lagged(l) => {
								col = col.push(
									text(format!(
										"{} dropped={} {}",
										t!("log.lagged_label"),
										l.dropped,
										l.detail.clone().unwrap_or_default()
									))
									.color(palette.system_text),
								);
							}
						}
					}

					let end_marker = container(text("")).id(format!("end-{:?}", pane));
					let col = col.push(end_marker);

					let log_id = format!("log-{:?}", pane);
					let log = scrollable(col).id(log_id).height(Length::Fill).width(Length::Fill);

					let is_join_active =
						app.insert_mode && app.insert_target == Some(InsertTarget::Join) && pane == app.focused_pane;
					let join_row = join_row(pane, pane_state, is_join_active, palette);

					let mut restrictions: Vec<String> = Vec::new();
					if let Some(state) = room_state {
						if state.emote_only == Some(true) {
							restrictions.push(t!("main.room_state_emote_only").to_string());
						}
						if state.subscribers_only == Some(true) {
							restrictions.push(t!("main.room_state_subscribers_only").to_string());
						}
						if state.unique_chat == Some(true) {
							restrictions.push(t!("main.room_state_unique_chat").to_string());
						}
						if state.followers_only == Some(true) {
							let label = if let Some(minutes) = state.followers_only_duration_minutes {
								format!("{} {}m", t!("main.room_state_followers_only"), minutes)
							} else {
								t!("main.room_state_followers_only").to_string()
							};
							restrictions.push(label);
						}
						if state.slow_mode == Some(true) {
							let label = if let Some(wait) = state.slow_mode_wait_time_seconds {
								format!("{} {}s", t!("main.room_state_slow_mode"), wait)
							} else {
								t!("main.room_state_slow_mode").to_string()
							};
							restrictions.push(label);
						}
					}

					let placeholder = if !connected {
						t!("main.placeholder_connect_to_send").to_string()
					} else if room_key.is_none() {
						t!("main.placeholder_no_active_room").to_string()
					} else if !can_send {
						t!("main.placeholder_no_permission").to_string()
					} else if !restrictions.is_empty() {
						format!("{} ({})", t!("main.placeholder_message"), restrictions.join(", "))
					} else {
						t!("main.placeholder_message").to_string()
					};

					let is_composer_active =
						app.insert_mode && app.insert_target == Some(InsertTarget::Composer) && pane == app.focused_pane;
					let mut input = text_input(&placeholder, &pane_state.composer)
						.on_input(move |v| Message::PaneComposerChanged(pane, v))
						.width(Length::Fill)
						.id(format!("composer-{:?}", pane));
					if can_compose {
						input = input.on_submit(Message::PaneSendPressed(pane));
					}

					let send_btn = if can_compose {
						button(text(t!("main.send_label"))).on_press(Message::PaneSendPressed(pane))
					} else {
						button(text(t!("main.send_label")).color(palette.text_muted))
					};

					let input_and_caret =
						container(row![input].spacing(4).align_y(Alignment::Center)).style(move |_theme| container::Style {
							text_color: Some(palette.text),
							background: Some(Background::Color(if is_composer_active {
								palette.panel_bg_2
							} else {
								palette.chat_bg
							})),
							border: Border {
								color: if is_composer_active {
									palette.accent_blue
								} else {
									palette.border
								},
								width: 1.0,
								radius: 6.0.into(),
							},
							shadow: Shadow::default(),
							snap: false,
						});

					let composer = row![input_and_caret, send_btn].spacing(8).align_y(Alignment::Center);

					column![log, rule::horizontal(1), join_row, composer]
						.spacing(8)
						.padding(8)
						.into()
				}
				None => {
					let connected = matches!(app.state.connection, ConnectionStatus::Connected { .. });
					let is_join_active =
						app.insert_mode && app.insert_target == Some(InsertTarget::Join) && pane == app.focused_pane;
					let join_row = join_row(pane, pane_state, is_join_active, palette);
					let placeholder = if !connected {
						t!("main.placeholder_connect_to_send").to_string()
					} else {
						t!("main.placeholder_no_active_room").to_string()
					};

					let input = text_input(&placeholder, &pane_state.composer)
						.on_input(move |v| Message::PaneComposerChanged(pane, v))
						.width(Length::Fill)
						.id(format!("composer-{:?}", pane));
					let send_btn = button(text(t!("main.send_label")).color(palette.text_muted));
					let composer = row![input, send_btn].spacing(8).align_y(Alignment::Center);

					let info = column![
						text(t!("main.info_join_begin")).color(palette.text_dim),
						text(t!("main.info_use_join_field")).color(palette.text_muted),
						text(t!("main.info_split_button")).color(palette.text_muted),
					]
					.spacing(8)
					.padding(12);

					let end_marker = container(text("")).id(format!("end-{:?}", pane));
					let col = info.push(end_marker);
					let log_id = format!("log-{:?}", pane);
					let log = scrollable(col).id(log_id).height(Length::Fill).width(Length::Fill);
					column![log, rule::horizontal(1), join_row, composer]
						.spacing(8)
						.padding(8)
						.into()
				}
			};

			let pane_body = container(body)
				.width(Length::Fill)
				.height(Length::Fill)
				.style(move |_theme| container::Style {
					text_color: Some(palette.text),
					background: Some(Background::Color(palette.chat_bg)),
					border: Border {
						color: palette.border,
						width: 1.0,
						radius: 8.0.into(),
					},
					shadow: Shadow::default(),
					snap: false,
				});

			pane_grid::Content::new(pane_body).title_bar(title_bar)
		};

		let mut grid = PaneGrid::new(&app.panes, view)
			.width(Length::Fill)
			.height(Length::Fill)
			.spacing(8)
			.on_click(Message::PaneClicked)
			.on_resize(10, Message::PaneResized);

		if app.pane_drag_enabled() {
			grid = grid.on_drag(Message::PaneDragged);
		}

		grid
	};

	container(panes).width(Length::Fill).height(Length::Fill).padding(12).into()
}

pub fn message_action_menu(app: &Chatty, palette: theme::Palette) -> Option<Element<'_, Message>> {
	let active = app.pending_message_action.as_ref()?;
	let (win_w, win_h) = app.window_size.unwrap_or((800.0, 600.0));
	let (raw_x, raw_y) = active
		.cursor_pos
		.or(app.last_cursor_pos)
		.unwrap_or((win_w * 0.5, win_h * 0.5));

	let menu_w = 220.0;
	let menu_h = 160.0;
	let x = raw_x.clamp(8.0, (win_w - menu_w - 8.0).max(8.0));
	let y = raw_y.clamp(8.0, (win_h - menu_h - 8.0).max(8.0));

	let mut actions = column![].spacing(6);
	actions = actions.push(button(text(t!("actions.reply"))).on_press(Message::ReplyToMessage(
		active.room.clone(),
		active.server_message_id.clone(),
		active.platform_message_id.clone(),
	)));

	if let Some(perms) = app.state.room_permissions.get(&active.room) {
		if perms.can_delete {
			actions = actions.push(button(text(t!("actions.delete"))).on_press(Message::DeleteMessage(
				active.room.clone(),
				active.server_message_id.clone(),
				active.platform_message_id.clone(),
			)));
		}
		if perms.can_timeout
			&& let Some(uid) = &active.author_id
		{
			actions = actions
				.push(button(text(t!("actions.timeout"))).on_press(Message::TimeoutUser(active.room.clone(), uid.clone())));
		}
		if perms.can_ban
			&& let Some(uid) = &active.author_id
		{
			actions =
				actions.push(button(text(t!("actions.ban"))).on_press(Message::BanUser(active.room.clone(), uid.clone())));
		}
	}

	let menu = container(actions.padding(6)).style(move |_theme| container::Style {
		text_color: Some(palette.text),
		background: Some(Background::Color(palette.panel_bg_2)),
		border: Border {
			color: palette.border,
			width: 1.0,
			radius: 6.0.into(),
		},
		shadow: Shadow::default(),
		snap: false,
	});

	let top = space().height(Length::Fixed(y));
	let left = space().width(Length::Fixed(x));
	let right = space().width(Length::Fill);
	let bottom = space().height(Length::Fill);

	let row = row![left, opaque(menu), right].height(Length::Shrink);
	let overlay = column![top, row, bottom].width(Length::Fill).height(Length::Fill);

	let backdrop =
		container(mouse_area(space().width(Length::Fill).height(Length::Fill)).on_press(Message::DismissMessageAction))
			.width(Length::Fill)
			.height(Length::Fill);

	let layered = stack([backdrop.into(), overlay.into()])
		.width(Length::Fill)
		.height(Length::Fill);

	Some(layered.into())
}
