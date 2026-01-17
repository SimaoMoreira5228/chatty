#![forbid(unsafe_code)]

pub mod endpoint {
	use std::net::SocketAddr;

	/// Parsed `quic://host:port` endpoint.
	#[derive(Debug, Clone, PartialEq, Eq, Hash)]
	pub struct QuicEndpoint {
		pub host: String,
		pub port: u16,
	}

	impl QuicEndpoint {
		/// Returns `host:port` (host preserved, IPv6 stays bracketed).
		pub fn hostport(&self) -> String {
			format!("{}:{}", self.host, self.port)
		}

		/// Convert to `SocketAddr` only if the host is an IP literal.
		pub fn to_socket_addr_if_ip_literal(&self) -> Result<SocketAddr, String> {
			self.hostport()
				.parse()
				.map_err(|_| format!("host must be an IP literal (DNS names not supported here): {}", self.host))
		}

		/// Parse a QUIC endpoint string in the form `quic://host:port`.
		pub fn parse(s: &str) -> Result<Self, String> {
			let s = s.trim();
			if s.is_empty() {
				return Err("endpoint must be non-empty (expected quic://host:port)".to_string());
			}

			let rest = s
				.strip_prefix("quic://")
				.ok_or_else(|| format!("invalid endpoint (expected quic://host:port): {s}"))?;

			if rest.contains('/') || rest.contains('?') || rest.contains('#') {
				return Err(format!(
					"invalid endpoint (expected quic://host:port without path/query/fragment): {s}"
				));
			}

			let (host, port_str) = rest
				.rsplit_once(':')
				.ok_or_else(|| format!("invalid endpoint (missing :port, expected quic://host:port): {s}"))?;

			let host = host.trim();
			if host.is_empty() {
				return Err(format!("invalid endpoint host (expected quic://host:port): {s}"));
			}

			if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) {
				return Err(format!(
					"invalid endpoint host (IPv6 must be bracketed like quic://[::1]:18203): {s}"
				));
			}

			let port: u16 = port_str
				.trim()
				.parse()
				.map_err(|_| format!("invalid endpoint port (expected 1..=65535): {s}"))?;

			if port == 0 {
				return Err(format!("invalid endpoint port (expected 1..=65535): {s}"));
			}

			Ok(Self {
				host: host.to_string(),
				port,
			})
		}
	}

	/// Validate `quic://host:port`.
	pub fn validate_quic_endpoint(s: &str) -> Result<(), String> {
		let _ = QuicEndpoint::parse(s)?;
		Ok(())
	}

	#[cfg(test)]
	mod tests {
		use super::*;

		#[test]
		fn parses_dns_hostname() {
			let e = QuicEndpoint::parse("quic://chatty.example.com:443").unwrap();
			assert_eq!(e.host, "chatty.example.com");
			assert_eq!(e.port, 443);
			assert_eq!(e.hostport(), "chatty.example.com:443");
		}

		#[test]
		fn parses_ipv4() {
			let e = QuicEndpoint::parse("quic://127.0.0.1:18203").unwrap();
			assert_eq!(e.host, "127.0.0.1");
			assert_eq!(e.port, 18203);
			assert_eq!(e.hostport(), "127.0.0.1:18203");
		}

		#[test]
		fn parses_bracketed_ipv6() {
			let e = QuicEndpoint::parse("quic://[::1]:18203").unwrap();
			assert_eq!(e.host, "[::1]");
			assert_eq!(e.port, 18203);
			assert_eq!(e.hostport(), "[::1]:18203");
		}

		#[test]
		fn rejects_unbracketed_ipv6() {
			let err = QuicEndpoint::parse("quic://::1:18203").unwrap_err();
			assert!(err.to_lowercase().contains("ipv6"));
		}

		#[test]
		fn rejects_path_query_fragment() {
			assert!(QuicEndpoint::parse("quic://127.0.0.1:18203/").is_err());
			assert!(QuicEndpoint::parse("quic://127.0.0.1:18203?x=y").is_err());
			assert!(QuicEndpoint::parse("quic://127.0.0.1:18203#frag").is_err());
		}

		#[test]
		fn rejects_port_zero_and_missing_port() {
			assert!(QuicEndpoint::parse("quic://127.0.0.1:0").is_err());
			assert!(QuicEndpoint::parse("quic://127.0.0.1").is_err());
		}

		#[test]
		fn to_socket_addr_if_ip_literal_accepts_ip_literals() {
			let e4 = QuicEndpoint::parse("quic://127.0.0.1:18203").unwrap();
			let a4 = e4.to_socket_addr_if_ip_literal().unwrap();
			assert_eq!(a4.to_string(), "127.0.0.1:18203");

			let e6 = QuicEndpoint::parse("quic://[::1]:18203").unwrap();
			let a6 = e6.to_socket_addr_if_ip_literal().unwrap();
			assert_eq!(a6.to_string(), "[::1]:18203");
		}

		#[test]
		fn to_socket_addr_if_ip_literal_rejects_dns() {
			let e = QuicEndpoint::parse("quic://chatty.example.com:443").unwrap();
			assert!(e.to_socket_addr_if_ip_literal().is_err());
		}
	}
}
