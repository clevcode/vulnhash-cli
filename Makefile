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

# The UI bundle is committed under web/dist, so building needs no toolchain
# beyond cargo. Nothing to do here; build.rs embeds what is checked in and
# says so if it no longer matches web/src.
bundle:
	@:

# Regenerate the committed bundle after changing web/src.
#
# ccweb resolves npm packages and bundles entirely in Go, so this needs no
# Node. It is not public, hence the checked-in result: anyone can build the
# CLI, and only changing its UI needs the tool. Without ccweb, build.rs falls
# back to npm and esbuild, which produces an equivalent bundle.
.PHONY: bundle-regen
bundle-regen:
	cd ../vulnhash && go run ./cmd/build-web -src ../vulnhash-cli/web -out ../vulnhash-cli/web/dist
	@echo "regenerated web/dist - commit it"

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
#   windows-arm64   no  - nothing on this side has an aarch64 import library
#                         for `synchronization`: zig's mingw tree ships only
#                         api-ms-win-core-synch-*.def, and the
#                         windows_aarch64_gnullvm crate ships only
#                         libwindows.0.53.0.a. True of the -gnullvm target as
#                         well as -gnu, so switching targets does not help.
#                         Build it on Windows; see the note below.
#   darwin-*        possible with a copied SDK, but the Mac builds better
#                         binaries. See "Cross-building darwin from Linux".
#
# The darwin and windows-arm64 rules below still work when run on a host that
# does have the right toolchain, which is what CI uses.
#
# --- Cross-building darwin from Linux ---------------------------------------
#
# It does work, with a copy of the macOS SDK:
#
#   SDKROOT=~/macos-sdk/MacOSX15.4.sdk PATH=/opt/zig:$$PATH \
#     cargo zigbuild --release --target aarch64-apple-darwin
#
# Both arches link, and the binaries run on a real Mac. Two things to know:
#
# It must be an older SDK. `cargo zigbuild` gets the whole tree compiled — C
# dependencies included, and zig ships libSystem.tbd — then fails at link. With
# no SDK it cannot find the IOKit and CoreFoundation frameworks that sysinfo
# (via tor-memquota) and iana-time-zone (via chrono) want. With the current SDK
# it finds them and still fails, on undefined _kCFAllocatorDefault and friends:
# those stubs now list only `arm64e-macos` among their targets, and while
# Apple's linker maps arm64 onto arm64e for system libraries, zig's does not.
# The 15.x SDKs still list plain `arm64-macos` and link cleanly.
#
# It costs reach. Zig defaults macOS binaries to a minimum of 13.0 and there is
# no way through cargo-zigbuild 0.23.0 to lower it: the wrapper it generates
# passes `-target aarch64-macos-none` with no version, MACOSX_DEPLOYMENT_TARGET
# is not threaded through, a second -target on the link line loses to the
# first, and pointing CC_* at a hand-rolled wrapper breaks cc-rs, which appends
# a triple of its own. Built on macOS the same source targets 11.0 on arm64 and
# 10.12 on x86_64 — the deployment target, not the SDK, is what sets that
# floor, so a newer SDK costs nothing here and an older one buys nothing.

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

# Runs on any Windows host with the MSVC ARM64 tools; an x64 box cross-builds
# it fine, it just cannot run the result. Two prerequisites that are easy to
# miss, both of which fail in ways that name neither:
#
#   1. Start from a Developer Command Prompt for the target, so INCLUDE and
#      LIB point at the arm64 headers and import libraries:
#        call "...\VC\Auxiliary\Build\vcvarsall.bat" x64_arm64
#      Without it clang compiles ring's C and fails without finding MSVC.
#   2. Put the VS LLVM directory on PATH:
#        set PATH=...\VC\Tools\Llvm\bin;%PATH%
#      ring needs clang for aarch64-windows; cc-rs reports it as
#      `failed to find tool "clang"`.
#
# Components: Microsoft.VisualStudio.Component.VC.Tools.ARM64 and
# Microsoft.VisualStudio.Component.VC.Llvm.Clang. Add them with
# `setup.exe modify --installPath <instance> --add <component> --quiet
# --norestart` — note that --wait is a bootstrapper flag and makes the
# installer exit 87, and that the installer is a WPF application, so over SSH
# it must be launched into an interactive session.
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
