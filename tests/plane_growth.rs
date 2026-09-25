use anyhow::Context;
use reshiki::{
    bonds::BondPreset,
    chains::BondDrawing,
    document::{Document, Point},
    projection::{
        self,
        growth::{self, Plane},
    },
};

fn cp() -> anyhow::Result<Document> {
    let mut doc = Document::default();
    let anchor = doc.add_atom("C", Point::default());
    let mut doc = reshiki::ligands::replace(&doc, anchor, "Cp").map_err(anyhow::Error::msg)?;
    doc.expand_abbreviations(&[anchor]);
    // Defined ligands may use a Kekule graph before a display toggle.
    for a in &mut doc.atoms {
        if a.element == "C" {
            a.aromatic = true;
        }
    }
    Ok(doc)
}
fn benzene() -> Document {
    reshiki::rings::Preset::Benzene.document(42., false)
}
fn start(doc: &Document) -> anyhow::Result<u64> {
    doc.atoms
        .iter()
        .filter(|a| a.element == "C")
        .nth(1)
        .map(|a| a.id)
        .context("Ring carbon")
}
fn tilt(doc: &mut Document) {
    let ids = doc.all_ids();
    projection::tilt(doc, &ids, 31., true);
    projection::tilt(doc, &ids, -57., false);
    reshiki::editing::transform_about(doc, &ids, Point::default(), 1., 23.);
}
fn add(
    doc: &Document,
    id: u64,
    cursor: Option<Point>,
    drawing: BondDrawing,
) -> anyhow::Result<(Document, u64)> {
    let plane = Plane::at(doc, id).context("Ring plane")?;
    let endpoint = match cursor {
        Some(p) => plane.endpoint(p, drawing),
        None => plane.outward(drawing.length),
    }
    .context("Endpoint")?;
    growth::place(doc, id, endpoint, "C", BondPreset::Single).map_err(anyhow::Error::msg)
}

#[test]
fn adding_before_or_after_tilt_produces_the_same_substituent_coordinates() -> anyhow::Result<()> {
    for flat in [cp()?, benzene()] {
        let id = start(&flat)?;
        let (mut first, new) = add(&flat, id, None, BondDrawing::default())?;
        // Keep the same ring pivot for both paths; including the new substituent
        // would change the selection centre, not the orientation being tested.
        let mut tilted = flat.clone();
        tilt(&mut tilted);
        let (after, added) = add(&tilted, id, None, BondDrawing::default())?;
        // Transform a copy with the ring and added atom, then align ring origins.
        tilt(&mut first);
        let offset = after.atom(id).context("After")?.position;
        let origin = first.atom(id).context("Before")?.position;
        let dz = after.atom(id).context("After")?.depth - first.atom(id).context("Before")?.depth;
        let ids = first.all_ids();
        first.translate(&ids, offset.x - origin.x, offset.y - origin.y);
        for a in &mut first.atoms {
            a.depth += dz;
        }
        let a = first.atom(new).context("First added")?;
        let b = after.atom(added).context("Second added")?;
        assert!(a.position.distance(b.position) < 0.001);
        assert!((a.depth - b.depth).abs() < 0.001);
        let origin = after.atom(id).context("Origin")?;
        assert!(
            (origin
                .position
                .distance(b.position)
                .hypot(origin.depth - b.depth)
                - 42.)
                .abs()
                < 0.001
        );
        let restored: Document = serde_json::from_str(&serde_json::to_string(&after)?)?;
        assert_eq!(restored, after);
    }
    Ok(())
}

#[test]
fn dragged_endpoints_snap_in_the_plane_and_free_drag_keeps_its_depth() -> anyhow::Result<()> {
    let mut flat = benzene();
    let id = start(&flat)?;
    let original = Plane::at(&flat, id)
        .context("Flat plane")?
        .outward(42.)
        .context("Point")?;
    let start = flat.atom(id).context("Origin")?.position;
    // Deliberately choose a point away from the default direction.
    let cursor = Point::new(original.position.x + 13., original.position.y - 19.);
    let mut projected = flat.clone();
    let probe = projected.add_atom("C", cursor);
    tilt(&mut projected);
    tilt(&mut flat);
    // Align the probe transformation with the unchanged ring pivot.
    let actual = flat.atom(id).context("Origin")?;
    let other = projected.atom(id).context("Origin")?;
    let p = projected.atom(probe).context("Probe")?;
    let pointer = p.position.offset(
        actual.position.x - other.position.x,
        actual.position.y - other.position.y,
    );
    let endpoint = Plane::at(&flat, id)
        .context("Plane")?
        .endpoint(pointer, BondDrawing::default().unconstrained(true))
        .context("Free endpoint")?;
    assert!(endpoint.position.distance(pointer) < 0.001);
    assert!((endpoint.depth - (p.depth + actual.depth - other.depth)).abs() < 0.001);
    let fixed = Plane::at(&flat, id)
        .context("Plane")?
        .endpoint(pointer, BondDrawing::default())
        .context("Fixed endpoint")?;
    assert!(
        (actual
            .position
            .distance(fixed.position)
            .hypot(actual.depth - fixed.depth)
            - 42.)
            .abs()
            < 0.001
    );
    assert!(start.distance(cursor) > 1.);
    Ok(())
}

