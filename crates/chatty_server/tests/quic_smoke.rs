#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, anyhow};
use chatty_client_core::{ClientConfigV1, SessionControl};
use chatty_protocol::framing::{DEFAULT_MAX_FRAME_SIZE, encode_frame};
use chatty_protocol::pb;
use quinn::{Endpoint, ServerConfig};
use tokio::sync::{RwLock, mpsc, oneshot};

const PROTOCOL_VERSION: u32 = 1;

static LOG_INIT: OnceLock<()> = OnceLock::new();

fn init_test_logging() {
	LOG_INIT.get_or_init(|| {
		if std::env::var_os("CHATTY_TEST_LOG").is_none() {
			return;
		}

		let _ = tracing_subscriber::fmt()
			.with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_string()))
			.with_target(false)
			.try_init();
	});
}

#[derive(Debug, Default)]
struct GlobalState {
	subscribed: bool,
}

fn unix_ms_now() -> i64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or(Duration::from_secs(0))
		.as_millis() as i64
}

fn make_quic_server(bind_addr: SocketAddr) -> anyhow::Result<(Endpoint, Vec<u8>)> {
	let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).context("generate self-signed cert")?;

	let cert_der = ck.cert.der().to_vec();
	let key_der = ck.signing_key.serialize_der();

	let cert_chain = vec![rustls::pki_types::CertificateDer::from(cert_der.clone())];
	let key = rustls::pki_types::PrivateKeyDer::try_from(key_der)
		.map_err(anyhow::Error::msg)
		.context("parse private key der")?;

	let mut tls_config = rustls::ServerConfig::builder()
		.with_no_client_auth()
		.with_single_cert(cert_chain, key)
		.context("build rustls server config")?;
	tls_config.alpn_protocols = vec![b"chatty-v1".to_vec()];

	let server_config = ServerConfig::with_crypto(Arc::new(quinn::crypto::rustls::QuicServerConfig::try_from(tls_config)?));
	let endpoint = Endpoint::server(server_config, bind_addr).context("bind quinn endpoint")?;

	Ok((endpoint, cert_der))
}

async fn send_envelope(send: &mut quinn::SendStream, env: pb::Envelope) -> anyhow::Result<()> {
	let frame = encode_frame(&env, DEFAULT_MAX_FRAME_SIZE).map_err(|e| anyhow!(e))?;
	send.write_all(&frame).await.context("write frame")?;
	Ok(())
}

