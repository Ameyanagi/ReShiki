//! Real application-renderer evidence and visible feedback regression checks.
use super::*;
use crate::app::atom_text;
use iced::advanced::{
    Layout, layout, mouse,
    renderer::Headless,
    widget::{Id, Operation, Tree},
};
use iced::{Event, Rectangle, Size};
use reshiki::document::Point;
use std::path::Path;

#[derive(Default)]
struct Texts(Vec<(String, Rectangle)>);
impl Operation for Texts {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn text(&mut self, _: Option<&Id>, bounds: Rectangle, value: &str) {
        self.0.push((value.into(), bounds));
    }
}

struct Ui {
    renderer: iced::Renderer,
    tree: Tree,
    size: Size,
}
impl Ui {
    async fn new(size: Size) -> Self {
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .unwrap(),
            tree: Tree::empty(),
            size,
        }
    }
    fn frame(&mut self, app: &mut App, output: Option<&Path>) -> Texts {
        let mut messages = vec![];
        let mut texts = Texts::default();
        let theme = app.theme();
        let mut view = app.view();
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.size, self.size),
        );
        let viewport = Rectangle::with_size(self.size);
        view.as_widget_mut().update(
            &mut self.tree,
            &Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &viewport,
        );
        view.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            &mut texts,
        );
        if let Some(output) = output {
            iced::advanced::Renderer::reset(&mut self.renderer, viewport);
            view.as_widget().draw(
                &self.tree,
                &mut self.renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: theme.palette().text,
                },
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &viewport,
            );
            let pixels = Headless::screenshot(
                &mut self.renderer,
                Size::new(self.size.width as u32, self.size.height as u32),
                1.,
                theme.palette().background,
            );
            image::save_buffer(
                output,
                &pixels,
                self.size.width as u32,
                self.size.height as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        drop(view);
        for message in messages {
            let _ = app.update(message);
        }
        texts
    }
    fn capture(&mut self, app: &mut App, directory: &Path, name: &str) -> Texts {
        for _ in 0..3 {
            self.frame(app, None);
        }
        self.frame(app, Some(&directory.join(format!("{name}.png"))))
    }
}
fn app() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.grid = false;
    app.tab.keyboard_drawing.leave();
    app.appearance.mode = crate::appearance::Mode::Light;
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Properties;
    app.tab.camera.zoom = 2.;
    app.status = "Ready".into();
    app
}
fn ethane(app: &mut App, y: f32) -> [u64; 2] {
    let a = app.tab.doc.add_atom("C", Point::new(-42., y));
    let b = app.tab.doc.add_atom("C", Point::new(0., y));
    app.tab.doc.add_bond(a, b, 1, "plain");
    [a, b]
}
async fn refresh(app: &mut App) {
    let key = app.property_request_key();
    let request = Request::molecule("analyze", app.property_document(&key));
    let _ = app.inspector_action(Action::RefreshProperties);
    let result = app.engine.execute(request).await.and_then(|response| {
        response
            .analysis
            .ok_or_else(|| "No molecular properties were returned.".into())
    });
    let _ = app.inspector_action(Action::PropertiesCalculated(key, Box::new(result)));
}
fn contains(texts: &Texts, value: &str) -> bool {
    texts.0.iter().any(|(text, _)| text == value)
}

fn warning_pixels(path: &Path, bounds: Rectangle) -> usize {
    let image = image::open(path).unwrap().into_rgb8();
    let left = bounds.x.max(0.) as u32;
    let top = bounds.y.max(0.) as u32;
    let right = (bounds.x + bounds.width).ceil().min(image.width() as f32) as u32;
    let bottom = (bounds.y + bounds.height).ceil().min(image.height() as f32) as u32;
    (top..bottom)
        .flat_map(|y| (left..right).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let [r, g, b] = image.get_pixel(*x, *y).0.map(i16::from);
            r > g + 40 && r > b + 40
        })
        .count()
}

