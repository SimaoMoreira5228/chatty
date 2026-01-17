import type { APIRoute } from "astro";

const TWITCH_TOKEN_URL = "https://id.twitch.tv/oauth2/token";
const TWITCH_USERS_URL = "https://api.twitch.tv/helix/users";

function requiredEnv(name: string) {
	const value = import.meta.env[name];
	if (!value) {
		throw new Error(`Missing ${name}`);
	}
	return value;
}

export const GET: APIRoute = async ({ cookies, redirect, url }) => {
	try {
		const code = url.searchParams.get("code") ?? "";
		const state = url.searchParams.get("state") ?? "";
		const storedState = cookies.get("chatty_twitch_state")?.value ?? "";
		if (!code || !state || state !== storedState) {
			return new Response("Invalid OAuth state", { status: 400 });
		}

		const clientId = requiredEnv("TWITCH_CLIENT_ID");
		const clientSecret = requiredEnv("TWITCH_CLIENT_SECRET");
		const redirectUri = requiredEnv("TWITCH_REDIRECT_URI");

		const tokenResp = await fetch(TWITCH_TOKEN_URL, {
			method: "POST",
			headers: { "Content-Type": "application/x-www-form-urlencoded" },
			body: new URLSearchParams({
				client_id: clientId,
				client_secret: clientSecret,
				code,
				grant_type: "authorization_code",
				redirect_uri: redirectUri,
			}).toString(),
		});

		if (!tokenResp.ok) {
			const text = await tokenResp.text();
			return new Response(`Twitch token exchange failed: ${text}`, { status: 502 });
		}

		const tokenJson = (await tokenResp.json()) as { access_token?: string };
		const accessToken = tokenJson.access_token ?? "";
		if (!accessToken) {
			return new Response("Twitch token missing", { status: 502 });
		}

		const userResp = await fetch(TWITCH_USERS_URL, {
			headers: { Authorization: `Bearer ${accessToken}`, "Client-Id": clientId },
		});

		if (!userResp.ok) {
			const text = await userResp.text();
			return new Response(`Twitch user lookup failed: ${text}`, { status: 502 });
		}

		const userJson = (await userResp.json()) as { data?: Array<{ id?: string; login?: string }> };
		const user = userJson.data?.[0];
		const params = new URLSearchParams({
			username: user?.login ?? "",
			user_id: user?.id ?? "",
			client_id: clientId,
			oauth_token: accessToken,
		});

		cookies.delete("chatty_twitch_state", { path: "/" });
		return redirect(`/twitch#${params.toString()}`, 302);
	} catch (err) {
		return new Response(`Twitch OAuth error: ${(err as Error).message}`, { status: 500 });
	}
};
