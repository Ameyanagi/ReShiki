use super::{LibraryLock, SaveOutcome, lock_error, with_library_lock};
use std::{fs::TryLockError, io};

#[cfg(unix)]
#[test]
fn guard_releases_lock_even_while_a_duplicate_description_survives() {
    for fail in [false, true] {
        let file = tempfile::NamedTempFile::new().unwrap();
        let lock = file.reopen().unwrap();
        lock.lock().unwrap();
        let retained = lock.try_clone().unwrap();
        let contender = file.reopen().unwrap();
        let lock = LibraryLock(Some(lock));
        assert!(matches!(
            contender.try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        let operation = if fail {
            Err("save failed".into())
        } else {
            Ok(())
        };
        let result = lock.finish(operation, std::fs::File::unlock);
        assert_eq!(result.is_err(), fail);
        contender
            .try_lock()
            .expect("guard must release the shared description");
        contender.unlock().unwrap();
        drop(retained);
    }
}

#[test]
fn normal_finish_distinguishes_committed_warnings_from_operation_failures() {
    for operation_failed in [false, true] {
        for release_failed in [false, true] {
            let file = tempfile::NamedTempFile::new().unwrap();
            let lock = file.reopen().unwrap();
            lock.lock().unwrap();
            let calls = std::cell::Cell::new(0);
            let operation = if operation_failed {
                Err("injected write failure before commit".to_owned())
            } else {
                Ok(())
            };
            let error = io::Error::from_raw_os_error(5);
            let diagnostic = format!("({:?}): {error}", error.kind());
            let result = LibraryLock(Some(lock)).finish(operation, |file| {
                calls.set(calls.get() + 1);
                file.unlock()?;
                // Only the reported cleanup result is injected. No real
                // operating-system unlock failure is claimed by this test.
                if release_failed { Err(error) } else { Ok(()) }
            });
            assert_eq!(calls.get(), 1);
            match (operation_failed, release_failed) {
                (false, false) => assert_eq!(result.unwrap(), SaveOutcome::default()),
                (false, true) => {
                    let warning = result.unwrap().release_warning.unwrap();
                    assert!(warning.starts_with("Templates saved, but"));
                    assert!(warning.contains(&diagnostic));
                }
                (true, false) => {
                    assert_eq!(result.unwrap_err(), "injected write failure before commit");
                }
                (true, true) => {
                    let error = result.unwrap_err();
                    assert!(error.starts_with("injected write failure before commit;"));
                    assert!(error.contains(&diagnostic));
                    assert!(!error.contains("Templates saved"));
                }
            }
            let contender = file.reopen().unwrap();
            contender.try_lock().unwrap();
            contender.unlock().unwrap();
        }
    }
}

#[cfg(unix)]
#[test]
fn failed_explicit_release_closes_once_without_a_drop_retry() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let lock = file.reopen().unwrap();
    lock.lock().unwrap();
    let retained = lock.try_clone().unwrap();
    let mut guard = LibraryLock(Some(lock));
    let error = guard
        .release(|_| Err(io::Error::other("injected unlock failure")))
        .unwrap_err();
    assert_eq!(error.to_string(), "injected unlock failure");
    assert!(guard.0.is_none());
    drop(guard);
    let contender = file.reopen().unwrap();
    assert!(matches!(
        contender.try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    // The injection deliberately did not release the OS lock. A Drop retry
    // would have released this shared description and failed the assertion.
    retained.unlock().unwrap();
    contender.try_lock().unwrap();
    contender.unlock().unwrap();
}

#[cfg(unix)]
#[test]
fn guard_unwind_releases_a_retained_description_without_masking_the_panic() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let lock = file.reopen().unwrap();
    lock.lock().unwrap();
    let retained = lock.try_clone().unwrap();
    let result = std::panic::catch_unwind(move || {
        let _lock = LibraryLock(Some(lock));
        panic!("injected operation panic");
    });
    assert_eq!(
        result.unwrap_err().downcast_ref::<&str>(),
        Some(&"injected operation panic")
    );
    let contender = file.reopen().unwrap();
    contender.try_lock().unwrap();
    contender.unlock().unwrap();
    drop(retained);
}

#[test]
fn library_operation_holds_lock_through_persist_and_releases_on_write_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("templates.json");
    let lock_path = path.with_extension("json.lock");
    std::fs::write(&path, b"before").unwrap();
    for write_fails in [true, false] {
        let before = std::fs::read(&path).unwrap();
        let outcome = with_library_lock(&lock_path, || {
            let contender = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&lock_path)
                .unwrap();
            assert!(matches!(
                contender.try_lock(),
                Err(TryLockError::WouldBlock)
            ));
            if write_fails {
                return Err("injected write failure before persist".into());
            }
            crate::storage::write_atomic(&path, b"committed")?;
            assert_eq!(std::fs::read(&path).unwrap(), b"committed");
            assert!(matches!(
                contender.try_lock(),
                Err(TryLockError::WouldBlock)
            ));
            Ok(())
        });
        if write_fails {
            assert_eq!(
                outcome.unwrap_err(),
                "injected write failure before persist"
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);
        } else {
            assert_eq!(outcome.unwrap(), SaveOutcome::default());
        }
        let contender = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        contender.try_lock().unwrap();
        contender.unlock().unwrap();
    }
}

#[test]
fn lock_errors_distinguish_contention_from_io_failures() {
    assert_eq!(
        lock_error(TryLockError::WouldBlock),
        "Another window is saving templates. Try again."
    );

    let error = io::Error::from_raw_os_error(5);
    let message = error.to_string();
    let kind = format!("{:?}", error.kind());
    let reported = lock_error(TryLockError::Error(error));
    assert!(reported.starts_with("Could not lock the template library"));
    assert!(reported.contains(&message));
    assert!(reported.contains(&kind));

    assert_eq!(
        lock_error(TryLockError::Error(io::Error::new(
            io::ErrorKind::Unsupported,
            "File locking is unavailable",
        ))),
        "Could not lock the template library (Unsupported): File locking is unavailable"
    );
}

#[test]
fn templates_keep_3d_models() {
    use crate::models3d::{ForceField, Model3d, Placed};
    use crate::{document::Document, templates::Anchor};
    let mut doc = Document::default();
    let a = doc.add_atom("C", Default::default());
    let b = doc.add_atom("N", crate::document::Point::new(42., 0.));
    doc.add_bond(a, b, 3, "plain");
    let placed = vec![
        Placed {
            id: a,
            xyz: [-0.6, 0., 0.],
        },
        Placed {
            id: b,
            xyz: [0.6, 0., 0.],
        },
    ];
    doc.models
        .push(Model3d::new(&doc, placed, vec![], ForceField::Mmff94s));
    doc.require_version(crate::document::MODELS_3D);
    let mut library = super::Library::default();
    library
        .add("Nitrile", "Mine", doc.clone(), Anchor::Atom(a))
        .unwrap();
    let bytes = serde_json::to_vec(&library).unwrap();
    let read = super::Library::from_bytes(&bytes).unwrap();
    assert_eq!(read.templates[0].document.models, doc.models);
}

#[test]
fn exported_library_bytes_are_the_saved_file() {
    use crate::{document::Document, templates::Anchor};
    let mut doc = Document::default();
    let a = doc.add_atom("C", Default::default());
    let mut library = super::Library::default();
    library.add("Methyl", "Mine", doc, Anchor::Atom(a)).unwrap();
    let bytes = library.to_bytes().unwrap();
    assert_eq!(super::Library::from_bytes(&bytes).unwrap(), library);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("My templates.reshiki-templates");
    library.save(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}
