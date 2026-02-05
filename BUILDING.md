BUILDING / COMPILING — Important Environment Variables and Examples

This file documents the build-time and runtime environment variables required when building release artifacts for the Chatty project. It covers the standalone client and the server.

Client — build-time variables

The standalone client embed some configuration at compile time. For release builds you must provide these environment variables. The build will panic (fail) when required values are missing.

- CHATTY_SERVER_ENDPOINT
  - Description: The QUIC endpoint for the server in the form `quic://host:port`.
  - Requirement: Required for release builds. Must be a valid QUIC endpoint.
  - Example:
    CHATTY_SERVER_ENDPOINT=quic://chatty.example.com:443

- CHATTY_HMAC_ENABLED
  - Description: Whether build-time HMAC is enabled for client-server authentication.
  - Requirement: Must be set for release builds (valid values: `true` or `false`).
  - Example (enable): CHATTY_HMAC_ENABLED=true

- CHATTY_HMAC_KEY
  - Description: The HMAC key (string) used when `CHATTY_HMAC_ENABLED=true`.
  - Requirement: Required in release builds when `CHATTY_HMAC_ENABLED=true`.
  - Example:
    CHATTY_HMAC_KEY="base64-or-secret-string"

- CHATTY_TWITCH_LOGIN_URL and CHATTY_KICK_LOGIN_URL
  - Description: URLs used by the client to open web-based platform logins (may be empty strings in release builds but the variables must be set).
  - Requirement: Must be present in release builds (may be empty string if you don't provide a login URL).
  - Example (provide Twitch, leave Kick empty):
    CHATTY_TWITCH_LOGIN_URL="https://chatty.example.com/twitch" CHATTY_KICK_LOGIN_URL=""

Example release build command (client):

CHATTY_SERVER_ENDPOINT=quic://chatty.example.com:443\
CHATTY_HMAC_ENABLED=true\
CHATTY_HMAC_KEY="s3cr3t"\
CHATTY_TWITCH_LOGIN_URL="https://chatty.example.com/twitch"\
CHATTY_KICK_LOGIN_URL=""\
cargo build -p chatty_client_gui --release

Notes for the client:

- When a server endpoint is locked via build-time injection, the GUI will not persist (write) or restore the server endpoint in `gui-settings.toml`. This prevents accidental or stale endpoints from being stored in user config files.
- If HMAC is enabled at build time and a key is supplied, the GUI hides the HMAC input (since the key is provided at build time). If HMAC is enabled but no key is supplied (development builds), the GUI will show an input to enter the HMAC key.

Server — runtime configuration

The server behavior is driven by a configuration file plus environment variables. See `crates/chatty_server/config/chatty_server.toml.example` for an example configuration.

Required OAuth client credentials (server runtime env):

- TWITCH_CLIENT_ID / TWITCH_CLIENT_SECRET
- KICK_CLIENT_ID / KICK_CLIENT_SECRET

These are read from the environment only (no config file fallback).

Notable runtime env var overrides:

- CHATTY_SERVER_AUTH_HMAC_SECRET
  - Description: HMAC secret used by the server for stateless access tokens and auth verification.
  - Usage: Can be set in environment when launching the server and will override the value in the config file.
  - Example runtime invocation:
    CHATTY_SERVER_AUTH_HMAC_SECRET="s3cr3t" cargo run -p chatty_server --release

Building and running the server (example):

# build

cargo build -p chatty_server --release

# run using a config file

cargo run -p chatty_server --release -- --config path/to/chatty_server.toml

# run with env override for auth secret

CHATTY_SERVER_AUTH_HMAC_SECRET="s3cr3t" cargo run -p chatty_server --release

Notes for the server:

- Server configuration is primarily read from `crates/chatty_server/config/chatty_server.toml` (see the `.example` file). Many values can be overridden via environment variables as indicated in the example config.
- HMAC secret is sensitive; avoid exposing it in logs or committing it to source.

Further tips

- Use the `Justfile` targets and cargo workspace examples for common tasks (see `Justfile` in repo root).
- CI / release pipelines should set the required environment variables (client build-time vars and any necessary server runtime overrides) before running `cargo build --release`.

If you want the project to automatically fail builds when required release-time credentials are missing, ensure you build with `--release` (the repository's `build.rs` implementations enforce presence in release builds).
