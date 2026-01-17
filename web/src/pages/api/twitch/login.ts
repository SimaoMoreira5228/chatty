import type { APIRoute } from "astro";
import crypto from "node:crypto";

const TWITCH_AUTHORIZE_URL = "https://id.twitch.tv/oauth2/authorize";

function requiredEnv(name: string) {
	const value = import.meta.env[name];
	if (!value) {
		throw new Error(`Missing ${name}`);
	}
	return value;
}

export const GET: APIRoute = async ({ cookies, redirect }) => {
	try {
		const clientId = requiredEnv("TWITCH_CLIENT_ID");
		const redirectUri = requiredEnv("TWITCH_REDIRECT_URI");
		const scope = import.meta.env.TWITCH_SCOPES ?? "";
		const state = crypto.randomUUID();
		const secure = import.meta.env.PROD;

		cookies.set("chatty_twitch_state", state, { path: "/", httpOnly: true, sameSite: "lax", secure, maxAge: 600 });

		const url = new URL(TWITCH_AUTHORIZE_URL);
		url.searchParams.set("client_id", clientId);
		url.searchParams.set("redirect_uri", redirectUri);
		url.searchParams.set("response_type", "code");
		url.searchParams.set("state", state);
		if (scope) {
			url.searchParams.set("scope", scope);
		}

		return redirect(url.toString(), 302);
	} catch (err) {
		return new Response(`Twitch OAuth misconfigured: ${(err as Error).message}`, { status: 500 });
	}
};
