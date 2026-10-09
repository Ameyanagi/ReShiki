use reshiki::{
    document::{Document, Point},
    editing::reference::Stretch,
    engine::{ChemistryEngine, LocalEngine, Request},
    templates::{Anchor, Connection, LIBRARY},
};

fn core() -> &'static reshiki::templates::Template {
    LIBRARY
        .iter()
        .find(|t| t.name == "Porphine (21H,23H)")
        .unwrap()
}
async fn analysis(engine: &LocalEngine, doc: &Document) -> reshiki::engine::Analysis {
    engine
        .execute(Request::molecule("analyze", doc.clone()))
        .await
        .unwrap()
        .analysis
        .unwrap()
}

#[tokio::test]
async fn chosen_free_base_has_two_opposite_nh_sites_and_persists_exact_identity() {
    let engine = LocalEngine::default();
    let core = core();
    let expected = analysis(&engine, &core.document).await;
    assert_eq!(expected.formula, "C20H14N4");
    assert_eq!(expected.inchikey, "RKCAIXNGYQCCAL-CEVVSZFKSA-N");
    assert_eq!(
        (
            expected.donors,
            expected.acceptors,
            expected.unpaired_electrons
        ),
        (2, 2, 0)
    );
    assert!(core.note.contains("opposite N–H"));
    for pt in [12., 14.4, 18.] {
        let mut base = Document::default();
        base.drawing_style.set_bond_length(pt);
        let (placed, ids) = core
            .place(
                &base,
                Point::new(250., 300.),
                None,
                5.,
                Anchor::Auto,
                Connection::Connect,
            )
            .unwrap();
        assert_eq!(ids.len(), 24);
        assert_eq!(analysis(&engine, &placed).await.inchikey, expected.inchikey);
        for bond in &placed.bonds {
            let length = placed
                .atom(bond.a)
                .unwrap()
                .position
                .distance(placed.atom(bond.b).unwrap().position);
            assert!((length - base.drawing_style.world(pt)).abs() < 0.0001);
        }
    }
    let native = Document::from_native_file(&core.document.file_json().unwrap()).unwrap();
    assert_eq!(native, core.document);
    for format in ["mol", "cdxml"] {
        let mut request = Request::molecule("export", core.document.clone());
        request.format = Some(format.into());
        let output = engine.execute(request).await.unwrap().output.unwrap();
        let reopened = engine
            .execute(Request::import(format, &output))
            .await
            .unwrap()
            .document
            .unwrap();
        let actual = analysis(&engine, &reopened).await;
        assert_eq!(actual.formula, expected.formula, "{format}");
        assert_eq!(actual.inchikey, expected.inchikey, "{format}");
    }
}

#[tokio::test]
async fn one_meso_phenyl_attaches_and_stretches_without_distorting_the_core_or_ring() {
    let engine = LocalEngine::default();
    let core = &core().document;
    let phenyl = LIBRARY.iter().find(|t| t.name == "Benzene").unwrap();
    let p = core.atom(5).unwrap().position;
    let (joined, _) = phenyl
        .place(
            core,
            p,
            Some(p.offset(42., -42.)),
            5.,
            Anchor::Atom(1),
            Connection::Connect,
        )
        .unwrap();
    let bridge = joined
        .bonds
        .iter()
        .find(|b| (b.a <= 24) != (b.b <= 24))
        .unwrap();
    let (fixed, moving) = if bridge.a <= 24 {
        (bridge.a, bridge.b)
    } else {
        (bridge.b, bridge.a)
    };
    assert_eq!(fixed, 5);
    let expected = analysis(&engine, &joined).await;
    assert_eq!(expected.formula, "C26H18N4");
    assert!(expected.inchikey.starts_with("IIKNTCGCRNEFET-"));
    let plan = Stretch::new(&joined, fixed, moving).unwrap();
    assert_eq!(plan.ids.len(), 6);
    let stretched = plan.apply(&joined, plan.length * 1.5).unwrap();
    for atom in &core.atoms {
        assert_eq!(stretched.atom(atom.id), Some(atom));
    }
    assert_eq!(stretched.bonds, joined.bonds);
    for bond in joined.bonds.iter().filter(|b| b.a > 24 && b.b > 24) {
        let before = joined
            .atom(bond.a)
            .unwrap()
            .position
            .distance(joined.atom(bond.b).unwrap().position);
        let after = stretched
            .atom(bond.a)
            .unwrap()
            .position
            .distance(stretched.atom(bond.b).unwrap().position);
        assert!((before - after).abs() < 0.0001);
    }
    assert_eq!(
        analysis(&engine, &stretched).await.inchikey,
        expected.inchikey
    );
    let reopened = Document::from_native_file(&stretched.file_json().unwrap()).unwrap();
    assert_eq!(
        analysis(&engine, &reopened).await.inchikey,
        expected.inchikey
    );
}
