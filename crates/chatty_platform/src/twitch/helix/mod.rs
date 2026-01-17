#![forbid(unsafe_code)]

use anyhow::Context;
use reqwest::StatusCode;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

const EVENTSUB_SUBSCRIPTIONS_PATH: &str = "/helix/eventsub/subscriptions";
const CHAT_MESSAGES_PATH: &str = "/helix/chat/messages";
const MODERATION_BANS_PATH: &str = "/helix/moderation/bans";
const MODERATION_CHAT_PATH: &str = "/helix/moderation/chat";
const MODERATION_MODERATORS_PATH: &str = "/helix/moderation/moderators";
const TOKEN_VALIDATE_URL: &str = "https://id.twitch.tv/oauth2/validate";
const TOKEN_REFRESH_URL: &str = "https://id.twitch.tv/oauth2/token";

fn retry_delay_from_headers(headers: &HeaderMap) -> Option<Duration> {
	if let Some(v) = headers.get(RETRY_AFTER)
		&& let Ok(s) = v.to_str()
		&& let Ok(secs) = s.trim().parse::<u64>()
	{
		return Some(Duration::from_secs(secs));
	}

	if let Some(v) = headers.get("Ratelimit-Reset")
		&& let Ok(s) = v.to_str()
		&& let Ok(reset_unix) = s.trim().parse::<u64>()
	{
		let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
		if reset_unix > now {
			return Some(Duration::from_secs(reset_unix - now));
		}
	}

	None
}

async fn send_with_retry(req: reqwest::RequestBuilder, label: &'static str) -> anyhow::Result<reqwest::Response> {
	let retry_builder = req.try_clone();
	let resp = req.send().await.with_context(|| format!("helix {label} send"))?;
	let status = resp.status();

	if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
		let body = resp.text().await.unwrap_or_default();
		anyhow::bail!("helix auth failed (status={status}) body={body}");
	}

	if status == StatusCode::TOO_MANY_REQUESTS
		&& let Some(delay) = retry_delay_from_headers(resp.headers())
		&& let Some(retry) = retry_builder
	{
		tokio::time::sleep(delay).await;
		let retry_resp = retry.send().await.with_context(|| format!("helix {label} retry send"))?;
		return Ok(retry_resp);
	}

	if status.is_server_error()
		&& let Some(retry) = retry_builder
	{
		tokio::time::sleep(Duration::from_millis(250)).await;
		let retry_resp = retry.send().await.with_context(|| format!("helix {label} retry send"))?;
		return Ok(retry_resp);
	}

	Ok(resp)
}

#[derive(Debug, Clone, Deserialize)]
pub struct TwitchTokenValidation {
	pub client_id: String,
	pub login: String,
	pub user_id: String,
	pub expires_in: u64,
	#[serde(default)]
	pub scopes: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct HelixClient {
	http: reqwest::Client,
	base_url: Url,
	client_id: String,
	bearer_token: String,
}

#[derive(Debug, Serialize)]
struct HelixChannelBroadcasterOnlyCondition<'a> {
	broadcaster_user_id: &'a str,
}

#[derive(Debug, Serialize)]
struct HelixChannelModerateCondition<'a> {
	broadcaster_user_id: &'a str,
	moderator_user_id: &'a str,
}

#[derive(Debug, Serialize)]
struct HelixChannelRaidConditionTo<'a> {
	to_broadcaster_user_id: &'a str,
}

#[derive(Debug, Serialize)]
struct HelixSendChatMessage<'a> {
	broadcaster_id: &'a str,
	sender_id: &'a str,
	message: &'a str,
	#[serde(skip_serializing_if = "Option::is_none")]
	reply_parent_message_id: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct HelixBanRequest<'a> {
	data: HelixBanData<'a>,
}

#[derive(Debug, Serialize)]
struct HelixBanData<'a> {
	user_id: &'a str,
	#[serde(skip_serializing_if = "Option::is_none")]
	duration: Option<u32>,
	#[serde(skip_serializing_if = "Option::is_none")]
	reason: Option<&'a str>,
}

impl HelixClient {
	pub(crate) fn new(base_url: Url, client_id: String, bearer_token: String) -> anyhow::Result<Self> {
		let http = reqwest::Client::builder()
			.user_agent("chatty/0.x (eventsub-ws)")
			.build()
			.context("build reqwest client")?;

		Ok(Self {
			http,
			base_url,
			client_id,
			bearer_token,
		})
	}

