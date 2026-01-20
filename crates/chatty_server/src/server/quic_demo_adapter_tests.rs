#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, anyhow};
use chatty_client_core::{ClientConfigV1, SessionControl};
use chatty_protocol::pb;
use tokio::sync::{RwLock, mpsc, oneshot};
use tracing::{debug, warn};

use crate::adapters::DemoAdapter;
use crate::quic::config::QuicServerConfig;
use crate::server::adapter_manager::{AdapterManagerConfig, start_global_adapter_manager};
use crate::server::audit::AuditService;
use crate::server::connection::{ConnectionSettings, handle_connection};
use crate::server::replay::{ReplayService, ReplayStoreConfig};
use crate::server::room_hub::{RoomHub, RoomHubConfig};
use crate::server::router::{RouterConfig, spawn_ingest_router};
use crate::server::state::GlobalState;

fn init_rustls_crypto_provider() {
	let _ = rustls::crypto::CryptoProvider::install_default(rustls::crypto::aws_lc_rs::default_provider());
}

async fn run_demo_server_with_cfg(
	endpoint: quinn::Endpoint,
	ready_tx: oneshot::Sender<SocketAddr>,
	replay_cfg: ReplayStoreConfig,
	max_connections: usize,
) -> anyhow::Result<()> {
	let local_addr = endpoint.local_addr().context("server local_addr")?;
	let _ = ready_tx.send(local_addr);

	let state = Arc::new(RwLock::new(GlobalState::default()));

	let demo = DemoAdapter::new().with_emit_interval(Duration::from_millis(10));
	let platform_adapters: Vec<Box<dyn chatty_platform::PlatformAdapter>> = vec![Box::new(demo)];

	let adapter_manager = Arc::new(start_global_adapter_manager(
		Arc::clone(&state),
		AdapterManagerConfig::default(),
		platform_adapters,
	));
	let room_hub = RoomHub::new(RoomHubConfig::default());
	let _router = spawn_ingest_router(Arc::clone(&adapter_manager), room_hub.clone(), RouterConfig::default());

	let settings = ConnectionSettings {
		auth_token: None,
		..ConnectionSettings::default()
	};

	let replay_service = Arc::new(ReplayService::new_in_memory(replay_cfg.clone()));
	let audit_service = Arc::new(AuditService::disabled());

	let mut handles = Vec::with_capacity(max_connections);

	for idx in 0..max_connections {
		let conn_id = (idx + 1) as u64;
		debug!(conn_id, "waiting for quic connection");
		let Some(connecting) = endpoint.accept().await else {
			return Err(anyhow!("server endpoint closed before accept"));
		};

		let connection = connecting.await.context("accept quic connection")?;
		debug!(conn_id, "accepted quic connection");
		let state = Arc::clone(&state);
		let adapter_manager = Arc::clone(&adapter_manager);
		let room_hub = room_hub.clone();
		let replay_service = Arc::clone(&replay_service);
		let settings = settings.clone();
		let audit_service = Arc::clone(&audit_service);

		handles.push((
			conn_id,
			tokio::spawn(async move {
				debug!(conn_id, "connection task started");
				handle_connection(
					conn_id,
					connection,
					state,
					adapter_manager,
					room_hub,
					replay_service,
					audit_service,
					settings,
				)
				.await
			}),
		));
	}

	let join_timeout = Duration::from_secs(5);
	for (conn_id, mut handle) in handles {
		debug!(conn_id, "joining connection task");
		match tokio::time::timeout(join_timeout, &mut handle).await {
			Ok(join_res) => match join_res {
				Ok(Ok(())) => debug!(conn_id, "connection task finished"),
				Ok(Err(e)) => {
					return Err(e).context(format!("connection task failed (conn_id={conn_id})"));
				}
				Err(e) => {
					return Err(anyhow!(e)).context(format!("connection task panicked (conn_id={conn_id})"));
				}
			},
			Err(_) => {
				warn!(conn_id, "connection task join timed out; aborting");
				handle.abort();
			}
		}
	}

	Ok(())
}

async fn run_demo_server(endpoint: quinn::Endpoint, ready_tx: oneshot::Sender<SocketAddr>) -> anyhow::Result<()> {
	run_demo_server_with_cfg(endpoint, ready_tx, ReplayStoreConfig::default(), 1).await
}

