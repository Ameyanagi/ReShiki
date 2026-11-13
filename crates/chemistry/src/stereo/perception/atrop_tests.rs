use super::*;
use crate::{smiles, stereo};

type TestResult = anyhow::Result<()>;

fn options(clean: bool) -> Options {
    Options {
        clean,
        force: true,
        flag_possible: true,
    }
}

// RDKit 2026.09.1 #9190, catch_chirality.cpp: Github #8108.
#[test]
fn atrop_assignment_replaces_or_preserves_wedge_tags() -> TestResult {
    let text = "OC1=C(C)C(N2C3=C(C(C(=O)N)=C2N)C=C(C)C(C)=N3)=C(C)C=C1";
    let coords = "(2.0992,3.5589,;2.2694,2.7517,;1.6554,2.2008,;0.8712,2.4571,;1.8255,1.3935,;1.0637,0.4452,;0.8122,-0.3353,;-0.0126,-0.3353,;-0.264,0.4452,;-1.0482,0.7015,;-1.6621,0.1505,;-1.2182,1.5087,;0.3998,0.9266,;0.3998,1.7516,;-0.425,-1.0498,;-0.0126,-1.7642,;-0.425,-2.4786,;0.8122,-1.7642,;1.2247,-2.4786,;1.2247,-1.0498,;2.6096,1.1372,;2.7797,0.33,;3.2236,1.6882,;3.0535,2.4954,),";
    for text in [
        format!("{text} |wD:4.3|"),
        format!("{text} |{coords}wD:4.3|"),
        "CC1=CC=CC(I)=C1N1C(C)=CC=C1Br |wU:7.7|".into(),
    ] {
        let mut input = smiles::read(&text)?.prepared.state;
        let axis = input
            .metadata
            .bonds
            .iter()
            .position(|b| matches!(b.stereo, 6 | 7))
            .unwrap();
        let correct = input.metadata.bonds[axis].stereo;
        if text.starts_with("CC1=") {
            assert_eq!(correct, 7);
        }
        let wrong = 13 - correct;
        input.metadata.bonds[axis].stereo = 0;
        let cached = perceive(
            &input,
            Options {
                force: false,
                ..options(true)
            },
        )
        .map_err(anyhow::Error::msg)?;
        assert_eq!(cached.metadata.bonds[axis].stereo, 0);
        let detected = perceive(&input, options(false)).map_err(anyhow::Error::msg)?;
        assert_eq!(detected.metadata.bonds[axis].stereo, correct);
        input.metadata.bonds[axis].stereo = wrong;
        for replace in [false, true] {
            let expected = if replace { correct } else { wrong };
            let assigned = perceive(&input, options(replace)).map_err(anyhow::Error::msg)?;
            assert_eq!(assigned.metadata.bonds[axis].stereo, expected);
            let drawn = stereo::from_directions(
                &input.graph,
                &input.metadata,
                &input.directions,
                input.conformer.as_ref().map(|c| c.positions.as_slice()),
                replace,
            )
            .map_err(anyhow::Error::msg)?;
            assert_eq!(drawn.metadata.bonds[axis].stereo, expected);
        }
    }
    Ok(())
}

#[test]
fn atrop_assignment_clears_ambiguous_candidate_tags() -> TestResult {
    let mut input = smiles::read("CC1=CC=CC(I)=C1N1C(C)=CC=C1Br |wU:7.7|")?
        .prepared
        .state;
    let axis = input
        .graph
        .bonds
        .iter()
        .position(|b| b.a == 7 && b.b == 8)
        .unwrap();
    for (id, bond) in input.graph.bonds.iter_mut().enumerate() {
        if id != axis && (bond.a == 7 || bond.b == 7) {
            if bond.b == 7 {
                std::mem::swap(&mut bond.a, &mut bond.b);
            }
            input.directions[id] = Direction::Wedge;
        }
    }
    let preserved = perceive(&input, options(false)).map_err(anyhow::Error::msg)?;
    assert_eq!(preserved.metadata.bonds[axis].stereo, 7);
    let replaced = perceive(&input, options(true)).map_err(anyhow::Error::msg)?;
    assert_eq!(replaced.metadata.bonds[axis].stereo, 0);
    Ok(())
}

// RDKit 2026.09.2 #9682: seven-membered axes are cleared; eight/nine survive.
#[test]
fn atrop_ring_cutoff_matches_sanitization_without_forcing_sssr() -> TestResult {
    for size in [7, 8, 9] {
        let text = format!("Cc1ccc2c(c1){}c1ccc(F)cc1-2", "C".repeat(size - 4));
        for direction in [Direction::Wedge, Direction::Hash] {
            let mut input = smiles::prepare(&text)?.state;
            let axis = input.graph.bonds.len() - 1;
            let marker = axis - 1;
            input.directions[marker] = direction;
            let drawn = stereo::from_directions(
                &input.graph,
                &input.metadata,
                &input.directions,
                None,
                true,
            )
            .map_err(anyhow::Error::msg)?;
            let perceived = drawn.metadata.bonds[axis].stereo;
            assert!(matches!(perceived, 6 | 7));
            input.metadata = drawn.metadata;
            let sanitized =
                crate::sanitize::sanitize(&input.graph, &input.metadata, &input.directions)?;
            let expected = if size < 8 { 0 } else { perceived };
            assert_eq!(sanitized.metadata.bonds[axis].stereo, expected);
            for clean in [false, true] {
                let result = perceive(&input, options(clean)).map_err(anyhow::Error::msg)?;
                assert_eq!(
                    result.metadata.bonds[axis].stereo,
                    if clean { expected } else { perceived }
                );
            }
            for kind in [RingKind::None, RingKind::Fast] {
                input.rings = RingCache {
                    kind,
                    atoms: if kind == RingKind::Fast {
                        rings::fast(&input.graph).map_err(anyhow::Error::msg)?.atoms
                    } else {
                        Vec::new()
                    },
                };
                let mut work = Work(50_000_000);
                let mut context =
                    Context::new(input.clone(), &mut work).map_err(anyhow::Error::msg)?;
                context
                    .detect_atropisomers(true, &mut work)
                    .map_err(anyhow::Error::msg)?;
                assert_eq!(context.state.rings.kind, kind);
                assert_eq!(context.state.metadata.bonds[axis].stereo, perceived);
            }
        }
    }
    Ok(())
}

#[test]
fn atrop_post_assignment_discards_small_ring_candidates() -> TestResult {
    let input = smiles::read("N1(n2c(C)ccc2Br)C(=O)[C@H](C)[C@H](C)C1=O |(-11.1517,1.8306,;-11.1517,3.3708,;-12.4855,4.1411,;-13.8193,3.371,;-12.4855,5.6813,;-9.8177,5.6813,;-9.8177,4.1411,;-8.4839,3.371,;-12.3975,0.9252,;-13.8622,1.4011,;-11.9217,-0.5394,;-12.8269,-1.7852,;-10.3817,-0.5394,;-9.4765,-1.7852,;-9.9059,0.9252,;-8.4413,1.4011,),wU:0.8,10.11,12.13|")?.prepared.state;
    for (bond, meta) in input.graph.bonds.iter().zip(&input.metadata.bonds) {
        if (bond.a, bond.b) == (0, 1) {
            assert!(matches!(meta.stereo, 6 | 7));
        }
        if (bond.a, bond.b) == (14, 0) || (bond.a, bond.b) == (0, 14) {
            assert_eq!(meta.stereo, 0);
        }
    }
    Ok(())
}
