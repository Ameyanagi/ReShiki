//! Optional, selection-independent access to arrangement and transform commands.
use super::{App, Message, workspace};
use iced::widget::{Space, button, canvas, container, row, scrollable, text, tooltip};
use iced::{Alignment, Element, Length};
use reshiki::editing::{Arrange, Transform};

const HEIGHT: f32 = 42.;

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Visible(bool),
    Layer(bool),
}

#[derive(Clone, Copy)]
enum Command {
    Layer(bool),
    Align(Arrange),
    Reflect(bool),
    Rotate,
}

impl Command {
    fn label(self) -> &'static str {
        match self {
            Self::Layer(true) => "Bring selected graphics and bonds to front",
            Self::Layer(false) => "Send selected graphics and bonds to back",
            Self::Align(Arrange::AlignLeft) => "Align left edges · Select at least two objects",
            Self::Align(Arrange::AlignHorizontal) => {
                "Align horizontal centers · Select at least two objects"
            }
            Self::Align(Arrange::AlignRight) => "Align right edges · Select at least two objects",
            Self::Align(Arrange::AlignTop) => "Align top edges · Select at least two objects",
            Self::Align(Arrange::AlignVertical) => {
                "Align vertical centers · Select at least two objects"
            }
            Self::Align(Arrange::AlignBottom) => "Align bottom edges · Select at least two objects",
            Self::Align(Arrange::DistributeHorizontal) => {
                "Distribute horizontally · Select at least three objects"
            }
            Self::Align(Arrange::DistributeVertical) => {
                "Distribute vertically · Select at least three objects"
            }
            Self::Reflect(true) => "Flip horizontal · Preserve stereochemistry",
            Self::Reflect(false) => "Flip vertical · Preserve stereochemistry",
            Self::Rotate => "Rotate selection 180°",
        }
    }

    fn message(self) -> Message {
        match self {
            Self::Layer(front) => Message::ObjectToolbar(Action::Layer(front)),
            Self::Align(action) => Message::Arrange(action),
            Self::Reflect(horizontal) => Message::Transform(if horizontal {
                Transform::FlipHorizontal
            } else {
                Transform::FlipVertical
            }),
            Self::Rotate => Message::Transform(Transform::Rotate(180.)),
        }
    }

    fn enabled(self, app: &App, objects: usize) -> bool {
        if app.cleanup.is_some() || app.joining.is_some() {
            return false;
        }
        match self {
            Self::Layer(_) => {
                app.doc
                    .graphics
                    .iter()
                    .any(|g| app.selected.contains(&g.id))
                    || app
                        .doc
                        .bonds
                        .iter()
                        .any(|b| app.selected.contains(&b.a) && app.selected.contains(&b.b))
            }
            Self::Align(Arrange::DistributeHorizontal | Arrange::DistributeVertical) => {
                objects >= 3
            }
            Self::Align(_) => objects >= 2,
            Self::Reflect(_) | Self::Rotate => !app.selected.is_empty(),
        }
    }
}

impl App {
    pub(super) fn object_toolbar_action(&mut self, action: Action) {
        match action {
            Action::Visible(visible) => {
                self.appearance.object_toolbar = visible;
                if let Err(error) = self.appearance.save() {
                    self.status = format!("Could not save toolbar preference: {error}");
                    self.error = true;
                }
            }
            Action::Layer(front) => self.layer_objects(front, true, true),
        }
    }

