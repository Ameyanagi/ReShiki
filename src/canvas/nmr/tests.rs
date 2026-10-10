use super::*;
use reshiki::chemistry::nmr::Nucleus;
use reshiki::document::Point as World;

fn fixture(nucleus: Nucleus) -> (Document, Report) {
    let doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/nmr/ethyl-acetate.rsk"
    ))
    .unwrap();
    let request = reshiki_io::nmr::prepare(&doc, &[]).unwrap();
    let report = reshiki_io::nmr::predict(&request, nucleus).unwrap();
    (doc, report)
}
fn camera() -> Camera {
    Camera {
        center: World::new(282.8, 219.),
        zoom: 2.5,
    }
}
fn bounds() -> Rectangle {
    Rectangle::with_size(Size::new(900., 600.))
}

#[test]
fn assignment_numbers_use_original_atoms_and_avoid_fixture_symbols_and_each_other() {
    let (doc, report) = fixture(Nucleus::C13);
    let original = doc.clone();
    let export = reshiki::scene::svg(&doc);
    let labels = placements(
        &doc,
        Context {
            report: &report,
            mode: LabelMode::Atom,
        },
        camera(),
        bounds(),
    );
    assert_eq!(
        labels.iter().map(|l| l.atom_id).collect::<Vec<_>>(),
        [1, 2, 4, 5]
    );
    for (index, label) in labels.iter().enumerate() {
        assert_eq!(label.text, format!("#{}", label.atom_id));
        assert!(
            !label.leader,
            "Sparse assignments should stay next to their vertices"
        );
        assert!(
            label
                .anchor
                .distance(nearest_point(label.bounds, label.anchor))
                <= 22.
        );
        assert_eq!(
            label.anchor,
            camera().screen(doc.atom(label.atom_id).unwrap().position, bounds())
        );
        for other in labels.iter().skip(index + 1) {
            assert!(!label.bounds.intersects(&other.bounds));
        }
        for atom in &doc.atoms {
            if let Some((lo, hi)) = reshiki::scene::atom_label_bounds(atom, &doc) {
                let lo = camera().screen(lo, bounds());
                let hi = camera().screen(hi, bounds());
                assert!(!label.bounds.intersects(&Rectangle {
                    x: lo.x,
                    y: lo.y,
                    width: hi.x - lo.x,
                    height: hi.y - lo.y
                }));
            }
        }
        let ink_bounds = padded(
            label.bounds,
            doc.drawing_style.line_width() * camera().zoom / 2.,
        );
        for bond in &doc.bonds {
            let a = camera().screen(doc.atom(bond.a).unwrap().position, bounds());
            let b = camera().screen(doc.atom(bond.b).unwrap().position, bounds());
            assert!(
                !segment_crosses(ink_bounds, a, b),
                "Label {} crosses a bond",
                label.text
            );
        }
    }
    assert_eq!(doc, original, "Assignments are not persistent atom labels");
    assert_eq!(reshiki::scene::svg(&doc), export);
}

#[test]
fn shift_and_combined_modes_follow_moved_atom_without_changing_predictions_or_export() {
    let (mut doc, report) = fixture(Nucleus::C13);
    let export = reshiki_io::nmr::to_tsv(&report);
    for mode in [LabelMode::Shift, LabelMode::Both] {
        let labels = placements(
            &doc,
            Context {
                report: &report,
                mode,
            },
            camera(),
            bounds(),
        );
        let carbonyl = labels.iter().find(|l| l.atom_id == 4).unwrap();
        assert!(carbonyl.text.contains("170.700 ppm"));
        assert_eq!(carbonyl.text.contains("#4"), mode == LabelMode::Both);
        assert!(!carbonyl.text.contains("¹³C"));
    }
    let before = placements(
        &doc,
        Context {
            report: &report,
            mode: LabelMode::Atom,
        },
        camera(),
        bounds(),
    );
    doc.atom_mut(2).unwrap().position.x += 30.;
    let after = placements(
        &doc,
        Context {
            report: &report,
            mode: LabelMode::Atom,
        },
        camera(),
        bounds(),
    );
    let before = before.iter().find(|l| l.atom_id == 2).unwrap();
    let after = after.iter().find(|l| l.atom_id == 2).unwrap();
    assert_eq!(after.anchor, before.anchor + Vector::new(75., 0.));
    assert_eq!(
        reshiki_io::nmr::fingerprint(&doc, &report.atom_ids).unwrap(),
        report.fingerprint
    );
    assert_eq!(reshiki_io::nmr::to_tsv(&report), export);
}

