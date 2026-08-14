# vulnhash

Portable timeline evidence for vulnerability disclosures.

Hash exact report bytes locally, submit only the hash, and retain the private
receipt returned by VulnHash. After the certified commitment enters a public
batch, the CLI can reconstruct its Merkle inclusion and independently check the
Base-mainnet anchor.

The evidence shows that a commitment existed no later than its Base transaction.
The earlier submission time is a VulnHash server assertion committed on chain
later. It does not prove authorship, independent discovery, earliest discovery,
technical validity, scope, or bounty eligibility.

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

When the source release gate is complete, a checkout builds with:

```bash
make build
```

A Rust toolchain is the only normal build requirement. The local web UI ships
prebuilt in `web/dist/` and is embedded from there, so no Node, npm, or network
access is involved in an ordinary build. Do not advertise a public clone URL
until the repository and license have been made accessible to unauthenticated
users and checked from a clean machine.

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

# Submission stores receipt.json beside the unchanged report. Export a copy
vulnhash receipt 1 --output ./my-finding.receipt.json

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

# Publish when ready
vulnhash publish 3

# Publish a finding together with every follow-up below it
vulnhash publish 3 -R

# Verify the local report and receipt, public manifest, and live Base state
vulnhash verify 1

# Verify without VulnHash, GitHub, an RPC provider, or any other network access
vulnhash verify 1 --manifest ./batch.json --offline

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

Publishing stays deliberate. `vulnhash publish 1` publishes that one
submission and nothing else; `-R` publishes it together with everything below
it:

```bash
vulnhash publish 1 -R
```

The tree does not have to be published as a unit, and it does not have to be
published top down: a follow-up can be published while its parent stays
sealed, which is what you want when one branch is fixed and the rest is not.

A recursive publish covers the follow-ups stored on this machine. If the
server knows of others — submitted from a different machine, whose documents
are not here — it says so rather than quietly leaving them out.

## JSON Output

Every command that reports something takes `-j` / `--json`, on either side of
the subcommand:

```bash
vulnhash list --json
vulnhash -j submit ./finding.md
```

Keys are always present, even when empty, so a consumer can index without
checking first. Timestamps are RFC 3339 in UTC. Failures come back in the same
format on stderr, with a non-zero exit status:

```json
{"error":"no submission with local id 99"}
```

Capturing the hash of something you just submitted:

```bash
hash=$(my-fuzzer --crash-report | vulnhash submit - --json | jq -r .hash)
```

The id is local to your machine; the hash is what travels. `vulnhash config
--json` reports whether a token is set but never prints it, so the output is
safe to log.

## Tor

Any command that talks to VulnHash, GitHub, or a Base RPC provider can use the
same embedded Arti client:

```bash
vulnhash submit ./my-finding.md --tor
```

To make that the default for every command, set it in the config:

```bash
vulnhash config tor on            # route everything through Tor
vulnhash config tor-ephemeral on  # keep no Tor state on disk
vulnhash config                   # show current settings
```

By default, Tor mode selects the official onion origin compiled into the signed
release. If the build has no official onion address, it fails closed. It does
not silently contact `vulnhash.com` through an exit. An operator or development
build can set the exact v3 address explicitly:

```bash
vulnhash config onion-server http://<56-character-address>.onion
```

To deliberately use the clearnet service through a Tor exit instead, explicitly
select its HTTPS URL:

```bash
vulnhash config server https://vulnhash.com
```

`--tor` and `--no-tor` override the configured setting for one command, in
either direction. Each network command reports `direct`, `tor-onion`, or
`tor-exit` and the destination host on stderr:

| config | flag | result |
| --- | --- | --- |
| off | *(none)* | direct |
| off | `--tor` | Tor |
| on | *(none)* | Tor |
| on | `--no-tor` | direct |

Passing both is not an error: the last one on the command line wins, so a
wrapper script can append `--no-tor` without knowing what it overrides.

