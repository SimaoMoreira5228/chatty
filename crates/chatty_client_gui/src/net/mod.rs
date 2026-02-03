use tokio::sync::mpsc;

pub mod api;
pub mod backend;
pub mod controller;
pub mod reconnect;
pub mod subscriptions;
pub mod types;

pub use backend::map_core_err;
pub use controller::{NetCommand, NetController, ShutdownHandle};
pub use types::UiEvent;

pub const CHATTY_UI_AUTO_CONNECT_ENV: &str = "CHATTY_UI_AUTO_CONNECT";
pub const CHATTY_UI_AUTO_SUBSCRIBE_ENV: &str = "CHATTY_UI_AUTO_SUBSCRIBE";

pub fn start_networking() -> (NetController, mpsc::UnboundedReceiver<UiEvent>, ShutdownHandle) {
	let (cmd_tx, cmd_rx) = mpsc::channel::<NetCommand>(128);
	let (ui_tx, ui_rx) = mpsc::unbounded_channel::<UiEvent>();
	let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

	let controller = NetController::new(cmd_tx);
	let join_handle = std::thread::Builder::new()
		.name("chatty-network".to_string())
		.spawn(move || {
			let rt = tokio::runtime::Builder::new_multi_thread()
				.enable_all()
				.worker_threads(2)
				.thread_name("chatty-network-worker")
				.build()
				.expect("failed to build tokio runtime for networking");
			rt.block_on(backend::run_network_task(cmd_rx, ui_tx, shutdown_rx));
		})
		.expect("failed to spawn network thread");

	let shutdown = ShutdownHandle::new(shutdown_tx, join_handle);

	(controller, ui_rx, shutdown)
}

pub(crate) fn is_truthy_env(v: &str) -> bool {
	matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

pub(crate) fn should_dev_auto_connect() -> bool {
	if !cfg!(debug_assertions) {
		return false;
	}
	std::env::var(CHATTY_UI_AUTO_CONNECT_ENV)
		.map(|v| is_truthy_env(&v))
		.unwrap_or(true)
}

pub(crate) fn dev_default_topics() -> Vec<String> {
	if let Ok(val) = std::env::var(CHATTY_UI_AUTO_SUBSCRIBE_ENV) {
		let topics: Vec<String> = val
			.split(',')
			.map(|s| s.trim())
			.filter(|s| !s.is_empty())
			.map(|s| s.to_string())
			.collect();
		if !topics.is_empty() {
			return topics;
		}
	}

	Vec::new()
}
