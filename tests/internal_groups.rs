use anyhow::{Context, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reshiki::{
    chemistry::cdxml::flatten_abbreviations,
    document::Document,
    engine::{LocalEngine, Request},
    exchange,
    scene::{self, Primitive},
};

// A desktop application returned this editable CDX after pasting our twenty
// internal-label examples. Its CCl2, CF2 and NMe labels contain real fragments,
// with two external connection points on the same attachment atom.
const RETURNED: &[u8] = include_bytes!("fixtures/internal-labels-returned.cdx");

fn check_groups(doc: &Document) -> anyhow::Result<()> {
    doc.validate().map_err(anyhow::Error::msg)?;
    ensure!(doc.atoms.len() == 80 && doc.bonds.len() == 60);
    ensure!(doc.abbreviations.len() == 12);
    ensure!(doc.atoms.iter().all(|a| a.element != "*"));
    for label in ["CCl2", "CF2", "NMe"] {
        let groups: Vec<_> = doc
            .abbreviations
            .iter()
            .filter(|g| g.label == label)
            .collect();
        ensure!(groups.len() == 4);
        for group in groups {
            ensure!(group.alignment.is_auto());
            let outside: Vec<_> = doc
                .bonds
                .iter()
                .filter(|b| group.members.contains(&b.a) != group.members.contains(&b.b))
                .collect();
            ensure!(outside.len() == 2);
            ensure!(
                outside
                    .iter()
                    .all(|b| b.a == group.anchor || b.b == group.anchor)
            );
        }
    }
    let parts = scene::primitives(doc);
    ensure!(
        parts
            .iter()
            .filter(|p| matches!(p,
                Primitive::Text { text, .. } if text == "Cl"
            ))
            .count()
            == 4
    );
    ensure!(!parts.iter().any(|p| matches!(p,
        Primitive::Text { text, .. } if text == "CCl2" || text == "CF2"
    )));
    Ok(())
}

#[tokio::test]
async fn returned_internal_groups_retain_both_bonds_and_real_chemistry() -> anyhow::Result<()> {
    let engine = LocalEngine::default();
    for (format, input) in [
        ("cdx", STANDARD.encode(RETURNED)),
        (
            "cdxml",
            exchange::from_cdx(RETURNED).map_err(anyhow::Error::msg)?,
        ),
    ] {
        let result = engine
            .request(Request::import(format, &input))
            .await
            .map_err(anyhow::Error::msg)?;
        ensure!(result.warnings.is_empty(), "{:?}", result.warnings);
        let analysis = result.analysis.context("Missing chemical analysis")?;
        ensure!(analysis.formula == "C56H144Cl8F8N8", "{}", analysis.formula);
        let doc = result.document.context("Missing drawing")?;
        check_groups(&doc)?;
        let mut contract = Request::molecule("abbreviate", doc.clone());
        contract.selected_ids = Some(
            doc.abbreviations
                .iter()
                .flat_map(|g| g.members.iter().copied())
                .collect(),
        );
        let contracted = engine.request(contract).await.map_err(anyhow::Error::msg)?;
        let contracted = contracted
            .document
            .context("Missing group-command drawing")?;
        ensure!(contracted.abbreviations == doc.abbreviations);
        check_groups(&contracted)?;

        let terminal = doc
            .atoms
            .iter()
            .find(|a| {
                a.element == "C"
                    && !doc.abbreviations.iter().any(|g| g.members.contains(&a.id))
                    && doc
                        .bonds
                        .iter()
                        .filter(|b| b.a == a.id || b.b == a.id)
                        .count()
                        == 1
            })
            .context("Missing terminal carbon")?;
        let mut replace = Request::molecule("abbreviate", doc.clone());
        replace.selected_ids = Some(vec![terminal.id]);
        replace.format = Some("replace".into());
        replace.text = Some("OMe".into());
        let replaced = engine.request(replace).await.map_err(anyhow::Error::msg)?;
        let replaced = replaced.document.context("Missing replacement drawing")?;
        ensure!(replaced.atoms.len() == 81 && replaced.bonds.len() == 61);
        ensure!(
            doc.abbreviations
                .iter()
                .all(|g| replaced.abbreviations.contains(g))
        );
        for export_format in ["cdxml", "cdx"] {
            let mut request = Request::molecule("export", doc.clone());
            request.format = Some(export_format.into());
            let output = engine
                .request(request)
                .await
                .map_err(anyhow::Error::msg)?
                .output
                .context("Missing export")?;
            let back = engine
                .request(Request::import(export_format, &output))
                .await
                .map_err(anyhow::Error::msg)?;
            ensure!(back.warnings.is_empty());
            let back_analysis = back.analysis.context("Missing returned analysis")?;
            ensure!(back_analysis.formula == analysis.formula);
            ensure!(back_analysis.inchikey == analysis.inchikey);
            check_groups(&back.document.context("Missing returned drawing")?)?;
        }
        let mut expanded = doc.clone();
        expanded.abbreviations.clear();
        let expanded = engine
            .request(Request::molecule("analyze", expanded))
            .await
            .map_err(anyhow::Error::msg)?;
        let expanded = expanded.analysis.context("Missing expanded analysis")?;
        ensure!(expanded.formula == analysis.formula && expanded.inchikey == analysis.inchikey);
    }
    Ok(())
}

const TWO_CONNECTIONS: &str = r#"<CDXML><page><fragment id="1">
<n id="2" p="0 0"/><n id="3" p="40 0"/>
<n id="4" p="20 -10" NodeType="Fragment" BondOrdering="5 6">
<fragment id="10" ConnectionOrder="11 12">
<n id="11" p="0 0" NodeType="ExternalConnectionPoint"/>
<n id="12" p="40 0" NodeType="ExternalConnectionPoint"/>
<n id="13" p="20 -10"/><n id="14" p="20 -30" Element="17"/>
<n id="15" p="30 -20" Element="17"/>
<b id="16" B="11" E="13"/><b id="17" B="12" E="13"/>
<b id="18" B="13" E="14"/><b id="19" B="13" E="15"/>
</fragment><t><s>CCl2</s></t></n>
<b id="5" B="2" E="4"/><b id="6" B="4" E="3"/>
</fragment></page></CDXML>"#;

#[test]
fn explicit_internal_connections_conserve_atoms_and_reject_ambiguous_definitions()
-> anyhow::Result<()> {
    let flat = flatten_abbreviations(TWO_CONNECTIONS)?;
    ensure!(flat.abbreviations.len() == 1);
    ensure!(flat.abbreviations[0].members.len() == 3);
    let tree = roxmltree::Document::parse(&flat.xml)?;
    ensure!(tree.descendants().filter(|n| n.has_tag_name("n")).count() == 5);
    ensure!(tree.descendants().filter(|n| n.has_tag_name("b")).count() == 4);
    for (from, to) in [
        ("BondOrdering=\"5 6\"", "BondOrdering=\"5 5\""),
        ("BondOrdering=\"5 6\"", "BondOrdering=\"5 99\""),
        ("BondOrdering=\"5 6\"", "BondOrdering=\"5\""),
        ("ConnectionOrder=\"11 12\"", "ConnectionOrder=\"11 11\""),
        ("ConnectionOrder=\"11 12\"", "ConnectionOrder=\"11 99\""),
        ("id=\"17\" B=\"12\" E=\"13\"", "id=\"17\" B=\"12\" E=\"14\""),
        (
            "id=\"17\" B=\"12\" E=\"13\"",
            "id=\"17\" B=\"12\" E=\"13\" Order=\"2\"",
        ),
        ("<b id=\"6\" B=\"4\" E=\"3\"/>", ""),
    ] {
        ensure!(TWO_CONNECTIONS.contains(from));
        ensure!(
            flatten_abbreviations(&TWO_CONNECTIONS.replace(from, to)).is_err(),
            "Accepted {to}"
        );
    }
    // Absent ConnectionOrder uses the source connection-node order.
    ensure!(
        flatten_abbreviations(&TWO_CONNECTIONS.replace(" ConnectionOrder=\"11 12\"", "")).is_ok()
    );
    Ok(())
}
