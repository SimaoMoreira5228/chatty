#![forbid(unsafe_code)]

use std::net::SocketAddr;

use chatty_client_core::{ClientConfigV1, DEFAULT_SERVER_ENDPOINT_QUIC, SessionControl};
use tracing::{info, warn};

fn usage_and_exit() -> ! {
	eprintln!(
		"Usage: chatty_client [--connect quic://host:port] [--addr ip:port] [--sni name] [--topic topic]...\n\
\n\
Options:\n\
	--connect   Server endpoint (alias: --endpoint) (default: baked build endpoint)\n\
	            Format: quic://host:port\n\
	--endpoint  Alias for --connect\n\
	--addr      Server SocketAddr (overrides DNS resolution from --connect)\n\
	            Default: derived from --connect (or baked endpoint)\n\
	--sni       TLS server name/SNI (overrides the host from --connect)\n\
	            Default: derived from --connect host\n\
	--topic     Topic to subscribe to (repeatable; default: room:twitch/demo)\n\
	--help      Show this help\n\
\n\
Notes:\n\
	Events are delivered over a second bidirectional QUIC stream.\n\
\n\
Examples:\n\
	chatty_client --connect quic://127.0.0.1:18203 --topic room:twitch/demo\n\
	chatty_client --connect quic://chatty.example.com:443 --topic room:twitch/a --topic room:twitch/b\n"
	);
	std::process::exit(2)
}

fn init_tracing() {
	let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info,chatty_client_core=debug".to_string());
	tracing_subscriber::fmt().with_env_filter(filter).with_target(false).init();
}

fn parse_args() -> (SocketAddr, String, Vec<String>) {
	let mut endpoint: String = DEFAULT_SERVER_ENDPOINT_QUIC.to_string();

	let mut addr_override: Option<SocketAddr> = None;
	let mut sni_override: Option<String> = None;

	let mut topics: Vec<String> = Vec::new();

	let mut it = std::env::args().skip(1);
	while let Some(arg) = it.next() {
		match arg.as_str() {
			"--help" | "-h" => usage_and_exit(),
			"--connect" | "--endpoint" => {
				let v = it.next().unwrap_or_else(|| usage_and_exit());
				if v.trim().is_empty() {
					eprintln!("--connect must be non-empty (expected quic://host:port)");
					usage_and_exit();
				}
				endpoint = v;
			}
			"--addr" => {
				let v = it.next().unwrap_or_else(|| usage_and_exit());
				let parsed: SocketAddr = v.parse().unwrap_or_else(|_| {
					eprintln!("Invalid --addr value: {v}");
					usage_and_exit()
				});
				addr_override = Some(parsed);
			}
			"--sni" => {
				let v = it.next().unwrap_or_else(|| usage_and_exit());
				if v.trim().is_empty() {
					eprintln!("--sni must be non-empty");
					usage_and_exit();
				}
				sni_override = Some(v);
			}
			"--topic" => {
				let t = it.next().unwrap_or_else(|| usage_and_exit());
				if t.trim().is_empty() {
					eprintln!("--topic must be non-empty");
					usage_and_exit();
				}
				topics.push(t);
			}
			other => {
				eprintln!("Unknown argument: {other}");
				usage_and_exit();
			}
		}
	}

	let (host, port) = ClientConfigV1::parse_quic_endpoint(&endpoint).unwrap_or_else(|e| {
		eprintln!("Invalid --endpoint value: {endpoint}\n{e}");
		usage_and_exit();
	});

	if topics.is_empty() {
		topics.push("room:twitch/demo".to_string());
	}

	let addr: SocketAddr = addr_override.unwrap_or_else(|| {
		// Placeholder when host isn't an IP literal; DNS resolves during connect.
		let ip_try: Result<SocketAddr, _> = format!("{host}:{port}").parse();
		ip_try.unwrap_or_else(|_| "0.0.0.0:0".parse().expect("valid placeholder addr"))
	});

	let sni: String = sni_override.unwrap_or(host);

	(addr, sni, topics)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	init_tracing();
	let (addr, sni, topics) = parse_args();

	let cfg = ClientConfigV1 {
		server_host: sni.clone(),
		server_port: addr.port(),
		server_addr: if addr.ip().is_unspecified() && addr.port() == 0 {
			None
		} else {
			Some(addr)
		},
		client_name: format!("chatty-client-cli/{}", env!("CARGO_PKG_VERSION")),
		client_instance_id: format!("cli-{}", std::process::id()),
		auth_token: std::env::var("CHATTY_CLIENT_AUTH_TOKEN").ok().and_then(|v| {
			let v = v.trim().to_string();
			(!v.is_empty()).then_some(v)
		}),
		user_oauth_token: std::env::var("CHATTY_CLIENT_USER_OAUTH_TOKEN").ok().and_then(|v| {
			let v = v.trim().to_string();
			(!v.is_empty()).then_some(v)
		}),
		twitch_client_id: std::env::var("CHATTY_CLIENT_TWITCH_CLIENT_ID").ok().and_then(|v| {
			let v = v.trim().to_string();
			(!v.is_empty()).then_some(v)
		}),
		twitch_user_id: std::env::var("CHATTY_CLIENT_TWITCH_USER_ID").ok().and_then(|v| {
			let v = v.trim().to_string();
			(!v.is_empty()).then_some(v)
		}),
		twitch_username: std::env::var("CHATTY_CLIENT_TWITCH_USERNAME").ok().and_then(|v| {
			let v = v.trim().to_string();
			(!v.is_empty()).then_some(v)
		}),
		kick_user_oauth_token: std::env::var("CHATTY_CLIENT_KICK_USER_OAUTH_TOKEN").ok().and_then(|v| {
			let v = v.trim().to_string();
			(!v.is_empty()).then_some(v)
		}),
		kick_user_id: std::env::var("CHATTY_CLIENT_KICK_USER_ID").ok().and_then(|v| {
			let v = v.trim().to_string();
			(!v.is_empty()).then_some(v)
		}),
		kick_username: std::env::var("CHATTY_CLIENT_KICK_USERNAME").ok().and_then(|v| {
			let v = v.trim().to_string();
			(!v.is_empty()).then_some(v)
		}),
		..ClientConfigV1::default()
	};

	let resolved = cfg.server_addr.map(|a| a.to_string()).unwrap_or_else(|| "<dns>".to_string());
	info!(server = %resolved, sni = %cfg.server_host, "connecting");

	let (mut control, _welcome) = SessionControl::connect(cfg).await?;
	let subscribed = control.subscribe(topics.clone()).await?;

	let ok_count = subscribed
		.results
		.iter()
		.filter(|r| r.status == (chatty_protocol::pb::subscription_result::Status::Ok as i32))
		.count();

	info!(
		ok = ok_count,
		total = subscribed.results.len(),
		"subscribed; opening events stream and entering events loop"
	);

	let mut events = control.open_events_stream().await?;

	events
		.run_events_loop(|ev| {
			if let Some(chatty_protocol::pb::event_envelope::Event::ChatMessage(cm)) = ev.event {
				let author = cm.message.as_ref().map(|m| m.author_display.as_str()).unwrap_or("<unknown>");
				let text = cm.message.as_ref().map(|m| m.text.as_str()).unwrap_or("");
				println!("[{} #{}] {}: {}", ev.topic, ev.cursor, author, text);
			} else {
				warn!("non-chat event on topic {} cursor {}", ev.topic, ev.cursor);
			}
		})
		.await?;

	Ok(())
}
