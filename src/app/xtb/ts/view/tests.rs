use super::*;

#[test]
fn image_selection_defaults_to_ts_and_drops_out_of_range_indices() {
    assert_eq!(selection(15, None, 6), Some(6));
    assert_eq!(selection(15, Some(3), 6), Some(3));
    assert_eq!(selection(15, Some(19), 6), Some(6));
    assert_eq!(selection(4, Some(usize::MAX), 8), Some(0));
    assert_eq!(selection(0, Some(0), 0), None);
}

#[test]
fn reopened_and_stale_profiles_explain_why_the_model_does_not_preview() {
    assert_eq!(profile_note(false, false), Some(RELOAD_NOTE));
    assert_eq!(profile_note(false, true), Some(RELOAD_NOTE));
    assert!(profile_note(true, true).unwrap().contains("saved energy"));
    assert_eq!(profile_note(true, false), None);
}

#[test]
fn climbing_image_and_selection_labels_use_one_based_image_numbers() {
    let energies = [0., 1., 3., 2., -1.];
    assert_eq!(climbing_image(&energies, None), 2);
    assert_eq!(climbing_image(&energies, Some(1)), 1);
    assert_eq!(climbing_image(&energies, Some(99)), 2);
    assert_eq!(climbing_image(&energies, Some(usize::MAX)), 2);
    assert_eq!(climbing_image(&[0., 3., 3., 0.], None), 1);
    assert_eq!(climbing_image(&[], None), 0);
    assert_eq!(
        selection_label(6, 15, 6, 26.5),
        "Image 7 of 15 · climbing image (TS) · ΔE 26.5 kcal/mol"
    );
    assert!(selection_label(14, 15, 6, -8.2).contains("product · ΔE −8.2"));
}

#[test]
fn chart_clicks_follow_the_nearest_image_even_for_reopened_profiles() {
    let chart = PathChart {
        deltas: vec![0., 2., 5., 1., -1.],
        selected: Some(2),
        climbing: 2,
        interactive: true,
        stale: false,
    };
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(420., 190.));
    assert_eq!(chart.nearest(chart.x(0, bounds), bounds), Some(0));
    assert_eq!(chart.nearest(chart.x(3, bounds) + 1., bounds), Some(3));
    assert_eq!(chart.nearest(bounds.width, bounds), Some(4));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn changed_bonds_use_saved_endpoints_and_lengths_use_ts_coordinates() {
    let frame = |positions_angstrom| Frame {
        numbers: vec![6, 6, 8],
        positions_angstrom,
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let reactant = frame(vec![[0., 0., 0.], [1.5, 0., 0.], [4., 0., 0.]]);
    let product = frame(vec![[0., 0., 0.], [3., 0., 0.], [4.4, 0., 0.]]);
    assert_eq!(
        changed_bonds(&reactant, &product),
        vec![("Forming", [1, 2]), ("Breaking", [0, 1])]
    );
    let ts = frame(vec![[0., 0., 0.], [2., 0., 0.], [4.1, 0., 0.]]);
    assert_eq!(bond_label(&ts, [1, 2], &[]), "C2···O3");
    assert_eq!(
        bond_label(&ts, [1, 2], &[None, Some(12), Some(6)]),
        "C12···O6"
    );
    assert!((bond_length(&ts, [1, 2]).unwrap() - 2.1).abs() < 1e-12);
    assert_eq!(bond_length(&ts, [1, 99]), None);
}
