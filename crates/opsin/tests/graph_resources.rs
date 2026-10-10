//! Exercise the pinned resource dialect, rather than a hand-picked subset of
//! generic SMILES. This is frozen OPSIN 2.9.0 XML in the crate, never a runtime
//! upstream checkout or network resource.
use opsin::{graph::Graph, smiles::build_fragment};

#[test]
fn build_every_literal_group_resource() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/xml");
    let mut files: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "xml"))
        .collect();
    files.sort();
    let mut examples = 0;
    let mut failures = Vec::new();
    for file in files {
        let xml = std::fs::read_to_string(&file).unwrap();
        let document = roxmltree::Document::parse_with_options(
            &xml,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
        for list in document.descendants().filter(|node| {
            node.has_tag_name("tokenList") && node.attribute("tagname") == Some("group")
        }) {
            for token in list.children().filter(|node| node.has_tag_name("token")) {
                let Some(smiles) = token.attribute("value").or_else(|| list.attribute("value"))
                else {
                    continue;
                };
                let labels = token
                    .attribute("labels")
                    .or_else(|| list.attribute("labels"))
                    .unwrap_or("none");
                let fragment_type = token
                    .attribute("type")
                    .or_else(|| list.attribute("type"))
                    .unwrap_or("");
                let mut graph = Graph::default();
                if let Err(error) = build_fragment(&mut graph, smiles, fragment_type, labels) {
                    failures.push(format!(
                        "{}: {:?}, {smiles}: {error}",
                        file.file_name().unwrap().to_string_lossy(),
                        token.text()
                    ));
                }
                examples += 1;
            }
        }
    }
    assert_eq!(examples, 3_181, "Pinned literal resource coverage changed");
    assert!(
        failures.is_empty(),
        "{} failures out of {examples} frozen resource groups:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