#[tokio::test]
#[ignore = "Opt-in real renderer feedback checks and matched before/after evidence"]
async fn feedback_layout_and_visual_evidence() {
    let directory = std::env::var_os("RESHIKI_LABEL_FEEDBACK_QA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("reshiki-label-feedback-qa"));
    std::fs::create_dir_all(&directory).unwrap();
    let baseline = std::env::var_os("RESHIKI_LABEL_FEEDBACK_QA_BASELINE").is_some();
    let prefix = if baseline { "before" } else { "after" };
    let mut ui = Ui::new(Size::new(1280., 820.)).await;
    let mut generic = app();
    let [left, _] = ethane(&mut generic, -48.);
    let [_, right] = ethane(&mut generic, 48.);
    for id in [left, right] {
        generic.tab.doc = reshiki::atom_text::apply(
            &generic.tab.doc,
            id,
            "[Cl,Br,I]",
            reshiki::atom_text::Mode::Auto,
        )
        .unwrap();
    }
    generic
        .tab
        .inspector_ui
        .update(Action::Section(Section::Molecule, true));
    refresh(&mut generic).await;
    std::fs::write(
        directory.join("generic-lists.rsk"),
        serde_json::to_vec_pretty(&generic.tab.doc).unwrap(),
    )
    .unwrap();
    let texts = ui.capture(
        &mut generic,
        &directory,
        &format!("{prefix}-generic-properties"),
    );
    let generic_text = texts
        .0
        .iter()
        .find(|(text, _)| {
            text.starts_with(if baseline {
                "Invalid drawing:"
            } else {
                "Generic structure ·"
            })
        })
        .unwrap();
    let red = warning_pixels(
        &directory.join(format!("{prefix}-generic-properties.png")),
        generic_text.1,
    );
    assert_eq!(
        red > 0,
        baseline,
        "Only the baseline generic note uses warning-red ink"
    );
    if !baseline {
        assert!(contains(
            &texts,
            "Generic structure · molecular properties are calculated after expansion."
        ));
        assert!(
            !texts
                .0
                .iter()
                .any(|(text, _)| text.starts_with("Invalid drawing:"))
        );
    }
    // The same recalculation as the panel's Check structure button.
    refresh(&mut generic).await;
    if !baseline {
        assert!(contains(
            &ui.frame(&mut generic, None),
            "Generic structure · molecular properties are calculated after expansion."
        ));
    }

    let mut invalid = app();
    let center = invalid.tab.doc.add_atom("C", Point::default());
    for i in 0..5 {
        let angle = std::f32::consts::TAU * i as f32 / 5.;
        let end = invalid
            .tab
            .doc
            .add_atom("F", Point::new(angle.cos() * 42., angle.sin() * 42.));
        invalid.tab.doc.add_bond(center, end, 1, "plain");
    }
    invalid
        .tab
        .inspector_ui
        .update(Action::Section(Section::Molecule, true));
    refresh(&mut invalid).await;
    std::fs::write(
        directory.join("invalid-valence.rsk"),
        serde_json::to_vec_pretty(&invalid.tab.doc).unwrap(),
    )
    .unwrap();
    ui.tree = Tree::empty();
    let texts = ui.capture(
        &mut invalid,
        &directory,
        &format!("{prefix}-invalid-valence"),
    );
    assert!(
        texts
            .0
            .iter()
            .any(|(text, _)| text.to_ascii_lowercase().contains("valence"))
    );
    let invalid_text = texts
        .0
        .iter()
        .find(|(text, _)| text.to_ascii_lowercase().contains("valence"))
        .unwrap();
    assert!(
        warning_pixels(
            &directory.join(format!("{prefix}-invalid-valence.png")),
            invalid_text.1
        ) > 0,
        "A genuine valence error stays warning-red"
    );

    let mut repeated = app();
    repeated.tool = Tool::Text;
    let [_, end] = ethane(&mut repeated, 0.);
    std::fs::write(
        directory.join("ethane.rsk"),
        serde_json::to_vec_pretty(&repeated.tab.doc).unwrap(),
    )
    .unwrap();
    let _ = repeated.update(Message::AtomText(atom_text::Action::Begin(Some(end))));
    let _ = repeated.update(Message::AtomText(atom_text::Action::Input("OMe".into())));
    let _ = repeated.update(Message::AtomText(atom_text::Action::Apply));
    repeated.tab.selected.clear();
    std::fs::write(
        directory.join("remembered-methoxy.rsk"),
        serde_json::to_vec_pretty(&repeated.tab.doc).unwrap(),
    )
    .unwrap();
    repeated.status = "Ready".into();
    ui.tree = Tree::empty();
    let texts = ui.capture(
        &mut repeated,
        &directory,
        &format!("{prefix}-remembered-label"),
    );
    if !baseline {
        assert!(contains(&texts, "Repeat: OMe"));
        assert!(
            texts
                .0
                .iter()
                .any(|(text, _)| text.contains(if crate::platform::mac() {
                    "Option-click"
                } else {
                    "Alt-click"
                }) && text.contains("Double-click"))
        );
    }
    let remembered = repeated.last_label.clone();
    let _ = repeated.update(Message::Escape);
    repeated.tool = Tool::Text;
    let texts = ui.capture(
        &mut repeated,
        &directory,
        &format!("{prefix}-escape-cleared"),
    );
    assert!(!texts.0.iter().any(|(text, _)| text.starts_with("Repeat: ")));
    if !baseline {
        assert!(
            texts
                .0
                .iter()
                .any(|(text, _)| text.starts_with("After naming an atom,"))
        );
    }
    repeated.last_label = remembered;
    let _ = repeated.update(Message::Tool(Tool::Bond(1)));
    let _ = repeated.update(Message::Tool(Tool::Text));
    let texts = ui.capture(&mut repeated, &directory, &format!("{prefix}-tool-cleared"));
    assert!(!texts.0.iter().any(|(text, _)| text.starts_with("Repeat: ")));

    let mut changed = app();
    let [_, end] = ethane(&mut changed, 0.);
    let _ = changed.update(Message::AtomText(atom_text::Action::Begin(Some(end))));
    let _ = changed.update(Message::AtomText(atom_text::Action::Input("CONMe2".into())));
    let _ = changed.update(Message::AtomText(atom_text::Action::Apply));
    changed.tab.selected.clear();
    changed
        .tab
        .inspector_ui
        .update(Action::Section(Section::Molecule, true));
    refresh(&mut changed).await;
    assert_eq!(changed.property_analysis().unwrap().formula, "C4H9NO");
    std::fs::write(
        directory.join("dimethylacetamide.rsk"),
        serde_json::to_vec_pretty(&changed.tab.doc).unwrap(),
    )
    .unwrap();
    ui.tree = Tree::empty();
    let texts = ui.capture(
        &mut changed,
        &directory,
        &format!("{prefix}-changed-status"),
    );
    assert!(contains(&texts, "Whole drawing · C4H9NO"));
    if !baseline {
        assert_eq!(changed.status, "Drawing changed");
        assert!(
            !texts
                .0
                .iter()
                .any(|(text, _)| text.contains("refresh properties"))
        );
    }

    let mut acetyl = app();
    acetyl.tool = Tool::Text;
    let [_, source] = ethane(&mut acetyl, -48.);
    let [_, target] = ethane(&mut acetyl, 48.);
    acetyl.tab.doc = reshiki::atom_text::apply(
        &acetyl.tab.doc,
        source,
        "COMe",
        reshiki::atom_text::Mode::Group,
    )
    .unwrap();
    std::fs::write(
        directory.join("acetyl-repeat.rsk"),
        serde_json::to_vec_pretty(&acetyl.tab.doc).unwrap(),
    )
    .unwrap();
    let _ = acetyl.update(Message::AtomText(atom_text::Action::Begin(Some(source))));
    let _ = acetyl.update(Message::AtomText(atom_text::Action::Input("Ac".into())));
    let _ = acetyl.update(Message::AtomText(atom_text::Action::Apply));
    let _ = acetyl.update(Message::AtomText(atom_text::Action::Repeat(target)));
    assert!(acetyl.tab.atom_text.is_none());
    acetyl.tab.selected.clear();
    acetyl.tool = Tool::Select;
    acetyl
        .tab
        .inspector_ui
        .update(Action::Section(Section::Molecule, true));
    refresh(&mut acetyl).await;
    if baseline {
        assert_eq!(acetyl.tab.doc.atom(target).unwrap().element, "Ac");
    } else {
        assert_eq!(acetyl.tab.doc.abbreviation(target).unwrap().label, "Ac");
        assert_eq!(acetyl.property_analysis().unwrap().formula, "C6H12O2");
        assert!(acetyl.tab.doc.atoms.iter().all(|atom| atom.element != "Ac"));
    }
    std::fs::write(
        directory.join(format!("{prefix}-acetyl-repeat.rsk")),
        serde_json::to_vec_pretty(&acetyl.tab.doc).unwrap(),
    )
    .unwrap();
    ui.tree = Tree::empty();
    ui.capture(&mut acetyl, &directory, &format!("{prefix}-acetyl-repeat"));
}
