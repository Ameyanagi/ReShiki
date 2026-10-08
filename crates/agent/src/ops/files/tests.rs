// Tests build their folders with std::fs and tempfile, outside every grant.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]
use super::*;
use crate::{
    access::AccessError,
    document::Document,
    ops::{
        exec::Hooks,
        headless::HeadlessHost,
        host::{Call, ToolHost},
        wire::RequestId,
    },
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    slice,
    sync::atomic::{AtomicI64, Ordering},
};

const AROMATIC_CDXML: &[u8] =
    include_bytes!("../../../../../tests/fixtures/aromatic-circle-native.cdxml");
const ABBREVIATIONS_CDX: &[u8] =
    include_bytes!("../../../../../tests/fixtures/abbreviations-native.cdx");
const BOND_JOIN_RSK: &[u8] =
    include_bytes!("../../../../../tests/fixtures/bond-join-regression.rsk");
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn call(tool: &str, arguments: Value) -> Call {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    Call {
        principal: Principal::local(),
        request: RequestId::Int(NEXT.fetch_add(1, Ordering::Relaxed)),
        tool: tool.into(),
        arguments,
        progress: None,
    }
}

fn host(grants: Grants) -> HeadlessHost {
    HeadlessHost::new("9.8.7", Budgets::default()).with_grants(Arc::new(grants))
}

/// Reading and writing in `dir`.
fn both(dir: &Path) -> Grants {
    Grants::open(&[dir.to_path_buf()], &[dir.to_path_buf()]).unwrap()
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

/// Every entry of `dir` with its contents, to show a call touched nothing.
fn inventory(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            (name, fs::read(entry.path()).unwrap_or_default())
        })
        .collect()
}

async fn run(host: &HeadlessHost, tool: &str, arguments: Value) -> ToolResult {
    host.call(call(tool, arguments)).await.unwrap()
}

async fn ok(host: &HeadlessHost, tool: &str, arguments: Value) -> Value {
    let result = run(host, tool, arguments).await;
    assert!(!result.is_error, "{tool}: {:?}", result.value);
    assert!(result.images.is_empty() && result.files.is_empty());
    Value::Object(result.value)
}

/// The code and message of a tool execution error.
async fn failure(host: &HeadlessHost, tool: &str, arguments: Value) -> (String, String) {
    let result = run(host, tool, arguments.clone()).await;
    assert!(result.is_error, "{tool} {arguments}: {:?}", result.value);
    let error = &result.value["error"];
    (
        error["code"].as_str().unwrap().to_owned(),
        error["message"].as_str().unwrap().to_owned(),
    )
}

fn open_args(path: &Path) -> Value {
    json!({"path": text(path), "format": "auto"})
}

fn save_args(document: &Value, path: &Path, overwrite: bool) -> Value {
    json!({"document": document, "path": text(path), "format": null, "overwrite": overwrite, "pages": null})
}

fn stored(host: &HeadlessHost, value: &Value) -> Arc<Document> {
    let handle = DocHandle::new(value["value"]["document"].as_str().unwrap()).unwrap();
    host.store()
        .snapshot(&Principal::local(), &handle, Access::Read)
        .unwrap()
        .doc
}

async fn ethanol(host: &HeadlessHost) -> Value {
    let imported = ok(host, "import", json!({"format": "smiles", "text": "CCO"})).await;
    imported["value"]["document"].clone()
}

fn documents(host: &HeadlessHost) -> usize {
    host.store().list(&Principal::local()).len()
}

#[test]
fn access_denied_is_a_tool_execution_error() {
    assert_eq!(ErrorKind::Access.code(), "access_denied");
    assert!(!ErrorKind::Access.is_protocol_error());
}

