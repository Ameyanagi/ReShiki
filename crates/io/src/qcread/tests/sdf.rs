use crate::qcread::{
    Error, JobKind, read,
    tests::{fixture, open},
};
use reshiki_qcdata::MAX_FRAMES;

#[test]
fn sdf_singlet_centres_contribute_no_unpaired_electrons() -> anyhow::Result<()> {
    let bytes = fixture("reshiki/ions_3d.sdf");
    let text = std::str::from_utf8(&bytes)?;
    let water = text
        .split_once("$$$$")
        .ok_or_else(|| anyhow::anyhow!("water record"))?
        .0;
    // Synthetic CH2 singlet made in memory from Water: the fixture has
    // "  3  2  0  0  0  0  0  0  0  0999 V2000", two explicit H rows,
    // and "  1  2  1  0  0  0  0" / "  1  3  1  0  0  0  0" bonds.
    let v2000 = water
        .replacen(" O ", " C ", 1)
        .replace("M  END", "M  RAD  1   1   1\nM  END");
    // Synthetic CHD singlet from Deuteroxide: "M  V30 COUNTS 2 1 0 0 0",
    // "M  V30 1 O 0.0 0.0 0.0 0 CHG=-1", and
    // "M  V30 2 H 0.0 0.0 0.9600000008 0 MASS=2" supply its native rows.
    let (_, deuteroxide) = text
        .split_once("Deuteroxide")
        .ok_or_else(|| anyhow::anyhow!("OD record"))?;
    let v3000 = format!("CHD singlet{deuteroxide}")
        .replace("COUNTS 2 1", "COUNTS 3 2")
        .replace("O 0.0 0.0 0.0 0 CHG=-1", "C 0.0 0.0 0.0 0 RAD=1")
        .replace(
            "M  V30 END ATOM",
            "M  V30 3 H 0.9294 0.0 -0.2404 0\nM  V30 END ATOM",
        )
        .replace("M  V30 END BOND", "M  V30 2 1 1 3\nM  V30 END BOND");
    for record in [v2000, v3000] {
        let file = read("singlet.mol", record.as_bytes())?;
        assert_eq!(file.jobs[0].steps[0].frame.numbers, [6, 1, 1]);
        assert_eq!(file.jobs[0].steps[0].frame.multiplicity, 1);
        assert!(file.warnings.is_empty(), "{:?}", file.warnings);
    }
    Ok(())
}

#[test]
fn sdf_warns_when_several_radical_centres_assume_high_spin() -> anyhow::Result<()> {
    let bytes = fixture("reshiki/ions_3d.sdf");
    let text = std::str::from_utf8(&bytes)?;
    // Synthetic OD + H diradical from "M  V30 1 O 0.0 0.0 0.0 0 CHG=-1":
    // neutralize OD with RAD=2 and add an unbonded RAD=2 hydrogen in memory.
    let (_, record) = text
        .split_once("Deuteroxide")
        .ok_or_else(|| anyhow::anyhow!("OD record"))?;
    let record = format!("Two radical centres{record}")
        .replace("COUNTS 2 1", "COUNTS 3 1")
        .replace("CHG=-1", "RAD=2")
        .replace(
            "M  V30 END ATOM",
            "M  V30 3 H 0.0 0.0 3.0 0 RAD=2\nM  V30 END ATOM",
        );
    let file = read("diradical.mol", record.as_bytes())?;
    assert_eq!(file.jobs[0].steps[0].frame.multiplicity, 3);
    assert_eq!(
        file.warnings,
        ["Record 1: several radical centres; assumed the high-spin state"]
    );
    Ok(())
}

