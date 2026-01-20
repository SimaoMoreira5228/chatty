#![forbid(unsafe_code)]

mod adapters;
mod config;
mod quic;
mod server;
mod util;

use std::net::SocketAddr;
use std::sync::Arc;

use chatty_platform::SecretString;
use chatty_platform::kick::{KickConfig, KickEventAdapter};
use chatty_platform::twitch::{TwitchConfig, TwitchEventSubAdapter};
use chatty_util::endpoint::QuicEndpoint;
use tokio::sync::RwLock;
use tracing::{info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::quic::config::QuicServerConfig;
use crate::server::adapter_manager::{AdapterManagerConfig, start_global_adapter_manager};
use crate::server::audit::AuditService;
use crate::server::connection::{ConnectionSettings, handle_connection};
use crate::server::health::{HealthState, spawn_health_server};
use crate::server::replay::{PersistentReplayBackend, ReplayService, ReplayStoreConfig};
use crate::server::room_hub::{RoomHub, RoomHubConfig};
use crate::server::router::{RouterConfig, spawn_ingest_router};
use crate::server::state::GlobalState;

/// Dev-only fake/demo adapter enable flag.
const CHATTY_ENABLE_FAKE_ADAPTER_ENV: &str = "CHATTY_ENABLE_FAKE_ADAPTER";

fn usage_and_exit() -> ! {
	eprintln!(
		"Usage: chatty_server [--bind quic://host:port]\n\
\n\
Options:\n\
\t--bind    Bind endpoint (default: quic://127.0.0.1:18203)\n\
\t         Format: quic://host:port\n\
\t--help   Show this help\n\
"
	);
	std::process::exit(2)
}

fn parse_args() -> SocketAddr {
	let mut bind_endpoint = "quic://127.0.0.1:18203".to_string();

	let mut it = std::env::args().skip(1);
	while let Some(arg) = it.next() {
		match arg.as_str() {
			"--help" | "-h" => usage_and_exit(),
			"--bind" | "--listen" => {
				let v = it.next().unwrap_or_else(|| usage_and_exit());
				if v.trim().is_empty() {
					eprintln!("--bind must be non-empty (expected quic://host:port)");
					usage_and_exit();
				}
				bind_endpoint = v;
			}
			other => {
				eprintln!("Unknown argument: {other}");
				usage_and_exit();
			}
		}
	}

	let bind = QuicEndpoint::parse(&bind_endpoint).unwrap_or_else(|e| {
		eprintln!("{e}");
		usage_and_exit();
	});

	let addr: SocketAddr = bind.to_socket_addr_if_ip_literal().unwrap_or_else(|e| {
		eprintln!("{e}");
		usage_and_exit();
	});

	addr
}

fn init_rustls_crypto_provider() {
	let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

fn init_tracing() {
	let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info,chatty_server=debug".to_string());

	let otlp_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
		.ok()
		.map(|v| v.trim().to_string())
		.filter(|v| !v.is_empty());
	let base = tracing_subscriber::registry()
		.with(tracing_subscriber::EnvFilter::new(filter))
		.with(tracing_subscriber::fmt::layer().with_target(false));

	if let Some(endpoint) = otlp_endpoint {
		use opentelemetry::global;
		use opentelemetry::trace::TracerProvider as _;
		use opentelemetry_otlp::WithExportConfig;

		match opentelemetry_otlp::SpanExporter::builder()
			.with_tonic()
			.with_endpoint(endpoint.clone())
			.build()
		{
			Ok(exporter) => {
				let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
					.with_batch_exporter(exporter)
					.build();
				let tracer = tracer_provider.tracer("chatty_server");
				global::set_tracer_provider(tracer_provider);

				let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
				base.with(otel_layer).init();
				info!(endpoint = %endpoint, "otlp tracing enabled");
			}
			Err(e) => {
				base.init();
				warn!(error = %e, "failed to initialize otlp tracing");
			}
		}
	} else {
		base.init();
	}
}

fn init_metrics(bind: Option<&str>) {
	let Some(bind) = bind else {
		return;
	};

	match bind.parse::<std::net::SocketAddr>() {
		Ok(addr) => {
			if let Err(e) = metrics_exporter_prometheus::PrometheusBuilder::new()
				.with_http_listener(addr)
				.install()
			{
				warn!(error = %e, "failed to start metrics exporter");
			} else {
				info!(%addr, "metrics exporter listening");
			}
		}
		Err(e) => {
			warn!(error = %e, %bind, "invalid metrics bind address (expected host:port)");
		}
	}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	init_rustls_crypto_provider();
	init_tracing();

	let bind_addr = parse_args();

	let config_path = crate::config::default_config_path()?;
	let server_cfg = crate::config::load_server_config_from_path(&config_path)?;
	info!(path = %config_path.display(), "loaded server config (toml + env overrides)");

	init_metrics(server_cfg.server.metrics_bind.as_deref());

	let health_state = HealthState::new();
	if let Some(bind) = server_cfg.server.health_bind.as_deref() {
		match bind.parse::<std::net::SocketAddr>() {
			Ok(addr) => {
				spawn_health_server(addr, health_state.clone());
				info!(%addr, "health server listening");
			}
			Err(e) => warn!(error = %e, %bind, "invalid health bind address (expected host:port)"),
		}
	}

	let quic_cfg = QuicServerConfig::dev(bind_addr);
	let endpoint = if let (Some(cert_path), Some(key_path)) = (
		server_cfg.server.tls_cert_path.as_deref(),
		server_cfg.server.tls_key_path.as_deref(),
	) {
		info!(cert = %cert_path.display(), key = %key_path.display(), "loading TLS cert/key");
		quic_cfg.bind_endpoint_with_tls(cert_path, key_path)?
	} else {
		let (endpoint, server_cert_der) = quic_cfg.bind_dev_endpoint()?;
		info!(
			bind = %bind_addr,
			cert_der_len = server_cert_der.len(),
			"chatty_server: QUIC endpoint ready (dev self-signed cert)"
		);
		endpoint
	};

	let conn_settings = ConnectionSettings {
		auth_token: server_cfg.auth_token.clone(),
		auth_hmac_secret: server_cfg.server.auth_hmac_secret.clone(),
		command_rate_limit_per_conn_burst: server_cfg.server.command_rate_limit_per_conn_burst,
		command_rate_limit_per_conn_per_minute: server_cfg.server.command_rate_limit_per_conn_per_minute,
		command_rate_limit_per_topic_burst: server_cfg.server.command_rate_limit_per_topic_burst,
		command_rate_limit_per_topic_per_minute: server_cfg.server.command_rate_limit_per_topic_per_minute,
		..ConnectionSettings::default()
	};

	let replay_cfg = ReplayStoreConfig {
		per_topic_capacity: if server_cfg.persistence.replay_enabled {
			server_cfg
				.persistence
				.replay_capacity
				.unwrap_or(ReplayStoreConfig::default().per_topic_capacity)
		} else {
			0
		},
		retention_secs: server_cfg.persistence.replay_retention_secs,
	};

	let replay_service = if server_cfg.persistence.enabled {
		let Some(database_url) = server_cfg.persistence.database_url.as_deref() else {
			return Err(anyhow::anyhow!("persistence enabled but no database_url configured"));
		};
		let backend = PersistentReplayBackend::connect(database_url, replay_cfg.per_topic_capacity).await?;
		Arc::new(ReplayService::new_persistent(backend, replay_cfg.clone()))
	} else if replay_cfg.per_topic_capacity > 0 {
		Arc::new(ReplayService::new_in_memory(replay_cfg.clone()))
	} else {
		Arc::new(ReplayService::disable_replay())
	};

	let audit_service = if server_cfg.persistence.enabled {
		let Some(database_url) = server_cfg.persistence.database_url.as_deref() else {
			return Err(anyhow::anyhow!("persistence enabled but no database_url configured"));
		};
		Arc::new(AuditService::connect(database_url).await?)
	} else {
		Arc::new(AuditService::disabled())
	};

	health_state.mark_ready();

	let mut next_conn_id: u64 = 1;

	loop {
		let Some(connecting) = endpoint.accept().await else {
			break;
		};

		let conn_id = next_conn_id;
		next_conn_id += 1;
		metrics::counter!("chatty_server_connections_total").increment(1);

		let server_cfg = server_cfg.clone();
		let conn_settings = conn_settings.clone();

		let replay_service = Arc::clone(&replay_service);
		let audit_service = Arc::clone(&audit_service);
		tokio::spawn(async move {
			match connecting.await {
				Ok(connection) => {
					tracing::info!(conn_id, remote = %connection.remote_address(), "accepted connection");

					let state = Arc::new(RwLock::new(GlobalState::default()));
					let mut platform_adapters: Vec<Box<dyn chatty_platform::PlatformAdapter>> = Vec::new();

					let client_id = server_cfg.twitch.client_id.clone().unwrap_or_default();
					if server_cfg.twitch.user_access_token.is_some() {
						warn!("twitch config: user_access_token ignored; user OAuth is required per-connection");
					}

					let mut twitch_cfg = TwitchConfig::new(client_id, SecretString::new(String::new()));

					if let Some(secret) = server_cfg.twitch.client_secret.clone() {
						twitch_cfg.client_secret = Some(secret);
					}
					if let Some(refresh_token) = server_cfg.twitch.refresh_token.clone() {
						twitch_cfg.refresh_token = Some(refresh_token);
					}
					if let Some(buffer) = server_cfg.twitch.refresh_buffer {
						twitch_cfg.refresh_buffer = buffer;
					}

					twitch_cfg.disable_refresh = server_cfg.twitch.disable_refresh;

					if let Some(ws_url) = server_cfg.twitch.eventsub_ws_url.clone() {
						twitch_cfg.eventsub_ws_url = ws_url;
					}
					if let Some(min) = server_cfg.twitch.reconnect_min_delay {
						twitch_cfg.reconnect_min_delay = min;
					}
					if let Some(max) = server_cfg.twitch.reconnect_max_delay {
						twitch_cfg.reconnect_max_delay = max;
					}
					if !server_cfg.twitch.broadcaster_id_overrides.is_empty() {
						twitch_cfg.broadcaster_id_overrides = server_cfg
							.twitch
							.broadcaster_id_overrides
							.iter()
							.map(|(k, v)| (k.clone(), v.clone()))
							.collect();
					}

					platform_adapters.push(Box::new(TwitchEventSubAdapter::new(twitch_cfg)));

					let kick_token = server_cfg
						.kick
						.user_access_token
						.clone()
						.unwrap_or_else(|| SecretString::new(String::new()));
					let mut kick_cfg = KickConfig::new(kick_token);
					if let Some(base_url) = server_cfg.kick.base_url.clone() {
						kick_cfg.base_url = base_url;
					}
					if let Some(path) = server_cfg.kick.webhook_path.clone() {
						kick_cfg.webhook_path = path;
					}
					if let Some(path) = server_cfg.kick.webhook_public_key_path.clone() {
						kick_cfg.webhook_public_key_path = Some(std::path::PathBuf::from(path));
					}
					if let Some(verify) = server_cfg.kick.webhook_verify_signatures {
						kick_cfg.webhook_verify_signatures = verify;
					}
					if let Some(enabled) = server_cfg.kick.webhook_auto_subscribe {
						kick_cfg.webhook_auto_subscribe = enabled;
					}
					if !server_cfg.kick.webhook_events.is_empty() {
						kick_cfg.webhook_events = server_cfg.kick.webhook_events.clone();
					}
					if let Some(bind) = server_cfg.kick.webhook_bind.clone() {
						match bind.parse::<std::net::SocketAddr>() {
							Ok(addr) => kick_cfg.webhook_bind = Some(addr),
							Err(e) => warn!(error = %e, bind = %bind, "kick webhook bind is invalid"),
						}
					}

					if !server_cfg.kick.broadcaster_id_overrides.is_empty() {
						kick_cfg.broadcaster_id_overrides = server_cfg
							.kick
							.broadcaster_id_overrides
							.iter()
							.map(|(k, v)| (k.clone(), v.clone()))
							.collect();
					}

					platform_adapters.push(Box::new(KickEventAdapter::new(kick_cfg)));

					platform_adapters.push(Box::new(crate::adapters::NullAdapter::new(chatty_domain::Platform::YouTube)));

					let fake_enabled = cfg!(debug_assertions)
						&& std::env::var(CHATTY_ENABLE_FAKE_ADAPTER_ENV)
							.map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
							.unwrap_or(false);

					if fake_enabled {
						info!(
							env = CHATTY_ENABLE_FAKE_ADAPTER_ENV,
							"starting dev-only fake adapter (enabled by env)"
						);
						platform_adapters.push(Box::new(crate::adapters::DemoAdapter::new()));
					}

					let adapter_manager = Arc::new(start_global_adapter_manager(
						Arc::clone(&state),
						AdapterManagerConfig::default(),
						platform_adapters,
					));
					let room_hub = RoomHub::new(RoomHubConfig::default());
					let _room_hub =
						spawn_ingest_router(Arc::clone(&adapter_manager), room_hub.clone(), RouterConfig::default());

					if let Err(e) = handle_connection(
						conn_id,
						connection,
						state,
						adapter_manager,
						room_hub,
						replay_service,
						audit_service,
						conn_settings,
					)
					.await
					{
						warn!(conn_id, error = %e, "connection handler exited with error");
					}
				}
				Err(e) => {
					warn!(conn_id, error = %e, "failed to establish QUIC connection");
				}
			}
		});
	}

	Ok(())
}
