#![forbid(unsafe_code)]

use std::sync::Arc;

use chatty_client_ui::net::UiEvent;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

pub(crate) type UiEventReceiver = mpsc::UnboundedReceiver<UiEvent>;

pub(crate) async fn recv_next(rx: Arc<Mutex<UiEventReceiver>>) -> Option<UiEvent> {
	let mut rx = rx.lock().await;
	rx.recv().await
}
