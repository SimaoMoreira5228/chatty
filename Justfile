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

BIN_CLIENT := chatty_client_gui
BIN_SERVER := chatty_server
PROFILE_DIR := profiles

profile-build target="client":
    if [ "{{target}}" = "server" ]; then BIN={{BIN_SERVER}}; else BIN={{BIN_CLIENT}}; fi
    cargo build --release -p $BIN

flamegraph target="client" *args:
    if [ "{{target}}" = "server" ]; then BIN={{BIN_SERVER}}; else BIN={{BIN_CLIENT}}; fi
    cargo build --release -p $BIN
    mkdir -p {{PROFILE_DIR}}
    perf record -F 99 -g -- target/release/$BIN {{ args }}
    perf script > {{PROFILE_DIR}}/$BIN.perf
    if command -v flamegraph.pl >/dev/null 2>&1; then
        perf script | flamegraph.pl > {{PROFILE_DIR}}/$BIN-flamegraph.svg
    elif command -v flamegraph >/dev/null 2>&1; then
        perf script | flamegraph > {{PROFILE_DIR}}/$BIN-flamegraph.svg
    else
        echo "FlameGraph script not found in PATH. Install FlameGraph (flamegraph.pl) to generate SVG." >&2
        echo "perf data saved at {{PROFILE_DIR}}/$BIN.perf"
    fi

perf-record target="client" *args:
    if [ "{{target}}" = "server" ]; then BIN={{BIN_SERVER}}; else BIN={{BIN_CLIENT}}; fi
    cargo build --release -p $BIN
    mkdir -p {{PROFILE_DIR}}
    perf record -F 99 -g -o {{PROFILE_DIR}}/$BIN.perf.data -- target/release/$BIN {{ args }}
    perf report -i {{PROFILE_DIR}}/$BIN.perf.data --stdio > {{PROFILE_DIR}}/$BIN-perf-report.txt || true
    echo "Perf data: {{PROFILE_DIR}}/$BIN.perf.data"

heaptrack target="client" *args:
    if [ "{{target}}" = "server" ]; then BIN={{BIN_SERVER}}; else BIN={{BIN_CLIENT}}; fi
    cargo build --release -p $BIN
    mkdir -p {{PROFILE_DIR}}
    heaptrack -o {{PROFILE_DIR}}/$BIN.heaptrack -- target/release/$BIN {{ args }}
    echo "Heaptrack data written to {{PROFILE_DIR}}/$BIN.heaptrack"

massif target="client" *args:
    if [ "{{target}}" = "server" ]; then BIN={{BIN_SERVER}}; else BIN={{BIN_CLIENT}}; fi
    cargo build --release -p $BIN
    mkdir -p {{PROFILE_DIR}}
    valgrind --tool=massif --massif-out-file={{PROFILE_DIR}}/massif-$BIN.out -- target/release/$BIN {{ args }}
    ms_print {{PROFILE_DIR}}/massif-$BIN.out > {{PROFILE_DIR}}/massif-$BIN.txt || true
    echo "Massif output: {{PROFILE_DIR}}/massif-$BIN.out (text: massif-$BIN.txt)"

callgrind target="client" *args:
    if [ "{{target}}" = "server" ]; then BIN={{BIN_SERVER}}; else BIN={{BIN_CLIENT}}; fi
    cargo build --release -p $BIN
    mkdir -p {{PROFILE_DIR}}
    valgrind --tool=callgrind --callgrind-out-file={{PROFILE_DIR}}/callgrind-$BIN.out -- target/release/$BIN {{ args }}
    echo "Callgrind output: {{PROFILE_DIR}}/callgrind-$BIN.out"

helgrind target="client" *args:
    if [ "{{target}}" = "server" ]; then BIN={{BIN_SERVER}}; else BIN={{BIN_CLIENT}}; fi
    cargo build --release -p $BIN
    mkdir -p {{PROFILE_DIR}}
    valgrind --tool=helgrind --log-file={{PROFILE_DIR}}/helgrind-$BIN.log -- target/release/$BIN {{ args }}
    echo "Helgrind log: {{PROFILE_DIR}}/helgrind-$BIN.log"

jemalloc target="client" duration="" *args:
    # Set BIN
    if [ "{{target}}" = "server" ]; then BIN={{BIN_SERVER}}; else BIN={{BIN_CLIENT}}; fi
    cargo build --release -p $BIN
    mkdir -p {{PROFILE_DIR}}
    # Allow user to override JEMALLOC_LIB path (LD_PRELOAD) and JE_MALLOC_CONF via env
    : ${JEMALLOC_LIB:=}
    : ${JE_MALLOC_CONF:=prof:true,lg_prof_sample:17,prof_prefix:jeprof}
    if [ -n "$JEMALLOC_LIB" ]; then
        LD_PRELOAD="$JEMALLOC_LIB" JE_MALLOC_CONF="$JE_MALLOC_CONF" \ 
            target/release/$BIN {{ args }} &
    else
        JE_MALLOC_CONF="$JE_MALLOC_CONF" target/release/$BIN {{ args }} &
    fi
    PID=$!
    echo "Running $BIN with jemalloc profiling (PID=$PID). JE_MALLOC_CONF=$JE_MALLOC_CONF"
    if [ -n "{{duration}}" ]; then
        sleep {{duration}}
        kill -INT $PID || true
    else
        echo "Run completes or press Ctrl-C to stop; jemalloc profile files (jeprof.*) will appear in cwd."
    fi

mimalloc target="client" *args:
    if [ "{{target}}" = "server" ]; then BIN={{BIN_SERVER}}; else BIN={{BIN_CLIENT}}; fi
    cargo build --release -p $BIN
    mkdir -p {{PROFILE_DIR}}
    : ${MIMALLOC_PATH:=}
    if [ -n "$MIMALLOC_PATH" ]; then
        LD_PRELOAD="$MIMALLOC_PATH" target/release/$BIN {{ args }}
    else
        echo "Set MIMALLOC_PATH to path of libmimalloc.so to preload mimalloc" >&2
        target/release/$BIN {{ args }}
    fi

jmalloc target="client" *args:
    if [ "{{target}}" = "server" ]; then BIN={{BIN_SERVER}}; else BIN={{BIN_CLIENT}}; fi
    cargo build --release -p $BIN
    mkdir -p {{PROFILE_DIR}}
    : ${JMALLOC_PATH:=}
    if [ -n "$JMALLOC_PATH" ]; then
        LD_PRELOAD="$JMALLOC_PATH" target/release/$BIN {{ args }}
    else
        echo "Set JMALLOC_PATH to path of your jmalloc .so to preload jmalloc" >&2
        target/release/$BIN {{ args }}
    fi
