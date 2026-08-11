VERSION := $(shell cargo metadata --format-version 1 --no-deps | python3 -c "import sys,json; print(json.load(sys.stdin)['packages'][0]['version'])")
BINARY := vulnhash
DIST := dist
ZIG ?= /opt/zig/zig
ZIG_DIR := $(dir $(ZIG))

# Cross-compilation goes through cargo-zigbuild rather than a raw
# CC="zig cc -target ..." override. cc-rs also appends the full Rust triple
# (x86_64-unknown-linux-musl), which zig 0.15 rejects as an unknown OS;
# cargo-zigbuild's wrapper normalises the triples so C dependencies (ring,
# libsqlite3-sys) compile.
#   cargo install cargo-zigbuild

# Platforms are named the way install.sh asks for them: <os>-<arch>.
# The release artifact for each is $(DIST)/$(BINARY)-$(VERSION)-<platform>.
LINUX_PLATFORMS   := linux-amd64 linux-arm64
DARWIN_PLATFORMS  := darwin-amd64 darwin-arm64
WINDOWS_PLATFORMS := windows-amd64 windows-arm64

# Platforms built by `make release`. The default is what a Linux host can
# actually produce; macOS and Windows arm64 need their own hosts and come from
# CI. Override to build a different set:
#   make release RELEASE_PLATFORMS="darwin-amd64 darwin-arm64"
RELEASE_PLATFORMS ?= $(LINUX_PLATFORMS) windows-amd64

.PHONY: all build bundle clean test release checksums $(LINUX_PLATFORMS) $(DARWIN_PLATFORMS) $(WINDOWS_PLATFORMS)

all: build

build: bundle
	cargo build --release

bundle:
	@if [ ! -d web/node_modules ]; then cd web && npm install; fi

test:
	cargo test

clean:
	cargo clean
	rm -rf web/node_modules $(DIST)

# --- Cross-compilation -------------------------------------------------------
#
# What can be built where, from a Linux host:
#
#   linux-amd64     yes, zig cc + musl
#   linux-arm64     yes, zig cc + musl
#   windows-amd64   yes, mingw-w64
#   windows-arm64   no  - zig's mingw tree ships no aarch64 import library for
#                         `synchronization`, so the link fails. Build it on a
#                         Windows arm64 host (see .github/workflows/release.yml).
#   darwin-*        no  - not for want of a compiler: `cargo zigbuild --target
#                         aarch64-apple-darwin` compiles the whole tree, C
#                         dependencies included, and zig ships libSystem.tbd.
#                         The link then fails on `framework 'IOKit'` and
#                         `framework 'CoreFoundation'`, wanted by sysinfo (via
#                         tor-memquota) and iana-time-zone (via chrono). Zig
#                         ships no framework stubs; those come from the macOS
#                         SDK. Build on macOS, where the SDK already is.
#
# The darwin and windows-arm64 rules below still work when run on a host that
# does have the right toolchain, which is what CI uses.

linux-amd64: bundle
	PATH="$(ZIG_DIR):$$PATH" cargo zigbuild --release --target x86_64-unknown-linux-musl
	@$(MAKE) stage TARGET=x86_64-unknown-linux-musl PLATFORM=linux-amd64 EXT=

linux-arm64: bundle
	PATH="$(ZIG_DIR):$$PATH" cargo zigbuild --release --target aarch64-unknown-linux-musl
	@$(MAKE) stage TARGET=aarch64-unknown-linux-musl PLATFORM=linux-arm64 EXT=

darwin-amd64: bundle
	cargo build --release --target x86_64-apple-darwin
	@$(MAKE) stage TARGET=x86_64-apple-darwin PLATFORM=darwin-amd64 EXT=

darwin-arm64: bundle
	cargo build --release --target aarch64-apple-darwin
	@$(MAKE) stage TARGET=aarch64-apple-darwin PLATFORM=darwin-arm64 EXT=

windows-amd64: bundle
	CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc \
	CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
	cargo build --release --target x86_64-pc-windows-gnu
	@$(MAKE) stage TARGET=x86_64-pc-windows-gnu PLATFORM=windows-amd64 EXT=.exe

# Expected to run on a Windows arm64 host, where the MSVC toolchain provides
# the import libraries zig's mingw tree is missing.
windows-arm64: bundle
	cargo build --release --target aarch64-pc-windows-msvc
	@$(MAKE) stage TARGET=aarch64-pc-windows-msvc PLATFORM=windows-arm64 EXT=.exe

# stage copies one freshly built binary into dist/ under its release name.
.PHONY: stage
stage:
	@mkdir -p $(DIST)
	cp target/$(TARGET)/release/$(BINARY)$(EXT) $(DIST)/$(BINARY)-$(VERSION)-$(PLATFORM)$(EXT)
	@echo "staged $(DIST)/$(BINARY)-$(VERSION)-$(PLATFORM)$(EXT)"

# --- Release -----------------------------------------------------------------

# dist is emptied first so artifacts from an earlier version cannot linger and
# end up listed in checksums.txt.
release:
	rm -rf $(DIST)
	@mkdir -p $(DIST)
	$(MAKE) $(RELEASE_PLATFORMS)
	@$(MAKE) checksums

# checksums.txt is written to a temporary file first: redirecting straight into
# dist/ would let the glob pick up the file being written.
checksums:
	cd $(DIST) && sha256sum $(BINARY)-* > ../checksums.txt.tmp && mv ../checksums.txt.tmp checksums.txt
	@echo "release artifacts in $(DIST):"
	@ls -1 $(DIST)