	fn authed(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
		req.header("Client-Id", &self.client_id)
			.header("Authorization", format!("Bearer {}", self.bearer_token))
	}

	async fn create_eventsub_subscription<TCondition: Serialize>(
		&self,
		kind: &'static str,
		version: &'static str,
		session_id: &str,
		condition: TCondition,
	) -> anyhow::Result<HelixCreateSubscriptionResponse> {
		let url = self.url(EVENTSUB_SUBSCRIPTIONS_PATH)?;

		let req = HelixCreateSubscriptionRequestGeneric {
			r#type: kind,
			version,
			condition,
			transport: HelixWebsocketTransport {
				method: "websocket",
				session_id,
			},
		};

		let resp = send_with_retry(
			self.authed(self.http.post(url)).json(&req),
			"POST /helix/eventsub/subscriptions",
		)
		.await
		.with_context(|| format!("helix POST {EVENTSUB_SUBSCRIPTIONS_PATH} send (type={kind})"))?;

		let status = resp.status();
		let body = resp
			.text()
			.await
			.with_context(|| format!("helix POST {EVENTSUB_SUBSCRIPTIONS_PATH} read body (type={kind})"))?;

		if status == StatusCode::CONFLICT {
			anyhow::bail!("helix create subscription conflict (type={kind}): body={body}");
		}
		if !status.is_success() {
			anyhow::bail!("helix create subscription failed (type={kind}): status={status} body={body}");
		}

		serde_json::from_str(&body).with_context(|| format!("helix create subscription parse json (type={kind})"))
	}

	fn url(&self, path_and_query: &str) -> anyhow::Result<Url> {
		self.base_url.join(path_and_query).context("join helix url")
	}

	pub(crate) async fn get_user_by_login(&self, login: &str) -> anyhow::Result<Option<HelixUser>> {
		let url = self.url(&format!("/helix/users?login={}", urlencoding::encode(login)))?;

		let resp = send_with_retry(self.authed(self.http.get(url)), "GET /helix/users")
			.await
			.context("helix GET /helix/users send")?;

		let status = resp.status();
		let body = resp.text().await.context("helix GET /helix/users read body")?;

		if !status.is_success() {
			anyhow::bail!("helix GET /helix/users failed: status={status} body={body}");
		}

		let parsed: HelixUsersResponse = serde_json::from_str(&body).context("helix users parse json")?;
		Ok(parsed.data.into_iter().next())
	}

	pub(crate) async fn get_token_user(&self) -> anyhow::Result<HelixUser> {
		let url = self.url("/helix/users")?;

		let resp = send_with_retry(self.authed(self.http.get(url)), "GET /helix/users (whoami)")
			.await
			.context("helix GET /helix/users (whoami) send")?;

		let status = resp.status();
		let body = resp.text().await.context("helix GET /helix/users (whoami) read body")?;

		if !status.is_success() {
			anyhow::bail!("helix GET /helix/users (whoami) failed: status={status} body={body}");
		}

		let parsed: HelixUsersResponse = serde_json::from_str(&body).context("helix users (whoami) parse json")?;

		parsed.data.into_iter().next().context("helix whoami returned empty data")
	}

	pub(crate) async fn is_user_moderator_in_channel(&self, broadcaster_id: &str, user_id: &str) -> anyhow::Result<bool> {
		let url = self.url(&format!(
			"{base}?broadcaster_id={b}&user_id={u}",
			base = MODERATION_MODERATORS_PATH,
			b = urlencoding::encode(broadcaster_id),
			u = urlencoding::encode(user_id),
		))?;

		let resp = send_with_retry(self.authed(self.http.get(url)), "GET /helix/moderation/moderators")
			.await
			.context("helix GET /helix/moderation/moderators send")?;

		let status = resp.status();
		let body = resp
			.text()
			.await
			.context("helix GET /helix/moderation/moderators read body")?;

		if !status.is_success() {
			anyhow::bail!("helix GET /helix/moderation/moderators failed: status={status} body={body}");
		}

		let parsed: HelixModeratorsResponse = serde_json::from_str(&body).context("helix moderators parse json")?;

		Ok(parsed.data.iter().any(|m| m.user_id == user_id))
	}

