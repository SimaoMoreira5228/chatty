use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use chatty_client_core::{ClientConfigV1, ClientCoreError, SessionControl};
use chatty_protocol::pb;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, MissedTickBehavior};
use tracing::{debug, info, warn};

use super::api::{BoxedSessionControl, BoxedSessionEvents};
use super::controller::NetCommand;
use super::reconnect::{RECONNECT_RESET_AFTER, schedule_reconnect};
use super::subscriptions::{reconcile_subscriptions_on_connect, subscribe_topics, topic_for_room, unsubscribe_topics};
use super::types::UiEvent;
use crate::net::{dev_default_topics, should_dev_auto_connect};
use crate::ui::components::chat_message::AssetRefUi;

const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(3);
const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(10);
const KEEPALIVE_MAX_FAILURES: u32 = 3;

pub fn ui_send_error(ui_tx: &mpsc::UnboundedSender<UiEvent>, message: String, cfg: Option<&ClientConfigV1>) {
	let server = cfg.map(|c| format!("{}:{}", c.server_host, c.server_port));
	let _ = ui_tx.send(UiEvent::ErrorWithServer { message, server });
}

pub fn map_core_err(e: ClientCoreError) -> String {
	match e {
		ClientCoreError::Endpoint(s) => s,
		ClientCoreError::Connect(s) => s,
		ClientCoreError::Framing(e) => e.to_string(),
		ClientCoreError::Protocol(s) => s,
		ClientCoreError::Io(s) => s,
		ClientCoreError::Other(s) => s,
	}
}

pub async fn run_network_task(
	cmd_rx: mpsc::Receiver<NetCommand>,
	ui_tx: mpsc::UnboundedSender<UiEvent>,
	shutdown_rx: oneshot::Receiver<()>,
) {
	run_network_task_with_session_factory(cmd_rx, ui_tx, shutdown_rx, |cfg, ui_tx| {
		Box::pin(connect_session(*cfg, ui_tx))
	})
	.await;
}

