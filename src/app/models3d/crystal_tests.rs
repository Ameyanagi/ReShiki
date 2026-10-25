use super::*;

#[test]
fn flattening_a_crystal_keeps_its_cell_without_assigning_distance_bond_chemistry() {
    for source in [
        include_str!("../../../tests/fixtures/crystal/nacl.cif"),
        include_str!("../../../tests/fixtures/crystal/co2.cif"),
    ] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.doc = reshiki::crystal::read("cif", source).unwrap().0;
        let ids: Vec<_> = app.tab.doc.atoms.iter().map(|a| a.id).collect();
        app.tab.doc.translate(&ids, 300., 100.);
        app.tab.doc.models[0].display = Display::ThreeD;
        app.tab.doc.models[0].repeat = [2, 1, 1];
        app.tab.doc.models[0].charges = true;
        app.tab.doc.models[0].rotation =
            models3d::turn(app.tab.doc.models[0].rotation, 50., 25., false);
        app.tab.selected = ids.clone();
        let before = app.tab.doc.clone();
        let centre = reshiki::editing::center(&before, &ids);
        let origin = models3d::frame_origin(&before, &before.models[0]);
        let _ = app.models3d_action(Action::Flatten);
        assert!(!app.error, "{}", app.status);
        assert!(app.status.starts_with("Shown as 2D from the 3D view"));
        assert_eq!(app.tab.doc.models.len(), 1);
        let model = &app.tab.doc.models[0];
        assert_eq!(model.display, Display::TwoD);
        assert_eq!(model.layout, Layout::View);
        assert_eq!(model.cell, before.models[0].cell);
        assert_eq!(model.sites, before.models[0].sites);
        assert_eq!(model.atoms, before.models[0].atoms);
        assert_eq!(model.repeat, before.models[0].repeat);
        assert_eq!(model.charges, before.models[0].charges);
        assert_eq!(app.tab.doc.bonds, before.bonds);
        assert_ne!(app.tab.doc.atoms, before.atoms);
        assert!(reshiki::editing::center(&app.tab.doc, &ids).distance(centre) < 1e-3);
        assert!(
            models3d::frame_origin(&app.tab.doc, model)
                .unwrap()
                .distance(origin.unwrap())
                < 1e-3
        );
        assert!(reshiki::models3d::crystal::touches(&app.tab.doc, &ids));
        app.step_history(false);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
}
