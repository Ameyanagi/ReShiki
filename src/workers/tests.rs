use super::*;

#[test]
fn every_available_flag_maps_to_its_worker() {
    let entries = [
        #[cfg(windows)]
        (EMF_IMPORT_WORKER, Worker::Emf(EMF_IMPORT_WORKER)),
        #[cfg(windows)]
        (EMF_FILE_WORKER, Worker::Emf(EMF_FILE_WORKER)),
        #[cfg(windows)]
        (EMF_OFFICE_WORKER, Worker::Emf(EMF_OFFICE_WORKER)),
        (NAMING_WORKER, Worker::Naming),
        (GEOMETRY_WORKER, Worker::Geometry),
        (INCHI_WORKER, Worker::Inchi),
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        (CLIPBOARD_WORKER, Worker::Clipboard),
        #[cfg(target_os = "macos")]
        (PRINT_WORKER, Worker::Print),
    ];
    for (flag, expected) in entries {
        assert_eq!(worker(OsStr::new(flag)), Some(expected), "{flag}");
    }
}

#[test]
fn ordinary_arguments_do_not_run_workers() {
    for flag in ["--not-a-worker", "--mcp", "--cli", ""] {
        assert_eq!(worker(OsStr::new(flag)), None, "{flag}");
        assert_eq!(run(OsStr::new(flag)), None, "{flag}");
    }
}