#[test]
fn access_errors_keep_their_code_under_the_operation_kind() {
    let path = || "/x/a.mol".to_owned();
    for (error, kind) in [
        (
            AccessError::PathInvalid {
                path: path(),
                rule: "dotdot",
            },
            ErrorKind::InvalidArguments,
        ),
        (
            AccessError::ExtensionNotAllowed {
                path: path(),
                allowed: vec!["rsk".into()],
            },
            ErrorKind::Access,
        ),
        (
            AccessError::PathNotGranted {
                path: path(),
                access: "reading",
                roots: Vec::new(),
            },
            ErrorKind::Access,
        ),
        (
            AccessError::PathEscapesRoot { path: path() },
            ErrorKind::Access,
        ),
        (AccessError::OsDenied { path: path() }, ErrorKind::Access),
        (
            AccessError::FileTooLarge {
                path: path(),
                limit: 9,
            },
            ErrorKind::Budget,
        ),
        (AccessError::FileExists { path: path() }, ErrorKind::Failed),
        (
            AccessError::FileNotFound { path: path() },
            ErrorKind::Failed,
        ),
        (
            AccessError::NotARegularFile { path: path() },
            ErrorKind::Failed,
        ),
        (
            AccessError::NoClobberUnsupported { path: path() },
            ErrorKind::Failed,
        ),
        (
            AccessError::Io {
                path: path(),
                message: "disk full".into(),
            },
            ErrorKind::Failed,
        ),
    ] {
        let code = error.code();
        let expected = format!("{code}: {error}");
        let mapped = OpError::from(error);
        assert_eq!(mapped.kind, kind, "{code}");
        if code == "path_not_granted" {
            assert!(mapped.message.starts_with(&expected), "{}", mapped.message);
            assert!(
                mapped.message.contains("--allow-read"),
                "{}",
                mapped.message
            );
            assert!(mapped.message.contains("agent-access.json"));
        } else {
            assert_eq!(mapped.message, expected);
        }
    }
}

#[test]
fn long_paths_and_lists_are_shortened_so_the_reason_and_hint_fit() {
    let short = || "/x/a.mol".to_owned();
    let long = || format!("/{}/a.mol", "p".repeat(4090));
    let errors: [fn(String) -> AccessError; 10] = [
        |path| AccessError::PathInvalid {
            path,
            rule: "dotdot",
        },
        |path| AccessError::PathEscapesRoot { path },
        |path| AccessError::NotARegularFile { path },
        |path| AccessError::FileTooLarge { path, limit: 9 },
        |path| AccessError::FileExists { path },
        |path| AccessError::FileNotFound { path },
        |path| AccessError::OsDenied { path },
        |path| AccessError::NoClobberUnsupported { path },
        |path| AccessError::ExtensionNotAllowed {
            path,
            allowed: READ.iter().map(|&ext| ext.into()).collect(),
        },
        |path| AccessError::PathNotGranted {
            path,
            access: "reading",
            roots: Vec::new(),
        },
    ];
    for error in errors {
        let expected = OpError::from(error(short())).message;
        let (head, rest) = expected.split_once(&short()).unwrap();
        let mapped = OpError::from(error(long())).message;
        assert!(
            mapped.chars().count() < OpError::MAX_MESSAGE_CHARS,
            "{mapped}"
        );
        assert!(
            mapped.starts_with(head) && mapped.ends_with(rest),
            "{mapped}"
        );
        assert!(mapped.contains("pp…pp"), "{mapped}");
        assert!(mapped.contains("a.mol"), "{mapped}");
    }
    // A path that fits is echoed whole.
    let fits = format!("/{}/a.mol", "p".repeat(400));
    let whole = OpError::from(AccessError::FileExists { path: fits.clone() }).message;
    assert_eq!(whole, format!("file_exists: \"{fits}\" already exists"));
    let granted = OpError::from(AccessError::PathNotGranted {
        path: long(),
        access: "writing",
        roots: (0..40).map(|n| format!("/granted/folder-{n}")).collect(),
    })
    .message;
    assert!(granted.chars().count() < OpError::MAX_MESSAGE_CHARS);
    assert!(
        granted.contains(
            "\" is not inside a folder granted for writing; granted folders: /granted/folder-0, "
        ),
        "{granted}"
    );
    assert!(
        granted.contains("/granted/folder-39. Folders are granted by the user"),
        "{granted}"
    );
    assert!(granted.ends_with("info lists them."), "{granted}");
    let failed = OpError::from(AccessError::Io {
        path: long(),
        message: "x".repeat(600),
    })
    .message;
    assert!(failed.chars().count() < OpError::MAX_MESSAGE_CHARS);
    assert!(
        failed.starts_with("io_error: I/O error on \"/pp"),
        "{failed}"
    );
    let message = format!("\": {}…{}", "x".repeat(49), "x".repeat(50));
    assert!(failed.ends_with(&message), "{failed}");
}