#[test]
fn proton_assignments_name_the_parent_and_unresolved_group_not_individual_protons() {
    let (doc, report) = fixture(Nucleus::H1);
    let original = reshiki_io::nmr::to_tsv(&report);
    let labels = placements(
        &doc,
        Context {
            report: &report,
            mode: LabelMode::Both,
        },
        camera(),
        bounds(),
    );
    assert_eq!(labels.len(), 3);
    for label in labels {
        let row = report
            .rows
            .iter()
            .find(|r| r.atom_id == label.atom_id)
            .unwrap();
        assert_eq!(
            label.text,
            format!(
                "#{} · {:.3} ppm",
                row.atom_id,
                row.statistics.as_ref().unwrap().median
            )
        );
        assert!(row.hydrogen_count > 1);
    }
    assert_eq!(reshiki_io::nmr::to_tsv(&report), original);
}

#[test]
fn offscreen_owners_do_not_leave_clamped_labels_on_empty_canvas() {
    let (doc, report) = fixture(Nucleus::C13);
    let labels = placements(
        &doc,
        Context {
            report: &report,
            mode: LabelMode::Both,
        },
        Camera {
            center: World::new(10000., 10000.),
            zoom: 2.5,
        },
        bounds(),
    );
    assert!(labels.is_empty());
}

#[test]
fn missing_or_nonfinite_predictions_do_not_gain_labels_or_selected_halos() {
    let (doc, mut report) = fixture(Nucleus::C13);
    report.rows[0].statistics = None;
    report.rows[1].statistics.as_mut().unwrap().median = f64::NAN;
    let context = Context {
        report: &report,
        mode: LabelMode::Atom,
    };
    assert_eq!(context.supported_ids(), [4, 5]);
    assert_eq!(
        placements(&doc, context, camera(), bounds())
            .iter()
            .map(|a| a.atom_id)
            .collect::<Vec<_>>(),
        [4, 5]
    );
}

#[test]
fn collapsed_assignments_retain_original_owners_at_the_visible_group_anchor() {
    let (mut doc, report) = fixture(Nucleus::C13);
    doc.abbreviations
        .push(reshiki::abbreviations::Abbreviation {
            label_style: None,
            label_color_override: false,
            highlight: None,
            label: "Et".into(),
            reverse_label: "Et".into(),
            anchor: 2,
            members: vec![1, 2],
            alignment: Default::default(),
        });
    let original = doc.clone();
    let report_bytes = reshiki_io::nmr::to_tsv(&report);
    let labels = placements(
        &doc,
        Context {
            report: &report,
            mode: LabelMode::Atom,
        },
        camera(),
        bounds(),
    );
    let anchor = camera().screen(doc.atom(2).unwrap().position, bounds());
    for id in [1, 2] {
        let label = labels.iter().find(|a| a.atom_id == id).unwrap();
        assert_eq!(label.anchor, anchor);
        assert_eq!(label.text, format!("#{id}"));
    }
    for (index, label) in labels.iter().enumerate() {
        for other in labels.iter().skip(index + 1) {
            assert!(!label.bounds.intersects(&other.bounds));
        }
    }
    assert_eq!(doc, original);
    assert_eq!(reshiki_io::nmr::to_tsv(&report), report_bytes);
}

#[test]
fn crowded_label_uses_a_leader_only_when_displaced_from_its_real_owner() {
    let (mut doc, mut report) = fixture(Nucleus::C13);
    report.rows.retain(|r| r.atom_id == 2);
    // A text label around the real carbon blocks close placements. It does not
    // belong to the report and cannot take over the assignment's owner.
    let blocker = doc.add_atom("O", World::new(246.4, 219.));
    let labels = placements(
        &doc,
        Context {
            report: &report,
            mode: LabelMode::Atom,
        },
        camera(),
        bounds(),
    );
    let [label] = labels.as_slice() else {
        panic!("One supported owner");
    };
    assert_eq!(label.atom_id, 2);
    assert_eq!(
        label.anchor,
        camera().screen(doc.atom(2).unwrap().position, bounds())
    );
    assert!(label.leader);
    let (lo, hi) = reshiki::scene::atom_label_bounds(doc.atom(blocker).unwrap(), &doc).unwrap();
    let lo = camera().screen(lo, bounds());
    let hi = camera().screen(hi, bounds());
    assert!(!label.bounds.intersects(&Rectangle {
        x: lo.x,
        y: lo.y,
        width: hi.x - lo.x,
        height: hi.y - lo.y
    }));
}

