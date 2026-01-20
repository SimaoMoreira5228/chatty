#![forbid(unsafe_code)]

mod adapter;
mod client;

pub use adapter::{KickConfig, KickEventAdapter};
use anyhow::anyhow;
pub use client::{KickClient, KickEventSpec, KickEventSubscription, KickTokenIntrospection, KickUserInfo};

pub struct KickValidatedToken {
	pub token: KickTokenIntrospection,
	pub user: Option<KickUserInfo>,
}

pub async fn validate_user_token(token: &str) -> anyhow::Result<KickValidatedToken> {
	let client = KickClient::new("https://api.kick.com", token);
	let introspection = client.introspect_token(token).await?;
	if introspection.active != Some(true) {
		return Err(anyhow!("kick token is not active"));
	}
	let user = client.get_current_user(token).await.ok().flatten();
	Ok(KickValidatedToken {
		token: introspection,
		user,
	})
}