#[test]
fn file_open_decodes_the_format_from_the_extension() {
    let decoded = |path: &str, format: &str| {
        decode_open(json!({"path": path, "format": format})).map(|open| (open.path, open.format))
    };
    for (path, format, expected) in [
        ("/x/a.MOL", "auto", import::Format::Mol),
        ("/x/a.mol", "mol", import::Format::Mol),
        ("/x/a.rsk", "auto", import::Format::Reshiki),
        ("/x/a.moruno", "reshiki", import::Format::Reshiki),
        ("/x/a.smi", "auto", import::Format::Smiles),
        ("/x/a.smiles", "smiles", import::Format::Smiles),
        ("/x/a.cdx", "auto", import::Format::Cdx),
        ("/x/a.rsmi", "auto", import::Format::Rsmi),
        ("a.inchi", "inchi", import::Format::Inchi),
    ] {
        assert_eq!(
            decoded(path, format),
            Ok((path.to_owned(), expected)),
            "{path} {format}"
        );
    }
    let mismatch = decoded("/x/a.cdxml", "mol").unwrap_err();
    assert_eq!(mismatch.kind, ErrorKind::InvalidArguments);
    assert_eq!(
        mismatch.message,
        "format mol does not match the .cdxml extension, which opens as cdxml"
    );
    for path in ["/x/a.exe", "/x/a.svg", "/x/a", ""] {
        let error = decoded(path, "auto").unwrap_err();
        assert_eq!(error.kind, ErrorKind::Access, "{path}");
        assert!(
            error.message.starts_with(&format!(
                "extension_not_allowed: \"{path}\" does not have an allowed extension (rsk, "
            )),
            "{}",
            error.message
        );
    }
    let long = format!("/{}.mol", "a".repeat(4091));
    assert_eq!(decoded(&long, "auto").map(drop), Ok(()));
    let error = decoded(&format!("/{long}"), "auto").unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidArguments);
    assert_eq!(
        error.message,
        "path_invalid: the path is 4097 bytes; at most 4096 are allowed"
    );
}