#[test]
fn sdf_limits_records_before_parsing_and_counts_the_skipped_tail() -> anyhow::Result<()> {
    let bytes = fixture("reshiki/ions_3d.sdf");
    let text = std::str::from_utf8(&bytes)?;
    // Repeat the fixture's "Water" record in memory, whose count line is
    // "  3  2  0  0  0  0  0  0  0  0999 V2000" and atom rows are O,H,H.
    // The malformed tail has three delimited records and one final record
    // without a delimiter; none may reach the MOL parser after the cap.
    let (water, _) = text
        .split_once("$$$$")
        .ok_or_else(|| anyhow::anyhow!("water record"))?;
    let text = format!(
        "{}{}invalid final record\n",
        format!("{water}$$$$\n").repeat(MAX_FRAMES),
        "invalid skipped record\n$$$$\n".repeat(3)
    );
    let file = read("many.sdf", text.as_bytes())?;
    assert_eq!(file.jobs[0].steps.len(), MAX_FRAMES);
    assert!(
        file.jobs[0]
            .steps
            .iter()
            .all(|s| s.frame.numbers == [8, 1, 1])
    );
    assert_eq!(
        file.warnings,
        [format!(
            "Read the first {MAX_FRAMES} of {} records",
            MAX_FRAMES + 4
        )]
    );
    Ok(())
}

#[test]
fn xtb_mol_keeps_all_explicit_hydrogens_in_file_order() {
    let file = open("cclib/xtb661_dvb_xtbtopo.mol");
    let [job] = &file.jobs[..] else {
        panic!("expected one geometry job")
    };
    assert_eq!(job.index, 0);
    assert_eq!(job.kind, JobKind::Geometry);
    assert!(job.completion.is_none() && job.result.is_none());
    assert_eq!(job.steps.len(), 1);
    let step = &job.steps[0];
    //  20 20  0     0  0            999 V2000
    assert_eq!(step.frame.numbers.len(), 20);
    assert_eq!(step.frame.numbers.iter().filter(|&&z| z == 1).count(), 10);
    assert_eq!(step.frame.charge, 0);
    assert_eq!(step.frame.multiplicity, 1);
    //     2.3054   -0.8504   -0.0000 H   0  0  0  0  0  0  0  0  0  0  0  0
    assert_eq!(step.frame.numbers[6], 1);
    assert_eq!(step.frame.positions_angstrom[6], [2.3054, -0.8504, 0.]);
    //  xtb: 6.6.1 (8d0f1dd)
    assert_eq!(file.program.as_deref(), Some("xtb"));
    assert_eq!(file.program_version.as_deref(), Some("6.6.1 (8d0f1dd)"));
    assert_eq!(step.label.as_deref(), Some("xtb: 6.6.1 (8d0f1dd)"));
    assert!(step.energy_hartree.is_none());
    // Divinylbenzene is planar (all z = 0) but the header says 3D: no warning.
    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
}

#[test]
fn sdf_keeps_v2000_and_v3000_charges_isotopes_and_positions() {
    let file = open("reshiki/ions_3d.sdf");
    let steps = &file.jobs[0].steps;
    assert_eq!(steps.len(), 3);
    assert!(file.warnings.is_empty() && file.program.is_none());
    // 3  2 ... V2000, then 5  4 ... V2000, then M  V30 COUNTS 2 1 0 0 0
    assert_eq!(
        steps
            .iter()
            .map(|s| s.frame.numbers.len())
            .collect::<Vec<_>>(),
        [3, 5, 2]
    );
    // M  CHG  1   1   1
    // M  V30 1 O 0.0 0.0 0.0 0 CHG=-1
    assert_eq!(
        steps.iter().map(|s| s.frame.charge).collect::<Vec<_>>(),
        [0, 1, -1]
    );
    assert!(
        steps
            .iter()
            .all(|s| s.frame.multiplicity == 1 && s.energy_hartree.is_none())
    );
    // M  V30 2 H 0.0 0.0 0.9600000008 0 MASS=2
    assert_eq!(steps[2].frame.numbers, [8, 1]);
    assert_eq!(steps[2].frame.isotopes, [0, 2]);
    for (actual, expected) in [
        //     0.9294    0.0000   -0.2404 H
        (steps[0].frame.positions_angstrom[2], [0.9294, 0., -0.2404]),
        //     0.5831   -0.5831   -0.5831 H
        (
            steps[1].frame.positions_angstrom[2],
            [0.5831, -0.5831, -0.5831],
        ),
        (steps[2].frame.positions_angstrom[1], [0., 0., 0.9600000008]),
    ] {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-9);
        }
    }
    assert_eq!(steps[2].label.as_deref(), Some("Deuteroxide"));
}

