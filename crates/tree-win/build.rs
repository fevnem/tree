//! Compile res/app.rc (icon, version info, comctl32-v6 + PerMonitorV2 manifest).
//!
//! llvm-rc ships with the LLVM that cargo-xwin already needs, so the resource
//! step is cross-platform with no Windows host involved.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=res/app.rc");
    println!("cargo:rerun-if-changed=res/tree.manifest");
    println!("cargo:rerun-if-changed=res/tree.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let res = out_dir.join("app.res");
    let rc = PathBuf::from("res/app.rc");

    let mut compiled = false;
    for tool in ["llvm-rc", "llvm-rc-18", "llvm-rc-19", "llvm-rc-17", "llvm-rc-16"] {
        let status = Command::new(tool)
            .arg("/fo")
            .arg(&res)
            .arg(&rc)
            .current_dir(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .status();
        if let Ok(status) = status {
            if status.success() {
                compiled = true;
                break;
            }
        }
    }

    if !compiled {
        println!(
            "cargo:warning=llvm-rc not found: building without icon/manifest/version info"
        );
        return;
    }

    // lld-link links a .res object directly.
    println!("cargo:rustc-link-arg={}", res.display());
}
