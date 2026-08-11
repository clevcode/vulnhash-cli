# vulnhash

Cryptographic proof-of-discovery for vulnerability research.

Submit a SHA-256 hash of your vulnerability finding the moment you discover it. Later, reveal
your proof document to demonstrate you were the first to find it.

## Install

```bash
curl -fsSL https://vulnhash.com/install.sh | sh
```

Installs to `~/.local/bin` (no root needed) and adds it to your PATH if it is
not there already. Set `VULNHASH_INSTALL_DIR` to install elsewhere, or
`VULNHASH_NO_MODIFY_PATH=1` to leave your shell config alone.

Releases are signed. If [minisign](https://jedisct1.github.io/minisign/) is
installed, the installer verifies the signature and refuses to continue if it
does not check out; without minisign it falls back to the checksum and tells
you how to verify by hand. Install minisign first if that matters to you, or
set `VULNHASH_REQUIRE_SIGNATURE=1` to make an unverifiable download fatal.

```
public key: RWTqtA0mrMWbFQ1XAcSC/r7WT0YZ/hpNrkC+gjrVA/aH9UESmSCRyp+7
```

On Windows:

```powershell
irm https://vulnhash.com/install.ps1 | iex
```

Or build from source:

```bash
git clone https://github.com/vulnhash/cli.git
cd cli
make build
```

A Rust toolchain is the only requirement. The local web UI ships prebuilt in
`web/dist/` and is embedded from there, so no Node, npm or network access is
involved in a build.

Changing the UI is the one thing that needs a bundler. `build.rs` compares
`web/dist/sources.sha256` against `web/src`, and when they differ it says so
and rebuilds with esbuild if npm is present, rather than quietly embedding a
stale interface. Commit the regenerated bundle along with the sources.

## Quick Start

```bash
# Configure your account (get a token from https://vulnhash.com)
vulnhash config token vh_your_token_here

# Submit a hash from a proof document
vulnhash submit ./my-finding.md

# Pipe directly from your fuzzer
my-fuzzer --crash-report | vulnhash submit -

# List your submissions, with the local id of each
vulnhash list

# Anywhere a hash is accepted, so is the local id or a hash prefix
vulnhash show 3
vulnhash show 1f4c9a

# Record a follow-up finding as a child of an earlier one
vulnhash submit ./deeper-finding.md --parent 3

# Add notes
vulnhash note 3 "Additional context about this finding"

# Publish when ready to disclose
vulnhash publish 3

# Publish a finding together with every follow-up below it
vulnhash publish 3 -R

# Verify a hash exists
vulnhash verify <hash>

# Browse findings in a local web UI
vulnhash server --port 9090
```

## Local Ids

`vulnhash list` shows a short id next to each hash, and every command that
takes a hash takes an id instead:

```
  ID  HASH                                    PARENT  STATUS     SYNCED  DATE
   4  52642224757f9dea...                          2  private    yes     2026-08-11 18:51
   3  f001e2b36d4554ce...                          1  published  yes     2026-08-11 18:51
```

Ids are local to your machine and never reused: deleting a submission does not
free its number for the next one, so an id in your notes always means the same
finding. The hash remains the identifier everyone else uses.

A reference is read as an id when it is all digits, as a hash when it is 64 hex
characters, and as a hash prefix otherwise.

## Follow-up Findings

A finding often leads to another. `--parent` records that link:

```bash
vulnhash submit ./root-cause.md              # id 1
vulnhash submit ./second-bug.md --parent 1   # id 2, a follow-up to 1
vulnhash submit ./deeper.md --parent 2       # id 3, a follow-up to 2
```

Publishing stays deliberate. `vulnhash publish 1` discloses that one
submission and nothing else; `-R` discloses it together with everything below
it:

```bash
vulnhash publish 1 -R
```

The tree does not have to be published as a unit, and it does not have to be
published top down: a follow-up can be disclosed while its parent stays
sealed, which is what you want when one branch is fixed and the rest is not.

A recursive publish covers the follow-ups stored on this machine. If the
server knows of others — submitted from a different machine, whose documents
are not here — it says so rather than quietly leaving them out.

## Tor

Any command that talks to the server can be routed through Tor:

```bash
vulnhash submit ./my-finding.md --tor
```

To make that the default for every command, set it in the config:

```bash
vulnhash config tor on            # route everything through Tor
vulnhash config tor-ephemeral on  # keep no Tor state on disk
vulnhash config                   # show current settings
```

`--tor` and `--no-tor` override the configured setting for a single command,
in either direction:

| config | flag | result |
| --- | --- | --- |
| off | *(none)* | direct |
| off | `--tor` | Tor |
| on | *(none)* | Tor |
| on | `--no-tor` | direct |

Passing both is not an error: the last one on the command line wins, so a
wrapper script can append `--no-tor` without knowing what it overrides.

## How It Works

1. **Hash**: Your proof document (markdown with crash reports, PoC code, writeups) is SHA-256 hashed
2. **Submit**: The hash is submitted to vulnhash.com with a timestamp
3. **Store**: The original document is saved locally in `~/.vulnhash/<hash>/PROOF.md`
4. **Prove**: When ready, publish your proof - we verify it matches the original hash

## Local Storage

All findings are stored in `~/.vulnhash/`, or in `$VULNHASH_HOME` when that is
set:

```
~/.vulnhash/
  config.json              # Server URL and auth token
  .ids/                    # Local id ledger, one marker file per id
  <hash>/
    PROOF.md               # Original proof document
    meta.json              # Metadata (timestamps, status, parent)
    notes/                 # Additional notes
      2024-01-15_143022.md
```

## Server Mode

`vulnhash server` starts a local web UI for browsing and managing your findings:

- View all local submissions
- Read proof documents and notes
- Add new notes
- Publish findings to vulnhash.com

## Proof Document Format

Your proof document is a standard Markdown file. Include whatever evidence you need:

````markdown
# CVE-2024-XXXXX: Buffer Overflow in libfoo

## Summary
Found a heap buffer overflow in libfoo v2.3.1 `parse_input()`.

## Crash Report
```
ASAN: heap-buffer-overflow on address 0x602000000098
READ of size 4 at 0x602000000098 thread T0
```

## PoC
```python
import struct
payload = b'A' * 256 + struct.pack('<Q', 0xdeadbeef)
```

## Trigger
```json
{"input": "AAAA...", "version": "2.3.1"}
```
````

## Building

Requires Go 1.25+ and Node.js 22+.

```bash
# Build for current platform
make build

# Build for all platforms
make build-all

# Clean
make clean
```

### Reproducible Builds

Releases are built with [GoReleaser](https://goreleaser.com/) and include:
- SBOM (SPDX format via Syft)
- SHA-256 checksums
- Statically linked binaries (CGO_ENABLED=0)
- Trimmed build paths (`-trimpath`)

## License

MIT
