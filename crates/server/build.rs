use std::process::Command;

fn main() {
    // Only rebuild frontend when its source changes
    println!("cargo:rerun-if-changed=../ui/frontend/src");
    println!("cargo:rerun-if-changed=../ui/frontend/package.json");
    println!("cargo:rerun-if-changed=../ui/frontend/vite.config.js");

    let frontend_dir = std::path::Path::new("../ui/frontend");

    // On Windows, npm is npm.cmd; on Unix it's just npm
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };

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