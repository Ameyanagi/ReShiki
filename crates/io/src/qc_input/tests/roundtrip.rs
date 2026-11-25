use super::*;
use reshiki_qcdata::Trajectory;

fn xyz_roundtrip(text: &str, frame: &Frame) {
    let (document, _) = crate::crystal::read("xyz", text).unwrap();
    assert_eq!(document.atoms.len(), frame.numbers.len());
    assert_eq!(document.models.len(), 1);
    let model = &document.models[0];
    for ((atom, &number), expected) in document
        .atoms
        .iter()
        .zip(&frame.numbers)
        .zip(&frame.positions_angstrom)
    {
        assert_eq!(
            atom.element,
            crate::chemistry::ELEMENTS[usize::from(number)].symbol
        );
        let placed = model
            .atoms
            .iter()
            .find(|placed| placed.id == atom.id)
            .unwrap();
        for (coordinate, expected) in placed.xyz.into_iter().zip(expected) {
            assert!((f64::from(coordinate) - expected).abs() < 1e-6);
        }
    }
}

#[test]
fn xyz_roundtrips_elements_order_and_cartesian_positions() {
    let mut frame = water();
    frame.positions_angstrom[1][2] = 0.125;
    frame.positions_angstrom[2][2] = -0.25;
    let output = write(
        &settings(Program::Xyz, Job::SinglePoint),
        &geometry(&frame),
        "water",
        GENERATOR,
    )
    .unwrap();
    xyz_roundtrip(&output.main.text, &frame);
}

