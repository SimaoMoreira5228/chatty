use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chatty_domain::RoomKey;
use tokio::sync::mpsc;

use super::api::BoxedSessionControl;
use super::backend::ensure_events_loop_started;
use super::types::UiEvent;
use crate::net::map_core_err;

pub fn topic_for_room(room: &RoomKey) -> String {
	format!("room:{}/{}", room.platform.as_str(), room.room_id.as_str())
}

pub async fn reconcile_subscriptions_on_connect(
	session: &mut BoxedSessionControl,
	topics_refcounts: &HashMap<String, usize>,
	cursor_by_topic: &Arc<Mutex<HashMap<String, u64>>>,
	ui_tx: &mpsc::UnboundedSender<UiEvent>,
	events_task: &mut Option<tokio::task::JoinHandle<()>>,
) -> Result<(), String> {
	let topics: Vec<String> = topics_refcounts
		.iter()
		.filter(|(_, c)| **c > 0)
		.map(|(t, _)| t.clone())
		.collect();

	if topics.is_empty() {
		return Ok(());
	}

	let mut subs = Vec::with_capacity(topics.len());
	{
		let cursors = cursor_by_topic.lock().unwrap();
		for topic in &topics {
			subs.push((topic.clone(), *cursors.get(topic).unwrap_or(&0)));
		}
	}

	session
		.subscribe(subs)
		.await
		.map_err(|e| format!("subscribe failed: {}", map_core_err(e)))?;

	ensure_events_loop_started(session, events_task, ui_tx, cursor_by_topic).await
}

pub async fn unsubscribe_topics(
	session: &mut BoxedSessionControl,
	topics: Vec<String>,
	_ui_tx: &mpsc::UnboundedSender<UiEvent>,
) -> Result<(), String> {
	session
		.unsubscribe(topics)
		.await
		.map_err(|e| format!("unsubscribe failed: {}", map_core_err(e)))?;

	Ok(())
}
