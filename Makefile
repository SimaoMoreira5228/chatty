# Chatty Makefile
#
# Usage:
#   make lint             # Run linter
#   make build            # Build project
#   make test             # Run tests
#   make run-server       # Run server
#   make run-client       # Run client
#
# Profiling:
#   make target=server flamegraph    # CPU flamegraph
#   make target=server perf-record   # Perf record + report
#   make target=server heaptrack     # Heaptrack memory profiling
#   make target=server massif        # Valgrind Massif
#   make target=server callgrind     # Valgrind Callgrind
#   make target=server helgrind      # Valgrind Helgrind
#
# Allocators:
#   make jmalloc target=client       # Run with jmalloc (set JEMALLOC_LIB=/path/to/libjemalloc.so)
#   make mimalloc target=client      # Run with mimalloc (set MIMALLOC_PATH=/path/to/libmimalloc.so)
#   make jmalloc target=client       # Run with jmalloc (set JMALLOC_PATH=/path/to/libjmalloc.so)

.PHONY: fmt lint build test run-server run-client proto proto-clean

fmt:
	./tools/dprint.dotslash fmt

lint:
	cargo clippy --fix --allow-dirty --allow-staged --all-features --all-targets
	python tools/i18n_audit.py

build:
	cargo build $(ARGS)

test:
	cargo test --all-features --all-targets $(ARGS)

run-server:
	cargo run -p chatty_server $(ARGS)

run-client:
	cargo run -p chatty_client_gui $(ARGS)

proto:
	cargo build -p chatty_protocol

proto-clean:
	cargo clean -p chatty_protocol
	cargo build -p chatty_protocol

BIN_CLIENT := chatty_client_gui
BIN_SERVER := chatty_server
PROFILE_DIR := profiles

profile-build:
	mkdir -p $(PROFILE_DIR)
ifneq ($(target),server)
	cargo build --release -p $(BIN_CLIENT)
else
	cargo build --release -p $(BIN_SERVER)
endif

perf-record: profile-build
ifneq ($(target),server)
	perf record -F 99 -g -o $(PROFILE_DIR)/$(BIN_CLIENT).perf.data -- target/release/$(BIN_CLIENT) $(ARGS)
	perf report -i $(PROFILE_DIR)/$(BIN_CLIENT).perf.data --stdio > $(PROFILE_DIR)/$(BIN_CLIENT)-perf-report.txt || true
	@echo "Perf data: $(PROFILE_DIR)/$(BIN_CLIENT).perf.data"
else
	perf record -F 99 -g -o $(PROFILE_DIR)/$(BIN_SERVER).perf.data -- target/release/$(BIN_SERVER) $(ARGS)
	perf report -i $(PROFILE_DIR)/$(BIN_SERVER).perf.data --stdio > $(PROFILE_DIR)/$(BIN_SERVER)-perf-report.txt || true
	@echo "Perf data: $(PROFILE_DIR)/$(BIN_SERVER).perf.data"
endif

flamegraph: profile-build
ifneq ($(target),server)
	perf record -F 99 -g -o $(PROFILE_DIR)/$(BIN_CLIENT).perf.data -- target/release/$(BIN_CLIENT) $(ARGS)
	perf script > $(PROFILE_DIR)/$(BIN_CLIENT).perf
	@(command -v flamegraph.pl >/dev/null 2>&1 && perf script | flamegraph.pl > $(PROFILE_DIR)/$(BIN_CLIENT)-flamegraph.svg) || \
	(command -v flamegraph >/dev/null 2>&1 && perf script | flamegraph > $(PROFILE_DIR)/$(BIN_CLIENT)-flamegraph.svg) || \
	echo "FlameGraph not found - perf data at $(PROFILE_DIR)/$(BIN_CLIENT).perf"
else
	perf record -F 99 -g -o $(PROFILE_DIR)/$(BIN_SERVER).perf.data -- target/release/$(BIN_SERVER) $(ARGS)
	perf script > $(PROFILE_DIR)/$(BIN_SERVER).perf
	@(command -v flamegraph.pl >/dev/null 2>&1 && perf script | flamegraph.pl > $(PROFILE_DIR)/$(BIN_SERVER)-flamegraph.svg) || \
	(command -v flamegraph >/dev/null 2>&1 && perf script | flamegraph > $(PROFILE_DIR)/$(BIN_SERVER)-flamegraph.svg) || \
	echo "FlameGraph not found - perf data at $(PROFILE_DIR)/$(BIN_SERVER).perf"
endif

heaptrack: profile-build
	mkdir -p $(PROFILE_DIR)
