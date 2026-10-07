use reshiki::{document::Document, template_library::Library, templates::Anchor};
use std::{hint::black_box, time::Instant};

fn measure(name: &str, mut operation: impl FnMut()) {
    for _ in 0..3 {
        operation();
    }
    let mut elapsed = Vec::new();
    for _ in 0..30 {
        let start = Instant::now();
        operation();
        elapsed.push(start.elapsed().as_secs_f64() * 1000.);
    }
    elapsed.sort_by(f64::total_cmp);
    eprintln!(
        "{name}: median {:.3} ms, p95 {:.3} ms",
        elapsed[15], elapsed[28]
    );
}

#[test]
#[ignore = "release-mode filesystem and parsing benchmark"]
fn file_library_workloads() {
    let drawing: Document = serde_json::from_str(include_str!(
        "../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    let text = serde_json::to_string_pretty(&drawing).unwrap();
    measure("native_parse_validate_1x", || {
        let doc: Document = serde_json::from_str(&text).unwrap();
        doc.validate().unwrap();
        black_box(doc);
    });
    measure("native_serialize_1x", || {
        black_box(serde_json::to_vec_pretty(&drawing).unwrap());
    });
    let mut library = Library::default();
    let mut fragment = Document::default();
    fragment.add_atom("O", Default::default());
    // Construct in linear time so setup does not dominate the benchmark.
    library
        .add("Template 0", "Mine", fragment, Anchor::Auto)
        .unwrap();
    let first = library.templates[0].clone();
    for index in 1..512 {
        let mut template = first.clone();
        template.id = format!("benchmark-{index}");
        template.name = format!("Template {index}");
        library.templates.push(template);
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("templates.json");
    library.save(&path).unwrap();
    measure("library_load_512", || {
        black_box(Library::load(&path).unwrap());
    });
    measure("library_save_checked_512", || {
        library.save_checked(&path, &library).unwrap();
    });
    let bytes = serde_json::to_vec(&library).unwrap();
    measure("library_parse_validate_512", || {
        black_box(Library::from_bytes(&bytes).unwrap());
    });
}