#[test]
fn trajectory_writes_all_steps_and_the_reader_recovers_the_first() {
    let frame = water();
    let product = shifted_water(0.05);
    let mut path = Trajectory {
        numbers: frame.numbers.clone(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        positions_angstrom: [
            &frame.positions_angstrom[..],
            &product.positions_angstrom[..],
        ]
        .into_iter()
        .flatten()
        .flatten()
        .copied()
        .collect(),
        energies_hartree: vec![-5.1, -5.2],
        labels: vec!["reactant".into(), "product".into()],
        minimizer_energies_hartree: vec![],
        cluster_sizes: vec![],
    };
    path.validate().unwrap();
    let text = write_xyz_trajectory(&path, "water").unwrap();
    assert_eq!(text.lines().count(), 10);
    assert_eq!(
        text.lines().nth(1).unwrap(),
        "reactant  E = -5.1000000000 Eh"
    );
    assert_eq!(
        text.lines().nth(6).unwrap(),
        "product  E = -5.2000000000 Eh"
    );
    xyz_roundtrip(&text, &frame);
    xyz_roundtrip(
        &text.lines().skip(5).collect::<Vec<_>>().join("\n"),
        &product,
    );
    path.labels.clear();
    path.energies_hartree.clear();
    let text = write_xyz_trajectory(&path, "water").unwrap();
    assert_eq!(text.lines().nth(1).unwrap(), "water step 1");
    assert_eq!(text.lines().nth(6).unwrap(), "water step 2");
    path.positions_angstrom.clear();
    path.labels = vec!["unsaved".into()];
    assert_eq!(write_xyz_trajectory(&path, "water").unwrap(), "");
    path.positions_angstrom = vec![0.0; 9];
    path.numbers[0] = 119;
    assert!(write_xyz_trajectory(&path, "water").is_err());
}

#[test]
fn sdf_v3000_preserves_3d_atoms_charges_isotopes_and_bonds() {
    let frame = Frame {
        numbers: vec![8, 6, 1, 1, 1],
        positions_angstrom: vec![
            [0.0, 0.0, 0.42],
            [1.43, 0.0, -0.3],
            [1.8, 0.9, 0.1],
            [1.8, -0.9, 0.1],
            [1.8, 0.0, -1.2],
        ],
        isotopes: vec![18, 13, 0, 0, 0],
        masses_amu: vec![],
        charge: -1,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let bonds = [(0, 1, 1), (1, 2, 1), (1, 3, 1), (1, 4, 1)];
    let charges = [-1, 0, 0, 0, 0];
    let properties = [
        ("energy".into(), "-10.5 Eh".into()),
        (
            "note\r\nname\rwith\nbreaks".into(),
            "line one\r\nline two\rline three\nline four".into(),
        ),
    ];
    let mut helium = water();
    helium.numbers = vec![2];
    helium.positions_angstrom = vec![[0.0, 0.0, 0.1]];
    let text = write_sdf(&[
        SdfEntry {
            frame: &frame,
            title: "methoxide",
            bonds: &bonds,
            formal_charges: &charges,
            properties: &properties,
        },
        SdfEntry {
            frame: &helium,
            title: "helium",
            bonds: &[],
            formal_charges: &[],
            properties: &[],
        },
    ])
    .unwrap();
    assert!(text.ends_with("$$$$\n"));
    assert_eq!(text.matches("$$$$").count(), 2);
    assert!(text.contains("> <energy>\n-10.5 Eh\n\n"));
    assert!(text.contains("> <note name with breaks>\nline one line two line three line four\n\n"));
    let records: Vec<_> = text
        .split("$$$$")
        .filter(|record| !record.trim().is_empty())
        .map(|record| record.trim_start_matches('\n'))
        .collect();
    for (record, expected) in records.iter().zip([&frame, &helium]) {
        let header = record.lines().nth(1).unwrap();
        assert_eq!(&header[20..22], "3D");
        assert!(record.contains("999 V3000\n"));
        let imported = crate::chemistry::molfile::read(record).unwrap();
        assert!(imported.annotations.is_3d);
        assert_eq!(
            imported.molecule.state.graph.atoms.len(),
            expected.numbers.len()
        );
        for ((atom, &number), position) in imported
            .molecule
            .state
            .graph
            .atoms
            .iter()
            .zip(&expected.numbers)
            .zip(&imported.molecule.positions)
        {
            assert_eq!(atom.atomic_number, number);
            assert!(position.z.is_finite());
        }
        for (position, expected) in imported
            .molecule
            .positions
            .iter()
            .zip(&expected.positions_angstrom)
        {
            for (actual, expected) in [position.x, position.y, position.z]
                .into_iter()
                .zip(expected)
            {
                assert!((actual - expected).abs() <= 1e-6);
            }
        }
    }
    let imported = crate::chemistry::molfile::read(records[0]).unwrap();
    let graph = &imported.molecule.state.graph;
    assert_eq!(
        graph
            .atoms
            .iter()
            .map(|atom| i32::from(atom.charge))
            .collect::<Vec<_>>(),
        charges
    );
    assert_eq!(
        graph
            .atoms
            .iter()
            .map(|atom| atom.isotope)
            .collect::<Vec<_>>(),
        frame.isotopes
    );
    assert_eq!(
        graph
            .bonds
            .iter()
            .map(|bond| (bond.a, bond.b, bond.order))
            .collect::<Vec<_>>(),
        bonds
    );
    assert!(!records[1].contains("BEGIN BOND"));
    assert!(
        crate::chemistry::molfile::read(records[1])
            .unwrap()
            .molecule
            .state
            .graph
            .bonds
            .is_empty()
    );
    assert_eq!(write_sdf(&[]).unwrap(), "");
}

#[test]
fn sdf_rejects_invalid_bonds_and_formal_charges() {
    let frame = water();
    for (bonds, formal_charges, expected) in [
        (
            &[(3, 0, 1)][..],
            &[][..],
            "A bond refers to an atom outside the structure.",
        ),
        (
            &[(0, usize::MAX, 1)][..],
            &[][..],
            "A bond refers to an atom outside the structure.",
        ),
        (
            &[(1, 1, 1)][..],
            &[][..],
            "Bonds must join two different atoms, once.",
        ),
        (
            &[(0, 1, 1), (1, 0, 2)][..],
            &[][..],
            "Bonds must join two different atoms, once.",
        ),
        (&[(0, 1, 0)][..], &[][..], "Bond orders must be 1 to 4."),
        (&[(0, 1, 5)][..], &[][..], "Bond orders must be 1 to 4."),
        (
            &[][..],
            &[0, 0, 0, 0][..],
            "There are more formal charges than atoms.",
        ),
        (&[][..], &[16][..], "Formal charges must be from -15 to 15."),
        (
            &[][..],
            &[-16][..],
            "Formal charges must be from -15 to 15.",
        ),
    ] {
        let error = write_sdf(&[SdfEntry {
            frame: &frame,
            title: "water",
            bonds,
            formal_charges,
            properties: &[],
        }])
        .unwrap_err();
        assert!(matches!(error, Error::Invalid(message) if message == expected));
    }
}

#[test]
fn sdf_writes_formal_charges_at_the_limits() {
    let text = write_sdf(&[SdfEntry {
        frame: &water(),
        title: "water",
        bonds: &[],
        formal_charges: &[-15, 15],
        properties: &[],
    }])
    .unwrap();
    assert!(text.contains(" CHG=-15\n"));
    assert!(text.contains(" CHG=15\n"));
}

#[test]
fn sdf_indents_property_values_that_begin_with_the_record_separator() {
    let text = write_sdf(&[SdfEntry {
        frame: &water(),
        title: "water",
        bonds: &[],
        formal_charges: &[],
        properties: &[
            ("exact".into(), "$$$$".into()),
            ("prefix".into(), "$$$$rest".into()),
            ("multiline".into(), "$$$$\r\nrest".into()),
            ("middle".into(), "amount $$$$".into()),
        ],
    }])
    .unwrap();
    assert!(text.contains("> <exact>\n $$$$\n\n"));
    assert!(text.contains("> <prefix>\n $$$$rest\n\n"));
    assert!(text.contains("> <multiline>\n $$$$ rest\n\n"));
    assert!(text.contains("> <middle>\namount $$$$\n\n"));
    assert_eq!(text.lines().filter(|line| *line == "$$$$").count(), 1);
}

#[test]
fn titles_keywords_and_negative_zero_are_normalized() {
    let mut frame = water();
    frame.positions_angstrom[0] = [-0.0, -0.000_000_001, 0.0];
    for program in Program::ALL {
        let mut settings = settings(program, Job::SinglePoint);
        settings.title = " \n @ !\r\n% #\r$ * water\nsecond line \n ".into();
        settings.extra = if program == Program::Gamess {
            "  $SCF DIRSCF=.T. $END  ".into()
        } else {
            "  TightSCF  ".into()
        };
        let output = write(&settings, &geometry(&frame), "sample", GENERATOR).unwrap();
        let text = &output.main.text;
        let title = match program {
            Program::Orca => "# water second line",
            _ => "water second line",
        };
        assert!(text.lines().any(|line| line == title), "{text}");
        assert!(!text.contains("-0.00000000"), "{text}");
        if program == Program::Gamess {
            assert!(text.contains("$SCF DIRSCF=.T. $END\n $DATA"));
        } else if program != Program::Xyz {
            assert!(
                text.lines()
                    .any(|line| (line.starts_with('#') || line.starts_with('!'))
                        && line.ends_with("TightSCF"))
            );
        }
        settings.title = " @\n% * ".into();
        assert!(
            write(&settings, &geometry(&frame), "sample", GENERATOR)
                .unwrap()
                .main
                .text
                .contains("molecule")
        );
    }
}
