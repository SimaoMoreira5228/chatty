mod? local

fmt *args:
    ./tools/dprint.dotslash fmt

lint *args:
    cargo clippy --fix --allow-dirty --allow-staged --all-features --all-targets {{ args }}
    python tools/i18n_audit.py

build *args:
    cargo build {{ args }}

test *args:
    cargo test --all-features --all-targets {{ args }}

run-server *args:
    cargo run -p chatty_server {{ args }}

run-client *args:
    cargo run -p chatty_client_gui {{ args }}

proto:
    cargo build -p chatty_protocol

proto-clean:
    cargo clean -p chatty_protocol
    cargo build -p chatty_protocol
