import type { APIRoute } from "astro";

const KICK_TOKEN_URL = "https://id.kick.com/oauth/token";
const KICK_USERS_URL = "https://api.kick.com/public/v1/users";

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
		const storedState = cookies.get("chatty_kick_state")?.value ?? "";
		const verifier = cookies.get("chatty_kick_verifier")?.value ?? "";
		if (!code || !state || state !== storedState || !verifier) {
			return new Response("Invalid OAuth state", { status: 400 });
		}

		const clientId = requiredEnv("KICK_CLIENT_ID");
		const clientSecret = requiredEnv("KICK_CLIENT_SECRET");
		const redirectUri = requiredEnv("KICK_REDIRECT_URI");

		const tokenResp = await fetch(KICK_TOKEN_URL, {
			method: "POST",
			headers: { "Content-Type": "application/x-www-form-urlencoded" },
			body: new URLSearchParams({
				grant_type: "authorization_code",
				client_id: clientId,
				client_secret: clientSecret,
				redirect_uri: redirectUri,
				code,
				code_verifier: verifier,
			}).toString(),
		});

		if (!tokenResp.ok) {
			const text = await tokenResp.text();
			return new Response(`Kick token exchange failed: ${text}`, { status: 502 });
		}

		const tokenJson = (await tokenResp.json()) as { access_token?: string };
		const accessToken = tokenJson.access_token ?? "";
		if (!accessToken) {
			return new Response("Kick token missing", { status: 502 });
		}

		const userResp = await fetch(KICK_USERS_URL, { headers: { Authorization: `Bearer ${accessToken}` } });

		if (!userResp.ok) {
			const text = await userResp.text();
			return new Response(`Kick user lookup failed: ${text}`, { status: 502 });
		}

		const userJson = (await userResp.json()) as {
			data?: Array<{ id?: number | string; username?: string }>;
		};
		const user = userJson.data?.[0];

		const params = new URLSearchParams({
			oauth_token: accessToken,
			user_id: user?.id?.toString() ?? "",
			username: user?.username ?? "",
		});

		cookies.delete("chatty_kick_state", { path: "/" });
		cookies.delete("chatty_kick_verifier", { path: "/" });
		return redirect(`/kick#${params.toString()}`, 302);
	} catch (err) {
		return new Response(`Kick OAuth error: ${(err as Error).message}`, { status: 500 });
	}
};
