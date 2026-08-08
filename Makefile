# fepdf - The Universal PDF Toolkit
# Makefile for multi-platform distribution

BINARY_NAME=fepdf
GUI_BINARY_NAME=ferruginous
CRATE_PATH=crates/fepdf
GUI_CRATE_PATH=crates/ferruginous
VERSION=$(shell grep "^version" crates/fepdf/Cargo.toml | head -n 1 | cut -d '"' -f 2)
DIST_DIR=out/dist

# Targets
TARGET_APPLE_SILICON=aarch64-apple-darwin
TARGET_APPLE_INTEL=x86_64-apple-darwin
TARGET_WINDOWS=x86_64-pc-windows-msvc
TARGET_LINUX=x86_64-unknown-linux-gnu

.PHONY: all help build-all clean dist audit audit-licenses

help:
	@echo "fepdf Build System v$(VERSION)"
	@echo "Usage:"
	@echo "  make build-all      - Build for all supported platforms"
	@echo "  make build-local    - Build for the current platform (Release)"
	@echo "  make clean          - Remove build artifacts"
	@echo "  make dist           - Package binaries into $(DIST_DIR)"
	@echo "  make audit          - Run full compliance audit (RR-15, cargo-deny, betterleaks)"
	@echo "  make audit-licenses - Run Cargo-native license audit via cargo-deny"
	@echo "  make setup-arlington - Prepare the Arlington PDF Model test environment"
	@echo "  make audit-external PDF=<file> - Run Arlington audit on a PDF file"

build-local:
	cargo build -p $(BINARY_NAME) --release
	cargo build -p $(GUI_BINARY_NAME) --release

run:
	cargo run -p $(GUI_BINARY_NAME)

build-all: build-mac build-win build-linux

build-mac:
	@echo "Building for macOS..."
	cargo build -p $(BINARY_NAME) --release --target $(TARGET_APPLE_SILICON)
	cargo build -p $(BINARY_NAME) --release --target $(TARGET_APPLE_INTEL)

build-win:
	@echo "Building for Windows..."
	cargo build -p $(BINARY_NAME) --release --target $(TARGET_WINDOWS)

build-linux:
	@echo "Building for Linux..."
	cargo build -p $(BINARY_NAME) --release --target $(TARGET_LINUX)

dist: build-all
	mkdir -p $(DIST_DIR)
	# macOS Universal Binary (lipo would be used here in a real environment)
	cp target/$(TARGET_APPLE_SILICON)/release/$(BINARY_NAME) $(DIST_DIR)/$(BINARY_NAME)-macos-arm64
	cp target/$(TARGET_APPLE_INTEL)/release/$(BINARY_NAME) $(DIST_DIR)/$(BINARY_NAME)-macos-x64
	cp target/$(TARGET_WINDOWS)/release/$(BINARY_NAME).exe $(DIST_DIR)/$(BINARY_NAME).exe
	cp target/$(TARGET_LINUX)/release/$(BINARY_NAME) $(DIST_DIR)/$(BINARY_NAME)-linux-x64
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

.PHONY: visual-test visual-update-ref

visual-test:
	python3 scripts/visual_regression.py

visual-update-ref:
	python3 scripts/visual_regression.py --update
