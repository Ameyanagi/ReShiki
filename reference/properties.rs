//! Every supported element/isotope and representative graphs against RDKit.
use reshiki::chemistry::{AtomFacts, Properties, RDKIT_VERSION, graph::Graph, properties};
use serde::Deserialize;
use std::{path::Path, process::Command};

type TestResult = anyhow::Result<()>;

#[derive(Deserialize)]
struct Reference {
    rdkit_version: String,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    atoms: Vec<AtomFacts>,
    graph: Graph,
    expected: Properties,
}

#[test]
fn agrees_with_rdkit_for_every_isotope_templates_and_hydrogen_representations() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let output = Command::new(python)
        .arg(root.join("reference/properties_reference.py"))
        .env("PYTHONUTF8", "1")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reference: Reference = serde_json::from_slice(&output.stdout)?;
    assert_eq!(
        reference.rdkit_version, RDKIT_VERSION,
        "Regenerate the atomic data when updating RDKit"
    );
    assert!(reference.cases.len() > 3500);
    // The generated RDKit table stays an independent oracle. Correct only the
    // six deliberate CIAAW nominal-average overrides, using RDKit's own
    // unlabelled single-atom cases to determine each mass difference.
    let mass_deltas = [
        (43, 97.),
        (105, 270.),
        (108, 270.),
        (113, 286.),
        (115, 289.),
        (117, 293.),
    ]
    .map(|(number, average)| {
        let name = format!("element {number}, isotope 0");
        let case = reference.cases.iter().find(|case| case.name == name);
        (
            number,
            average - case.expect("RDKit single-atom case").expected.mass,
        )
    });
    for case in reference.cases {
        let mass_correction = mass_deltas
            .iter()
            .map(|&(number, delta)| {
                let count = case
                    .atoms
                    .iter()
                    .filter(|atom| atom.atomic_number == number && atom.isotope == 0)
                    .count();
                count as f64 * delta
            })
            .sum::<f64>();
        for actual in [
            properties(&case.atoms).map_err(anyhow::Error::msg)?,
            properties(&case.graph.atom_facts().map_err(anyhow::Error::msg)?)
                .map_err(anyhow::Error::msg)?,
        ] {
            assert_eq!(actual.formula, case.expected.formula, "{}", case.name);
            assert_eq!(
                actual.unpaired_electrons, case.expected.unpaired_electrons,
                "{}",
                case.name
            );
            // Native wheels may fuse multiply-adds (e.g. the final H contribution).
            // Permit only floating-point noise, never formula/isotope/count changes.
            for (value, expected) in [
                (actual.mass, case.expected.mass + mass_correction),
                (actual.exact_mass, case.expected.exact_mass),
            ] {
                assert!(
                    (value - expected).abs() <= expected.abs().max(1.) * 1e-12,
                    "{}: {value:?} != {expected:?}",
                    case.name
                );
            }
        }
    }
    Ok(())
}

#[test]
fn rejects_invalid_atom_facts_and_excessive_input() {
    let carbon = AtomFacts {
        atomic_number: 6,
        isotope: 0,
        charge: 0,
        hydrogens: 4,
        radical_electrons: 0,
    };
    assert!(
        properties(&[AtomFacts {
            atomic_number: 119,
            ..carbon
        }])
        .is_err()
    );
    assert!(properties(&vec![carbon; 100_001]).is_err());
    for field in [
        "atomic_number",
        "isotope",
        "charge",
        "hydrogens",
        "radical_electrons",
    ] {
        for value in [
            serde_json::json!(1.5),
            serde_json::json!(65536),
            serde_json::Value::Null,
        ] {
            let mut input = serde_json::json!({
                "atomic_number": 6, "isotope": 0, "charge": 0, "hydrogens": 4, "radical_electrons": 0,
            });
            input[field] = value;
            assert!(
                serde_json::from_value::<AtomFacts>(input).is_err(),
                "{field}"
            );
        }
    }
}
