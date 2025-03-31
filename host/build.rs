use std::env;
use std::process::Command;

fn main() {
    let guest = env::current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .join("guest");

    let status = Command::new("cargo")
        .current_dir(&guest)
        .arg("component")
        .arg("build")
        .arg("--release")
        .arg("--target")
        .arg("wasm32-wasip1")
        .status()
        .unwrap();

    if !status.success() {
        panic!("Failed to build {}", guest.display());
    }

    println!("cargo:rerun-if-changed={}", guest.display());
}