fn client_cfg(server_addr: SocketAddr, instance_id: &str) -> ClientConfigV1 {
	ClientConfigV1 {
		server_host: "localhost".to_string(),
		server_port: server_addr.port(),
		server_addr: Some(server_addr),
		client_name: "chatty-test-client".to_string(),
		client_instance_id: instance_id.to_string(),
		user_oauth_token: None,
		..ClientConfigV1::default()
	}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn demo_adapter_end_to_end_event_flow() -> anyhow::Result<()> {
	init_rustls_crypto_provider();

	let bind_addr: SocketAddr = "127.0.0.1:0".parse().context("parse bind addr")?;
	let quic_cfg = QuicServerConfig::dev(bind_addr);
	let (endpoint, _cert_der) = quic_cfg.bind_dev_endpoint()?;

	let (ready_tx, ready_rx) = oneshot::channel::<SocketAddr>();
	let server_task = tokio::spawn(async move { run_demo_server(endpoint, ready_tx).await });

	let mut server_addr = ready_rx.await.context("server ready")?;
	if server_addr.ip().is_unspecified() {
		server_addr.set_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)));
	}

	let cfg = client_cfg(server_addr, "demo-adapter-test");

	let (mut control, _welcome) = SessionControl::connect(cfg).await.context("client connect")?;
	let topic = "room:twitch/demo".to_string();
	let _ = control.subscribe(vec![topic.clone()]).await.context("subscribe")?;

	let mut events = control.open_events_stream().await.context("open events stream")?;

	let (ev_tx, mut ev_rx) = mpsc::channel::<chatty_protocol::pb::EventEnvelope>(8);
	let events_task = tokio::spawn(async move {
		events
			.run_events_loop(|ev| {
				let _ = ev_tx.try_send(ev);
			})
			.await
	});

	let mut matched = false;
	let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
	while !matched {
		let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
		let ev = tokio::time::timeout(timeout, ev_rx.recv())
			.await
			.context("timeout waiting for event")?
			.context("events channel closed")?;

		assert_eq!(ev.topic, topic);
		if let Some(chatty_protocol::pb::event_envelope::Event::ChatMessage(cm)) = ev.event {
			let msg = cm.message.expect("chat message is present");
			assert!(msg.text.contains("demo ingest message"));
			matched = true;
		}
	}

	events_task.abort();
	let _ = events_task.await;

	let server_res = server_task.await.context("server join")?;
	server_res.context("server run")?;

	Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnect_resumes_with_replay() -> anyhow::Result<()> {
	init_rustls_crypto_provider();

	let bind_addr: SocketAddr = "127.0.0.1:0".parse().context("parse bind addr")?;
	let quic_cfg = QuicServerConfig::dev(bind_addr);
	let (endpoint, _cert_der) = quic_cfg.bind_dev_endpoint()?;

	let (ready_tx, ready_rx) = oneshot::channel::<SocketAddr>();
	let server_task =
		tokio::spawn(async move { run_demo_server_with_cfg(endpoint, ready_tx, ReplayStoreConfig::default(), 2).await });

	let mut server_addr = ready_rx.await.context("server ready")?;
	if server_addr.ip().is_unspecified() {
		server_addr.set_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)));
	}

	let cfg = client_cfg(server_addr, "resume-test");
	let topic = "room:twitch/demo".to_string();

	let (mut control, _welcome) = SessionControl::connect(cfg.clone()).await.context("client connect")?;
	let _ = control.subscribe(vec![topic.clone()]).await.context("subscribe")?;
	let mut events = control.open_events_stream().await.context("open events stream")?;

	let (ev_tx, mut ev_rx) = mpsc::channel::<pb::EventEnvelope>(32);
	let events_task = tokio::spawn(async move {
		events
			.run_events_loop(|ev| {
				let _ = ev_tx.try_send(ev);
			})
			.await
	});

	let mut cursors = Vec::new();
	for _ in 0..5 {
		let ev = tokio::time::timeout(Duration::from_secs(5), ev_rx.recv())
			.await
			.context("timeout waiting for event")?
			.context("events channel closed")?;
		cursors.push(ev.cursor);
	}
	let last_cursor = *cursors.last().expect("cursor captured");

	events_task.abort();
	let _ = events_task.await;
	control.close(0, "test disconnect");
	drop(control);

	tokio::time::sleep(Duration::from_millis(50)).await;

	let (mut control, _welcome) = SessionControl::connect(cfg).await.context("client reconnect")?;
	let replay_from = last_cursor.saturating_sub(2);
	let _ = control
		.subscribe_with_cursors(vec![(topic.clone(), replay_from)])
		.await
		.context("subscribe with cursors")?;

	let mut events = control.open_events_stream().await.context("open events stream (reconnect)")?;
	let (ev_tx, mut ev_rx) = mpsc::channel::<pb::EventEnvelope>(16);
	let events_task = tokio::spawn(async move {
		events
			.run_events_loop(|ev| {
				let _ = ev_tx.try_send(ev);
			})
			.await
	});

	let first = tokio::time::timeout(Duration::from_secs(5), ev_rx.recv())
		.await
		.context("timeout waiting for replay event")?
		.context("events channel closed")?;

	assert!(first.cursor > replay_from);
	assert!(first.cursor <= last_cursor);

	events_task.abort();
	let _ = events_task.await;
	control.close(0, "test done");
	drop(control);

	let server_res = server_task.await.context("server join")?;
	server_res.context("server run")?;

	Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnect_reports_lagged_when_replay_exhausted() -> anyhow::Result<()> {
	init_rustls_crypto_provider();

	let bind_addr: SocketAddr = "127.0.0.1:0".parse().context("parse bind addr")?;
	let quic_cfg = QuicServerConfig::dev(bind_addr);
	let (endpoint, _cert_der) = quic_cfg.bind_dev_endpoint()?;

	let replay_cfg = ReplayStoreConfig {
		per_topic_capacity: 2,
		retention_secs: None,
	};
	let (ready_tx, ready_rx) = oneshot::channel::<SocketAddr>();
	let server_task = tokio::spawn(async move { run_demo_server_with_cfg(endpoint, ready_tx, replay_cfg, 2).await });

	let mut server_addr = ready_rx.await.context("server ready")?;
	if server_addr.ip().is_unspecified() {
		server_addr.set_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)));
	}

	let cfg = client_cfg(server_addr, "lag-test");
	let topic = "room:twitch/demo".to_string();

	let (mut control, _welcome) = SessionControl::connect(cfg.clone()).await.context("client connect")?;
	let _ = control.subscribe(vec![topic.clone()]).await.context("subscribe")?;
	let mut events = control.open_events_stream().await.context("open events stream")?;

	let (ev_tx, mut ev_rx) = mpsc::channel::<pb::EventEnvelope>(32);
	let events_task = tokio::spawn(async move {
		events
			.run_events_loop(|ev| {
				let _ = ev_tx.try_send(ev);
			})
			.await
	});

	for _ in 0..5 {
		let _ = tokio::time::timeout(Duration::from_secs(5), ev_rx.recv())
			.await
			.context("timeout waiting for event")?
			.context("events channel closed")?;
	}

	events_task.abort();
	let _ = events_task.await;
	control.close(0, "test disconnect");
	drop(control);

	tokio::time::sleep(Duration::from_millis(50)).await;

	let (mut control, _welcome) = SessionControl::connect(cfg).await.context("client reconnect")?;
	let subscribed = control
		.subscribe_with_cursors(vec![(topic.clone(), 1)])
		.await
		.context("subscribe with cursors")?;

	let status = subscribed.results.first().map(|r| r.status).unwrap_or_default();
	assert_eq!(status, pb::subscription_result::Status::ReplayNotAvailable as i32);

	let mut events = control.open_events_stream().await.context("open events stream (reconnect)")?;
	let (ev_tx, mut ev_rx) = mpsc::channel::<pb::EventEnvelope>(16);
	let events_task = tokio::spawn(async move {
		events
			.run_events_loop(|ev| {
				let _ = ev_tx.try_send(ev);
			})
			.await
	});

	let first = tokio::time::timeout(Duration::from_secs(5), ev_rx.recv())
		.await
		.context("timeout waiting for lagged event")?
		.context("events channel closed")?;

	match first.event {
		Some(pb::event_envelope::Event::TopicLagged(_)) => {}
		other => anyhow::bail!("expected TopicLagged event, got: {other:?}"),
	}

	events_task.abort();
	let _ = events_task.await;
	control.close(0, "test done");
	drop(control);

	let server_res = server_task.await.context("server join")?;
	server_res.context("server run")?;

	Ok(())
}
