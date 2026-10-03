//! Stamp the simulation build so replays can tell which sim produced them.
use std::process::Command;

fn main() {
    // hash of the simulation sources + data: any change to sim behaviour changes it
    let out = Command::new("git").args(["log", "-1", "--format=%h", "--", "src", "data"]).output();
    let mut stamp = out.ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let dirty = Command::new("git").args(["status", "--porcelain", "--", "src", "data"]).output();
    if dirty.map_or(false, |o| !o.stdout.is_empty()) {
        stamp.push_str("+dirty");
    }
    println!("cargo:rustc-env=EE_SIM_VERSION={stamp}");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=data");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