#[tokio::test]
#[ignore = "Opt-in actual canvas NMR labels and narrowly scoped selection halo"]
async fn actual_nmr_selection_is_a_filled_halo_and_ordinary_selection_is_unchanged() {
    use iced::advanced::renderer::Headless;
    use iced::{Renderer, Theme};
    let (doc, mut report) = fixture(Nucleus::C13);
    let original = doc.file_json().unwrap();
    let export = reshiki::scene::svg(&doc);
    let mut renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .expect("Actual renderer is required");
    let render = |renderer: &mut Renderer, context: Option<Context<'_>>| {
        canvas_pixels(renderer, &doc, context, &Theme::Light)
    };
    let ordinary = render(&mut renderer, None);
    let nmr = render(
        &mut renderer,
        Some(Context {
            report: &report,
            mode: LabelMode::Atom,
        }),
    );
    let anchor = camera().screen(doc.atom(2).unwrap().position, bounds());
    let ring_ink = |pixels: &[u8]| {
        let mut count = 0;
        // Right-hand edge of the selected circle is clear of both attached
        // diagonal bonds and the nearby assignment text.
        for y in -1..=1 {
            for x in 7..=9 {
                let offset = (((anchor.y.round() as i32 + y) as usize * 900)
                    + (anchor.x.round() as i32 + x) as usize)
                    * 4;
                let rgb = &pixels[offset..offset + 3];
                count += usize::from(rgb[1] as i16 - rgb[0] as i16 > 35);
            }
        }
        count
    };
    assert!(
        ring_ink(&ordinary) >= 2,
        "Ordinary selection retains its outline"
    );
    assert_eq!(ring_ink(&nmr), 0, "NMR selection has no outlined ring");
    assert_teal_halo_pixels(&mut renderer, &nmr, doc.canvas_theme, anchor);
    let center = (anchor.y.round() as usize * 900 + anchor.x.round() as usize) * 4;
    assert_eq!(
        &nmr[center..center + 3],
        &ordinary[center..center + 3],
        "The halo stays beneath the molecular bond ink",
    );
    assert!(nmr[center..center + 3].iter().all(|&channel| channel < 32));
    let snapshot = doc.file_json().unwrap();
    assert_eq!(snapshot, original);
    assert_eq!(reshiki::scene::svg(&doc), export);

    // A retained report whose selected atom has no prediction must not restyle
    // ordinary selection. Exclude all other labels to compare entire pixels.
    report.rows.clear();
    let empty_report = render(
        &mut renderer,
        Some(Context {
            report: &report,
            mode: LabelMode::Atom,
        }),
    );
    assert_eq!(empty_report, ordinary);
    assert_eq!(render(&mut renderer, None), ordinary);

    let hover_pixels = |renderer: &mut Renderer, nmr_ids: &[u64]| {
        use iced::advanced::{Renderer as _, graphics::geometry::Renderer as _};
        renderer.reset(bounds());
        let mut frame = super::super::layered::Frame::new(renderer, bounds().size())
            .with_canvas(doc.canvas_theme);
        frame.fill_rectangle(Point::ORIGIN, bounds().size(), iced::Color::WHITE);
        super::super::markers::Markers::hover(&doc, &[2]).draw_with_nmr(
            &mut frame,
            camera(),
            bounds(),
            false,
            nmr_ids,
        );
        for geometry in frame.finish() {
            renderer.draw_geometry(geometry);
        }
        Headless::screenshot(renderer, Size::new(900, 600), 1., iced::Color::WHITE)
    };
    assert_eq!(
        hover_pixels(&mut renderer, &[2]),
        hover_pixels(&mut renderer, &[]),
        "NMR ownership must not restyle ordinary hover",
    );
}