#[test]
fn angle_snaps_commute_with_tilt_instead_of_snapping_in_screen_space() -> anyhow::Result<()> {
    for flat in [cp()?, benzene()] {
        let id = start(&flat)?;
        let origin = flat.atom(id).context("Origin")?.position;
        let radial = Plane::at(&flat, id)
            .context("Plane")?
            .outward(42.)
            .context("Radial")?
            .position;
        let ux = (radial.x - origin.x) / 42.;
        let uy = (radial.y - origin.y) / 42.;
        for angle in [22_f32, 47., -68.] {
            let (sin, cos) = angle.to_radians().sin_cos();
            let cursor = origin.offset(70. * (ux * cos - uy * sin), 70. * (uy * cos + ux * sin));
            let expected = Plane::at(&flat, id)
                .context("Plane")?
                .endpoint(cursor, BondDrawing::default())
                .context("Expected")?;
            let mut probes = flat.clone();
            let cursor_id = probes.add_atom("C", cursor);
            let expected_id = probes.add_atom("C", expected.position);
            let mut tilted = flat.clone();
            tilt(&mut tilted);
            tilt(&mut probes);
            let a = tilted.atom(id).context("Origin")?;
            let b = probes.atom(id).context("Origin")?;
            let shift = Point::new(a.position.x - b.position.x, a.position.y - b.position.y);
            let cursor = probes
                .atom(cursor_id)
                .context("Cursor")?
                .position
                .offset(shift.x, shift.y);
            let actual = Plane::at(&tilted, id)
                .context("Tilted plane")?
                .endpoint(cursor, BondDrawing::default())
                .context("Actual")?;
            let expected = probes.atom(expected_id).context("Expected")?;
            assert!(
                actual
                    .position
                    .distance(expected.position.offset(shift.x, shift.y))
                    < 0.001
            );
            assert!((actual.depth - (expected.depth + a.depth - b.depth)).abs() < 0.001);
        }
    }
    Ok(())
}

#[test]
fn cp_keyboard_growth_replaces_one_stored_hydrogen_and_keeps_charge() -> anyhow::Result<()> {
    let mut doc = cp()?;
    tilt(&mut doc);
    let before = reshiki::attachments::composition(&doc).map_err(anyhow::Error::msg)?;
    for id in doc.atoms.iter().filter(|a| a.element == "C").map(|a| a.id) {
        let (grown, added) = reshiki::hotkeys::atom_edit(&doc, id, "1", 42.)
            .context("Shortcut")?
            .map_err(anyhow::Error::msg)?;
        assert_eq!(grown.atom(id).context("Ring atom")?.explicit_h, 0);
        assert_eq!(grown.atoms.iter().map(|a| a.charge).sum::<i32>(), -1);
        let expected = Plane::at(&doc, id)
            .context("Plane")?
            .outward(42.)
            .context("Endpoint")?;
        assert!((grown.atom(added).context("New atom")?.depth - expected.depth).abs() < 0.001);
        let after = reshiki::attachments::composition(&grown).map_err(anyhow::Error::msg)?;
        assert_eq!(before.formula, "C5H5-");
        assert_eq!(after.formula, "C6H7-");
    }
    Ok(())
}

#[test]
fn only_unambiguous_planar_aromatic_ring_atoms_inherit_a_plane() -> anyhow::Result<()> {
    for preset in [
        reshiki::rings::Preset::Regular,
        reshiki::rings::Preset::ChairUp,
        reshiki::rings::Preset::HaworthSix,
    ] {
        let mut doc = preset.document(42., false);
        tilt(&mut doc);
        assert!(doc.atoms.iter().all(|a| Plane::at(&doc, a.id).is_none()));
    }
    let mut doc = cp()?;
    let point = doc
        .atoms
        .iter()
        .find(|a| !a.centroid.is_empty())
        .context("Point")?
        .id;
    let metal = doc.add_atom("Fe", Point::new(100., 100.));
    doc.add_bond(point, metal, 1, "plain");
    assert!(Plane::at(&doc, point).is_none());
    assert!(Plane::at(&doc, metal).is_none());
    let id = start(&doc)?;
    doc.atom_mut(id).context("Carbon")?.depth = 8.;
    assert!(
        Plane::at(&doc, id).is_none(),
        "Do not flatten a nonplanar ring"
    );
    let mut doc = benzene();
    let id = start(&doc)?;
    let ids = doc.all_ids();
    projection::tilt(&mut doc, &ids, 85., false);
    projection::tilt(&mut doc, &ids, 5., false);
    let atom = doc.atom(id).context("Carbon")?;
    for free in [false, true] {
        let end = Plane::at(&doc, id)
            .context("Edge-on plane")?
            .endpoint(
                atom.position.offset(40., 60.),
                BondDrawing::default().unconstrained(free),
            )
            .context("Stable endpoint")?;
        assert!(end.depth.is_finite());
        assert!(end.depth.abs() < 10_000.);
        growth::place(&doc, id, end, "C", BondPreset::Single).map_err(anyhow::Error::msg)?;
    }
    Ok(())
}
