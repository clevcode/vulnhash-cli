//! Prepares the local web UI that `vulnhash server` embeds.
//!
//! The bundle is committed under `web/dist/`, so an ordinary build needs
//! nothing but cargo — no Node, no npm, no network. That is what lets release
//! binaries be built on hosts that have a Rust toolchain and nothing else.
//!
//! `web/dist/sources.sha256` records what the committed bundle was built
//! from. When it no longer matches the sources beside it, this script says so
//! rather than quietly embedding a stale UI, and rebuilds with esbuild if npm
//! happens to be available.
//!
//! Regenerating the bundle needs a bundler. The maintainer's path is ccweb,
//! which resolves npm packages and bundles entirely in Go:
//!
//! ```text
//! cd ../vulnhash && go run ./cmd/build-web \
//!     -src ../vulnhash-cli/web -out ../vulnhash-cli/web/dist
//! ```
//!
//! ccweb is not public, so the fallback here is npm and esbuild, which
//! produces an equivalent bundle from the same sources.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

const DIGEST_FILE: &str = "sources.sha256";

fn main() {
    println!("cargo:rerun-if-changed=web/src");
    println!("cargo:rerun-if-changed=web/index.html");
    println!("cargo:rerun-if-changed=web/package.json");
    println!("cargo:rerun-if-changed=web/dist");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let web = PathBuf::from("web");
    let committed = web.join("dist");

    let sources_digest = sources_digest(&web);

    if has_bundle(&committed) {
        match recorded_digest(&committed) {
            Some(recorded) if recorded == sources_digest => {
                emit(&committed);
                return;
            }
            Some(_) => warn(
                "web/dist was built from different sources than the ones in web/src. \
                 Regenerate it and commit the result.",
            ),
            None => warn(&format!(
                "web/dist has no {DIGEST_FILE}, so it cannot be checked against web/src."
            )),
        }

        // The bundle is stale. Rebuilding is better where we can, but a host
        // without npm still has to produce a working binary, so fall back to
        // shipping what is committed rather than failing the build.
        match build_with_npm(&web, &out_dir) {
            Ok(fresh) => {
                warn("rebuilt the UI from web/src for this build only; web/dist is still stale.");
                emit(&fresh);
            }
            Err(why) => {
                warn(&format!(
                    "could not rebuild the UI ({why}); embedding the committed bundle as it is."
                ));
                emit(&committed);
            }
        }
        return;
    }

    // No committed bundle at all: the only path that still requires npm.
    match build_with_npm(&web, &out_dir) {
        Ok(fresh) => emit(&fresh),
        Err(why) => panic!(
            "no bundle in web/dist and it could not be built: {why}\n\
             Commit a bundle, or install Node so esbuild can produce one."
        ),
    }
}

fn has_bundle(dir: &Path) -> bool {
    dir.join("index.html").is_file() && dir.join("main.js").is_file()
}

fn recorded_digest(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join(DIGEST_FILE))
        .ok()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
}

fn emit(dir: &Path) {
    let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    println!("cargo:rustc-env=BUNDLE_DIR={}", dir.display());
}

fn warn(message: &str) {
    println!("cargo:warning={message}");
}

/// Hash every input the bundle is built from.
///
/// Mirrors SourcesDigest in the vulnhash server's cmd/build-web: index.html,
/// package.json and everything under src/, in sorted path order, each
/// contributing its relative path, a NUL, its length as eight big-endian
/// bytes, and its contents. The two implementations have to produce the same
/// string, and there is a test on the Go side that checks they do.
fn sources_digest(web: &Path) -> String {
    let mut paths = Vec::new();
    for name in ["index.html", "package.json"] {
        if web.join(name).is_file() {
            paths.push(name.to_string());
        }
    }
    collect(&web.join("src"), web, &mut paths);
    paths.sort();

    let mut hasher = Sha256::new();
    for rel in &paths {
        let content = match std::fs::read(web.join(rel)) {
            Ok(c) => c,
            Err(_) => continue,
        };
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        hasher.update((content.len() as u64).to_be_bytes());
        hasher.update(&content);
    }
    format!("{:x}", hasher.finalize())
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, root, out);
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Bundle with esbuild, the way anyone without ccweb has to.
fn build_with_npm(web: &Path, out_dir: &str) -> Result<PathBuf, String> {
    let bundle_dir = PathBuf::from(format!("{out_dir}/bundle"));
    std::fs::create_dir_all(&bundle_dir).map_err(|e| e.to_string())?;

    // Always install, rather than only when node_modules is missing. A tree
    // left by ccweb's npm client looks complete but has no esbuild binary in
    // it — ccweb bundles with a Go esbuild and never needs one — so presence
    // alone does not mean npx can run.
    let status = Command::new("npm")
        .args(["install", "--prefix", "web"])
        .status()
        .map_err(|e| format!("npm not available: {e}"))?;
    if !status.success() {
        return Err("npm install failed".into());
    }

    let status = Command::new("npx")
        .current_dir(web)
        .args([
            "esbuild",
            "src/main.tsx",
            "--bundle",
            "--minify",
            &format!("--outdir={}", bundle_dir.display()),
            "--format=esm",
            "--jsx=automatic",
            "--loader:.tsx=tsx",
            "--loader:.ts=ts",
        ])
        .status()
        .map_err(|e| format!("npx not available: {e}"))?;
    if !status.success() {
        return Err("esbuild failed".into());
    }

    let html = std::fs::read_to_string(web.join("index.html")).map_err(|e| e.to_string())?;
    let html = html.replace(r#"src="src/main.tsx""#, r#"src="/main.js""#);
    std::fs::write(bundle_dir.join("index.html"), html).map_err(|e| e.to_string())?;

    Ok(bundle_dir)
}