pub async fn run_network_task_with_session_factory<F>(
	mut cmd_rx: mpsc::Receiver<NetCommand>,
	ui_tx: mpsc::UnboundedSender<UiEvent>,
	mut shutdown_rx: oneshot::Receiver<()>,
	mut connect_fn: F,
) where
	F: FnMut(
		Box<ClientConfigV1>,
		mpsc::UnboundedSender<UiEvent>,
	) -> Pin<Box<dyn Future<Output = Option<BoxedSessionControl>> + Send>>,
{
	let mut session: Option<BoxedSessionControl> = None;
	let mut events_task: Option<tokio::task::JoinHandle<()>> = None;
	let mut topics_refcounts: HashMap<String, usize> = HashMap::new();
	let cursor_by_topic: Arc<Mutex<HashMap<String, u64>>> = Arc::new(Mutex::new(HashMap::new()));
	let mut last_connect_cfg: Option<ClientConfigV1> = None;
	let mut reconnect_attempt: u32 = 0;
	let mut reconnect_deadline: Option<Instant> = None;
	let mut keepalive_failures: u32 = 0;
	let mut last_successful_connect_time: Option<Instant> = None;

	let mut keepalive_tick = tokio::time::interval(KEEPALIVE_INTERVAL);
	keepalive_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

	fn bump_reconnect_attempt(reconnect_attempt: &mut u32, last_successful: Option<Instant>) -> u32 {
		if let Some(last) = last_successful {
			if Instant::now().duration_since(last) > RECONNECT_RESET_AFTER {
				*reconnect_attempt = 1;
			} else {
				*reconnect_attempt = reconnect_attempt.saturating_add(1).max(1);
			}
		} else {
			*reconnect_attempt = reconnect_attempt.saturating_add(1).max(1);
		}
		*reconnect_attempt
	}

	let mut dev_auto_connect_fired = false;

	loop {
		tokio::select! {
			_ = &mut shutdown_rx => {
				let _ = ui_tx.send(UiEvent::Disconnected { reason: "shutdown".to_string() });
				if let Some(s) = session.as_ref() {
					s.close(0, "shutdown");
				}
				if let Some(t) = events_task.take() {
					t.abort();
				}
				break;
			}

			_ = keepalive_tick.tick(), if session.is_some() => {
				if let Some(s) = session.as_mut() {
					let client_time_unix_ms = SystemTime::now()
						.duration_since(SystemTime::UNIX_EPOCH)
						.map(|d| d.as_millis() as i64)
						.unwrap_or(0);

					let ping_res = tokio::time::timeout(KEEPALIVE_TIMEOUT, s.ping(client_time_unix_ms)).await;
					match ping_res {
						Ok(Ok(_)) => {
							keepalive_failures = 0;
						}
						Ok(Err(e)) => {
							keepalive_failures = keepalive_failures.saturating_add(1);
							warn!(failure = keepalive_failures, error = %map_core_err(e), "keepalive failed");
						}
						Err(_) => {
							keepalive_failures = keepalive_failures.saturating_add(1);
							warn!(failure = keepalive_failures, "keepalive timeout");
						}
					}

					if keepalive_failures >= KEEPALIVE_MAX_FAILURES {
						let _ = ui_tx.send(UiEvent::Disconnected {
							reason: "keepalive failed; reconnecting".to_string(),
						});

						if let Some(t) = events_task.take() {
							t.abort();
						}

						if let Some(s) = session.as_ref() {
							s.close(0, "keepalive failed");
						}

						session = None;
						keepalive_failures = 0;

						if let Some(_cfg) = last_connect_cfg.clone() {
							let attempt = bump_reconnect_attempt(&mut reconnect_attempt, last_successful_connect_time);
							let (deadline, ms) = schedule_reconnect(attempt);
							reconnect_deadline = Some(deadline);
							let _ = ui_tx.send(UiEvent::Reconnecting {
								attempt: reconnect_attempt,
								next_retry_in_ms: ms,
							});
						}
					}
				}
			}

			cmd = cmd_rx.recv() => {
				let Some(cmd) = cmd else {
					let _ = ui_tx.send(UiEvent::Disconnected { reason: "ui dropped controller".to_string() });
					if let Some(s) = session.as_ref() {
						s.close(0, "ui dropped controller");
					}
					if let Some(t) = events_task.take() {
						t.abort();
					}
					break;
				};

				match cmd {
					NetCommand::Connect { cfg } => {
						last_connect_cfg = Some(*cfg.clone());
						reconnect_attempt = 0;
						reconnect_deadline = None;
						let _ = ui_tx.send(UiEvent::Connecting);
						if let Some(t) = events_task.take() { t.abort(); }
						if let Some(s) = session.as_ref() { s.close(0, "reconnect"); }
						session = connect_fn(cfg.clone(), ui_tx.clone()).await;

						if let Some(s) = session.as_mut() {
							if cfg!(debug_assertions) {
								for topic in dev_default_topics() {
									*topics_refcounts.entry(topic).or_insert(0) += 1;
								}
							}

							if let Err(e) = reconcile_subscriptions_on_connect(
								s,
								&topics_refcounts,
								&cursor_by_topic,
								&ui_tx,
								&mut events_task,
							)
							.await {
								ui_send_error(&ui_tx, e, last_connect_cfg.as_ref());
								s.close(0, "subscribe failed");
								session = None;
							} else {
								last_successful_connect_time = Some(Instant::now());
								reconnect_attempt = 0;
								reconnect_deadline = None;
							}
						} else if let Some(_cfg) = last_connect_cfg.clone() {
							let attempt = bump_reconnect_attempt(&mut reconnect_attempt, last_successful_connect_time);
							let (deadline, ms) = schedule_reconnect(attempt);
							reconnect_deadline = Some(deadline);
							let _ = ui_tx.send(UiEvent::Reconnecting {
								attempt,
								next_retry_in_ms: ms,
							});
						}
					}

					NetCommand::Disconnect { reason } => {
						if let Some(t) = events_task.take() { t.abort(); }
						if let Some(s) = session.as_ref() { s.close(0, &reason); }
						session = None;
						last_connect_cfg = None;
						reconnect_attempt = 0;
						reconnect_deadline = None;
						let _ = ui_tx.send(UiEvent::Disconnected { reason });
					}


					NetCommand::SubscribeRoomKey { room } => {
						let topic = topic_for_room(&room);
						let count = topics_refcounts.entry(topic.clone()).or_insert(0);
						let was_zero = *count == 0;
						*count += 1;

						if was_zero
							&& let Some(s) = session.as_mut()
								&& let Err(e) = subscribe_topics(s, vec![topic.clone()], &cursor_by_topic, &ui_tx, &mut events_task).await {
									ui_send_error(&ui_tx, e, last_connect_cfg.as_ref());
								}
					}

					NetCommand::UnsubscribeRoomKey { room } => {
						let topic = topic_for_room(&room);
						let mut became_zero = false;
						if let Some(count) = topics_refcounts.get_mut(&topic) {
							if *count > 1 {
								*count -= 1;
							} else {
								topics_refcounts.remove(&topic);
								became_zero = true;
							}
						}

						if became_zero
							&& let Some(s) = session.as_mut()
								&& let Err(e) = unsubscribe_topics(s, vec![topic], &ui_tx).await {
									ui_send_error(&ui_tx, e, last_connect_cfg.as_ref());
								}
					}

					NetCommand::SendCommand { command } => {
						if let Some(s) = session.as_mut() {
							match s.send_command(command).await {
								Ok(result) => {
									let _ = ui_tx.send(UiEvent::CommandResult {
										status: result.status,
										detail: result.detail,
									});
								}
								Err(e) => {
									ui_send_error(&ui_tx, map_core_err(e), last_connect_cfg.as_ref());
								}
							}
						} else {
							ui_send_error(&ui_tx, "not connected".to_string(), last_connect_cfg.as_ref());
						}
					}
				}
			}

			_ = tokio::time::sleep(Duration::from_millis(200)), if cfg!(debug_assertions) && !dev_auto_connect_fired && should_dev_auto_connect() => {
				dev_auto_connect_fired = true;
				let _ = ui_tx.send(UiEvent::Connecting);
				if let Some(t) = events_task.take() { t.abort(); }
				if let Some(s) = session.as_ref() { s.close(0, "dev auto-connect"); }
				session = connect_fn(Box::default(), ui_tx.clone()).await;

				if let Some(s) = session.as_mut() {
					if cfg!(debug_assertions) {
						for topic in dev_default_topics() {
							*topics_refcounts.entry(topic).or_insert(0) += 1;
						}
					}
					if let Err(e) = reconcile_subscriptions_on_connect(s, &topics_refcounts, &cursor_by_topic, &ui_tx, &mut events_task).await {
						ui_send_error(&ui_tx, e, last_connect_cfg.as_ref());
						s.close(0, "subscribe failed");
						session = None;
					}
				}
			}

			_ = async {
				if let Some(deadline) = reconnect_deadline {
					tokio::time::sleep_until(deadline).await;
				}
			}, if reconnect_deadline.is_some() => {
				if let Some(cfg) = last_connect_cfg.clone() {
					let _ = ui_tx.send(UiEvent::Connecting);
					if let Some(t) = events_task.take() { t.abort(); }
					if let Some(s) = session.as_ref() { s.close(0, "reconnect"); }
					session = connect_fn(Box::new(cfg), ui_tx.clone()).await;
					if let Some(s) = session.as_mut() {
						if let Err(e) = reconcile_subscriptions_on_connect(s, &topics_refcounts, &cursor_by_topic, &ui_tx, &mut events_task).await {
							ui_send_error(&ui_tx, e, last_connect_cfg.as_ref());
							s.close(0, "subscribe failed");
							session = None;
						} else {
							last_successful_connect_time = Some(Instant::now());
							reconnect_attempt = 0;
							reconnect_deadline = None;
						}
					} else {
						let attempt = bump_reconnect_attempt(&mut reconnect_attempt, last_successful_connect_time);
						let (deadline, ms) = schedule_reconnect(attempt);
						reconnect_deadline = Some(deadline);
						let _ = ui_tx.send(UiEvent::Reconnecting {
							attempt,
							next_retry_in_ms: ms,
						});
					}
				}
			}
		}
	}
}