#[test]
fn file_save_decodes_the_format_from_the_extension() {
    let decoded = |path: &str, format: Value, pages: Value| {
        decode_save(json!({"document": "doc_1", "path": path, "format": format, "overwrite": false, "pages": pages}))
            .map(|save| (save.format, save.pages))
    };
    for (path, format, expected) in [
        ("/x/a.rsk", json!(null), SaveFormat::Reshiki),
        ("/x/a.rsk", json!("reshiki"), SaveFormat::Reshiki),
        ("/x/a.PNG", json!(null), SaveFormat::Png),
        ("/x/a.svg", json!("svg"), SaveFormat::Svg),
        ("/x/a.smi", json!("smiles"), SaveFormat::Smiles),
        ("/x/a.inchi", json!(null), SaveFormat::Inchi),
    ] {
        assert_eq!(
            decoded(path, format.clone(), Value::Null),
            Ok((expected, false)),
            "{path} {format}"
        );
    }
    assert_eq!(
        decoded("/x/a.pdf", json!(null), json!(true)),
        Ok((SaveFormat::Pdf, true))
    );
    let pages = decoded("/x/a.svg", json!(null), json!(true)).unwrap_err();
    assert_eq!(pages.kind, ErrorKind::InvalidArguments);
    assert_eq!(pages.message, "pages is valid only with format pdf");
    let mismatch = decoded("/x/a.png", json!("svg"), Value::Null).unwrap_err();
    assert_eq!(mismatch.kind, ErrorKind::InvalidArguments);
    assert_eq!(
        mismatch.message,
        "format svg does not match the .png extension, which saves as png"
    );
    for path in [
        "/x/a.exe",
        "/x/a.rxn",
        "/x/a.cdx",
        "/x/a.reshiki",
        "/x/a.emf",
    ] {
        let error = decoded(path, json!(null), Value::Null).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Access, "{path}");
        assert!(
            error.message.starts_with("extension_not_allowed: ")
                && error
                    .message
                    .ends_with("(rsk, mol, cdxml, smi, smiles, inchi, svg, pdf, png)"),
            "{}",
            error.message
        );
    }
    for format in SAVE_FORMATS {
        let parsed: SaveFormat = serde_json::from_value(json!(format)).unwrap();
        assert_eq!(parsed.name(), *format);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_open_gives_what_import_gives_for_the_same_text() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(both(dir.path()));
    // A MOL file the engine wrote.
    let ethanol = ethanol(&host).await;
    let exported = run(
        &host,
        "export",
        json!({"document": ethanol, "format": "mol", "pages": null}),
    )
    .await;
    let [mol] = exported.files.as_slice() else {
        panic!("one mol file: {:?}", exported.value)
    };
    for (name, bytes, format, import_text) in [
        ("ethanol.mol", mol.bytes.clone(), "mol", None),
        ("aromatic.cdxml", AROMATIC_CDXML.to_vec(), "cdxml", None),
        (
            "abbreviations.cdx",
            ABBREVIATIONS_CDX.to_vec(),
            "cdx",
            Some(STANDARD.encode(ABBREVIATIONS_CDX)),
        ),
        ("bond-join.rsk", BOND_JOIN_RSK.to_vec(), "reshiki", None),
    ] {
        let path = dir.path().join(name);
        fs::write(&path, &bytes).unwrap();
        let opened = ok(&host, "file_open", open_args(&path)).await;
        let import_text = import_text.unwrap_or_else(|| String::from_utf8(bytes.clone()).unwrap());
        let imported = ok(
            &host,
            "import",
            json!({"format": format, "text": import_text}),
        )
        .await;
        assert_eq!(
            opened["value"]["source"],
            json!({"path": text(&path), "bytes": bytes.len()}),
            "{name}"
        );
        for field in ["counts", "analysis", "revision"] {
            assert_eq!(
                opened["value"][field], imported["value"][field],
                "{name} {field}"
            );
        }
        assert_eq!(opened["warnings"], imported["warnings"], "{name}");
        assert_eq!(opened["validation"], imported["validation"], "{name}");
        assert_ne!(opened["value"]["document"], imported["value"]["document"]);
        assert_eq!(stored(&host, &opened), stored(&host, &imported), "{name}");
        // Import results carry no source.
        assert!(imported["value"].get("source").is_none());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_save_writes_every_format() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(both(dir.path()));
    let document = ethanol(&host).await;
    for (ext, check) in [
        (
            "svg",
            (|bytes| String::from_utf8_lossy(bytes).contains("<svg")) as fn(&[u8]) -> bool,
        ),
        ("pdf", |bytes| bytes.starts_with(b"%PDF-")),
        ("png", |bytes| bytes.starts_with(PNG_SIGNATURE)),
        ("mol", |bytes| {
            String::from_utf8_lossy(bytes).contains("M  END")
        }),
        ("cdxml", |bytes| {
            String::from_utf8_lossy(bytes).contains("<CDXML")
        }),
        ("smi", |bytes| {
            String::from_utf8_lossy(bytes).trim() == "CCO"
        }),
        ("inchi", |bytes| bytes.starts_with(b"InChI=1S/C2H6O/")),
        ("rsk", |bytes| Document::from_native_file(bytes).is_ok()),
    ] {
        let path = dir.path().join(format!("ethanol.{ext}"));
        let saved = ok(&host, "file_save", save_args(&document, &path, false)).await;
        let bytes = fs::read(&path).unwrap();
        assert!(check(&bytes), "{ext}");
        let format = FileFormat::of(&text(&path)).unwrap().export.unwrap();
        let receipt = &saved["value"]["receipt"];
        assert_eq!(receipt["path"], text(&path), "{ext}");
        assert_eq!(receipt["format"], format, "{ext}");
        assert_eq!(receipt["byte_len"], bytes.len(), "{ext}");
        assert_eq!(receipt["replaced"], false, "{ext}");
        assert_eq!(receipt["detail"].is_string(), ext == "png", "{ext}");
        assert_eq!(saved["validation"], json!({"status": "valid"}));
        if matches!(ext, "mol" | "cdxml" | "smi" | "inchi") {
            let exported = run(
                &host,
                "export",
                json!({"document": document, "format": format, "pages": null}),
            )
            .await;
            assert_eq!(exported.files[0].bytes, bytes, "{ext}");
        }
    }
    let names: Vec<String> = inventory(dir.path()).into_keys().collect();
    assert_eq!(names.len(), 8, "no temporary file is left: {names:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_saved_drawing_opens_as_an_equal_document() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("bond-join.rsk");
    fs::write(&source, BOND_JOIN_RSK).unwrap();
    let host = host(both(dir.path()));
    let opened = ok(&host, "file_open", open_args(&source)).await;
    let original = stored(&host, &opened);
    let copy = dir.path().join("copy.rsk");
    ok(
        &host,
        "file_save",
        save_args(&opened["value"]["document"], &copy, false),
    )
    .await;
    assert_eq!(fs::read(&copy).unwrap(), original.file_json().unwrap());
    let reopened = ok(&host, "file_open", open_args(&copy)).await;
    assert_eq!(*stored(&host, &reopened), original.current());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_grants_every_path_is_refused_and_nothing_is_touched() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.mol"), b"not read").unwrap();
    let before = inventory(dir.path());
    let host = host(Grants::none());
    let document = ethanol(&host).await;
    let (code, message) = failure(&host, "file_open", open_args(&dir.path().join("a.mol"))).await;
    assert_eq!(code, "access_denied");
    assert!(message.starts_with("path_not_granted: "), "{message}");
    assert!(message.contains("no folders are granted for reading"));
    assert!(message.contains("--allow-read"), "{message}");
    let (code, message) = failure(
        &host,
        "file_save",
        save_args(&document, &dir.path().join("b.svg"), true),
    )
    .await;
    assert_eq!(code, "access_denied");
    assert!(message.starts_with("path_not_granted: "), "{message}");
    assert!(message.contains("no folders are granted for writing"));
    // A long path still leaves room for the reason and the hint.
    let long = ["a", "b", "c"]
        .iter()
        .fold(dir.path().to_path_buf(), |path, name| {
            path.join(name.repeat(200))
        })
        .join("a.mol");
    assert!(text(&long).chars().count() > OpError::MAX_MESSAGE_CHARS);
    let (code, message) = failure(&host, "file_open", open_args(&long)).await;
    assert_eq!(code, "access_denied");
    assert!(message.starts_with("path_not_granted: "), "{message}");
    assert!(message.contains("…"), "{message}");
    assert!(message.contains("no folders are granted for reading"));
    assert!(message.ends_with("info lists them."), "{message}");
    assert_eq!(inventory(dir.path()), before);
    assert_eq!(documents(&host), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_and_write_roots_are_separate() {
    let dir = tempfile::tempdir().unwrap();
    let mol = dir.path().join("a.mol");
    fs::write(&mol, b"not read").unwrap();
    let before = inventory(dir.path());
    let read_only = host(Grants::open(&[dir.path().to_path_buf()], &[]).unwrap());
    let document = ethanol(&read_only).await;
    let (code, message) = failure(
        &read_only,
        "file_save",
        save_args(&document, &dir.path().join("b.mol"), false),
    )
    .await;
    assert_eq!(code, "access_denied");
    assert!(message.starts_with("path_not_granted: "), "{message}");
    let write_only = host(Grants::open(&[], &[dir.path().to_path_buf()]).unwrap());
    let (code, message) = failure(&write_only, "file_open", open_args(&mol)).await;
    assert_eq!(code, "access_denied");
    assert!(message.starts_with("path_not_granted: "), "{message}");
    assert_eq!(documents(&write_only), 0);
    assert_eq!(inventory(dir.path()), before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn existing_files_are_kept_unless_overwrite_is_true() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ethanol.smi");
    fs::write(&path, b"kept").unwrap();
    let host = host(both(dir.path()));
    let document = ethanol(&host).await;
    let (code, message) = failure(&host, "file_save", save_args(&document, &path, false)).await;
    assert_eq!(code, "failed");
    assert!(message.starts_with("file_exists: "), "{message}");
    assert_eq!(fs::read(&path).unwrap(), b"kept");
    let saved = ok(&host, "file_save", save_args(&document, &path, true)).await;
    assert_eq!(saved["value"]["receipt"]["replaced"], true);
    assert_eq!(
        String::from_utf8(fs::read(&path).unwrap()).unwrap().trim(),
        "CCO"
    );
    assert_eq!(inventory(dir.path()).len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn executables_and_dotdot_paths_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("tool.exe"), b"MZ").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("a.mol"), b"not read").unwrap();
    let before = inventory(dir.path());
    let host = host(both(dir.path()));
    let document = ethanol(&host).await;
    let exe = dir.path().join("tool.exe");
    let (code, message) = failure(&host, "file_open", open_args(&exe)).await;
    assert_eq!(code, "access_denied");
    assert!(message.starts_with("extension_not_allowed: "), "{message}");
    let (code, message) = failure(&host, "file_save", save_args(&document, &exe, true)).await;
    assert_eq!(code, "access_denied");
    assert!(message.starts_with("extension_not_allowed: "), "{message}");
    let dotdot = dir.path().join("sub").join("..").join("a.mol");
    let (code, message) = failure(&host, "file_open", open_args(&dotdot)).await;
    assert_eq!(code, "invalid_arguments");
    assert!(message.starts_with("path_invalid: "), "{message}");
    let (code, message) = failure(&host, "file_save", save_args(&document, &dotdot, true)).await;
    assert_eq!(code, "invalid_arguments");
    assert!(message.starts_with("path_invalid: "), "{message}");
    assert_eq!(inventory(dir.path()), before);
    assert_eq!(documents(&host), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_oversize_file_is_refused_before_the_engine() {
    let dir = tempfile::tempdir().unwrap();
    let budgets = Budgets {
        max_text_bytes: 64,
        ..Budgets::default()
    };
    let host = HeadlessHost::new("9.8.7", budgets).with_grants(Arc::new(both(dir.path())));
    // Not a MOL file: only the engine would say so.
    let fits = dir.path().join("fits.mol");
    fs::write(&fits, [b'x'; 64]).unwrap();
    let (code, _) = failure(&host, "file_open", open_args(&fits)).await;
    assert_eq!(code, "failed");
    let oversize = dir.path().join("oversize.mol");
    fs::write(&oversize, [b'x'; 65]).unwrap();
    let (code, message) = failure(&host, "file_open", open_args(&oversize)).await;
    assert_eq!(code, "budget");
    assert!(message.starts_with("file_too_large: "), "{message}");
    assert!(message.contains("64-byte limit"), "{message}");
    assert_eq!(documents(&host), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_over_max_output_bytes_is_refused_and_nothing_is_touched() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["kept.rsk", "kept.svg"] {
        fs::write(dir.path().join(name), b"kept").unwrap();
    }
    let before = inventory(dir.path());
    let budgets = Budgets {
        max_output_bytes: 1,
        ..Budgets::default()
    };
    let host = HeadlessHost::new("9.8.7", budgets).with_grants(Arc::new(both(dir.path())));
    let empty = ok(&host, "document_new", json!({})).await["document"].clone();
    let document = ethanol(&host).await;
    for (document, name, overwrite) in [
        (&empty, "new.rsk", false),
        (&document, "new.rsk", false),
        (&document, "kept.rsk", true),
        (&document, "new.svg", false),
        (&document, "kept.svg", true),
    ] {
        let path = dir.path().join(name);
        let (code, message) =
            failure(&host, "file_save", save_args(document, &path, overwrite)).await;
        assert_eq!(code, "budget", "{name}");
        assert!(
            message.contains("bytes; max_output_bytes allows at most 1."),
            "{message}"
        );
    }
    assert_eq!(inventory(dir.path()), before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_call_creates_no_file_and_no_document() {
    let dir = tempfile::tempdir().unwrap();
    let mol = dir.path().join("a.mol");
    fs::write(&mol, b"not read").unwrap();
    let before = inventory(dir.path());
    let host = host(both(dir.path()));
    let document = ethanol(&host).await;
    let exec = host.exec().clone();
    for request in [
        call(
            "file_save",
            save_args(&document, &dir.path().join("b.svg"), true),
        ),
        call("file_open", open_args(&mol)),
    ] {
        let id = request.request.clone();
        exec.set_hooks(Hooks {
            acquired: Some(Arc::new({
                let exec = exec.clone();
                move || exec.cancel(&Principal::local(), &id)
            })),
            ..Hooks::default()
        });
        let error = host.call(request).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::Cancelled);
    }
    assert_eq!(inventory(dir.path()), before);
    assert_eq!(documents(&host), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn info_lists_the_grants_and_the_file_extensions() {
    let dir = tempfile::tempdir().unwrap();
    let (read, write): (PathBuf, PathBuf) = (dir.path().join("in"), dir.path().join("out"));
    fs::create_dir(&read).unwrap();
    fs::create_dir(&write).unwrap();
    let files = json!({
        "read": ["rsk", "reshiki", "moruno", "mol", "rxn", "rsmi", "cdxml", "cdx", "smi", "smiles", "inchi"],
        "write": ["rsk", "mol", "cdxml", "smi", "smiles", "inchi", "svg", "pdf", "png"],
    });
    let granted = host(Grants::open(slice::from_ref(&read), slice::from_ref(&write)).unwrap());
    let info = ok(&granted, "info", json!({})).await;
    assert_eq!(
        info["grants"],
        json!({"read": [text(&read)], "write": [text(&write)]})
    );
    assert_eq!(info["files"], files);
    let none = ok(&host(Grants::none()), "info", json!({})).await;
    assert_eq!(none["grants"], json!({"read": [], "write": []}));
    assert_eq!(none["files"], files);
}