Tor mode also routes GitHub manifest retrieval and live Base verification. An
HTTPS Base RPC endpoint is reached through a Tor exit unless that provider has
an onion origin. The built-in `https://mainnet.base.org` endpoint is Base's
[rate-limited public endpoint](https://docs.base.org/base-chain/api-reference/rpc-overview);
configure a suitable read-only provider for reliable production verification.
The VulnHash bearer token is never forwarded to GitHub or the RPC host.

Official releases must be built only after the production onion identity has
been initialised and backed up:

```bash
VULNHASH_OFFICIAL_ONION_SERVER=http://<production-address>.onion \
  cargo build --release
```

Do not generate a substitute identity on a build machine. Losing the production
identity changes the address and requires a new signed release.

## How It Works

1. **Hash locally:** the CLI hashes the exact input bytes with SHA-256 and saves
   them as `PROOF.md`.
2. **Submit:** VulnHash receives the document hash, assigns a private UUID and
   server time, and binds those with the account ID and username into a
   certified hash.
3. **Retain:** a successful submission is considered synchronized only after
   its versioned private receipt has been validated against `PROOF.md` and
   durably stored. If receipt storage fails, the CLI tells you how to recover it.
4. **Anchor:** VulnHash targets sealing the oldest pending submission within one
   hour, anchors the batch root on Base, waits for confirmations, and publishes
   every ordered certified hash in a commit-pinned GitHub manifest. Delays and
   retries can extend completion beyond one hour.
5. **Verify:** the CLI recomputes the report hash and receipt commitment,
   reconstructs the batch root, validates the manifest, and optionally checks
   the transaction, receipt, calldata, event, confirmations, and contract state
   against Base.

## Receipts and Verification

`receipt.json` contains the private preimage of the certified hash: the report
hash, submission UUID, server timestamp, account ID, and username. The UUID is a
salt, so keep the receipt private while the report is sealed. Keep the receipt
and exact report bytes together; neither a public Merkle root nor the report by
itself is enough to recover the complete evidence chain.

`vulnhash verify` has distinct modes:

| Inputs | Result |
| --- | --- |
| No local receipt | Looks up the server record and says independent evidence was not checked |
| Local report + receipt | Fetches the API locator and commit-pinned manifest, validates inclusion, then checks live Base state |
| `--manifest FILE` | Uses that local manifest instead of GitHub, then checks live Base state |
| `--manifest FILE --offline` | Makes no network requests; validates report, receipt, manifest, Merkle root, and embedded chain metadata without querying current Base state |

The first successful online verification caches the public manifest as
`anchor-batch.json`, so it can be retained or mirrored independently. A raw
GitHub URL returned by the API is only a convenience locator; verification
checks the commit pin, schema, ordered leaves, root, chain identity, and anchor
metadata.

The public batch log, versioned schemas, deployed contract metadata,
dependency-free verifier, and mirroring instructions are available at
[`clevcode/vulnhash-data`](https://github.com/clevcode/vulnhash-data). Batch
data is CC0-1.0; repository tooling and documentation are MIT-licensed.

## Local Storage

All findings are stored in `~/.vulnhash/`, or in `$VULNHASH_HOME` when that is
set:

```
~/.vulnhash/
  config.json              # Server URL and auth token
  .ids/                    # Local id ledger, one marker file per id
  <hash>/
    PROOF.md               # Original proof document
    receipt.json           # Private portable receipt (restrictive permissions)
    anchor-batch.json      # Cached public complete batch manifest
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

Requires a current Rust toolchain. Node.js is needed only to regenerate the
local web UI when `web/src` changes; ordinary builds embed the committed bundle.

```bash
# Build for current platform
make build

# Build for all platforms
make build-all

# Clean
make clean
```

### Release builds

The release workflow builds Linux, macOS, and Windows artifacts for x86-64 and
ARM64, then generates SHA-256 checksums. Installer signature claims must be
checked against the actual staged release and public key before publication;
workflow completion alone is not proof that the deployed files were signed.

## License

MIT