pub async fn ensure_events_loop_started(
	session: &mut BoxedSessionControl,
	events_task: &mut Option<tokio::task::JoinHandle<()>>,
	ui_tx: &mpsc::UnboundedSender<UiEvent>,
	cursor_by_topic: &Arc<Mutex<HashMap<String, u64>>>,
) -> Result<(), String> {
	if events_task.is_some() {
		return Ok(());
	}

	let events = session
		.open_events_stream()
		.await
		.map_err(|e| format!("open events stream failed: {}", map_core_err(e)))?;

	*events_task = Some(spawn_events_loop(events, ui_tx.clone(), Arc::clone(cursor_by_topic)));

	Ok(())
}

fn spawn_events_loop(
	mut events: BoxedSessionEvents,
	ui_tx: mpsc::UnboundedSender<UiEvent>,
	cursor_by_topic: Arc<Mutex<HashMap<String, u64>>>,
) -> tokio::task::JoinHandle<()> {
	tokio::spawn(async move {
		let res = events
			.run_events_loop(Box::new(|ev| {
				let topic = ev.topic.clone();
				let cursor = ev.cursor;
				let event_kind = match ev.event.as_ref() {
					Some(pb::event_envelope::Event::ChatMessage(_)) => "chat_message",
					Some(pb::event_envelope::Event::TopicLagged(_)) => "topic_lagged",
					Some(pb::event_envelope::Event::Permissions(_)) => "permissions",
					Some(pb::event_envelope::Event::AssetBundle(_)) => "asset_bundle",
					Some(pb::event_envelope::Event::RoomState(_)) => "room_state",
					None => "empty",
				};
				debug!(%topic, cursor, %event_kind, "events stream received");
				if let Some(pb::event_envelope::Event::AssetBundle(bundle)) = ev.event.as_ref() {
					info!(%topic, cache_key = %bundle.cache_key, emote_count = bundle.emotes.len(), badge_count = bundle.badges.len(), "events stream asset bundle received");
				}

				{
					let mut cursors = cursor_by_topic.lock().unwrap();
					let entry = cursors.entry(topic.clone()).or_insert(0);
					if cursor > *entry {
						*entry = cursor;
					}
				}
				if let Some(ui_ev) = map_event_envelope_to_ui_event(ev) {
					if let Err(e) = ui_tx.send(ui_ev) {
						warn!(error = ?e, %topic, cursor, "failed to send UiEvent from events loop - UI receiver may be dropped");
					}
				} else {
					debug!(%topic, cursor, %event_kind, "event not mapped to UiEvent");
				}
			}))
			.await;

		match res {
			Ok(()) => {
				let _ = ui_tx.send(UiEvent::Disconnected {
					reason: "events stream closed".to_string(),
				});
			}
			Err(e) => {
				let msg = map_core_err(e);
				let _ = ui_tx.send(UiEvent::Disconnected { reason: msg });
			}
		}
	})
}

