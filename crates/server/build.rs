use std::process::Command;

fn main() {
    // Only rebuild frontend when its source changes
    println!("cargo:rerun-if-changed=../ui/frontend/src");
    println!("cargo:rerun-if-changed=../ui/frontend/package.json");
    println!("cargo:rerun-if-changed=../ui/frontend/package-lock.json");
    println!("cargo:rerun-if-changed=../ui/frontend/index.html");
    println!("cargo:rerun-if-changed=../ui/frontend/vite.config.js");

    let frontend_dir = std::path::Path::new("../ui/frontend");

    // On Windows, npm is npm.cmd; on Unix it's just npm
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };

    // If the static output directory already contains built assets (e.g. Docker
    // multi-stage build where the frontend stage ran separately), support using
    // those prebuilt assets instead of running npm in this build script.
    let static_out = std::path::Path::new("../../public/static");
    let has_built_assets = static_out.exists()
        && std::fs::read_dir(static_out).map_or(false, |mut d| d.next().is_some());
    println!("cargo:rerun-if-env-changed=FYR_USE_PREBUILT_FRONTEND");
    let use_prebuilt_assets = std::env::var("FYR_USE_PREBUILT_FRONTEND")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "on"))
        .unwrap_or(false);
    let npm_available = Command::new(npm).arg("--version").output().is_ok();
    if has_built_assets && (use_prebuilt_assets || !npm_available) {
        return;
    }
    // Install dependencies if node_modules is missing
    let node_modules = frontend_dir.join("node_modules");
    if !node_modules.exists() {
        let status = Command::new(npm)
            .args(["ci"])
            .current_dir(frontend_dir)
            .status()
            .expect("npm ci failed — is Node.js installed?");
        assert!(status.success(), "npm ci exited with error");
    }

    // Build the frontend
    let status = Command::new(npm)
        .args(["run", "build"])
        .current_dir(frontend_dir)
        .status()
        .expect("npm run build failed — is Node.js installed?");
    assert!(status.success(), "npm run build exited with error");
}
