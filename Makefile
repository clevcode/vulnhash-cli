VERSION := $(shell cargo metadata --format-version 1 --no-deps | python3 -c "import sys,json; print(json.load(sys.stdin)['packages'][0]['version'])")
BINARY := vulnhash

.PHONY: all build clean test release

all: build

build: bundle
	cargo build --release

bundle:
	@if [ ! -d web/node_modules ]; then cd web && npm install; fi

test:
	cargo test

clean:
	cargo clean
	rm -rf web/node_modules

# Cross-compilation targets
.PHONY: linux-x64 linux-arm64 macos-x64 macos-arm64

linux-x64: bundle
	CC="/opt/zig/zig cc -target x86_64-linux-musl" \
	cargo build --release --target x86_64-unknown-linux-musl

linux-arm64: bundle
	CC="/opt/zig/zig cc -target aarch64-linux-musl" \
	cargo build --release --target aarch64-unknown-linux-musl

# Release: build all targets, create tarballs with checksums
release: linux-x64 linux-arm64
	mkdir -p dist
	cp target/x86_64-unknown-linux-musl/release/$(BINARY) dist/$(BINARY)-$(VERSION)-linux-amd64
	cp target/aarch64-unknown-linux-musl/release/$(BINARY) dist/$(BINARY)-$(VERSION)-linux-arm64
	cd dist && sha256sum * > checksums.txt
