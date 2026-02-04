#![forbid(unsafe_code)]

use std::sync::Arc;

use tokio::sync::{Mutex, mpsc};
use tracing::info;

use crate::net::UiEvent;

pub(crate) type UiEventReceiver = mpsc::UnboundedReceiver<UiEvent>;

pub(crate) async fn recv_next(rx: Arc<Mutex<UiEventReceiver>>) -> Option<UiEvent> {
	info!("recv_next: waiting for next UiEvent");
	let mut rx = rx.lock().await;
	let ev = rx.recv().await;
	if let Some(ref e) = ev {
		info!(?e, "recv_next: got UiEvent");
	} else {
		info!("recv_next: got UiEvent? false");
	}

	ev
}
