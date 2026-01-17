#![forbid(unsafe_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, anyhow};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct AuthClaims {
	pub sub: String,
	pub exp: u64,
}

pub fn verify_hmac_token(token: &str, secret: &str) -> anyhow::Result<AuthClaims> {
	let parts = token.split('.').collect::<Vec<_>>();
	if parts.len() != 3 || parts[0] != "v1" {
		return Err(anyhow!("invalid token format"));
	}

	let payload_b64 = parts[1];
	let sig_b64 = parts[2];

	let payload = URL_SAFE_NO_PAD.decode(payload_b64).context("decode token payload")?;
	let expected_sig = sign(payload_b64.as_bytes(), secret.as_bytes());
	let provided_sig = URL_SAFE_NO_PAD.decode(sig_b64).context("decode token signature")?;

	if !constant_time_eq(&expected_sig, &provided_sig) {
		return Err(anyhow!("invalid token signature"));
	}

	let claims: AuthClaims = serde_json::from_slice(&payload).context("parse token claims")?;
	let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
	if claims.exp <= now {
		return Err(anyhow!("token expired"));
	}

	Ok(claims)
}

fn sign(payload_b64: &[u8], secret: &[u8]) -> Vec<u8> {
	let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("hmac key");
	mac.update(payload_b64);
	mac.finalize().into_bytes().to_vec()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
	if a.len() != b.len() {
		return false;
	}

	let mut diff = 0u8;
	for (x, y) in a.iter().zip(b.iter()) {
		diff |= x ^ y;
	}

	diff == 0
}
