//! The actual bottom dock, including chrome, at the minimum window size.
use super::*;
use crate::app::accessibility::Action as WindowAction;
use crate::app::xtb::{Action, View, palette};
use iced::advanced::widget::{Id, Operation, operation::Scrollable};

#[derive(Default, Debug)]
struct Bounds {
    dock: Option<Rectangle>,
    body: Option<Rectangle>,
    failure_details: bool,
}

impl Operation for Bounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&Id::from("xtb-results-bottom")) {
            self.dock = Some(bounds);
        }
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _: Rectangle,
        _: Vector,
        _: &mut dyn Scrollable,
    ) {
        if id == Some(&Id::from("xtb-results-body")) {
            self.body = Some(bounds);
        }
    }

    fn text(&mut self, _: Option<&Id>, _: Rectangle, text: &str) {
        self.failure_details |= text.contains("deliberate renderer failure");
    }
}

fn measure(app: &App, renderer: &iced::Renderer, size: Size) -> Bounds {
    measure_element(app.view(), renderer, size)
}

fn measure_element(
    mut view: Element<'_, Message>,
    renderer: &iced::Renderer,
    size: Size,
) -> Bounds {
    let mut tree = Tree::new(view.as_widget());
    let node =
        view.as_widget_mut()
            .layout(&mut tree, renderer, &layout::Limits::new(Size::ZERO, size));
    let mut bounds = Bounds::default();
    view.as_widget_mut()
        .operate(&mut tree, Layout::new(&node), renderer, &mut bounds);
    bounds
}

fn act(app: &mut App, action: palette::Action) {
    let _ = app.update(Message::Xtb(Action::Palette(action)));
}

fn window(app: &mut App, size: Size) {
    let id = iced::window::Id::unique();
    let _ = app.accessibility_action(WindowAction::Window(
        id,
        iced::window::Event::Opened {
            position: None,
            size,
        },
    ));
    let _ = app.accessibility_action(WindowAction::Geometry(id, size, 1.));
}

fn fixture(app: &mut App, renderer: &iced::Renderer, size: Size) {
    window(app, size);
    features::xtb_results(app, &features::Driver::new(renderer));
    // Add levels and a light vibration spectrum so the same real palette
    // fixture exercises every body-height choice without starting a worker.
    let mut result = (*app.tab.doc.science.results[0].result).clone();
    result.orbitals = Some(
        serde_json::from_value(serde_json::json!({
            "core_electrons": [2, 0, 0, 0, 0],
            "alpha": {
                "energies_hartree": [-0.7, -0.5, -0.4, -0.3, 0.2, 0.4],
                "occupations": [2., 2., 2., 2., 0., 0.]
            },
            "beta": "restricted"
        }))
        .unwrap(),
    );
    result.vibrations = Some(
        serde_json::from_value(serde_json::json!({
            "frequencies_cm1": [650., 1400., 2900.],
            "ir_intensities_km_mol": [50., 120., 80.]
        }))
        .unwrap(),
    );
    // The omitted-data note exercises variable footer chrome as well.
    result.omitted_bytes = Some(2048);
    let result = std::sync::Arc::new(result);
    app.tab.doc.science.results[0].result = result.clone();
    app.xtb_show_result(&result);
}

#[tokio::test]
#[ignore = "Opt-in renderer bounds and snapshots; optional RESHIKI_XTB_PALETTE_QA_DIR"]
async fn bottom_results_palette_stays_within_40_percent_of_the_window() {
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .expect("Headless renderer");
    let output = std::env::var_os("RESHIKI_XTB_PALETTE_QA_DIR").map(std::path::PathBuf::from);
    if let Some(output) = &output {
        std::fs::create_dir_all(output).unwrap();
    }
    for (name, mode) in [
        ("light", crate::appearance::Mode::Light),
        ("dark", crate::appearance::Mode::Dark),
    ] {
        for size in [Size::new(1040., 712.), Size::new(1440., 900.)] {
            let (mut app, _) = App::new();
            app.appearance.mode = mode;
            fixture(&mut app, &renderer, size);
            assert_eq!(app.xtb.dock.collapsed, size.height < 800.);
            if app.xtb.dock.collapsed {
                let bounds = measure(&app, &renderer, size);
                assert!(bounds.body.is_none(), "collapsed: {bounds:?}");
                act(&mut app, palette::Action::Collapse);
            }
            for stale in [false, true] {
                if stale {
                    let before = app.tab.doc.clone();
                    let carbon = app.tab.doc.atoms[0].id;
                    let added = app.tab.doc.add_atom("C", World::new(0., 40.));
                    app.tab.doc.add_bond(carbon, added, 1, "plain");
                    app.changed(before);
                }
                for view in [
                    View::Summary,
                    View::Steps,
                    View::Vibrations,
                    View::Orbitals,
                    View::Log,
                ] {
                    act(&mut app, palette::Action::View(view));
                    let bounds = measure(&app, &renderer, size);
                    let dock = bounds.dock.expect("bottom palette");
                    let body = bounds.body.expect("expanded body");
                    assert!(
                        dock.height <= size.height * 0.4 + 0.1,
                        "{name} {size:?} {view:?} stale {stale}: {bounds:?}"
                    );
                    assert!(dock.y >= 0. && dock.y + dock.height <= size.height + 0.1);
                    assert!(body.height >= 60.);
                    if size.height == 900. && view != View::Orbitals {
                        assert_eq!(body.height, 150., "{view:?}: {bounds:?}");
                    }
                    if let Some(output) = &output
                        && !stale
                        && matches!(view, View::Steps | View::Vibrations)
                    {
                        snapshot(
                            &mut app,
                            &mut renderer,
                            size,
                            &output.join(format!(
                                "{name}-{}-{}.png",
                                size.width,
                                view.name().to_lowercase(),
                            )),
                        );
                    }
                    eprintln!("{name} {size:?} {view:?} stale {stale}: {bounds:?}");
                }
            }
            let (mut failure, _) = App::new();
            failure.appearance.mode = mode;
            window(&mut failure, size);
            features::xtb_halocarbon(&mut failure);
            failure.xtb_failed_answer("A deliberate renderer failure: ".repeat(80));
            let bounds = measure_element(
                failure.xtb_bottom_palette().expect("failure palette"),
                &renderer,
                size,
            );
            assert_eq!(bounds.failure_details, size.height >= 800.);
            if failure.xtb.dock.collapsed {
                act(&mut failure, palette::Action::Collapse);
            }
            let bounds = measure_element(
                failure
                    .xtb_bottom_palette()
                    .expect("expanded failure palette"),
                &renderer,
                size,
            );
            assert!(bounds.failure_details);
            assert!(bounds.dock.unwrap().height <= size.height * 0.4 + 0.1);
        }
    }
}
