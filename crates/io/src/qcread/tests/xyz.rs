use crate::qcread::{
    Error, JobKind, read,
    tests::{fixture, open},
};
use reshiki_qcdata::{MAX_FRAME_ATOMS, MAX_FRAMES};

#[test]
fn irc_xyz_round_trips_traced_frames_and_hartree_energies() -> anyhow::Result<()> {
    use reshiki_qcdata::{IrcEndpoint, IrcPath, IrcRelaxedEnd, KCAL_PER_HARTREE, Trajectory};

    let energies = [-5.00123456789, -4.99, -5.0201];
    let positions = [-0.25, 0., 0.5].into_iter().flat_map(|x| [x, 0., 0.]);
    let path = IrcPath::new(
        Trajectory {
            numbers: vec![2],
            isotopes: vec![],
            masses_amu: vec![4.],
            charge: 0,
            multiplicity: 1,
            positions_angstrom: positions.collect(),
            energies_hartree: energies.to_vec(),
            labels: vec![],
            minimizer_energies_hartree: vec![],
            cluster_sizes: vec![],
            rmsd_to_best_angstrom: vec![],
        },
        1,
        [IrcEndpoint::Reactant, IrcEndpoint::Product],
        [false; 2],
        [-0.8, 1.2].map(|x| IrcRelaxedEnd {
            energy_hartree: -5.04,
            positions_angstrom: vec![x, 0., 0.],
        }),
    )?;
    let xyz = crate::qc_input::write_irc_xyz(&path, -5., KCAL_PER_HARTREE, "kcal/mol")?;
    let file = read("reaction-irc.xyz", xyz.as_bytes())?;
    let steps = &file.jobs[0].steps;
    assert_eq!(steps.len(), path.steps());
    for (i, (step, energy)) in steps.iter().zip(energies).enumerate() {
        assert_eq!(step.energy_hartree, Some(energy));
        assert_eq!(
            step.frame.positions_angstrom,
            path.frame(i).unwrap().positions_angstrom
        );
        let s = path.arc_lengths_sqrt_amu_bohr[i];
        let delta = (energy + 5.) * KCAL_PER_HARTREE;
        assert_eq!(
            step.label.as_deref(),
            Some(
                format!("E = {energy} Eh · s = {s:.6} amu½·bohr · ΔE = {delta:.6} kcal/mol")
                    .as_str()
            )
        );
    }
    Ok(())
}

#[test]
fn extended_xyz_accepts_atomic_number_property_columns() -> anyhow::Result<()> {
    let bytes = fixture("cclib/uracil_two.xyz");
    let text = std::str::from_utf8(&bytes)?;
    // Synthetic extxyz made in memory from the first two uracil atom rows:
    // "N   5.435   3.248   -0.916" and "C   4.091   3.266   -0.635".
    // Place the atomic number after coordinates and an unrelated id column.
    let rows = text
        .lines()
        .skip(2)
        .take(2)
        .zip([7, 6])
        .map(|(row, z)| {
            let (_, xyz) = row
                .split_once(char::is_whitespace)
                .ok_or_else(|| anyhow::anyhow!("atom row"))?;
            Ok(format!("{xyz} 99 {z}\n"))
        })
        .collect::<anyhow::Result<String>>()?;
    let text = format!("2\nProperties=pos:R:3:id:I:1:Z:I:1 charge=0 mult=2\n{rows}");
    let file = read("numbers.extxyz", text.as_bytes())?;
    let frame = &file.jobs[0].steps[0].frame;
    assert_eq!(frame.numbers, [7, 6]);
    assert_eq!(frame.positions_angstrom[0], [5.435, 3.248, -0.916]);
    assert_eq!(frame.positions_angstrom[1], [4.091, 3.266, -0.635]);
    assert!(file.warnings.is_empty());
    Ok(())
}

#[test]
fn extended_xyz_prefers_species_when_atomic_numbers_are_also_present() -> anyhow::Result<()> {
    let bytes = fixture("cclib/uracil_two.xyz");
    let text = std::str::from_utf8(&bytes)?;
    // Synthetic one-atom frame from "N   5.435   3.248   -0.916".
    // Conflicting Z=9 makes choosing the species column observable in both orders.
    let row = text
        .lines()
        .nth(2)
        .ok_or_else(|| anyhow::anyhow!("nitrogen row"))?;
    let (_, xyz) = row
        .split_once(char::is_whitespace)
        .ok_or_else(|| anyhow::anyhow!("coordinates"))?;
    for (properties, row) in [
        ("species:S:1:pos:R:3:Z:I:1", format!("{row} 9")),
        ("Z:I:1:pos:R:3:species:S:1", format!("9 {xyz} N")),
    ] {
        let text = format!("1\nProperties={properties} charge=0 mult=2\n{row}\n");
        let file = read("species.extxyz", text.as_bytes())?;
        assert_eq!(file.jobs[0].steps[0].frame.numbers, [7]);
    }
    Ok(())
}

