//! Select independently captured Linux expectations without inspecting Rust output.
use std::ffi::OsStr;

pub fn fixture(name: &str) -> anyhow::Result<String> {
    select(
        name,
        std::env::var_os("RESHIKI_TEST_LINUX_GOLDENS").as_deref(),
    )
}

fn select(name: &str, selector: Option<&OsStr>) -> anyhow::Result<String> {
    let Some(selector) = selector else {
        return Ok(name.into());
    };
    anyhow::ensure!(
        selector == "ubuntu-22.04-x64",
        "Unknown Linux golden fixture selector: {selector:?}"
    );
    anyhow::ensure!(
        cfg!(all(target_os = "linux", target_arch = "x86_64")),
        "Ubuntu 22.04 x64 goldens require a Linux x64 test target"
    );
    let selected = match name {
        "depict-geometry-linux-native.json.gz" => "depict-geometry-ubuntu-22.04-x64-native.json.gz",
        "depict-rings-linux-native.json.gz" => "depict-rings-ubuntu-22.04-x64-native.json.gz",
        "depict-attachment-linux-native.json.gz" => {
            "depict-attachment-ubuntu-22.04-x64-native.json.gz"
        }
        "depict-templates-linux-native.json.gz" => {
            "depict-templates-ubuntu-22.04-x64-native.json.gz"
        }
        _ => name,
    };
    Ok(selected.into())
}

#[test]
fn explicit_platform_selection_preserves_other_audits_and_rejects_unknown_profiles()
-> anyhow::Result<()> {
    let source = "depict-geometry-linux-native.json.gz";
    assert_eq!(select(source, None)?, source);
    assert!(select(source, Some(OsStr::new("unknown"))).is_err());
    let selected = Some(OsStr::new("ubuntu-22.04-x64"));
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        assert_eq!(
            select(source, selected)?,
            "depict-geometry-ubuntu-22.04-x64-native.json.gz"
        );
        for unchanged in [
            "depict-geometry-native.json.gz",
            "depict-geometry-windows-native.json.gz",
            "depict-seeds-linux-native.json.gz",
        ] {
            assert_eq!(select(unchanged, selected)?, unchanged);
        }
    } else {
        assert!(select(source, selected).is_err());
    }
    Ok(())
}