fn assert_teal_halo_pixels(
    renderer: &mut iced::Renderer,
    actual: &[u8],
    page: reshiki::canvas_theme::CanvasTheme,
    anchor: Point,
) {
    use iced::advanced::{Renderer as _, graphics::geometry::Renderer as _, renderer::Headless};
    use iced::widget::canvas::{Frame, Path};
    // Draw an independent known display tint using the actual backend. The
    // reference does not call Markers or the production theme-color helper.
    let (paper, halo) = match page {
        reshiki::canvas_theme::CanvasTheme::Light => {
            (iced::Color::WHITE, iced::Color::from_rgb8(208, 241, 235))
        }
        reshiki::canvas_theme::CanvasTheme::Dark => {
            (iced::Color::BLACK, iced::Color::from_rgb8(14, 47, 41))
        }
    };
    renderer.reset(bounds());
    let mut frame = Frame::new(renderer, bounds().size());
    frame.fill_rectangle(Point::ORIGIN, bounds().size(), paper);
    frame.fill(&Path::circle(anchor, 8.), halo);
    renderer.draw_geometry(frame.into_geometry());
    let expected = Headless::screenshot(renderer, Size::new(900, 600), 1., paper);
    // The upper interior is away from both descending bonds, the circle's
    // antialiased edge and its assignment text. Check an area, not one pixel.
    for dy in -6..=-4 {
        for dx in -2..=2 {
            let x = (anchor.x.round() as i32 + dx) as usize;
            let y = (anchor.y.round() as i32 + dy) as usize;
            let offset = (y * 900 + x) * 4;
            let actual = &actual[offset..offset + 3];
            let expected = &expected[offset..offset + 3];
            assert!(
                actual
                    .iter()
                    .zip(expected)
                    .all(|(&a, &b)| a.abs_diff(b) <= 1),
                "Visible NMR halo at ({x},{y}) on {page:?} must match the soft teal reference: {actual:?} vs {expected:?}",
            );
            assert!(
                actual[1] as i16 - actual[0] as i16 >= 25
                    && actual[2] as i16 - actual[0] as i16 >= 20,
                "The filled NMR halo must remain visibly teal on {page:?}: {actual:?}",
            );
        }
    }
}

fn canvas_pixels(
    renderer: &mut iced::Renderer,
    doc: &Document,
    context: Option<Context<'_>>,
    theme: &iced::Theme,
) -> Vec<u8> {
    use iced::advanced::{Renderer as _, graphics::geometry::Renderer as _, renderer::Headless};
    use iced::widget::canvas::Program;
    let mut canvas = super::super::tests::chain_canvas(doc, super::super::ChainMode::Straight);
    canvas.tool = super::super::Tool::Select;
    canvas.camera = camera();
    canvas.selected = &[2];
    canvas.nmr = context;
    renderer.reset(bounds());
    for geometry in canvas.draw(
        &super::super::State::default(),
        renderer,
        theme,
        bounds(),
        iced::mouse::Cursor::Unavailable,
    ) {
        renderer.draw_geometry(geometry);
    }
    Headless::screenshot(renderer, Size::new(900, 600), 1., iced::Color::WHITE)
}

#[tokio::test]
#[ignore = "Opt-in actual NMR label/halo page colors with independent chrome themes"]
async fn assignment_ink_is_legible_on_light_and_dark_pages_and_independent_of_chrome() {
    use iced::advanced::renderer::Headless;
    let (mut doc, report) = fixture(Nucleus::C13);
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .expect("Actual renderer is required");
    let context = Context {
        report: &report,
        mode: LabelMode::Atom,
    };
    let mut unsupported_report = report.clone();
    unsupported_report.rows.clear();
    for page in reshiki::canvas_theme::CanvasTheme::ALL {
        doc.canvas_theme = page;
        let light_chrome = canvas_pixels(&mut renderer, &doc, Some(context), &iced::Theme::Light);
        let dark_chrome = canvas_pixels(&mut renderer, &doc, Some(context), &iced::Theme::Dark);
        assert_eq!(
            light_chrome, dark_chrome,
            "NMR ink follows {page:?} page colors"
        );
        for label in placements(&doc, context, camera(), bounds()) {
            let mut ink = 0;
            for y in label.bounds.y.floor() as usize
                ..(label.bounds.y + label.bounds.height).ceil() as usize
            {
                for x in label.bounds.x.floor() as usize
                    ..(label.bounds.x + label.bounds.width).ceil() as usize
                {
                    let offset = (y * 900 + x) * 4;
                    let rgb = &dark_chrome[offset..offset + 3];
                    ink += usize::from(
                        rgb[1] as i16 - rgb[0] as i16 > 30 && rgb[2] as i16 - rgb[0] as i16 > 20,
                    );
                }
            }
            assert!(ink >= 3, "Teal {} remains visible on {page:?}", label.text);
        }
        let anchor = camera().screen(doc.atom(2).unwrap().position, bounds());
        assert_teal_halo_pixels(&mut renderer, &dark_chrome, page, anchor);
        let ordinary = canvas_pixels(&mut renderer, &doc, None, &iced::Theme::Dark);
        let unsupported = canvas_pixels(
            &mut renderer,
            &doc,
            Some(Context {
                report: &unsupported_report,
                mode: LabelMode::Atom,
            }),
            &iced::Theme::Dark,
        );
        assert_eq!(
            ordinary, unsupported,
            "Unsupported selected owners retain ordinary paint on {page:?}",
        );
    }
}
