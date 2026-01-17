import type { APIRoute } from "astro";
import crypto from "node:crypto";

const KICK_AUTHORIZE_URL = "https://id.kick.com/oauth/authorize";

function requiredEnv(name: string) {
	const value = process.env[name];
	if (!value) {
		throw new Error(`Missing ${name}`);
	}
	return value;
}

function base64Url(input: Buffer) {
	return input
		.toString("base64")
		.replace(/\+/g, "-")
		.replace(/\//g, "_")
		.replace(/=+$/g, "");
}

export const GET: APIRoute = async ({ cookies, redirect }) => {
	try {
		const clientId = requiredEnv("KICK_CLIENT_ID");
		const redirectUri = requiredEnv("KICK_REDIRECT_URI");
		const scope = requiredEnv("KICK_SCOPES") ?? "";

		const state = crypto.randomUUID();
		const verifier = base64Url(crypto.randomBytes(32));
		const challenge = base64Url(crypto.createHash("sha256").update(verifier).digest());
		const secure = import.meta.env.PROD;

		cookies.set("chatty_kick_state", state, { path: "/", httpOnly: true, sameSite: "lax", secure, maxAge: 600 });
		cookies.set("chatty_kick_verifier", verifier, { path: "/", httpOnly: true, sameSite: "lax", secure, maxAge: 600 });

		const url = new URL(KICK_AUTHORIZE_URL);
		url.searchParams.set("client_id", clientId);
		url.searchParams.set("redirect_uri", redirectUri);
		url.searchParams.set("response_type", "code");
		url.searchParams.set("code_challenge", challenge);
		url.searchParams.set("code_challenge_method", "S256");
		url.searchParams.set("state", state);
		if (scope) {
			url.searchParams.set("scope", scope);
		}

		return redirect(url.toString(), 302);
	} catch (err) {
		console.error("Kick OAuth misconfiguration:", err);
		return new Response(`Kick OAuth misconfigured: ${(err as Error).message}`, { status: 500 });
	}
};