	pub(crate) async fn create_chat_message_subscription(
		&self,
		session_id: &str,
		broadcaster_user_id: &str,
		user_id: &str,
	) -> anyhow::Result<HelixCreateSubscriptionResponse> {
		self.create_eventsub_subscription(
			"channel.chat.message",
			"1",
			session_id,
			HelixChatMessageCondition {
				broadcaster_user_id,
				user_id,
			},
		)
		.await
	}

	pub(crate) async fn create_chat_message_delete_subscription(
		&self,
		session_id: &str,
		broadcaster_user_id: &str,
		user_id: &str,
	) -> anyhow::Result<HelixCreateSubscriptionResponse> {
		self.create_eventsub_subscription(
			"channel.chat.message_delete",
			"1",
			session_id,
			HelixChatMessageCondition {
				broadcaster_user_id,
				user_id,
			},
		)
		.await
	}

	pub(crate) async fn send_chat_message(
		&self,
		broadcaster_id: &str,
		sender_id: &str,
		message: &str,
		reply_to: Option<&str>,
	) -> anyhow::Result<()> {
		let url = self.url(CHAT_MESSAGES_PATH)?;
		let req = HelixSendChatMessage {
			broadcaster_id,
			sender_id,
			message,
			reply_parent_message_id: reply_to,
		};
		let resp = send_with_retry(self.authed(self.http.post(url)).json(&req), "POST /helix/chat/messages")
			.await
			.context("helix POST /helix/chat/messages send")?;
		let status = resp.status();
		let body = resp.text().await.unwrap_or_default();
		if !status.is_success() {
			anyhow::bail!("helix send chat failed: status={status} body={body}");
		}
		Ok(())
	}

	pub(crate) async fn delete_chat_message(
		&self,
		broadcaster_id: &str,
		moderator_id: &str,
		message_id: &str,
	) -> anyhow::Result<()> {
		let url = self.url(&format!(
			"{base}?broadcaster_id={b}&moderator_id={m}&message_id={msg}",
			base = MODERATION_CHAT_PATH,
			b = urlencoding::encode(broadcaster_id),
			m = urlencoding::encode(moderator_id),
			msg = urlencoding::encode(message_id),
		))?;
		let resp = send_with_retry(self.authed(self.http.delete(url)), "DELETE /helix/moderation/chat")
			.await
			.context("helix DELETE /helix/moderation/chat send")?;
		let status = resp.status();
		let body = resp.text().await.unwrap_or_default();
		if !status.is_success() {
			anyhow::bail!("helix delete message failed: status={status} body={body}");
		}
		Ok(())
	}

	pub(crate) async fn ban_user(
		&self,
		broadcaster_id: &str,
		moderator_id: &str,
		user_id: &str,
		duration_seconds: Option<u32>,
		reason: Option<&str>,
	) -> anyhow::Result<()> {
		let url = self.url(&format!(
			"{base}?broadcaster_id={b}&moderator_id={m}",
			base = MODERATION_BANS_PATH,
			b = urlencoding::encode(broadcaster_id),
			m = urlencoding::encode(moderator_id),
		))?;
		let req = HelixBanRequest {
			data: HelixBanData {
				user_id,
				duration: duration_seconds,
				reason,
			},
		};
		let resp = send_with_retry(self.authed(self.http.post(url)).json(&req), "POST /helix/moderation/bans")
			.await
			.context("helix POST /helix/moderation/bans send")?;
		let status = resp.status();
		let body = resp.text().await.unwrap_or_default();
		if !status.is_success() {
			anyhow::bail!("helix ban user failed: status={status} body={body}");
		}
		Ok(())
	}

	pub(crate) async fn create_channel_ban_subscription(
		&self,
		session_id: &str,
		broadcaster_user_id: &str,
	) -> anyhow::Result<HelixCreateSubscriptionResponse> {
		self.create_eventsub_subscription(
			"channel.ban",
			"1",
			session_id,
			HelixChannelBroadcasterOnlyCondition { broadcaster_user_id },
		)
		.await
	}

	#[allow(dead_code)]
	pub(crate) async fn create_channel_unban_subscription(
		&self,
		session_id: &str,
		broadcaster_user_id: &str,
	) -> anyhow::Result<HelixCreateSubscriptionResponse> {
		self.create_eventsub_subscription(
			"channel.unban",
			"1",
			session_id,
			HelixChannelBroadcasterOnlyCondition { broadcaster_user_id },
		)
		.await
	}

