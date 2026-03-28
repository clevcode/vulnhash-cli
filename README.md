# vulnhash

Cryptographic proof-of-discovery for vulnerability research.

Submit a SHA-256 hash of your vulnerability finding the moment you discover it. Later, reveal
your proof document to demonstrate you were the first to find it.

## Install

Download a prebuilt binary from [Releases](https://github.com/vulnhash/cli/releases), or build from source:

```bash
git clone https://github.com/vulnhash/cli.git
cd cli
make build
```

## Quick Start

```bash
# Configure your account (get a token from https://vulnhash.com)
vulnhash config token vh_your_token_here

# Submit a hash from a proof document
vulnhash submit ./my-finding.md

# Pipe directly from your fuzzer
my-fuzzer --crash-report | vulnhash submit -

# List your submissions
vulnhash list

# View details
vulnhash show <hash>

# Add notes
vulnhash note <hash> "Additional context about this finding"

# Publish when ready to disclose
vulnhash publish <hash>

# Verify a hash exists
vulnhash verify <hash>

# Browse findings in a local web UI
vulnhash server --port 9090
```

## How It Works

1. **Hash**: Your proof document (markdown with crash reports, PoC code, writeups) is SHA-256 hashed
2. **Submit**: The hash is submitted to vulnhash.com with a timestamp
3. **Store**: The original document is saved locally in `~/.vulnhash/<hash>/PROOF.md`
4. **Prove**: When ready, publish your proof - we verify it matches the original hash

## Local Storage

All findings are stored in `~/.vulnhash/`:

```
~/.vulnhash/
  config.json              # Server URL and auth token
  <hash>/
    PROOF.md               # Original proof document
    meta.json              # Metadata (timestamps, status)
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
