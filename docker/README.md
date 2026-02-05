# Docker deployment (Caddy)

This folder contains Docker assets for deploying Chatty with a Caddy HTTPS ingress and a server-rendered website (OAuth).

## Services

- `chatty-server`: QUIC server and Kick websocket handler.
- `chatty-web`: Astro server for public pages + OAuth callbacks.
- `caddy`: TLS termination, static site hosting, and webhook reverse proxy.

## Required files

Create a config file at:

- `docker/server-config/config.toml`

You can start from the example in `crates/chatty_server/config/chatty_server.toml.example`.

Kick websocket settings are configured in the server config (see `kick.pusher_ws_url`).

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

The server also reads TWITCH_CLIENT_ID / TWITCH_CLIENT_SECRET and KICK_CLIENT_ID / KICK_CLIENT_SECRET from the environment.

## Ports

- `18203/udp` QUIC server (exposed from host)
- `18207` health HTTP (internal only)
- `18208` metrics HTTP (internal only)
- `80/443` Caddy HTTPS ingress
- `4321` web server (internal only)

## OAuth redirect URLs

Configure your OAuth apps with these redirect URLs:

- `https://$CHATTY_DOMAIN/api/twitch/callback`
- `https://$CHATTY_DOMAIN/api/kick/callback`

Kick chat ingestion uses the Pusher websocket connection.