#[test]
fn extended_xyz_atomic_number_columns_require_integers() -> anyhow::Result<()> {
    let bytes = fixture("cclib/uracil_two.xyz");
    let text = std::str::from_utf8(&bytes)?;
    // Synthetic one-atom frame from "N   5.435   3.248   -0.916".
    // Leave the symbol in place while declaring the column as integer Z.
    let row = text
        .lines()
        .nth(2)
        .ok_or_else(|| anyhow::anyhow!("nitrogen row"))?;
    let text = format!("1\nProperties=Z:I:1:pos:R:3 charge=0 mult=2\n{row}\n");
    assert_eq!(
        read("invalid.extxyz", text.as_bytes()),
        Err(Error::Invalid(
            "Line 3: Expected an atomic number in the Z column".into()
        ))
    );
    Ok(())
}

#[test]
fn uracil_reads_two_geometries_without_guessing_energy() {
    let file = open("cclib/uracil_two.xyz");
    let [job] = &file.jobs[..] else {
        panic!("expected one geometry job")
    };
    assert_eq!(job.index, 0);
    assert_eq!(job.kind, JobKind::Geometry);
    assert!(job.result.is_none() && job.completion.is_none() && file.program.is_none());
    assert_eq!(job.steps.len(), 2);
    assert_eq!(
        job.steps
            .iter()
            .map(|s| s.frame.numbers.len())
            .collect::<Vec<_>>(),
        [12, 12]
    );
    // N   5.435   3.248   -0.916
    assert_eq!(job.steps[0].frame.numbers[0], 7);
    assert_eq!(
        job.steps[0].frame.positions_angstrom[0],
        [5.435, 3.248, -0.916]
    );
    // N          5.42432        3.24732       -0.94320
    assert_eq!(
        job.steps[1].frame.positions_angstrom[0],
        [5.42432, 3.24732, -0.94320]
    );
    // Energy:     -97.0646597
    assert_eq!(
        job.steps[1].label.as_deref(),
        Some("Energy:     -97.0646597")
    );
    assert!(
        job.steps.iter().all(|s| s.energy_hartree.is_none()
            && s.frame.charge == 0
            && s.frame.multiplicity == 1)
    );
    assert_eq!(
        file.warnings,
        ["Charge and spin are not recorded in XYZ; assumed neutral and the lowest spin state"]
    );
}

#[test]
fn orca_trajectory_reads_hartree_at_every_frame() {
    let file = open("cclib/example_MEP_trj.xyz");
    let steps = &file.jobs[0].steps;
    assert_eq!(file.program.as_deref(), Some("ORCA"));
    assert_eq!(steps.len(), 10);
    assert!(
        steps
            .iter()
            .all(|s| s.frame.numbers.len() == 14 && s.energy_hartree.is_some())
    );
    // Coordinates from ORCA-job example_MEP E -344.387777144914
    assert_eq!(steps[0].energy_hartree, Some(-344.387777144914));
    // H       2.085716     -0.996734      0.422314
    assert_eq!(
        steps[0].frame.positions_angstrom[0],
        [2.085716, -0.996734, 0.422314]
    );
    // Coordinates from ORCA-job example_MEP E -344.378604518079
    assert_eq!(steps[9].energy_hartree, Some(-344.378604518079));
}

#[test]
fn xtb_trajectory_reads_hartree_and_printed_version() {
    let file = open("cclib/xtb661_dvb_xtbopt.xyz");
    let steps = &file.jobs[0].steps;
    assert_eq!(file.program.as_deref(), Some("xtb"));
    assert_eq!(file.program_version.as_deref(), Some("6.6.1 (8d0f1dd)"));
    assert_eq!(steps.len(), 6);
    assert!(
        steps
            .iter()
            .all(|s| s.frame.numbers.len() == 20 && s.energy_hartree.is_some())
    );
    // energy: -26.425939359008 gnorm: 0.088050967149 xtb: 6.6.1 (8d0f1dd)
    assert_eq!(steps[0].energy_hartree, Some(-26.425939359008));
    // C            0.27867948000000       -1.36683162000000        0.00000000000000
    assert_eq!(
        steps[0].frame.positions_angstrom[0],
        [0.27867948, -1.36683162, 0.]
    );
    // energy: -26.438242468165 gnorm: 0.000596915040 xtb: 6.6.1 (8d0f1dd)
    assert_eq!(steps[5].energy_hartree, Some(-26.438242468165));
}