    /// Graphics and bond depth are distinct drawing layers. A mixed toolbar
    /// selection changes both in one history step, using the existing commands'
    /// ordering rules. Text and arrows have no editable stacking order.
    pub(super) fn layer_objects(&mut self, front: bool, graphics: bool, bonds: bool) {
        let before = self.doc.clone();
        if graphics {
            let edge = if front {
                self.doc
                    .graphics
                    .iter()
                    .map(|g| g.layer)
                    .max()
                    .unwrap_or(0)
                    .max(0)
                    .saturating_add(1)
            } else {
                self.doc
                    .graphics
                    .iter()
                    .map(|g| g.layer)
                    .min()
                    .unwrap_or(0)
                    .min(0)
                    .saturating_sub(1)
            };
            for graphic in &mut self.doc.graphics {
                if self.selected.contains(&graphic.id) {
                    graphic.layer = edge;
                }
            }
        }
        if bonds {
            let edge = if front {
                self.doc
                    .bonds
                    .iter()
                    .map(|b| b.z_order)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1)
            } else {
                self.doc
                    .bonds
                    .iter()
                    .map(|b| b.z_order)
                    .min()
                    .unwrap_or(0)
                    .min(-1)
                    .saturating_sub(1)
            };
            for bond in &mut self.doc.bonds {
                if self.selected.contains(&bond.a) && self.selected.contains(&bond.b) {
                    bond.z_order = edge;
                }
            }
        }
        self.changed(before);
    }

    pub(super) fn object_toolbar(&self) -> Element<'_, Message> {
        let objects = self.alignment_count();
        let mut commands = row![text("Objects").size(11).style(workspace::muted_text)]
            .spacing(3)
            .align_y(Alignment::Center);
        for (index, command) in [
            Command::Layer(true),
            Command::Layer(false),
            Command::Align(Arrange::AlignLeft),
            Command::Align(Arrange::AlignHorizontal),
            Command::Align(Arrange::AlignRight),
            Command::Align(Arrange::AlignTop),
            Command::Align(Arrange::AlignVertical),
            Command::Align(Arrange::AlignBottom),
            Command::Align(Arrange::DistributeHorizontal),
            Command::Align(Arrange::DistributeVertical),
            Command::Reflect(true),
            Command::Reflect(false),
            Command::Rotate,
        ]
        .into_iter()
        .enumerate()
        {
            if [2, 8, 10].contains(&index) {
                commands = commands.push(Space::new().width(8));
            }
            let enabled = command.enabled(self, objects);
            commands = commands.push(workspace::hover_hint(
                button(canvas(Glyph(command, enabled)).width(24).height(24))
                    .padding(3)
                    .on_press_maybe(enabled.then(|| command.message()))
                    .style(workspace::control(false)),
                command.label(),
                tooltip::Position::Bottom,
            ));
        }
        let strip = scrollable(commands)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new().width(3).scroller_width(3),
            ))
            .width(Length::Fill);
        container(
            row![
                strip,
                workspace::hover_hint(
                    button(text("×").size(18))
                        .padding([0, 7])
                        .on_press(Message::ObjectToolbar(Action::Visible(false)))
                        .style(workspace::control(false)),
                    "Hide object toolbar · Show again in View",
                    tooltip::Position::Bottom,
                )
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .height(HEIGHT)
        .padding([6, 10])
        .style(workspace::panel)
        .into()
    }
}

