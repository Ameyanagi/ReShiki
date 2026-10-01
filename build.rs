use anyhow::Context;
use std::{env, path::PathBuf, process::Command};

fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=packaging/windows/reshiki.rc");
    println!("cargo:rerun-if-changed=assets/branding/reshiki.ico");
    if env::var("CARGO_CFG_TARGET_OS").context("Missing Cargo target OS")? == "windows"
        && env::var("CARGO_CFG_TARGET_ENV").context("Missing Cargo target environment")? == "msvc"
    {
        let resource =
            PathBuf::from(env::var_os("OUT_DIR").context("Missing build output directory")?)
                .join("reshiki.res");
        let status = Command::new("rc.exe")
            .args(["/nologo", "/I", "assets/branding", "/fo"])
            .arg(&resource)
            .arg("packaging/windows/reshiki.rc")
            .status()
            .context("Could not run the Windows resource compiler; select the MSVC developer environment")?;
        anyhow::ensure!(status.success(), "Could not compile the ReShiki app icon");
        println!("cargo:rustc-link-arg-bin=reshiki={}", resource.display());
    }
    Ok(())
}
