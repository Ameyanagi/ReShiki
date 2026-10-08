use anyhow::Context;
use std::{env, fs, path::PathBuf, process::Command};

fn numeric_version(version: &str) -> anyhow::Result<[u16; 4]> {
    // Match scripts/build_release.py: prerelease/build identity belongs in the
    // string fields. Windows stores four unsigned 16-bit numeric components.
    let base = version.split(['-', '+']).next().unwrap_or_default();
    let components: Vec<_> = base.split('.').collect();
    anyhow::ensure!(
        components.len() == 3,
        "Expected a three-component numeric package version"
    );
    let mut result = [0; 4];
    for (index, component) in components.into_iter().enumerate() {
        anyhow::ensure!(
            !component.is_empty()
                && component.bytes().all(|byte| byte.is_ascii_digit())
                && (component.len() == 1 || !component.starts_with('0')),
            "Expected a three-component numeric package version"
        );
        result[index] = component
            .parse()
            .context("Package version exceeds a Windows version component")?;
    }
    Ok(result)
}

fn rc_string(value: &str) -> anyhow::Result<String> {
    // RC uses doubled quotes, rather than C's backslash-quote escape. Escape
    // backslashes separately so they cannot introduce RC escape sequences.
    let mut escaped = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\"\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            control if control.is_control() => {
                anyhow::bail!("Windows resource strings cannot contain control characters")
            }
            other => escaped.push(other),
        }
    }
    escaped.push_str("\\0\"");
    Ok(escaped)
}

fn version_header(version: &str) -> anyhow::Result<String> {
    let [major, minor, patch, revision] = numeric_version(version)?;
    let flags = if version.split('+').next().unwrap_or_default().contains('-') {
        "VS_FF_PRERELEASE"
    } else {
        "0"
    };
    Ok(format!(
        "// Generated from CARGO_PKG_VERSION; do not edit.\n\
         #define RESHIKI_NUMERIC_VERSION {major},{minor},{patch},{revision}\n\
         #define RESHIKI_VERSION_STRING {}\n\
         #define RESHIKI_FILE_FLAGS {flags}\n",
        rc_string(version)?
    ))
}

fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=packaging/windows/reshiki.rc");
    println!("cargo:rerun-if-changed=assets/branding/reshiki.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");
    if env::var("CARGO_CFG_TARGET_OS").context("Missing Cargo target OS")? == "windows"
        && env::var("CARGO_CFG_TARGET_ENV").context("Missing Cargo target environment")? == "msvc"
    {
        let output_dir =
            PathBuf::from(env::var_os("OUT_DIR").context("Missing build output directory")?);
        let version = env::var("CARGO_PKG_VERSION").context("Missing Cargo package version")?;
        fs::write(
            output_dir.join("reshiki-version.h"),
            version_header(&version)?,
        )
        .context("Could not write the Windows version resource header")?;
        let resource = output_dir.join("reshiki.res");
        let status = Command::new("rc.exe")
            .args(["/nologo", "/I", "assets/branding", "/I"])
            .arg(&output_dir)
            .arg("/fo")
            .arg(&resource)
            .arg("packaging/windows/reshiki.rc")
            .status()
            .context("Could not run the Windows resource compiler; select the MSVC developer environment")?;
        anyhow::ensure!(
            status.success(),
            "Could not compile the ReShiki Windows resources"
        );
        println!("cargo:rustc-link-arg-bin=reshiki={}", resource.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_numeric_versions_match_packaging_convention() {
        for version in [
            "0.11.0",
            "0.11.0-nightly.20261009.36501221724.1",
            "0.11.0+build.123",
        ] {
            assert_eq!(numeric_version(version).unwrap(), [0, 11, 0, 0]);
        }
        assert_eq!(
            numeric_version("65535.65535.65535").unwrap(),
            [65535, 65535, 65535, 0]
        );
        for invalid in [
            "0.11",
            "65536.1.1",
            "1.2.bad",
            "../0.11.0",
            "01.2.3",
            "1..3",
        ] {
            assert!(numeric_version(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn resource_header_preserves_exact_nightly_identity() {
        let nightly = "0.11.0-nightly.20261009.36501221724.1";
        let header = version_header(nightly).unwrap();
        assert!(header.contains("#define RESHIKI_NUMERIC_VERSION 0,11,0,0\n"));
        assert!(header.contains(&format!(
            "#define RESHIKI_VERSION_STRING \"{nightly}\\0\"\n"
        )));
        assert!(header.contains("#define RESHIKI_FILE_FLAGS VS_FF_PRERELEASE\n"));
        for stable in ["0.11.0", "0.11.0+build-with-hyphen"] {
            assert!(
                version_header(stable)
                    .unwrap()
                    .contains("#define RESHIKI_FILE_FLAGS 0\n")
            );
        }
    }

    #[test]
    fn resource_strings_escape_quotes_backslashes_and_lines() {
        assert_eq!(
            rc_string("quoted \"value\"\\path\n\r\t").unwrap(),
            "\"quoted \"\"value\"\"\\\\path\\n\\r\\t\\0\""
        );
        assert!(rc_string("truncated\0value").is_err());
    }

    #[test]
    fn executable_resource_exposes_signing_identity_and_retains_icon() {
        let resource = include_str!("packaging/windows/reshiki.rc");
        for declaration in [
            "1 ICON \"reshiki.ico\"",
            "FILEVERSION RESHIKI_NUMERIC_VERSION",
            "PRODUCTVERSION RESHIKI_NUMERIC_VERSION",
            "VALUE \"ProductName\", \"ReShiki\\0\"",
            "VALUE \"ProductVersion\", RESHIKI_VERSION_STRING",
            "VALUE \"FileVersion\", RESHIKI_VERSION_STRING",
        ] {
            assert!(resource.contains(declaration), "missing {declaration}");
        }
    }
}
