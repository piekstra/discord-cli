# discord-cli — task runner: build/test/lint/fmt/clean/install/deps, a cheap
# `smoke` check, and an aggregate `verify` gate. Thin wrappers over cargo so a
# green local run predicts a green CI run.

BIN := discord
CARGO := cargo

.PHONY: all build release test lint fmt fmt-check clean install deps smoke verify audit dev

all: verify

build: SIGN_TARGET = target/debug/$(BIN)
build:
	$(CARGO) build
	@$(SIGN)

release: SIGN_TARGET = target/release/$(BIN)
release:
	$(CARGO) build --release
	@$(SIGN)

test:
	$(CARGO) test --all

lint:
	$(CARGO) clippy --all-targets -- -D warnings

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

clean:
	$(CARGO) clean


deps:
	$(CARGO) fetch

# Cheap sanity checks needing no config or network: version + top-level help.
smoke: release
	./target/release/$(BIN) --version
	./target/release/$(BIN) --help >/dev/null
	@for grp in auth me guilds channels messages send delete app config api self-update completions info; do \
		./target/release/$(BIN) $$grp --help >/dev/null || exit 1; \
	done
	@echo "smoke ok"

# Dependency license/advisory gate (matches CI). Needs cargo-deny installed.
audit:
	$(CARGO) deny check

# Aggregate pre-push gate: a green run here predicts green CI.
verify: fmt-check lint test smoke
	@echo "verify ok"

# Debug build re-signed with the stable pk-cli-codesign identity so macOS
# keychain "Always Allow" grants survive rebuilds (see cli-common/scripts).
dev: SIGN_TARGET = target/debug/$(BIN)
dev:
	cargo build
	@$(SIGN)

# `cargo install` ad-hoc signs, giving the binary a new code identity every
# time. macOS scopes keychain "Always Allow" grants to that identity, so an
# unsigned reinstall silently revokes them. Re-signing with the stable shared
# identity keeps one grant valid across every future install.
install: SIGN_TARGET = $${CARGO_INSTALL_ROOT:-$$HOME/.cargo}/bin/$(BIN)
install:
	$(CARGO) install --path . --force --locked
	@$(SIGN)

# Shared re-signing step. No-ops with a note when the helper or identity is
# absent (CI, Linux, a machine that hasn't run setup-dev-signing.sh).
define SIGN
if [ -x "$$HOME/Dev/cli-common/scripts/dev-sign.sh" ]; then \
	"$$HOME/Dev/cli-common/scripts/dev-sign.sh" "$(SIGN_TARGET)"; \
else echo "cli-common/scripts/dev-sign.sh not found — $(SIGN_TARGET) left ad-hoc signed"; fi
endef
