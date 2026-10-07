use super::{Error, filename, resolve};
use std::{ffi::OsStr, path::Path};

fn executable(path: &Path) -> anyhow::Result<()> {
    std::fs::write(path, b"fixture; discovery must not execute this file")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

#[test]
fn resolves_application_itself_with_spaces_and_unicode() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    for folder in ["ReShiki portable 日本語", "ReShiki.app/Contents/MacOS"] {
        let directory = root.path().join(folder);
        std::fs::create_dir_all(&directory)?;
        let application = directory.join("reshiki");
        executable(&application)?;
        // A stale sibling is never used, even if it is executable.
        executable(&directory.join(filename()))?;
        assert_eq!(resolve(&application, None)?, application.canonicalize()?);
    }
    Ok(())
}

#[test]
fn explicit_development_override_must_exist_and_be_absolute() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let helper = root.path().join(filename());
    executable(&helper)?;
    let application = root.path().join("different/reshiki");
    assert_eq!(
        resolve(&application, Some(helper.as_os_str()))?,
        helper.canonicalize()?
    );
    for supplied in ["", "reshiki-inchi-helper", "../helper"] {
        assert!(matches!(
            resolve(&application, Some(OsStr::new(supplied))),
            Err(Error::RelativeOverride)
        ));
    }
    assert!(matches!(
        resolve(&application, Some(root.path().join("missing").as_os_str())),
        Err(Error::Unavailable { .. })
    ));
    assert!(matches!(
        resolve(&application, Some(root.path().as_os_str())),
        Err(Error::NotExecutable(_))
    ));
    Ok(())
}

#[test]
fn missing_application_is_an_error_without_sibling_fallback() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    executable(&root.path().join(filename()))?;
    assert!(matches!(
        resolve(&root.path().join("other/reshiki"), None),
        Err(Error::Unavailable { .. })
    ));
    assert!(matches!(
        resolve(Path::new("reshiki"), None),
        Err(Error::ApplicationPath)
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_nonexecutable_files() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("reshiki"), b"not executable")?;
    assert!(matches!(
        resolve(&root.path().join("reshiki"), None),
        Err(Error::NotExecutable(_))
    ));
    Ok(())
}
