//! Fast fixture checks and an optimized-build CLI calculation of the metal cluster.
use super::fixtures;
#[cfg(feature = "xtb")]
use super::headless;
#[cfg(feature = "xtb")]
use std::time::Duration;

fn xyz(text: &str) -> (Vec<&str>, Vec<[f64; 3]>) {
    let mut lines = text.lines();
    let count: usize = lines.next().expect("XYZ atom count").parse().unwrap();
    lines.next().expect("XYZ comment");
    let mut symbols = Vec::new();
    let mut positions = Vec::new();
    for line in lines {
        let fields: Vec<_> = line.split_whitespace().collect();
        assert_eq!(fields.len(), 4, "XYZ row: {line}");
        symbols.push(fields[0]);
        positions.push(std::array::from_fn(|axis| {
            let coordinate: f64 = fields[axis + 1].parse().expect("XYZ coordinate");
            assert!(coordinate.is_finite());
            coordinate
        }));
    }
    assert_eq!(symbols.len(), count);
    (symbols, positions)
}

fn distance(positions: &[[f64; 3]], a: usize, b: usize) -> f64 {
    positions[a]
        .iter()
        .zip(positions[b])
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
}

#[test]
fn diruthenium_fixtures_preserve_atomic_state_and_record_the_hydride_isomers() {
    for (suffix, expected_bridges) in [
        ("seed", 4),
        ("asymmetric_seed", 4),
        ("gfn2", 2),
        ("gxtb", 4),
    ] {
        let input = std::fs::read_to_string(
            fixtures().join(format!("diruthenium_tetrahydride_{suffix}.xyz")),
        )
        .expect("cluster fixture");
        let (symbols, positions) = xyz(&input);
        // All atoms are explicit, including the methyl and bridge hydrogens.
        let numbers: Vec<u8> = symbols
            .iter()
            .map(|symbol| match *symbol {
                "H" => 1,
                "C" => 6,
                "Ru" => 44,
                other => panic!("unexpected cluster element {other}"),
            })
            .collect();
        assert_eq!(numbers.len(), 56);
        assert_eq!(numbers.iter().filter(|&&z| z == 44).count(), 2);
        assert_eq!(numbers.iter().filter(|&&z| z == 6).count(), 20);
        assert_eq!(numbers.iter().filter(|&&z| z == 1).count(), 34);
        let electrons = numbers.iter().map(|&z| u32::from(z)).sum::<u32>();
        assert_eq!(electrons, 242);
        assert_eq!(electrons % 2, 0, "neutral singlet electron parity");
        let comment = input.lines().nth(1).unwrap();
        assert!(comment.contains("charge=0 uhf=0"));
        assert_eq!(&numbers[..2], &[44, 44]);
        assert!(numbers[52..].iter().all(|&z| z == 1));
        assert!((2.30..2.55).contains(&distance(&positions, 0, 1)));
        // A 2 Å cutoff lies well between bridging contacts (about 1.7 Å)
        // and the detached Ru···H distances (about 2.8 Å).
        let bridges = (52..56)
            .filter(|&h| (0..2).all(|ru| distance(&positions, ru, h) < 2.0))
            .count();
        assert_eq!(bridges, expected_bridges, "{suffix}");
    }
}

#[test]
#[cfg(feature = "xtb")]
#[cfg_attr(
    debug_assertions,
    ignore = "Run with --release: full metal optimization and Hessian need an optimized engine"
)]
fn diruthenium_cluster_calculates_and_checks_both_local_minima() {
    let source = fixtures().join("diruthenium_tetrahydride_asymmetric_seed.xyz");
    let input = std::fs::read_to_string(&source).expect("cluster seed");
    let (symbols, _) = xyz(&input);

    for (method, expected_energy, expected_bridges) in
        [("gfn2", -66.7269150, 2), ("gxtb", -972.2711498, 4)]
    {
        let dir = tempfile::tempdir().expect("cluster calculation folder");
        let output = headless::run_cli(
            dir.path(),
            &[
                "calculate",
                "--task",
                "opt+freq",
                "--method",
                method,
                "--charge",
                "0",
                "--uhf",
                "0",
                "--threads",
                "1",
                "--convergence",
                "tight",
                source.to_str().unwrap(),
            ],
            b"",
            Duration::from_secs(180),
        );
        assert!(
            output.status.success(),
            "{method}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("cluster JSON");
        let value = &json["value"];
        assert_eq!(value["method"], method);
        assert_eq!(value["atoms"], 56);
        assert_eq!(value["charge"], 0);
        assert_eq!(value["multiplicity"], 1);
        assert_eq!(value["engine"]["name"], "axtb");
        assert_eq!(value["engine"]["rev"], reshiki_xtb_worker::AXTB_REV);
        assert_eq!(value["optimization"]["converged"], true);
        let energy = value["energy_hartree"].as_f64().expect("cluster energy");
        // A computed regression, separate from the crystallographic comparison below.
        assert!(
            (energy - expected_energy).abs() < 1e-4,
            "{method}: {energy}"
        );
        let optimized = value["optimization"]["xyz"]
            .as_str()
            .expect("optimized XYZ");
        let (actual_symbols, positions) = xyz(optimized);
        assert_eq!(actual_symbols, symbols, "{method}: atom identity and order");
        // Broad physical bounds detect atom/units/coordinate regressions. The actual
        // method errors relative to experiment are documented in the fixture notes.
        let ru_ru = distance(&positions, 0, 1);
        assert!((2.30..2.55).contains(&ru_ru), "{method}: Ru–Ru {ru_ru} Å");
        for (metal, ring) in [(0, 2), (1, 27)] {
            for atom in 0..5 {
                let ru_c = distance(&positions, metal, ring + atom);
                assert!((2.10..2.35).contains(&ru_c), "{method}: Ru–C {ru_c} Å");
                let c_c = distance(&positions, ring + atom, ring + (atom + 1) % 5);
                assert!((1.35..1.50).contains(&c_c), "{method}: ring C–C {c_c} Å");
            }
        }
        let mut bridges = 0;
        for hydride in 52..56 {
            let contacts = [
                distance(&positions, 0, hydride),
                distance(&positions, 1, hydride),
            ];
            assert!(contacts.iter().any(|&r| (1.4..1.9).contains(&r)));
            if contacts.iter().all(|&r| (1.6..1.9).contains(&r)) {
                bridges += 1;
            } else {
                assert!(
                    contacts.iter().any(|&r| r > 2.4),
                    "{method}: ambiguous terminal H{}: {contacts:?}",
                    hydride + 1
                );
            }
        }
        assert_eq!(bridges, expected_bridges, "{method}");
        let frequencies = value["frequencies"]["frequencies_cm1"]
            .as_array()
            .expect("cluster Hessian frequencies");
        assert_eq!(frequencies.len(), 3 * 56 - 6);
        // The lowest g-xTB torsions are only about 1–3 cm⁻¹. Allow small
        // numerical shifts of these modes across platforms.
        assert!(frequencies.iter().all(|v| v.as_f64().unwrap() > -10.0));
    }
}
