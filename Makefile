# fepdf Multi-Crate Workspace Makefile

# Package names differ from the binaries they produce: the CLI crate is
# fepdf-cli but ships as `fepdf`. `cargo -p` needs the package, `cp` the binary.
CLI_PACKAGE=fepdf-cli
CLI_BINARY=fepdf
GUI_PACKAGE=fepdf-gui
GUI_BINARY=fepdf-gui
# Crates inherit version.workspace, so the literal lives in the root manifest.
VERSION=$(shell grep "^version" Cargo.toml | head -n 1 | cut -d '"' -f 2)
DIST_DIR=out/dist

# Targets
TARGET_APPLE_SILICON=aarch64-apple-darwin
TARGET_APPLE_INTEL=x86_64-apple-darwin
TARGET_WINDOWS=x86_64-pc-windows-msvc
TARGET_LINUX=x86_64-unknown-linux-gnu

.PHONY: all help test check clippy fmt build-all build-local run clean dist audit audit-licenses visual-test visual-update-ref

help:
	@echo "fepdf Build & Audit System v$(VERSION)"
	@echo "Usage:"
	@echo "  make check          - Fast workspace compilation check"
	@echo "  make test           - Run full workspace unit/integration tests"
	@echo "  make clippy         - Run Clippy lints with -D warnings"
	@echo "  make fmt            - Check code formatting"
	@echo "  make audit          - Run full compliance audit (RR-15, cargo-deny, betterleaks)"
	@echo "  make audit-licenses - Run Cargo-native license audit via cargo-deny"
	@echo "  make build-local    - Build CLI and GUI binaries for current host"
	@echo "  make build-all      - Cross-compile binaries for all supported platforms"
	@echo "  make run            - Launch the desktop GUI application"
	@echo "  make clean          - Remove build artifacts"

check:
	cargo check --workspace

test:
	cargo test --workspace

clippy:
	cargo clippy --workspace -- -D warnings

fmt:
	cargo fmt --all --check

build-local:
	cargo build -p $(CLI_PACKAGE) --release
	cargo build -p $(GUI_PACKAGE) --release

run:
	cargo run -p $(GUI_PACKAGE)

build-all: build-mac build-win build-linux

build-mac:
	@echo "Building for macOS..."
	cargo build -p $(CLI_PACKAGE) --release --target $(TARGET_APPLE_SILICON)
	cargo build -p $(CLI_PACKAGE) --release --target $(TARGET_APPLE_INTEL)

build-win:
	@echo "Building for Windows..."
	cargo build -p $(CLI_PACKAGE) --release --target $(TARGET_WINDOWS)

build-linux:
	@echo "Building for Linux..."
	cargo build -p $(CLI_PACKAGE) --release --target $(TARGET_LINUX)

dist: build-all
	mkdir -p $(DIST_DIR)
	cp target/$(TARGET_APPLE_SILICON)/release/$(CLI_BINARY) $(DIST_DIR)/$(CLI_BINARY)-macos-arm64
	cp target/$(TARGET_APPLE_INTEL)/release/$(CLI_BINARY) $(DIST_DIR)/$(CLI_BINARY)-macos-x64
	cp target/$(TARGET_WINDOWS)/release/$(CLI_BINARY).exe $(DIST_DIR)/$(CLI_BINARY).exe
	cp target/$(TARGET_LINUX)/release/$(CLI_BINARY) $(DIST_DIR)/$(CLI_BINARY)-linux-x64
	@echo "Artifacts ready in $(DIST_DIR)/"

clean:
	cargo clean
	rm -rf out/

audit:
	./scripts/audit/verify_compliance.sh

audit-licenses:
	cargo deny check licenses

setup-arlington:
	@echo "Setting up Arlington PDF Model environment..."
	python3 -m venv .arlington-venv
	./.arlington-venv/bin/pip install --upgrade pip
	./.arlington-venv/bin/pip install pikepdf sly pandas
	@echo "Setup complete. Use 'make audit-external PDF=<file>' to verify compliance."

audit-external:
	@if [ -z "$(PDF)" ]; then echo "Error: Please specify target PDF using PDF=<file>"; exit 1; fi
	./scripts/audit/arlington_audit.sh $(PDF)

visual-test:
	python3 scripts/visual_regression.py

visual-update-ref:
	python3 scripts/visual_regression.py --update