	pub(crate) async fn create_channel_cheer_subscription(
		&self,
		session_id: &str,
		broadcaster_user_id: &str,
	) -> anyhow::Result<HelixCreateSubscriptionResponse> {
		self.create_eventsub_subscription(
			"channel.cheer",
			"1",
			session_id,
			HelixChannelBroadcasterOnlyCondition { broadcaster_user_id },
		)
		.await
	}

	pub(crate) async fn create_channel_subscribe_subscription(
		&self,
		session_id: &str,
		broadcaster_user_id: &str,
	) -> anyhow::Result<HelixCreateSubscriptionResponse> {
		self.create_eventsub_subscription(
			"channel.subscribe",
			"1",
			session_id,
			HelixChannelBroadcasterOnlyCondition { broadcaster_user_id },
		)
		.await
	}

	pub(crate) async fn create_channel_raid_to_subscription(
		&self,
		session_id: &str,
		to_broadcaster_user_id: &str,
	) -> anyhow::Result<HelixCreateSubscriptionResponse> {
		self.create_eventsub_subscription(
			"channel.raid",
			"1",
			session_id,
			HelixChannelRaidConditionTo { to_broadcaster_user_id },
		)
		.await
	}

	pub(crate) async fn create_channel_moderate_subscription(
		&self,
		session_id: &str,
		broadcaster_user_id: &str,
		moderator_user_id: &str,
	) -> anyhow::Result<HelixCreateSubscriptionResponse> {
		self.create_eventsub_subscription(
			"channel.moderate",
			"1",
			session_id,
			HelixChannelModerateCondition {
				broadcaster_user_id,
				moderator_user_id,
			},
		)
		.await
	}

	pub(crate) async fn list_eventsub_subscriptions_by_type(
		&self,
		subscription_type: &str,
		after: Option<&str>,
	) -> anyhow::Result<HelixListSubscriptionsResponse> {
		let mut path = format!(
			"{base}?type={ty}",
			base = EVENTSUB_SUBSCRIPTIONS_PATH,
			ty = urlencoding::encode(subscription_type)
		);
		if let Some(after) = after {
			path.push_str("&after=");
			path.push_str(&urlencoding::encode(after));
		}

		let url = self.url(&path)?;

		let resp = send_with_retry(self.authed(self.http.get(url)), "GET /helix/eventsub/subscriptions")
			.await
			.context("helix GET /helix/eventsub/subscriptions send")?;

		let status = resp.status();
		let body = resp
			.text()
			.await
			.context("helix GET /helix/eventsub/subscriptions read body")?;

		if !status.is_success() {
			anyhow::bail!("helix list subscriptions failed: status={status} body={body}");
		}

		serde_json::from_str(&body).context("helix list subscriptions parse json")
	}

	pub(crate) async fn list_all_eventsub_subscriptions_by_type(
		&self,
		subscription_type: &str,
	) -> anyhow::Result<Vec<HelixSubscriptionData>> {
		let mut out: Vec<HelixSubscriptionData> = Vec::new();
		let mut after: Option<String> = None;

		loop {
			let page = self
				.list_eventsub_subscriptions_by_type(subscription_type, after.as_deref())
				.await?;

			out.extend(page.data.into_iter());

			let next = page.pagination.and_then(|p| p.cursor);
			if next.is_none() {
				break;
			}
			after = next;
		}

		Ok(out)
	}

	pub(crate) async fn delete_subscription(&self, subscription_id: &str) -> anyhow::Result<()> {
		let url = self.url(&format!(
			"{base}?id={}",
			urlencoding::encode(subscription_id),
			base = EVENTSUB_SUBSCRIPTIONS_PATH
		))?;

		let resp = send_with_retry(self.authed(self.http.delete(url)), "DELETE /helix/eventsub/subscriptions")
			.await
			.context("helix DELETE /helix/eventsub/subscriptions send")?;

		let status = resp.status();
		if status == StatusCode::NO_CONTENT || status.is_success() {
			return Ok(());
		}

		let body = resp
			.text()
			.await
			.context("helix DELETE /helix/eventsub/subscriptions read body")?;
		anyhow::bail!("helix delete subscription failed: status={status} body={body}");
	}
}