async fn connect_session(cfg: ClientConfigV1, ui_tx: mpsc::UnboundedSender<UiEvent>) -> Option<BoxedSessionControl> {
	info!(server_host = %cfg.server_host, server_port = cfg.server_port, "connecting...");
	info!(client_instance_id = %cfg.client_instance_id, "calling SessionControl::connect");

	let (session, welcome) = match SessionControl::connect(cfg.clone()).await {
		Ok((s, w)) => (s, w),
		Err(e) => {
			debug!(error = ?e, "SessionControl::connect returned error");
			let msg = map_core_err(e);
			ui_send_error(&ui_tx, msg.clone(), Some(&cfg));
			let _ = ui_tx.send(UiEvent::Disconnected { reason: msg });
			return None;
		}
	};

	info!(server_name=%welcome.server_name, server_instance=%welcome.server_instance_id, "SessionControl::connect succeeded (welcome received)");

	match ui_tx.send(UiEvent::Connected {
		server_name: welcome.server_name.clone(),
		server_instance_id: welcome.server_instance_id.clone(),
	}) {
		Ok(()) => {
			info!(server=%welcome.server_name, instance=%welcome.server_instance_id, "sent UiEvent::Connected to UI thread")
		}
		Err(e) => warn!(error = ?e, "failed to send UiEvent::Connected - UI receiver may be dropped"),
	}

	Some(Box::new(session))
}

