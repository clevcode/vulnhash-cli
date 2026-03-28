use std::process::Command;

fn main() {
    // Rerun only when frontend sources change
    println!("cargo:rerun-if-changed=web/src/");
    println!("cargo:rerun-if-changed=web/index.html");
    println!("cargo:rerun-if-changed=web/package.json");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let bundle_dir = format!("{out_dir}/bundle");
    std::fs::create_dir_all(&bundle_dir).unwrap();

    // Install npm deps if needed
    if !std::path::Path::new("web/node_modules").exists() {
        let status = Command::new("npm")
            .args(["install", "--prefix", "web"])
            .status()
            .expect("npm install failed");
        assert!(status.success(), "npm install failed");
    }

    // Run esbuild
    let status = Command::new("npx")
        .current_dir("web")
        .args([
            "esbuild", "src/main.tsx",
            "--bundle",
            "--minify",
            &format!("--outdir={bundle_dir}"),
            "--format=esm",
            "--jsx=automatic",
            "--loader:.tsx=tsx",
            "--loader:.ts=ts",
        ])
        .status()
        .expect("esbuild failed");
    assert!(status.success(), "esbuild failed");

    // Copy index.html, rewriting the script src
    let html = std::fs::read_to_string("web/index.html").unwrap();
    let html = html.replace(
        r#"src="src/main.tsx""#,
        r#"src="/main.js""#,
    );
    std::fs::write(format!("{bundle_dir}/index.html"), html).unwrap();

    // Export bundle paths for include_bytes!/include_str!
    println!("cargo:rustc-env=BUNDLE_DIR={bundle_dir}");
}
