use super::*;

fn frame(numbers: &[u8], positions: &[[f64; 3]]) -> Frame {
    Frame {
        numbers: numbers.to_vec(),
        positions_angstrom: positions.to_vec(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    }
}

fn four_bridges() -> Frame {
    frame(
        &[44, 44, 1, 1, 1, 1],
        &[
            [0., 0., -1.23],
            [0., 0., 1.23],
            [0.8, 0.8, 0.],
            [-0.8, 0.8, 0.],
            [-0.8, -0.8, 0.],
            [0.8, -0.8, 0.],
        ],
    )
}

#[test]
fn two_bridge_to_terminal_changes_and_the_reverse_are_reported() {
    let input = four_bridges();
    let mut output = input.clone();
    output.positions_angstrom[2] = [0., 0., -2.8];
    output.positions_angstrom[4] = [0., 0., 2.8];
    assert_eq!(
        hydride_bridge_changes(&input, &output),
        Some(HydrideBridgeChanges { lost: 2, gained: 0 })
    );
    assert_eq!(
        hydride_bridge_changes(&output, &input),
        Some(HydrideBridgeChanges { lost: 0, gained: 2 })
    );
}

#[test]
fn ordinary_shifts_rigid_motion_and_atom_order_do_not_change_bridges() {
    let input = four_bridges();
    let mut output = input.clone();
    for xyz in &mut output.positions_angstrom[2..] {
        xyz[0] *= 1.15;
        xyz[1] *= 1.15;
    }
    for xyz in &mut output.positions_angstrom {
        *xyz = [xyz[2] + 5., xyz[0] - 4., xyz[1] + 7.];
    }
    assert_eq!(hydride_bridge_changes(&input, &output), None);

    let order = [4, 0, 2, 5, 1, 3];
    let reorder = |frame: &Frame| {
        let mut reordered = frame.clone();
        reordered.numbers = order.iter().map(|&i| frame.numbers[i]).collect();
        reordered.positions_angstrom = order.iter().map(|&i| frame.positions_angstrom[i]).collect();
        reordered
    };
    assert_eq!(
        hydride_bridge_changes(&reorder(&input), &reorder(&output)),
        None
    );
}

#[test]
fn drawing_ids_map_independently_reordered_atoms() {
    let mut input = four_bridges();
    input.drawing_atoms = (10..16).map(Some).collect();
    let mut output = input.clone();
    output.positions_angstrom[2] = [0., 0., -2.8];
    output.numbers.reverse();
    output.positions_angstrom.reverse();
    output.drawing_atoms.reverse();
    assert_eq!(
        hydride_bridge_changes(&input, &output),
        Some(HydrideBridgeChanges { lost: 1, gained: 0 })
    );
}

#[test]
fn count_preserving_hydride_exchanges_and_metal_partner_changes_are_reported() {
    let mut input = four_bridges();
    input.positions_angstrom[2] = [0., 0., -2.8];
    let mut output = four_bridges();
    output.positions_angstrom[3] = [0., 0., 2.8];
    assert_eq!(
        hydride_bridge_changes(&input, &output),
        Some(HydrideBridgeChanges { lost: 1, gained: 1 })
    );

    let input = frame(
        &[44, 44, 44, 1],
        &[[0., 0., 0.], [2.5, 0., 0.], [5., 0., 0.], [1.25, 1., 0.]],
    );
    let mut output = input.clone();
    output.positions_angstrom[3] = [3.75, 1., 0.];
    assert_eq!(
        hydride_bridge_changes(&input, &output),
        Some(HydrideBridgeChanges { lost: 1, gained: 1 })
    );
}

#[test]
fn contact_buffer_ignores_boundary_drift_in_either_direction() {
    // For Ru, definite contacts end at 2.02 Å, possible contacts at 2.32 Å.
    let geometry = |radius: f64| {
        frame(
            &[44, 44, 1],
            &[
                [0., 0., -1.2],
                [0., 0., 1.2],
                [(radius * radius - 1.44).sqrt(), 0., 0.],
            ],
        )
    };
    let a = geometry(1.99);
    let b = geometry(2.05);
    assert_eq!(hydride_bridge_changes(&a, &b), None);
    assert_eq!(hydride_bridge_changes(&b, &a), None);
    assert!(hydride_bridge_changes(&a, &geometry(2.35)).is_some());
}

#[test]
fn organic_hydrogens_and_dihydrogen_are_not_metal_hydride_bridges() {
    for (partner, distance) in [(6, 1.1), (7, 1.0), (8, 0.98), (1, 0.75)] {
        let input = frame(
            &[44, 44, 1, partner],
            &[
                [0., 0., -1.2],
                [0., 0., 1.2],
                [1., 0., 0.],
                [1. + distance, 0., 0.],
            ],
        );
        let mut output = input.clone();
        output.positions_angstrom[0][2] = -4.;
        assert_eq!(hydride_bridge_changes(&input, &output), None, "Z={partner}");
    }
}

#[test]
fn perception_is_not_specific_to_ruthenium() {
    let input = frame(&[26, 26, 1], &[[0., 0., -1.2], [0., 0., 1.2], [0., 1., 0.]]);
    let mut output = input.clone();
    output.positions_angstrom[2] = [0., 0., -2.7];
    assert_eq!(
        hydride_bridge_changes(&input, &output),
        Some(HydrideBridgeChanges { lost: 1, gained: 0 })
    );
}

#[test]
fn nonmatching_atom_sets_are_not_compared() {
    let input = four_bridges();
    let mut output = input.clone();
    output.numbers[1] = 26;
    assert_eq!(hydride_bridge_changes(&input, &output), None);
}

#[test]
fn organic_and_single_metal_geometries_have_no_bridge_changes() {
    for numbers in [[6, 1, 1], [44, 1, 1]] {
        let input = frame(&numbers, &[[0., 0., 0.], [0., 0., -1.5], [0., 0., 1.5]]);
        let mut output = input.clone();
        output.positions_angstrom[1][2] = -4.;
        assert_eq!(hydride_bridge_changes(&input, &output), None);
    }
}

#[test]
fn diruthenium_reference_methods_have_different_bridge_outcomes() {
    let read = |text: &str| {
        let rows: Vec<_> = text
            .lines()
            .skip(2)
            .map(|line| line.split_whitespace().collect::<Vec<_>>())
            .collect();
        let numbers: Vec<_> = rows
            .iter()
            .map(|row| match row[0] {
                "H" => 1,
                "C" => 6,
                "Ru" => 44,
                symbol => panic!("unexpected fixture element {symbol}"),
            })
            .collect();
        let positions: Vec<_> = rows
            .iter()
            .map(|row| {
                [
                    row[1].parse().unwrap(),
                    row[2].parse().unwrap(),
                    row[3].parse().unwrap(),
                ]
            })
            .collect();
        frame(&numbers, &positions)
    };
    let input = read(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/xtb/diruthenium_tetrahydride_seed.xyz"
    )));
    let gfn2 = read(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/xtb/diruthenium_tetrahydride_gfn2.xyz"
    )));
    let gxtb = read(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/xtb/diruthenium_tetrahydride_gxtb.xyz"
    )));
    assert_eq!(
        hydride_bridge_changes(&input, &gfn2),
        Some(HydrideBridgeChanges { lost: 2, gained: 0 })
    );
    assert_eq!(hydride_bridge_changes(&input, &gxtb), None);
}