#[test]
fn extended_xyz_reorders_columns_and_accepts_different_atom_counts() {
    // Replace the second uracil frame with four of its coordinate rows, reformatting them
    // as extended XYZ with new elements to exercise numeric species and isotopes.
    let bytes = fixture("cclib/uracil_two.xyz");
    let text = std::str::from_utf8(&bytes).unwrap();
    let (first, second) = text.split_at(text.lines().take(14).map(|s| s.len() + 1).sum());
    let rows = second
        .lines()
        .skip(2)
        .zip(["cl", "D", "T", "2"])
        .map(|(row, name)| {
            let (_, xyz) = row.split_once(char::is_whitespace).unwrap();
            format!("{xyz} {name} 0\n")
        })
        .collect::<String>();
    let prefix = "Properties=pos:R:3:species:S:1:id:I:1 charge=-1 mult=1 energy=-27.211386245988 Lattice=\"3 0 0 0 3 0 0 0 3\"";
    // Cl + D + T + He has 21 electrons before adding one for the charge.
    let xyz = format!("{first}4\n{prefix} pbc=\"T F F\"\n{rows}\n\n");
    let file = read("mixed.extxyz", xyz.as_bytes()).unwrap();
    let steps = &file.jobs[0].steps;
    assert_eq!(steps[0].frame.numbers.len(), 12);
    assert_eq!(steps[1].frame.numbers, [17, 1, 1, 2]);
    assert_eq!(steps[1].frame.isotopes, [0, 2, 3, 0]);
    assert_eq!(steps[1].frame.charge, -1);
    assert_eq!(steps[1].frame.multiplicity, 1);
    assert_eq!(steps[1].energy_hartree, Some(-1.));
    // C          4.08846        3.26356       -0.64127
    assert_eq!(
        steps[1].frame.positions_angstrom[1],
        [4.08846, 3.26356, -0.64127]
    );
    assert_eq!(
        file.warnings[1],
        "The cell is ignored; atoms are read as a molecule"
    );
    let molecular = xyz.replace("T F F", "F F F");
    assert_eq!(
        read("mixed.extxyz", molecular.as_bytes())
            .unwrap()
            .warnings
            .len(),
        1
    );
}

#[test]
fn incomplete_xyz_reports_the_frame_and_missing_line() {
    // Truncate the uracil fixture before the last atom in frame 2.
    let bytes = fixture("cclib/uracil_two.xyz");
    let text = std::str::from_utf8(&bytes).unwrap();
    let truncated = text.lines().take(27).collect::<Vec<_>>().join("\n");
    assert_eq!(
        read("truncated.xyz", truncated.as_bytes()),
        Err(Error::Invalid(
            "Line 28: The file ends before frame 2's 12 atoms".into()
        ))
    );
    // Change the first element in the same fixture to an unknown symbol.
    let unknown = text.replacen("N   5.435", "Xx   5.435", 1);
    assert_eq!(
        read("unknown.xyz", unknown.as_bytes()),
        Err(Error::Invalid("Line 3: 'Xx' is not an element".into()))
    );
}

#[test]
fn xyz_limits_kept_frames_and_atoms_per_frame() {
    // Repeat the two-frame uracil fixture, and separately increase its first declared count.
    let bytes = fixture("cclib/uracil_two.xyz");
    let total = MAX_FRAMES + 4;
    let file = read("many.xyz", &bytes.repeat(total / 2)).unwrap();
    assert_eq!(file.jobs[0].steps.len(), MAX_FRAMES);
    assert_eq!(
        file.warnings.last().unwrap(),
        &format!("Read the first {MAX_FRAMES} of {total} frames")
    );
    let text = std::str::from_utf8(&bytes).unwrap();
    let count = MAX_FRAME_ATOMS + 1;
    let huge = text.replacen("12\n", &format!("{count}\n"), 1);
    assert_eq!(
        read("huge.xyz", huge.as_bytes()),
        Err(Error::Unsupported(format!(
            "Frame 1 has {count} atoms; ReShiki reads at most {MAX_FRAME_ATOMS}"
        )))
    );
}

#[test]
fn a_comment_that_is_one_number_is_the_frame_energy_in_hartree() {
    // CREST's crest_conformers.xyz writes each conformer's energy alone.
    let water = "3\n   -5.07017600\nO 0 0 0\nH 0.96 0 0\nH -0.24 0.93 0\n\
                 3\n -5.06901\nO 0 0 0\nH 0.97 0 0\nH -0.30 0.90 0\n\
                 3\nwater, frame 3\nO 0 0 0\nH 0.95 0.1 0\nH -0.20 0.95 0.1\n";
    let file = read("crest_conformers.xyz", water.as_bytes()).unwrap();
    let energies: Vec<_> = file.jobs[0]
        .steps
        .iter()
        .map(|s| s.energy_hartree)
        .collect();
    assert_eq!(energies, [Some(-5.070176), Some(-5.06901), None]);
}
