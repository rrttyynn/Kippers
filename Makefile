EXE    := kippers
TARGET := $(shell rustc --print host-tuple)

RUSTFLAGS ?= -C target-cpu=native
export RUSTFLAGS

ifdef MSYSTEM
	NAME := $(EXE).exe
	ENV  := UNIX
else ifeq ($(OS),Windows_NT)
	NAME := $(EXE).exe
	ENV  := WINDOWS
else
	NAME := $(EXE)
	ENV  := UNIX
endif

ifeq ($(ENV),UNIX)
	PGO_MOVE := mv "target/$(TARGET)/release/$(EXE)" "$(NAME)"
else
	PGO_MOVE := move /Y "target\$(TARGET)\release\$(EXE).exe" "$(NAME)"
endif

.PHONY: all pgo x64-check checkdeps clean help

all: ## Build the engine
	cargo rustc --release --bin kippers -- --emit link=$(NAME)

pgo: ## Build with profile-guided optimization
	cargo pgo instrument
	cargo pgo run -- bench
	cargo pgo optimize
	$(PGO_MOVE)

x64-check: ## Check compilation for x86-64 v1-v4
	RUSTFLAGS="-C target-cpu=x86-64" cargo check --target x86_64-unknown-linux-gnu --no-default-features
	RUSTFLAGS="-C target-cpu=x86-64-v2" cargo check --target x86_64-unknown-linux-gnu --no-default-features
	RUSTFLAGS="-C target-cpu=x86-64-v3" cargo check --target x86_64-unknown-linux-gnu --no-default-features
	RUSTFLAGS="-C target-cpu=x86-64-v4 -C target-feature=+gfni,+avx512bw,+avx512vl,+avx512vbmi,+avx512vbmi2,+avx512vnni,+avx512bitalg" cargo check --target x86_64-unknown-linux-gnu --no-default-features

checkdeps: ## Verify build dependencies are installed
	@echo "-- required --"
	@command -v rustc >/dev/null 2>&1 && echo "  rustc        ok" || (echo "  rustc        MISSING"; exit 1)
	@echo "-- pgo --"
	@command -v cargo-pgo >/dev/null 2>&1 && echo "  cargo-pgo    ok" || echo "  cargo-pgo    missing (run: cargo install cargo-pgo)"
clean: ## Remove build artifacts
	cargo clean
	rm -f "$(EXE)" "$(EXE).exe"

help: ## Show this help
	@awk 'BEGIN {FS = ":.*##"} /^[a-zA-Z0-9_-]+:.*?##/ { \
		printf "  %-12s %s\n", $$1, $$2 \
	}' $(MAKEFILE_LIST)
