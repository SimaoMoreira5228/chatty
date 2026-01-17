#![forbid(unsafe_code)]

use std::fs;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use quinn::{Endpoint, ServerConfig};
use rustls_pemfile::{certs, private_key};

/// Chatty QUIC server configuration (v1).
pub struct QuicServerConfig {
	pub bind_addr: SocketAddr,

	/// ALPN protocol identifiers accepted by the server.
	pub alpn_protocols: Vec<Vec<u8>>,

	pub max_concurrent_bidi_streams: u32,

	pub max_concurrent_uni_streams: u32,
}

impl QuicServerConfig {
	/// Reasonable defaults for local development.
	pub fn dev(bind_addr: SocketAddr) -> Self {
		Self {
			bind_addr,
			alpn_protocols: vec![b"chatty-v1".to_vec()],
			max_concurrent_bidi_streams: 64,
			max_concurrent_uni_streams: 64,
		}
	}

	/// Build a bound QUIC `Endpoint` and return the DER-encoded certificate.
	pub fn bind_dev_endpoint(&self) -> anyhow::Result<(Endpoint, Vec<u8>)> {
		let (server_config, cert_der) = self.build_dev_server_config()?;
		let endpoint = Endpoint::server(server_config, self.bind_addr).context("bind quinn endpoint")?;
		Ok((endpoint, cert_der))
	}

	/// Build a bound QUIC `Endpoint` using the provided TLS cert and key.
	pub fn bind_endpoint_with_tls(&self, cert_path: &Path, key_path: &Path) -> anyhow::Result<Endpoint> {
		let server_config = self.build_server_config_from_files(cert_path, key_path)?;
		let endpoint = Endpoint::server(server_config, self.bind_addr).context("bind quinn endpoint")?;
		Ok(endpoint)
	}

	/// Build a dev-only `ServerConfig` with a generated self-signed cert.
	pub fn build_dev_server_config(&self) -> anyhow::Result<(ServerConfig, Vec<u8>)> {
		let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).context("generate self-signed cert")?;

		let cert_der = ck.cert.der().to_vec();
		let key_der = ck.signing_key.serialize_der();

		let cert_chain = vec![rustls::pki_types::CertificateDer::from(cert_der.clone())];
		let key = rustls::pki_types::PrivateKeyDer::try_from(key_der).map_err(|e| anyhow!("parse private key der: {e}"))?;

		let mut tls_config = rustls::ServerConfig::builder()
			.with_no_client_auth()
			.with_single_cert(cert_chain, key)
			.context("build rustls server config")?;

		tls_config.alpn_protocols = self.alpn_protocols.clone();

		let quic_tls = quinn::crypto::rustls::QuicServerConfig::try_from(tls_config)
			.context("convert rustls ServerConfig -> quinn QuicServerConfig")?;

		let mut server_config = ServerConfig::with_crypto(Arc::new(quic_tls));

		let mut transport = quinn::TransportConfig::default();
		transport.max_concurrent_bidi_streams(quinn::VarInt::from_u32(self.max_concurrent_bidi_streams));
		transport.max_concurrent_uni_streams(quinn::VarInt::from_u32(self.max_concurrent_uni_streams));
		server_config.transport_config(Arc::new(transport));

		Ok((server_config, cert_der))
	}

	fn build_server_config_from_files(&self, cert_path: &Path, key_path: &Path) -> anyhow::Result<ServerConfig> {
		let cert_chain = load_cert_chain(cert_path)?;
		let key = load_private_key(key_path)?;

		let mut tls_config = rustls::ServerConfig::builder()
			.with_no_client_auth()
			.with_single_cert(cert_chain, key)
			.context("build rustls server config")?;

		tls_config.alpn_protocols = self.alpn_protocols.clone();

		let quic_tls = quinn::crypto::rustls::QuicServerConfig::try_from(tls_config)
			.context("convert rustls ServerConfig -> quinn QuicServerConfig")?;

		let mut server_config = ServerConfig::with_crypto(Arc::new(quic_tls));

		let mut transport = quinn::TransportConfig::default();
		transport.max_concurrent_bidi_streams(quinn::VarInt::from_u32(self.max_concurrent_bidi_streams));
		transport.max_concurrent_uni_streams(quinn::VarInt::from_u32(self.max_concurrent_uni_streams));
		server_config.transport_config(Arc::new(transport));

		Ok(server_config)
	}
}

fn load_cert_chain(path: &Path) -> anyhow::Result<Vec<rustls::pki_types::CertificateDer<'static>>> {
	let pem = fs::read(path).with_context(|| format!("read tls cert: {}", path.display()))?;
	let mut reader = BufReader::new(&pem[..]);
	let certs = certs(&mut reader).collect::<Result<Vec<_>, _>>().context("parse tls certs")?;

	if certs.is_empty() {
		return Err(anyhow!("no certificates found in {}", path.display()));
	}

	Ok(certs)
}

fn load_private_key(path: &Path) -> anyhow::Result<rustls::pki_types::PrivateKeyDer<'static>> {
	let pem = fs::read(path).with_context(|| format!("read tls key: {}", path.display()))?;
	let mut reader = BufReader::new(&pem[..]);
	let Some(key) = private_key(&mut reader).context("parse tls key")? else {
		return Err(anyhow!("no private key found in {}", path.display()));
	};
	Ok(key)
}
