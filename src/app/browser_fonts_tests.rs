//! A browser has no system fonts: the interface draws with Liberation Sans
//! and, for the symbols it lacks, the bundled ReShiki Keys subset.

/// The characters in string and character literals of `source`, outside
/// comments.
fn literal_chars(source: &str) -> Vec<char> {
    let mut chars = vec![];
    for line in source.lines() {
        let code = line.trim_start();
        if code.starts_with("//") {
            continue;
        }
        let mut quoted = false;
        let mut escaped = false;
        let line: Vec<char> = code.chars().collect();
        for (i, &c) in line.iter().enumerate() {
            if quoted {
                match c {
                    _ if escaped => escaped = false,
                    '\\' => escaped = true,
                    '"' => quoted = false,
                    _ => chars.push(c),
                }
            } else if c == '"' {
                quoted = true;
            } else if c == '/' && line.get(i + 1) == Some(&'/') {
                break;
            } else if c == '\'' && line.get(i + 2) == Some(&'\'') {
                chars.push(line[i + 1]);
            }
        }
    }
    chars
}

fn sources(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.contains("test") || name.contains("qa") || name.contains("snapshots") {
            continue;
        }
        if path.is_dir() {
            sources(&path, found);
        } else if name.ends_with(".rs") {
            found.push(path);
        }
    }
}

#[test]
fn browser_fonts_draw_every_interface_symbol() {
    let faces = [
        &include_bytes!("../../crates/model/fonts/LiberationSans-Regular.ttf")[..],
        &include_bytes!("../../assets/fonts/ReShikiKeys.ttf")[..],
    ];
    let faces: Vec<_> = faces
        .iter()
        .map(|bytes| ttf_parser::Face::parse(bytes, 0).expect("bundled font"))
        .collect();
    let mut files = vec![];
    sources(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app"),
        &mut files,
    );
    assert!(files.len() > 50, "found only {} sources", files.len());
    let mut missing = std::collections::BTreeSet::new();
    for file in files {
        let source = std::fs::read_to_string(&file).expect("source");
        for c in literal_chars(&source) {
            if !c.is_ascii()
                && !c.is_control()
                && faces.iter().all(|face| face.glyph_index(c).is_none())
            {
                missing.insert(format!("U+{:04X} {c} in {}", c as u32, file.display()));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "Add these to SYMBOLS in scripts/subset_keys_font.py: {missing:#?}"
    );
}
