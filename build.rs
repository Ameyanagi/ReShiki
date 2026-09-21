use anyhow::Context;
use std::{env, path::PathBuf, process::Command};

fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=native/macos/Clipboard.swift");
    println!("cargo:rerun-if-changed=native/macos/ClipboardSupport.swift");
    println!("cargo:rerun-if-changed=native/macos/Print.swift");
    println!("cargo:rerun-if-changed=native/macos/PrintSupport.swift");
    if env::var("CARGO_CFG_TARGET_OS").context("Missing Cargo target OS")? == "macos" {
        let helper =
            PathBuf::from(env::var_os("OUT_DIR").context("Missing build output directory")?)
                .join("reshiki-clipboard");
        let status = Command::new("swiftc")
            .args([
                "-O",
                "native/macos/ClipboardSupport.swift",
                "native/macos/Clipboard.swift",
                "-o",
            ])
            .arg(&helper)
            .status()?;
        if !status.success() {
            return Err(anyhow::anyhow!(
                "Could not build the macOS clipboard helper"
            ));
        }
        println!(
            "cargo:rustc-env=RESHIKI_CLIPBOARD_HELPER={}",
            helper.display()
        );
        let print_helper = helper.with_file_name("reshiki-print");
        let status = Command::new("swiftc")
            .args([
                "-O",
                "native/macos/PrintSupport.swift",
                "native/macos/Print.swift",
                "-o",
            ])
            .arg(&print_helper)
            .status()?;
        if !status.success() {
            return Err(anyhow::anyhow!("Could not build the macOS print helper"));
        }
        println!(
            "cargo:rustc-env=RESHIKI_PRINT_HELPER={}",
            print_helper.display()
        );
    }
    Ok(())
}
