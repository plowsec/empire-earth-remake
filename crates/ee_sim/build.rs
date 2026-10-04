//! Stamp the simulation build with a hash of its sources and data, so replays and LAN
//! peers can tell whether two builds simulate identically. Content-based on purpose: the
//! same code gives the same stamp whatever the git state (commits, dirty trees, clones).
use std::fs;
use std::path::Path;

fn fnv(h: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *h ^= *b as u64;
        *h = h.wrapping_mul(0x100000001b3);
    }
}

fn walk(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, files);
        } else if matches!(p.extension().and_then(|x| x.to_str()), Some("rs") | Some("ron")) {
            files.push(p);
        }
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    // the simulation itself and the lockstep protocol it runs under
    for d in ["src", "data", "../ee_net/src"] {
        walk(&root.join(d), &mut files);
    }
    files.sort();
    let mut h: u64 = 0xcbf29ce484222325;
    for f in &files {
        let rel = f.strip_prefix(root).unwrap_or(f).to_string_lossy().replace('\\', "/");
        fnv(&mut h, rel.as_bytes());
        if let Ok(b) = fs::read(f) {
            // line endings must not matter (Windows checkouts)
            let norm: Vec<u8> = b.into_iter().filter(|&c| c != b'\r').collect();
            fnv(&mut h, &norm);
        }
    }
    println!("cargo:rustc-env=EE_SIM_VERSION={:012x}", h & 0xffff_ffff_ffff);
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=data");
    println!("cargo:rerun-if-changed=../ee_net/src");
}
