use super::*;
use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize)]
struct Reference {
    name: String,
    formula: String,
    inchikey: String,
    cid: u64,
    source: String,
    phosphate: bool,
    base: String,
}

#[tokio::test]
async fn nucleosides_match_independent_pubchem_records_and_chemical_exports() -> Result<()> {
    let references: Vec<Reference> =
        serde_json::from_str(include_str!("../../assets/nucleosides.json"))?;
    assert_eq!(references.len(), 15);
    for reference in references {
        assert!(
            reference
                .source
                .contains(&format!("/cid/{}/", reference.cid))
        );
        let template = LIBRARY
            .iter()
            .find(|t| t.name == reference.name)
            .context("Missing nucleoside")?;
        let engine = LocalEngine::default();
        let analysis = engine
            .execute(Request::molecule("analyze", template.document.clone()))
            .await
            .map_err(anyhow::Error::msg)?
            .analysis
            .context("Missing analysis")?;
        assert_eq!(analysis.formula, reference.formula, "{}", reference.name);
        assert_eq!(analysis.inchikey, reference.inchikey, "{}", reference.name);
        for format in ["mol", "smiles", "cdxml", "cdx"] {
            let mut request = Request::molecule("export", template.document.clone());
            request.format = Some(format.into());
            let output = engine
                .execute(request)
                .await
                .map_err(anyhow::Error::msg)?
                .output
                .context("Missing export")?;
            let restored = engine
                .execute(Request::import(format, &output))
                .await
                .map_err(anyhow::Error::msg)?
                .analysis
                .context("Missing imported analysis")?;
            assert_eq!(
                restored.inchikey, reference.inchikey,
                "{} {format}",
                reference.name
            );
        }
    }
    Ok(())
}

#[test]
fn nucleoside_bases_are_above_c1_and_phosphate_is_on_o5() -> Result<()> {
    let references: Vec<Reference> =
        serde_json::from_str(include_str!("../../assets/nucleosides.json"))?;
    for reference in references {
        let template = LIBRARY
            .iter()
            .find(|t| t.name == reference.name)
            .context("Missing nucleoside")?;
        let doc = &template.document;
        let c1 = &doc.atoms[0];
        let nitrogen = doc
            .atom(c1.stereo.as_ref().context("Missing C1′ stereo")?.neighbors[2])
            .context("Missing glycosidic N")?;
        assert_eq!(nitrogen.element, "N");
        assert!(nitrogen.position.y < c1.position.y);
        assert_eq!(
            doc.bonds
                .iter()
                .filter(|b| b.a == nitrogen.id || b.b == nitrogen.id)
                .count(),
            3
        );
        assert_eq!(nitrogen.label_h, 0);
        let neighbors: Vec<_> = doc
            .bonds
            .iter()
            .filter_map(|b| {
                let other = if b.a == nitrogen.id {
                    b.b
                } else if b.b == nitrogen.id {
                    b.a
                } else {
                    return None;
                };
                (other != c1.id).then_some(other)
            })
            .collect();
        let degree = |id| doc.bonds.iter().filter(|b| b.a == id || b.b == id).count();
        if matches!(reference.base.as_str(), "Adenine" | "Guanine") {
            // N9 touches fused C4 (which also touches N3); N7 instead touches
            // fused C5, whose two remaining neighbors are both carbon.
            assert!(
                neighbors.iter().any(|id| degree(*id) == 3
                    && doc
                        .bonds
                        .iter()
                        .filter_map(|b| {
                            if b.a == *id {
                                doc.atom(b.b)
                            } else if b.b == *id {
                                doc.atom(b.a)
                            } else {
                                None
                            }
                        })
                        .filter(|a| a.element == "N")
                        .count()
                        == 2),
                "{} must attach at N9",
                reference.name
            );
        } else {
            // N1 touches unsubstituted C6; the N3 alternative has substituted
            // C2 and C4 neighbors, both of degree three.
            assert!(
                neighbors.iter().any(|id| degree(*id) == 2),
                "{} must attach at N1",
                reference.name
            );
        }
        for bond in doc.bonds.iter().filter(|b| !b.projection) {
            assert!(
                (doc.atom(bond.a)
                    .unwrap()
                    .position
                    .distance(doc.atom(bond.b).unwrap().position)
                    - 42.)
                    .abs()
                    < 0.001,
                "{}: unequal base/substituent bond",
                reference.name
            );
        }
        let phosphorus = doc.atoms.iter().find(|a| a.element == "P");
        assert_eq!(phosphorus.is_some(), reference.phosphate);
        if let Some(phosphorus) = phosphorus {
            let c4 = &doc.atoms[3];
            let c5 = doc
                .atom(c4.stereo.as_ref().context("Missing C4′ stereo")?.neighbors[2])
                .context("Missing C5′")?;
            assert!(phosphorus.position.x < c5.position.x);
            let o5 = doc
                .atoms
                .iter()
                .find(|o| {
                    o.element == "O"
                        && doc.bond_between(o.id, c5.id).is_some()
                        && doc.bond_between(o.id, phosphorus.id).is_some()
                })
                .context("Phosphate must connect through O5′")?;
            assert_eq!(o5.label_h, 0);
            assert_eq!(
                doc.bonds
                    .iter()
                    .filter(|b| b.a == phosphorus.id || b.b == phosphorus.id)
                    .count(),
                4
            );
        }
        // Separate, visible atoms must have enough clearance for their labels.
        for (i, atom) in doc.atoms.iter().enumerate() {
            for other in &doc.atoms[i + 1..] {
                if doc.atom_drawn(atom.id) && doc.atom_drawn(other.id) {
                    assert!(
                        atom.position.distance(other.position) > 20.,
                        "{}: {} overlaps {}",
                        reference.name,
                        atom.id,
                        other.id
                    );
                }
            }
        }
    }
    Ok(())
}