async fn run_minimal_server(
	endpoint: Endpoint,
	state: Arc<RwLock<GlobalState>>,
	ready_tx: oneshot::Sender<SocketAddr>,
) -> anyhow::Result<()> {
	init_test_logging();

	tracing::debug!("server: starting run_minimal_server()");

	let local_addr = endpoint.local_addr().context("server local_addr")?;
	tracing::info!(?local_addr, "server: endpoint bound");
	let _ = ready_tx.send(local_addr);

	tracing::debug!("server: awaiting endpoint.accept()");
	let Some(connecting) = endpoint.accept().await else {
		return Err(anyhow!("server endpoint closed before accept"));
	};

	tracing::debug!("server: awaiting connecting.await");
	let connection = connecting.await.context("accept quic connection")?;
	tracing::info!(remote = %connection.remote_address(), "server: accepted QUIC connection");

	tracing::debug!("server: awaiting accept_bi(control)");
	let (mut control_send, mut control_recv) = connection.accept_bi().await.context("accept_bi (control)")?;
	tracing::info!("server: accepted control bidirectional stream");

	let (tx, mut rx) = mpsc::unbounded_channel::<pb::Envelope>();
	let reader = tokio::spawn(async move {
		let mut buf: Vec<u8> = Vec::with_capacity(16 * 1024);
		let mut tmp = [0u8; 8192];

		loop {
			let n = match control_recv.read(&mut tmp).await {
				Ok(Some(n)) => n,
				Ok(None) => return Ok::<(), anyhow::Error>(()),
				Err(e) => return Err(anyhow!(e).context("control read failed")),
			};
			buf.extend_from_slice(&tmp[..n]);

			loop {
				match chatty_protocol::decode_frame::<pb::Envelope>(&buf, DEFAULT_MAX_FRAME_SIZE) {
					Ok((env, used)) => {
						buf.drain(0..used);
						if tx.send(env).is_err() {
							return Ok(());
						}
					}
					Err(chatty_protocol::FramingError::InsufficientData { .. }) => break,
					Err(e) => return Err(anyhow!(e).context("decode control frame failed")),
				}
			}
		}
	});

	tracing::debug!("server: waiting for Hello");
	let _hello = loop {
		let env = rx.recv().await.ok_or_else(|| anyhow!("no Hello received"))?;
		match env.msg {
			Some(pb::envelope::Msg::Hello(h)) => break h,
			_ => continue,
		}
	};
	tracing::info!("server: received Hello");

	tracing::debug!("server: sending Welcome");
	send_envelope(
		&mut control_send,
		pb::Envelope {
			version: PROTOCOL_VERSION,
			request_id: String::new(),
			msg: Some(pb::envelope::Msg::Welcome(pb::Welcome {
				server_name: "chatty-server-test".to_string(),
				server_instance_id: "test-instance".to_string(),
				server_time_unix_ms: unix_ms_now(),
				max_frame_bytes: DEFAULT_MAX_FRAME_SIZE as u32,
				selected_codec: pb::Codec::Protobuf as i32,
			})),
		},
	)
	.await
	.context("send Welcome")?;
	tracing::info!("server: sent Welcome");

	let (events_tx, events_rx) = oneshot::channel::<anyhow::Result<(quinn::SendStream, quinn::RecvStream)>>();
	let events_accept_task = tokio::spawn(async move {
		tracing::debug!("server: background accept task awaiting accept_bi(events)");
		let res = connection.accept_bi().await.context("accept_bi (events)");
		let _ = events_tx.send(res);
		Ok::<(), anyhow::Error>(())
	});

	tracing::debug!("server: waiting for Subscribe");
	let subscribed_topic = loop {
		let env = rx.recv().await.ok_or_else(|| anyhow!("no Subscribe received"))?;
		match env.msg {
			Some(pb::envelope::Msg::Subscribe(s)) => {
				let topic = s.subs.first().map(|ss| ss.topic.clone()).unwrap_or_default();

				{
					let mut st = state.write().await;
					st.subscribed = true;
				}

				tracing::debug!(%topic, "server: sending Subscribed");
				send_envelope(
					&mut control_send,
					pb::Envelope {
						version: PROTOCOL_VERSION,
						request_id: String::new(),
						msg: Some(pb::envelope::Msg::Subscribed(pb::Subscribed {
							results: vec![pb::SubscriptionResult {
								topic: topic.clone(),
								status: pb::subscription_result::Status::Ok as i32,
								current_cursor: 0,
								detail: String::new(),
							}],
						})),
					},
				)
				.await
				.context("send Subscribed")?;

				tracing::info!(%topic, "server: processed Subscribe and sent Subscribed");
				break topic;
			}
			_ => continue,
		}
	};

	let (mut events_send, _events_recv) = match events_rx.await {
		Ok(Ok((send, recv))) => (send, recv),
		Ok(Err(e)) => {
			return Err(e).context("failed to accept events stream (background accept task)");
		}
		Err(_) => {
			return Err(anyhow!(
				"events stream accept task dropped (likely connection closed before server observed events stream)"
			));
		}
	};
	tracing::info!("server: accepted events bidirectional stream");

	let event = pb::EventEnvelope {
		topic: subscribed_topic,
		cursor: 1,
		server_time_unix_ms: unix_ms_now(),
		event: Some(pb::event_envelope::Event::ChatMessage(pb::ChatMessageEvent {
			origin: Some(pb::Origin {
				platform: pb::Platform::Twitch as i32,
				channel: "demo_channel".to_string(),
				channel_display: "DemoChannel".to_string(),
			}),
			message: Some(pb::ChatMessage {
				author_id: "123".to_string(),
				author_login: "demo_user".to_string(),
				author_display: "DemoUser".to_string(),
				text: "synthetic smoke-test message".to_string(),
				platform_time_unix_ms: unix_ms_now(),
				badge_ids: Vec::new(),
			}),
			server_message_id: "server-msg-1".to_string(),
			platform_message_id: String::new(),
			reply: None,
		})),
	};

	tracing::debug!("server: sending EventEnvelope on events stream");
	send_envelope(
		&mut events_send,
		pb::Envelope {
			version: PROTOCOL_VERSION,
			request_id: String::new(),
			msg: Some(pb::envelope::Msg::Event(event)),
		},
	)
	.await
	.context("send event")?;
	tracing::info!("server: sent synthetic event");

	let _ = events_send.finish();

	events_accept_task.await??;

	match reader.await {
		Ok(Ok(())) => {}
		Ok(Err(e)) => {
			tracing::debug!(error = %e, "server: control reader ended (expected during shutdown)");
		}
		Err(join_err) => {
			tracing::debug!(error = %join_err, "server: control reader task join error (ignored in smoke test)");
		}
	}

	tracing::debug!("server: exiting run_minimal_server()");
	Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quic_smoke_client_receives_synthetic_event() -> anyhow::Result<()> {
	init_test_logging();
	tracing::debug!("client(test): starting quic_smoke_client_receives_synthetic_event");

	let _ = rustls::crypto::CryptoProvider::install_default(rustls::crypto::aws_lc_rs::default_provider());

	let bind_addr: SocketAddr = "127.0.0.1:0".parse().context("parse bind addr")?;
	let (endpoint, _cert_der) = make_quic_server(bind_addr)?;

	let state = Arc::new(RwLock::new(GlobalState::default()));
	let (ready_tx, ready_rx) = oneshot::channel::<SocketAddr>();

	let server_state = Arc::clone(&state);
	let server_task = tokio::spawn(async move { run_minimal_server(endpoint, server_state, ready_tx).await });

	let mut server_addr = ready_rx.await.context("server ready")?;

	if server_addr.ip().is_unspecified() {
		server_addr.set_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)));
	}

	tracing::info!(?server_addr, "client(test): server ready");

	let cfg = ClientConfigV1 {
		server_host: "localhost".to_string(),
		server_port: server_addr.port(),
		server_addr: Some(server_addr),
		client_name: "chatty-test-client".to_string(),
		client_instance_id: "test-instance".to_string(),
		..ClientConfigV1::default()
	};

	tracing::debug!("client(test): Session::connect()");
	let (mut control, _welcome) = SessionControl::connect(cfg).await.context("client connect")?;
	tracing::info!("client(test): connected");

	let topic = "room:twitch/demo".to_string();
	tracing::debug!(%topic, "client(test): subscribe()");
	let _subscribed = control.subscribe(vec![topic.clone()]).await.context("subscribe")?;
	tracing::info!(%topic, "client(test): subscribed");

	let mut events = control.open_events_stream().await.context("open events stream")?;

	let (got_tx, got_rx) = oneshot::channel::<pb::EventEnvelope>();

	let mut sent = Some(got_tx);
	let session_task = tokio::spawn(async move {
		events
			.run_events_loop(|ev| {
				tracing::debug!(topic = %ev.topic, cursor = ev.cursor, "client(test): received event");
				if let Some(tx) = sent.take() {
					let _ = tx.send(ev);
				}
			})
			.await
	});

	let ev = tokio::time::timeout(Duration::from_secs(5), got_rx)
		.await
		.context("timeout waiting for event")?
		.context("event channel closed")?;

	assert_eq!(ev.topic, topic);
	assert_eq!(ev.cursor, 1);
	match ev.event {
		Some(pb::event_envelope::Event::ChatMessage(cm)) => {
			let msg = cm.message.expect("chat message is present");
			assert_eq!(msg.author_display, "DemoUser");
			assert_eq!(msg.text, "synthetic smoke-test message");
		}
		other => panic!("expected ChatMessage event, got: {other:?}"),
	}

	{
		let st = state.read().await;
		assert!(st.subscribed, "server should have processed Subscribe");
	}

	session_task.abort();
	let _ = session_task.await;

	let server_res = server_task.await.context("server join")?;
	server_res.context("server run")?;

	Ok(())
}