fn map_event_envelope_to_ui_event(ev: pb::EventEnvelope) -> Option<UiEvent> {
	let topic = ev.topic;

	match ev.event {
		Some(pb::event_envelope::Event::ChatMessage(cm)) => {
			let (author_login, author_display, text, badge_ids, emotes) = cm
				.message
				.as_ref()
				.map(|m| {
					let login = if m.author_login.is_empty() {
						"unknown".to_string()
					} else {
						m.author_login.clone()
					};
					let display = if m.author_display.is_empty() {
						None
					} else {
						Some(m.author_display.clone())
					};
					let text = m.text.clone();
					let badges = m.badge_ids.clone();
					let emotes = m
						.emotes
						.iter()
						.map(|emote| AssetRefUi {
							id: emote.id.clone(),
							name: emote.name.clone(),
							image_url: emote.image_url.clone(),
							image_format: emote.image_format.clone(),
							width: emote.width,
							height: emote.height,
						})
						.collect();
					(login, display, text, badges, emotes)
				})
				.unwrap_or_else(|| ("unknown".to_string(), None, "".to_string(), Vec::new(), Vec::new()));

			Some(UiEvent::ChatMessage {
				topic,
				author_login,
				author_display,
				author_id: cm.message.as_ref().and_then(|m| {
					if m.author_id.is_empty() {
						None
					} else {
						Some(m.author_id.clone())
					}
				}),
				text,
				server_message_id: if cm.server_message_id.is_empty() {
					None
				} else {
					Some(cm.server_message_id)
				},
				platform_message_id: if cm.platform_message_id.is_empty() {
					None
				} else {
					Some(cm.platform_message_id)
				},
				badge_ids,
				emotes,
			})
		}
		Some(pb::event_envelope::Event::TopicLagged(lag)) => Some(UiEvent::TopicLagged {
			topic,
			dropped: lag.dropped,
			detail: if lag.detail.is_empty() {
				"lagged".to_string()
			} else {
				lag.detail
			},
		}),
		Some(pb::event_envelope::Event::Permissions(perms)) => Some(UiEvent::RoomPermissions {
			topic,
			can_send: perms.can_send,
			can_reply: perms.can_reply,
			can_delete: perms.can_delete,
			can_timeout: perms.can_timeout,
			can_ban: perms.can_ban,
			is_moderator: perms.is_moderator,
			is_broadcaster: perms.is_broadcaster,
		}),
		Some(pb::event_envelope::Event::RoomState(state)) => {
			let settings = state.settings.unwrap_or_default();
			Some(UiEvent::RoomState {
				topic,
				emote_only: settings.emote_only,
				subscribers_only: settings.subscribers_only,
				unique_chat: settings.unique_chat,
				slow_mode: settings.slow_mode,
				slow_mode_wait_time_seconds: settings.slow_mode_wait_time_seconds,
				followers_only: settings.followers_only,
				followers_only_duration_minutes: settings.followers_only_duration_minutes,
			})
		}
		Some(pb::event_envelope::Event::AssetBundle(bundle)) => {
			let cache_key = if bundle.cache_key.is_empty() {
				format!("provider:{}:origin:{}", bundle.provider, topic)
			} else {
				bundle.cache_key
			};
			let etag = if bundle.etag.is_empty() { None } else { Some(bundle.etag) };
			let emotes = bundle
				.emotes
				.into_iter()
				.map(|emote| AssetRefUi {
					id: emote.id,
					name: emote.name,
					image_url: emote.image_url,
					image_format: emote.image_format,
					width: emote.width,
					height: emote.height,
				})
				.collect();
			let badges = bundle
				.badges
				.into_iter()
				.map(|badge| AssetRefUi {
					id: badge.id,
					name: badge.name,
					image_url: badge.image_url,
					image_format: badge.image_format,
					width: badge.width,
					height: badge.height,
				})
				.collect();

			Some(UiEvent::AssetBundle {
				topic,
				cache_key,
				etag,
				provider: bundle.provider,
				scope: bundle.scope,
				emotes,
				badges,
			})
		}
		None => None,
	}
}