#[test]
fn sdf_rejects_hydrogens_without_coordinates_and_query_atoms() {
    // Retain only the water oxygen from ions_3d.sdf, removing both hydrogen rows and bonds.
    let bytes = fixture("reshiki/ions_3d.sdf");
    let text = std::str::from_utf8(&bytes).unwrap();
    let oxygen = text.lines().take(5).collect::<Vec<_>>().join("\n");
    let oxygen = format!("{}\nM  END\n", oxygen.replacen("  3  2", "  1  0", 1));
    assert_eq!(
        read("implicit.mol", oxygen.as_bytes()),
        Err(Error::Unsupported(
            "Record 1 has implicit hydrogens; a 3D calculation needs every atom with coordinates"
                .into()
        ))
    );
    let query = oxygen.replacen(" O ", " A ", 1);
    assert_eq!(
        read("query.mol", query.as_bytes()),
        Err(Error::Unsupported(
            "Record 1 contains unsupported MOL chemistry: query or unknown atom symbol".into()
        ))
    );
}

#[test]
fn sdf_flags_only_flat_2d_records_and_ignores_empty_trailing_records() {
    // Mark the fixture's records as 2D and append empty records after the final delimiter:
    // water still has z = 0.96, so it is read as 3D without a warning.
    let bytes = fixture("reshiki/ions_3d.sdf");
    let text = std::str::from_utf8(&bytes).unwrap();
    let text = format!("{}$$$$\n\n", text.replace("3D", "2D"));
    let file = read("marked.sdf", text.as_bytes()).unwrap();
    assert_eq!(file.jobs[0].steps.len(), 3);
    assert_eq!(file.jobs[0].steps[0].frame.positions_angstrom[1][2], 0.96);
    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
    // The planar xtb record marked 2D is flagged.
    let bytes = fixture("cclib/xtb661_dvb_xtbtopo.mol");
    let text = std::str::from_utf8(&bytes).unwrap().replacen("3D", "2D", 1);
    let file = read("flat.mol", text.as_bytes()).unwrap();
    assert_eq!(file.warnings, ["Record 1 is 2D (all z = 0)"]);
}

#[test]
fn sdf_uses_radical_count_when_it_agrees_with_electron_parity() {
    // Make a neutral doublet radical from the fixture's OD- record.
    let bytes = fixture("reshiki/ions_3d.sdf");
    let text = std::str::from_utf8(&bytes).unwrap();
    let (_, record) = text.split_once("Deuteroxide").unwrap();
    let radical = format!("OD radical{}", record.replace("CHG=-1", "RAD=2"));
    let file = read("radical.mol", radical.as_bytes()).unwrap();
    let frame = &file.jobs[0].steps[0].frame;
    assert_eq!(frame.numbers, [8, 1]);
    assert_eq!((frame.charge, frame.multiplicity), (0, 2));
    assert!(file.warnings.is_empty());

    // Add a Co atom with two asserted radical electrons. The three radicals
    // disagree with the resulting even electron count.
    let inconsistent = radical.replace("COUNTS 2 1", "COUNTS 3 1").replace(
        "M  V30 END ATOM",
        "M  V30 3 Co 0.0 0.0 3.0 0 RAD=3\nM  V30 END ATOM",
    );
    let file = read("radical.mol", inconsistent.as_bytes()).unwrap();
    assert_eq!(file.jobs[0].steps[0].frame.multiplicity, 1);
    assert_eq!(
        file.warnings,
        ["Record 1: radical count disagrees with electron parity; assumed the lowest spin state"]
    );
}