ifneq ($(target),server)
	heaptrack -o $(PROFILE_DIR)/$(BIN_CLIENT).heaptrack -- target/release/$(BIN_CLIENT) $(ARGS)
	@echo "Heaptrack: $(PROFILE_DIR)/$(BIN_CLIENT).heaptrack"
else
	heaptrack -o $(PROFILE_DIR)/$(BIN_SERVER).heaptrack -- target/release/$(BIN_SERVER) $(ARGS)
	@echo "Heaptrack: $(PROFILE_DIR)/$(BIN_SERVER).heaptrack"
endif

massif: profile-build
	mkdir -p $(PROFILE_DIR)
ifneq ($(target),server)
	valgrind --tool=massif --massif-out-file=$(PROFILE_DIR)/massif-$(BIN_CLIENT).out -- target/release/$(BIN_CLIENT) $(ARGS)
	ms_print $(PROFILE_DIR)/massif-$(BIN_CLIENT).out > $(PROFILE_DIR)/massif-$(BIN_CLIENT).txt || true
	@echo "Massif: $(PROFILE_DIR)/massif-$(BIN_CLIENT).out"
else
	valgrind --tool=massif --massif-out-file=$(PROFILE_DIR)/massif-$(BIN_SERVER).out -- target/release/$(BIN_SERVER) $(ARGS)
	ms_print $(PROFILE_DIR)/massif-$(BIN_SERVER).out > $(PROFILE_DIR)/massif-$(BIN_SERVER).txt || true
	@echo "Massif: $(PROFILE_DIR)/massif-$(BIN_SERVER).out"
endif

callgrind: profile-build
	mkdir -p $(PROFILE_DIR)
ifneq ($(target),server)
	valgrind --tool=callgrind --callgrind-out-file=$(PROFILE_DIR)/callgrind-$(BIN_CLIENT).out -- target/release/$(BIN_CLIENT) $(ARGS)
	@echo "Callgrind: $(PROFILE_DIR)/callgrind-$(BIN_CLIENT).out"
else
	valgrind --tool=callgrind --callgrind-out-file=$(PROFILE_DIR)/callgrind-$(BIN_SERVER).out -- target/release/$(BIN_SERVER) $(ARGS)
	@echo "Callgrind: $(PROFILE_DIR)/callgrind-$(BIN_SERVER).out"
endif

helgrind: profile-build
	mkdir -p $(PROFILE_DIR)
ifneq ($(target),server)
	valgrind --tool=helgrind --log-file=$(PROFILE_DIR)/helgrind-$(BIN_CLIENT).log -- target/release/$(BIN_CLIENT) $(ARGS)
	@echo "Helgrind: $(PROFILE_DIR)/helgrind-$(BIN_CLIENT).log"
else
	valgrind --tool=helgrind --log-file=$(PROFILE_DIR)/helgrind-$(BIN_SERVER).log -- target/release/$(BIN_SERVER) $(ARGS)
	@echo "Helgrind: $(PROFILE_DIR)/helgrind-$(BIN_SERVER).log"
endif

jemalloc: profile-build
ifneq ($(target),server)
	@echo "Running $(BIN_CLIENT) with jemalloc profiling..."
	JE_MALLOC_CONF=prof:true,lg_prof_sample:17,prof_prefix:jeprof $(JEMALLOC_LIB) target/release/$(BIN_CLIENT) $(ARGS) &
	@echo "Run complete or Ctrl-C. Profile files: jeprof.*"
else
	@echo "Running $(BIN_SERVER) with jemalloc profiling..."
	JE_MALLOC_CONF=prof:true,lg_prof_sample:17,prof_prefix=jeprof $(JEMALLOC_LIB) target/release/$(BIN_SERVER) $(ARGS) &
	@echo "Run complete or Ctrl-C. Profile files: jeprof.*"
endif

mimalloc: profile-build
ifndef MIMALLOC_PATH
	$(error MIMALLOC_PATH is not set)
endif
ifneq ($(target),server)
	LD_PRELOAD=$(MIMALLOC_PATH) target/release/$(BIN_CLIENT) $(ARGS)
else
	LD_PRELOAD=$(MIMALLOC_PATH) target/release/$(BIN_SERVER) $(ARGS)
endif

jmalloc: profile-build
ifndef JMALLOC_PATH
	$(error JMALLOC_PATH is not set)
endif
ifneq ($(target),server)
	LD_PRELOAD=$(JMALLOC_PATH) target/release/$(BIN_CLIENT) $(ARGS)
else
	LD_PRELOAD=$(JMALLOC_PATH) target/release/$(BIN_SERVER) $(ARGS)
endif