pub async fn validate_user_token(access_token: &str) -> anyhow::Result<TwitchTokenValidation> {
	let http = reqwest::Client::builder()
		.user_agent("chatty/0.x (oauth-validate)")
		.build()
		.context("build reqwest client")?;

	let resp = http
		.get(TOKEN_VALIDATE_URL)
		.header("Authorization", format!("OAuth {}", access_token))
		.send()
		.await
		.context("twitch validate token request")?;

	let status = resp.status();
	let body = resp.text().await.context("twitch validate token read body")?;

	if !status.is_success() {
		anyhow::bail!("twitch validate token failed: status={status} body={body}");
	}

	serde_json::from_str(&body).context("twitch validate token parse json")
}

#[derive(Debug, Deserialize)]
pub struct TwitchTokenRefreshResponse {
	pub access_token: String,
	#[serde(default)]
	pub refresh_token: Option<String>,
	pub expires_in: u64,
}

pub async fn refresh_user_token(
	client_id: &str,
	client_secret: &str,
	refresh_token: &str,
) -> anyhow::Result<TwitchTokenRefreshResponse> {
	let http = reqwest::Client::builder()
		.user_agent("chatty/0.x (oauth-refresh)")
		.build()
		.context("build reqwest client")?;

	let resp = http
		.post(TOKEN_REFRESH_URL)
		.form(&[
			("grant_type", "refresh_token"),
			("client_id", client_id),
			("client_secret", client_secret),
			("refresh_token", refresh_token),
		])
		.send()
		.await
		.context("twitch refresh token request")?;

	let status = resp.status();
	let body = resp.text().await.context("twitch refresh token read body")?;

	if !status.is_success() {
		anyhow::bail!("twitch refresh token failed: status={status} body={body}");
	}

	serde_json::from_str(&body).context("twitch refresh token parse json")
}

#[derive(Debug, Deserialize)]
pub(crate) struct HelixUsersResponse {
	pub(crate) data: Vec<HelixUser>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HelixUser {
	pub(crate) id: String,
	#[allow(dead_code)]
	pub(crate) login: String,

	#[allow(dead_code)]
	#[serde(default)]
	pub(crate) display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HelixModeratorsResponse {
	pub(crate) data: Vec<HelixModerator>,

	#[allow(dead_code)]
	#[serde(default)]
	pub(crate) pagination: Option<HelixPagination>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HelixModerator {
	pub(crate) user_id: String,

	#[allow(dead_code)]
	pub(crate) user_login: String,

	#[allow(dead_code)]
	pub(crate) user_name: String,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub(crate) struct HelixCreateSubscriptionRequest<'a> {
	#[serde(rename = "type")]
	pub(crate) r#type: &'a str,
	pub(crate) version: &'a str,
	pub(crate) condition: HelixChatMessageCondition<'a>,
	pub(crate) transport: HelixWebsocketTransport<'a>,
}

#[derive(Debug, Serialize)]
struct HelixCreateSubscriptionRequestGeneric<'a, TCondition> {
	#[serde(rename = "type")]
	r#type: &'static str,
	version: &'static str,
	condition: TCondition,
	transport: HelixWebsocketTransport<'a>,
}

#[derive(Debug, Serialize)]
pub(crate) struct HelixChatMessageCondition<'a> {
	pub(crate) broadcaster_user_id: &'a str,
	pub(crate) user_id: &'a str,
}

#[derive(Debug, Serialize)]
pub(crate) struct HelixWebsocketTransport<'a> {
	pub(crate) method: &'a str,
	pub(crate) session_id: &'a str,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HelixCreateSubscriptionResponse {
	pub(crate) data: Vec<HelixSubscriptionData>,

	#[allow(dead_code)]
	#[serde(default)]
	pub(crate) total: Option<u64>,

	#[allow(dead_code)]
	#[serde(default)]
	pub(crate) total_cost: Option<u64>,

	#[allow(dead_code)]
	#[serde(default)]
	pub(crate) max_total_cost: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HelixListSubscriptionsResponse {
	pub(crate) data: Vec<HelixSubscriptionData>,
	#[serde(default)]
	pub(crate) pagination: Option<HelixPagination>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HelixPagination {
	#[serde(default)]
	pub(crate) cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HelixSubscriptionData {
	pub(crate) id: String,

	#[allow(dead_code)]
	pub(crate) status: String,

	#[allow(dead_code)]
	#[serde(rename = "type")]
	pub(crate) r#type: String,

	#[allow(dead_code)]
	pub(crate) version: String,
	#[serde(default)]
	pub(crate) condition: serde_json::Value,

	#[allow(dead_code)]
	#[serde(default)]
	pub(crate) transport: Option<serde_json::Value>,
}
