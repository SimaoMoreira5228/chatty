# Docker deployment (Caddy)

This folder contains Docker assets for deploying Chatty with a Caddy HTTPS ingress and a server-rendered website (OAuth).

## Services

- `chatty-server`: QUIC server and webhook handler.
- `chatty-web`: Astro server for public pages + OAuth callbacks.
- `caddy`: TLS termination, static site hosting, and webhook reverse proxy.

## Required files

Create a config file at:

- `docker/server-config/config.toml`

You can start from the example in `crates/chatty_server/config/chatty_server.toml.example`.

Make sure the Kick webhook settings match your external URL:

- `kick.webhook_path = "/kick/events"`
- Optional: `kick.system_access_token = "..."` to manage webhook subscriptions without using the first user token.

## Environment

Create `docker/.env` based on the example:

- `CHATTY_DOMAIN`
- `CHATTY_ACME_EMAIL`
- `TWITCH_CLIENT_ID`
- `TWITCH_CLIENT_SECRET`
- `TWITCH_REDIRECT_URI`
- `TWITCH_SCOPES`
- `KICK_CLIENT_ID`
- `KICK_CLIENT_SECRET`
- `KICK_REDIRECT_URI`
- `KICK_SCOPES`

Optional:

- `CHATTY_KICK_SYSTEM_ACCESS_TOKEN`

## Ports

- `18203/udp` QUIC server (exposed from host)
- `18206` webhook HTTP (internal only)
- `18207` health HTTP (internal only)
- `18208` metrics HTTP (internal only)
- `80/443` Caddy HTTPS ingress
- `4321` web server (internal only)

## Webhook URL

Configure Kick to call:

- `https://$CHATTY_DOMAIN/kick/events`

## OAuth redirect URLs

Configure your OAuth apps with these redirect URLs:

- `https://$CHATTY_DOMAIN/api/twitch/callback`
- `https://$CHATTY_DOMAIN/api/kick/callback`

Caddy forwards `/kick/events` to the server’s webhook listener.
