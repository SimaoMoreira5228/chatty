import type { APIRoute } from "astro";

const KICK_USERS_URL = "https://api.kick.com/public/v1/users";

export const POST: APIRoute = async ({ request }) => {
	try {
		const data = await request.json();
		const token = String(data?.oauth_token ?? "");
		if (!token) {
			return new Response("missing oauth_token", { status: 400 });
		}

		const resp = await fetch(KICK_USERS_URL, { headers: { Authorization: `Bearer ${token}` } });

		if (!resp.ok) {
			const text = await resp.text();
			return new Response(`Kick user lookup failed: ${text}`, { status: 502 });
		}

		const json = await resp.json();
		const user = json?.data?.[0] ?? null;
		const user_id = user?.user_id?.toString?.() ?? "";
		const username = user?.name ?? "";

		return new Response(JSON.stringify({ user_id, username }), { headers: { "Content-Type": "application/json" } });
	} catch (err) {
		console.error("Kick lookup error:", err);
		return new Response(`Kick lookup error: ${(err as Error).message}`, { status: 500 });
	}
};