struct Glyph(Command, bool);
impl<Message> canvas::Program<Message> for Glyph {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &iced::Renderer,
        theme: &iced::Theme,
        bounds: iced::Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        use canvas::{Path, Stroke};
        use iced::{Point, Size};
        let mut frame =
            crate::canvas::layered::Frame::new(renderer, bounds.size()).with_theme(theme);
        let ink = if self.1 {
            iced::Color::from_rgb8(51, 62, 72)
        } else {
            iced::Color::from_rgb8(187, 193, 199)
        };
        let line = |frame: &mut crate::canvas::layered::Frame<'_>, points: &[(f32, f32)]| {
            frame.stroke(
                &Path::new(|p| {
                    if let Some((&(x, y), rest)) = points.split_first() {
                        p.move_to(Point::new(x, y));
                        for &(x, y) in rest {
                            p.line_to(Point::new(x, y));
                        }
                    }
                }),
                Stroke::default().with_width(1.5).with_color(ink),
            );
        };
        let rect = |frame: &mut crate::canvas::layered::Frame<'_>, x, y, w, h, filled| {
            let path = Path::rectangle(Point::new(x, y), Size::new(w, h));
            if filled {
                frame.fill(&path, ink);
            } else {
                frame.stroke(&path, Stroke::default().with_width(1.3).with_color(ink));
            }
        };
        match self.0 {
            Command::Layer(front) => {
                rect(&mut frame, 3., 3., 12., 12., !front);
                // Clear the overlap so the foreground rectangle reads as stacked.
                frame.fill(
                    &Path::rectangle(Point::new(9., 9.), Size::new(12., 12.)),
                    iced::Color::from_rgb8(250, 251, 252),
                );
                rect(&mut frame, 9., 9., 12., 12., front);
            }
            Command::Align(action) => {
                let transpose = matches!(
                    action,
                    Arrange::AlignTop
                        | Arrange::AlignVertical
                        | Arrange::AlignBottom
                        | Arrange::DistributeVertical
                );
                let map = |x, y| if transpose { (y, x) } else { (x, y) };
                let bar = |frame: &mut crate::canvas::layered::Frame<'_>, x, y, w, h| {
                    if transpose {
                        rect(frame, y, x, h, w, true);
                    } else {
                        rect(frame, x, y, w, h, true);
                    }
                };
                if matches!(
                    action,
                    Arrange::DistributeHorizontal | Arrange::DistributeVertical
                ) {
                    line(&mut frame, &[map(3., 2.), map(3., 22.)]);
                    line(&mut frame, &[map(21., 2.), map(21., 22.)]);
                    for (x, y, h) in [(3., 7., 10.), (10., 4., 16.), (18., 9., 6.)] {
                        bar(&mut frame, x, y, 3., h);
                    }
                } else {
                    let anchor = match action {
                        Arrange::AlignLeft | Arrange::AlignTop => 3.,
                        Arrange::AlignRight | Arrange::AlignBottom => 21.,
                        _ => 12.,
                    };
                    line(&mut frame, &[map(anchor, 2.), map(anchor, 22.)]);
                    for (y, w) in [(5., 13.), (14., 8.)] {
                        let x = if anchor == 3. {
                            4.
                        } else if anchor == 21. {
                            20. - w
                        } else {
                            12. - w / 2.
                        };
                        bar(&mut frame, x, y, w, 5.);
                    }
                }
            }
            Command::Reflect(horizontal) => {
                let map = |x, y| if horizontal { (x, y) } else { (y, x) };
                line(&mut frame, &[map(12., 2.), map(12., 22.)]);
                line(
                    &mut frame,
                    &[map(8., 5.), map(3., 18.), map(8., 18.), map(8., 5.)],
                );
                line(
                    &mut frame,
                    &[map(16., 5.), map(21., 18.), map(16., 18.), map(16., 5.)],
                );
            }
            Command::Rotate => {
                frame.stroke(
                    &Path::new(|p| {
                        p.move_to(Point::new(4., 14.));
                        p.bezier_curve_to(
                            Point::new(1., 2.),
                            Point::new(22., 2.),
                            Point::new(20., 14.),
                        );
                    }),
                    Stroke::default().with_width(1.5).with_color(ink),
                );
                line(&mut frame, &[(16., 11.), (20., 15.), (23., 10.)]);
                frame.fill_text(canvas::Text {
                    content: "180°".into(),
                    position: Point::new(2., 14.),
                    size: iced::Pixels(9.),
                    color: ink,
                    ..Default::default()
                });
            }
        }
        frame.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::document::{Document, Point};
    use reshiki::graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle};

    fn fixture() -> App {
        let (mut app, _) = App::new();
        app.doc = Document::default();
        for (x, y, element) in [(40., 40., "O"), (190., 100., "N"), (360., 65., "Cl")] {
            let a = app.doc.add_atom("C", Point::new(x, y));
            let b = app.doc.add_atom(element, Point::new(x + 40., y));
            app.doc.add_bond(a, b, 1, "plain");
        }
        app.selected = app.doc.all_ids();
        app.appearance.object_toolbar = true;
        app.inspector_open = false;
        app.status = "Object toolbar · Align, distribute, reflect and rotate the selection".into();
        app
    }

    #[test]
    fn controls_count_molecules_and_groups_and_require_supported_layers() {
        let mut app = fixture();
        assert_eq!(app.alignment_count(), 3);
        assert!(Command::Align(Arrange::DistributeHorizontal).enabled(&app, app.alignment_count()));
        app.selected.truncate(4);
        assert_eq!(app.alignment_count(), 2);
        assert!(Command::Align(Arrange::AlignLeft).enabled(&app, app.alignment_count()));
        assert!(
            !Command::Align(Arrange::DistributeHorizontal).enabled(&app, app.alignment_count())
        );
        app.selected.truncate(2);
        assert_eq!(app.alignment_count(), 1);
        assert!(!Command::Align(Arrange::AlignLeft).enabled(&app, app.alignment_count()));
        assert!(Command::Layer(true).enabled(&app, 1));
        app.selected.truncate(1);
        assert!(!Command::Layer(true).enabled(&app, 1));
        assert!(Command::Rotate.enabled(&app, 1));
        app.selected.clear();
        assert!(!Command::Reflect(true).enabled(&app, 0));
        assert!(!Command::Rotate.enabled(&app, 0));

        let mut grouped = fixture();
        grouped.doc.group_selection(&grouped.selected[..4]).unwrap();
        assert_eq!(grouped.alignment_count(), 2);
        assert!(Command::Align(Arrange::AlignLeft).enabled(&grouped, grouped.alignment_count()));
        assert!(
            !Command::Align(Arrange::DistributeHorizontal)
                .enabled(&grouped, grouped.alignment_count())
        );
        grouped.doc.group_selection(&grouped.selected).unwrap();
        assert_eq!(grouped.alignment_count(), 1);
        assert!(!Command::Align(Arrange::AlignLeft).enabled(&grouped, grouped.alignment_count()));
    }

    #[test]
    fn toolbar_setting_roundtrips_and_does_not_modify_the_drawing() {
        let mut app = fixture();
        let before = app.doc.clone();
        for visible in [false, true] {
            let _ = app.update(Message::ObjectToolbar(Action::Visible(visible)));
            assert_eq!(app.appearance.object_toolbar, visible);
            assert_eq!(app.doc, before);
            assert!(!app.history.can_undo());
            let bytes = serde_json::to_vec(&app.appearance).unwrap();
            let reloaded: crate::appearance::Settings = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(reloaded.object_toolbar, visible);
        }
        let legacy: crate::appearance::Settings =
            serde_json::from_str(r#"{"mode":"dark"}"#).unwrap();
        assert!(!legacy.object_toolbar);
        assert_eq!(legacy.mode, crate::appearance::Mode::Dark);
    }

    #[test]
    fn mixed_layers_are_one_undo_step_and_survive_save_reopen() {
        let mut app = fixture();
        let id = app.doc.next_id();
        app.doc.graphics.push(Graphic::dragged(
            id,
            GraphicKind::Rectangle,
            Point::new(10., 10.),
            Point::new(100., 100.),
            GraphicStyle::default(),
            BracketSides::Both,
            false,
        ));
        app.selected = vec![app.doc.atoms[0].id, app.doc.atoms[1].id, id];
        let before = app.doc.clone();
        let _ = app.update(Command::Layer(true).message());
        assert_eq!(app.doc.graphics[0].layer, 1);
        assert_eq!(app.doc.bonds[0].z_order, 1);
        assert_eq!(app.doc.bonds[1].z_order, 0);
        let after = app.doc.clone();
        let reopened: Document =
            serde_json::from_slice(&serde_json::to_vec(&after).unwrap()).unwrap();
        assert_eq!(reopened, after);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        assert!(!app.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, after);
        let _ = app.update(Command::Layer(false).message());
        assert_eq!(app.doc.graphics[0].layer, -1);
        assert_eq!(app.doc.bonds[0].z_order, -2);
    }

    #[test]
    fn existing_layer_commands_keep_their_individual_domains() {
        let mut app = fixture();
        let before = app.doc.clone();
        let _ = app.update(Message::GraphicLayer(true));
        assert_eq!(app.doc, before);
        let _ = app.update(Message::BondDepth(true));
        assert!(app.doc.bonds.iter().all(|b| b.z_order == 1));
    }

    #[test]
    fn toolbar_arrangement_and_transforms_use_standard_undo() {
        for command in [
            Command::Align(Arrange::AlignTop),
            Command::Align(Arrange::DistributeHorizontal),
            Command::Reflect(true),
            Command::Reflect(false),
            Command::Rotate,
        ] {
            let mut app = fixture();
            let before = app.doc.clone();
            let _ = app.update(command.message());
            assert_ne!(app.doc, before);
            let after = app.doc.clone();
            let _ = app.update(Message::Undo);
            assert_eq!(app.doc, before);
            let _ = app.update(Message::Redo);
            assert_eq!(app.doc, after);
        }
    }

    #[tokio::test]
    #[ignore = "Opt-in actual renderer evidence and pointer/layout checks"]
    async fn object_toolbar_headless_snapshot() {
        use iced::advanced::{layout, mouse, renderer::Headless, widget::Tree};
        let mut app = fixture();
        let directory = std::path::Path::new("artifacts/object-toolbar-qa");
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(
            directory.join("objects.rsk"),
            serde_json::to_vec_pretty(&app.doc).unwrap(),
        )
        .unwrap();
        for (name, width, height, selected, dark, settings) in [
            ("selected", 1280, 820, true, false, false),
            ("empty-selection", 1280, 820, false, false, false),
            ("compact", 1040, 680, true, false, true),
            ("dark", 1280, 820, true, true, false),
        ] {
            app.appearance.mode = if dark {
                crate::appearance::Mode::Dark
            } else {
                crate::appearance::Mode::Light
            };
            app.view_open = settings;
            app.selected = if selected { app.doc.all_ids() } else { vec![] };
            app.viewport = iced::Size::new(width as f32 - 70., height as f32 - 235.);
            app.fit();
            let mut renderer = <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .unwrap();
            // Toolbar height and hide control stay fixed when the selection changes.
            let toolbar_size = iced::Size::new(width as f32 - 430., HEIGHT);
            let mut toolbar = app.object_toolbar();
            let mut tree = Tree::new(toolbar.as_widget());
            let node = toolbar.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(toolbar_size, toolbar_size),
            );
            assert_eq!(node.size().height, HEIGHT);
            let mut command_messages = Vec::new();
            for event in [
                iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ] {
                toolbar.as_widget_mut().update(
                    &mut tree,
                    &event,
                    iced::advanced::Layout::new(&node),
                    mouse::Cursor::Available(iced::Point::new(66., HEIGHT / 2.)),
                    &renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut iced::advanced::Shell::new(&mut command_messages),
                    &iced::Rectangle::with_size(toolbar_size),
                );
            }
            assert_eq!(
                command_messages
                    .iter()
                    .any(|m| matches!(m, Message::ObjectToolbar(Action::Layer(true)))),
                selected
            );
            let cursor =
                mouse::Cursor::Available(iced::Point::new(toolbar_size.width - 23., HEIGHT / 2.));
            let mut messages = Vec::new();
            for event in [
                iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ] {
                toolbar.as_widget_mut().update(
                    &mut tree,
                    &event,
                    iced::advanced::Layout::new(&node),
                    cursor,
                    &renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut iced::advanced::Shell::new(&mut messages),
                    &iced::Rectangle::with_size(toolbar_size),
                );
            }
            assert!(
                messages
                    .iter()
                    .any(|m| matches!(m, Message::ObjectToolbar(Action::Visible(false))))
            );
            drop(toolbar);
            let size = iced::Size::new(width as f32, height as f32);
            let theme = app.theme();
            let mut view = app.view();
            let mut tree = Tree::new(view.as_widget());
            let node =
                view.as_widget_mut()
                    .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
            let mut messages = Vec::new();
            view.as_widget_mut().update(
                &mut tree,
                &iced::Event::Window(iced::window::Event::RedrawRequested(
                    std::time::Instant::now(),
                )),
                iced::advanced::Layout::new(&node),
                mouse::Cursor::Unavailable,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &iced::Rectangle::with_size(size),
            );
            view.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style::default(),
                iced::advanced::Layout::new(&node),
                mouse::Cursor::Unavailable,
                &iced::Rectangle::with_size(size),
            );
            let pixels = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.,
                theme.palette().background,
            );
            image::save_buffer(
                directory.join(format!("{name}.png")),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
}
