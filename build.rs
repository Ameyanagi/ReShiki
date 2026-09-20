use std::{env, path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=native/macos/Clipboard.swift");
    if env::var("CARGO_CFG_TARGET_OS")? == "macos" {
        let helper = PathBuf::from(env::var_os("OUT_DIR").ok_or("Missing build output directory")?)
            .join("moruno-clipboard");
        let status = Command::new("swiftc")
            .args(["-O", "native/macos/Clipboard.swift", "-o"])
            .arg(&helper)
            .status()?;
        if !status.success() {
            return Err("Could not build the macOS clipboard helper".into());
        }
        println!(
            "cargo:rustc-env=MORUNO_CLIPBOARD_HELPER={}",
            helper.display()
        );
    }
    Ok(())
}
