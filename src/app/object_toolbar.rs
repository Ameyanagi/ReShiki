//! Align, distribute, order, flip and rotate at the right end of the Select context row.
use super::context_menu::{self, Page};
use super::{App, Message, workspace};
use iced::widget::{canvas, row, text, tooltip};
use iced::{Alignment, Element};
use reshiki::editing::{Arrange, Transform};

const MENU_BUTTON: f32 = 38.;
const ICON_BUTTON: f32 = 30.;
const GAP: f32 = 2.;
/// Width of the full group: three menu buttons, a divider and three icon buttons.
pub(super) const GROUP_WIDTH: f32 = 3. * MENU_BUTTON + 11. + 3. * ICON_BUTTON + 6. * GAP;
/// Width of the collapsed `Arrange ▾` button.
pub(super) const COMPACT_WIDTH: f32 = 80.;

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Visible(bool),
    Layer(bool),
}

#[derive(Clone, Copy)]
pub(super) enum Command {
    Layer(bool),
    Align(Arrange),
    Reflect(bool),
    Rotate,
}

impl Command {
    pub(super) const HORIZONTAL: [Self; 3] = [
        Self::Align(Arrange::AlignLeft),
        Self::Align(Arrange::AlignHorizontal),
        Self::Align(Arrange::AlignRight),
    ];
    pub(super) const VERTICAL: [Self; 3] = [
        Self::Align(Arrange::AlignTop),
        Self::Align(Arrange::AlignVertical),
        Self::Align(Arrange::AlignBottom),
    ];
    pub(super) const DISTRIBUTE: [Self; 2] = [
        Self::Align(Arrange::DistributeHorizontal),
        Self::Align(Arrange::DistributeVertical),
    ];
    pub(super) const ORDER: [Self; 2] = [Self::Layer(true), Self::Layer(false)];
    pub(super) const TRANSFORM: [Self; 3] =
        [Self::Reflect(true), Self::Reflect(false), Self::Rotate];

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Layer(true) => "Bring to front",
            Self::Layer(false) => "Send to back",
            Self::Align(Arrange::AlignLeft) => "Align left edges",
            Self::Align(Arrange::AlignHorizontal) => "Align horizontal centers",
            Self::Align(Arrange::AlignRight) => "Align right edges",
            Self::Align(Arrange::AlignTop) => "Align top edges",
            Self::Align(Arrange::AlignVertical) => "Align vertical centers",
            Self::Align(Arrange::AlignBottom) => "Align bottom edges",
            Self::Align(Arrange::DistributeHorizontal) => "Distribute horizontally",
            Self::Align(Arrange::DistributeVertical) => "Distribute vertically",
            Self::Reflect(true) => "Flip horizontal",
            Self::Reflect(false) => "Flip vertical",
            Self::Rotate => "Rotate 180°",
        }
    }

    /// Why the command is unavailable, for its tooltip.
    pub(super) fn requirement(self) -> &'static str {
        match self {
            Self::Layer(_) => "Select graphics or bonds",
            Self::Align(Arrange::DistributeHorizontal | Arrange::DistributeVertical) => {
                "Select at least three objects"
            }
            Self::Align(_) => "Select at least two objects",
            Self::Reflect(_) | Self::Rotate => "Select objects first",
        }
    }

    /// Disabled menu row standing in for the command's whole group.
    pub(super) fn unavailable(self) -> &'static str {
        match self {
            Self::Layer(_) => "Order needs graphics or bonds",
            Self::Align(Arrange::DistributeHorizontal | Arrange::DistributeVertical) => {
                "Distribute needs 3 objects"
            }
            Self::Align(_) => "Align needs 2 objects",
            Self::Reflect(_) | Self::Rotate => "Flip and rotate need a selection",
        }
    }

    pub(super) fn message(self) -> Message {
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

    pub(super) fn enabled(self, app: &App, objects: usize) -> bool {
        if app.tab.cleanup.is_some() || app.tab.joining.is_some() {
            return false;
        }
        match self {
            Self::Layer(_) => {
                app.tab
                    .doc
                    .graphics
                    .iter()
                    .any(|g| app.tab.selected.contains(&g.id))
                    || app
                        .tab
                        .doc
                        .bonds
                        .iter()
                        .any(|b| app.tab.selected.contains(&b.a) && app.tab.selected.contains(&b.b))
            }
            Self::Align(Arrange::DistributeHorizontal | Arrange::DistributeVertical) => {
                objects >= 3
            }
            Self::Align(_) => objects >= 2,
            Self::Reflect(_) | Self::Rotate => !app.tab.selected.is_empty(),
        }
    }
}

impl App {
    pub(super) fn object_toolbar_action(&mut self, action: Action) {
        match action {
            Action::Visible(visible) => {
                self.appearance.arrange_controls = visible;
                if let Err(error) = self.appearance.save() {
                    self.status = format!("Could not save arrange controls preference: {error}");
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
        let before = self.tab.doc.clone();
        if graphics {
            let edge = if front {
                self.tab
                    .doc
                    .graphics
                    .iter()
                    .map(|g| g.layer)
                    .max()
                    .unwrap_or(0)
                    .max(0)
                    .saturating_add(1)
            } else {
                self.tab
                    .doc
                    .graphics
                    .iter()
                    .map(|g| g.layer)
                    .min()
                    .unwrap_or(0)
                    .min(0)
                    .saturating_sub(1)
            };
            for graphic in &mut self.tab.doc.graphics {
                if self.tab.selected.contains(&graphic.id) {
                    graphic.layer = edge;
                }
            }
        }
        if bonds {
            let edge = if front {
                self.tab
                    .doc
                    .bonds
                    .iter()
                    .map(|b| b.z_order)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1)
            } else {
                self.tab
                    .doc
                    .bonds
                    .iter()
                    .map(|b| b.z_order)
                    .min()
                    .unwrap_or(0)
                    .min(-1)
                    .saturating_sub(1)
            };
            for bond in &mut self.tab.doc.bonds {
                if self.tab.selected.contains(&bond.a) && self.tab.selected.contains(&bond.b) {
                    bond.z_order = edge;
                }
            }
        }
        self.changed(before);
    }

    /// Arrange group for the context row. `left` is its x offset over the
    /// canvas, which anchors the menus under their buttons.
    pub(super) fn arrange_group(&self, left: f32, compact: bool) -> Element<'_, Message> {
        let objects = self.alignment_count();
        let open = |page, x| Message::ContextMenu(context_menu::Action::Open(page, x));
        if compact {
            // Every arrange command needs a selection.
            let enabled = !self.tab.selected.is_empty();
            return self.menu_anchor(
                Page::Arrange,
                reshiki::accessibility::button(
                    "arrange-compact",
                    "Arrange: align, distribute, order, flip and rotate",
                    row![text("Arrange").size(12), workspace::caret(9.)]
                        .spacing(5)
                        .align_y(Alignment::Center),
                )
                .width(COMPACT_WIDTH)
                .padding([7, 9])
                .style(workspace::control(false))
                .on_press_maybe(enabled.then(|| open(Page::Arrange, left))),
                if enabled {
                    "Align, distribute, order, flip and rotate"
                } else {
                    "Arrange · Select objects first"
                },
            );
        }
        // Each menu's commands share one availability rule; `icon` stands for them.
        let menu = |icon: Command, page, name: &str, index: f32| {
            let enabled = icon.enabled(self, objects);
            self.menu_anchor(
                page,
                reshiki::accessibility::button(
                    format!("arrange-menu-{page:?}"),
                    name,
                    row![
                        canvas(Glyph(icon, enabled)).width(24).height(24),
                        workspace::caret(9.)
                    ]
                    .spacing(1)
                    .align_y(Alignment::Center),
                )
                .width(MENU_BUTTON)
                .padding([3, 3])
                .style(workspace::control(false))
                .on_press_maybe(enabled.then(|| open(page, left + index * (MENU_BUTTON + GAP)))),
                if enabled {
                    name.to_owned()
                } else {
                    format!("{name} · {}", icon.requirement())
                },
            )
        };
        let mut group = row![
            menu(
                Command::Align(Arrange::AlignLeft),
                Page::AlignObjects,
                "Align",
                0.
            ),
            menu(
                Command::Align(Arrange::DistributeHorizontal),
                Page::Distribute,
                "Distribute",
                1.
            ),
            menu(Command::Layer(true), Page::Order, "Order", 2.),
            workspace::divider(),
        ]
        .spacing(GAP)
        .align_y(Alignment::Center);
        for command in Command::TRANSFORM {
            let enabled = command.enabled(self, objects);
            let keys = super::shortcuts::label(&command.message()).filter(|_| enabled);
            group = group.push(workspace::hover_keys(
                reshiki::accessibility::button(
                    match command {
                        Command::Reflect(true) => "arrange-flip-horizontal",
                        Command::Reflect(false) => "arrange-flip-vertical",
                        _ => "arrange-rotate",
                    },
                    command.name(),
                    canvas(Glyph(command, enabled)).width(24).height(24),
                )
                .width(ICON_BUTTON)
                .padding(3)
                .on_press_maybe(enabled.then(|| command.message()))
                .style(workspace::control(false)),
                &match (enabled, command) {
                    (false, _) => format!("{} · {}", command.name(), command.requirement()),
                    (true, Command::Reflect(_)) => {
                        format!("{}\nPreserves stereochemistry", command.name())
                    }
                    _ => command.name().to_owned(),
                },
                keys,
                tooltip::Position::Bottom,
            ));
        }
        group.into()
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
        app.tab.doc = Document::default();
        for (x, y, element) in [(40., 40., "O"), (190., 100., "N"), (360., 65., "Cl")] {
            let a = app.tab.doc.add_atom("C", Point::new(x, y));
            let b = app.tab.doc.add_atom(element, Point::new(x + 40., y));
            app.tab.doc.add_bond(a, b, 1, "plain");
        }
        app.tab.selected = app.tab.doc.all_ids();
        app
    }

    #[test]
    fn controls_count_molecules_and_groups_and_require_supported_layers() {
        let mut app = fixture();
        assert_eq!(app.alignment_count(), 3);
        assert!(Command::Align(Arrange::DistributeHorizontal).enabled(&app, app.alignment_count()));
        app.tab.selected.truncate(4);
        assert_eq!(app.alignment_count(), 2);
        assert!(Command::Align(Arrange::AlignLeft).enabled(&app, app.alignment_count()));
        assert!(
            !Command::Align(Arrange::DistributeHorizontal).enabled(&app, app.alignment_count())
        );
        app.tab.selected.truncate(2);
        assert_eq!(app.alignment_count(), 1);
        assert!(!Command::Align(Arrange::AlignLeft).enabled(&app, app.alignment_count()));
        assert!(Command::Layer(true).enabled(&app, 1));
        app.tab.selected.truncate(1);
        assert!(!Command::Layer(true).enabled(&app, 1));
        assert!(Command::Rotate.enabled(&app, 1));
        app.tab.selected.clear();
        assert!(!Command::Reflect(true).enabled(&app, 0));
        assert!(!Command::Rotate.enabled(&app, 0));

        let mut grouped = fixture();
        grouped
            .tab
            .doc
            .group_selection(&grouped.tab.selected[..4])
            .unwrap();
        assert_eq!(grouped.alignment_count(), 2);
        assert!(Command::Align(Arrange::AlignLeft).enabled(&grouped, grouped.alignment_count()));
        assert!(
            !Command::Align(Arrange::DistributeHorizontal)
                .enabled(&grouped, grouped.alignment_count())
        );
        grouped
            .tab
            .doc
            .group_selection(&grouped.tab.selected)
            .unwrap();
        assert_eq!(grouped.alignment_count(), 1);
        assert!(!Command::Align(Arrange::AlignLeft).enabled(&grouped, grouped.alignment_count()));
    }

    #[test]
    fn arrange_setting_roundtrips_and_ignores_the_retired_toolbar_key() {
        let mut app = fixture();
        let before = app.tab.doc.clone();
        for visible in [false, true] {
            let _ = app.update(Message::ObjectToolbar(Action::Visible(visible)));
            assert_eq!(app.appearance.arrange_controls, visible);
            assert_eq!(app.tab.doc, before);
            assert!(!app.tab.history.can_undo());
            let bytes = serde_json::to_vec(&app.appearance).unwrap();
            let reloaded: crate::appearance::Settings = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(reloaded.arrange_controls, visible);
        }
        for legacy in [
            r#"{"mode":"dark"}"#,
            r#"{"mode":"dark","object_toolbar":false}"#,
        ] {
            let legacy: crate::appearance::Settings = serde_json::from_str(legacy).unwrap();
            assert!(legacy.arrange_controls);
            assert_eq!(legacy.mode, crate::appearance::Mode::Dark);
        }
    }

    #[test]
    fn mixed_layers_are_one_undo_step_and_survive_save_reopen() {
        let mut app = fixture();
        let id = app.tab.doc.next_id();
        app.tab.doc.graphics.push(Graphic::dragged(
            id,
            GraphicKind::Rectangle,
            Point::new(10., 10.),
            Point::new(100., 100.),
            GraphicStyle::default(),
            BracketSides::Both,
            false,
        ));
        app.tab.selected = vec![app.tab.doc.atoms[0].id, app.tab.doc.atoms[1].id, id];
        let before = app.tab.doc.clone();
        let _ = app.update(Command::Layer(true).message());
        assert_eq!(app.tab.doc.graphics[0].layer, 1);
        assert_eq!(app.tab.doc.bonds[0].z_order, 1);
        assert_eq!(app.tab.doc.bonds[1].z_order, 0);
        let after = app.tab.doc.clone();
        let reopened: Document =
            serde_json::from_slice(&serde_json::to_vec(&after).unwrap()).unwrap();
        assert_eq!(reopened, after);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);
        let _ = app.update(Command::Layer(false).message());
        assert_eq!(app.tab.doc.graphics[0].layer, -1);
        assert_eq!(app.tab.doc.bonds[0].z_order, -2);
    }

    #[test]
    fn bond_depth_changes_only_bonds() {
        let mut app = fixture();
        let id = app.tab.doc.next_id();
        app.tab.doc.graphics.push(Graphic::dragged(
            id,
            GraphicKind::Rectangle,
            Point::new(10., 10.),
            Point::new(100., 100.),
            GraphicStyle::default(),
            BracketSides::Both,
            false,
        ));
        app.tab.selected.push(id);
        let graphics = app.tab.doc.graphics.clone();
        let _ = app.update(Message::BondDepth(true));
        assert!(app.tab.doc.bonds.iter().all(|b| b.z_order == 1));
        assert_eq!(app.tab.doc.graphics, graphics);
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
            let before = app.tab.doc.clone();
            let _ = app.update(command.message());
            assert_ne!(app.tab.doc, before);
            let after = app.tab.doc.clone();
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            let _ = app.update(Message::Redo);
            assert_eq!(app.tab.doc, after);
        }
    }

    #[tokio::test]
    #[ignore = "Opt-in renderer layout and pointer checks"]
    async fn arrange_buttons_match_their_menu_anchors_in_a_fixed_height_row() {
        use iced::advanced::{Layout, Shell, clipboard, layout, mouse, renderer::Headless};
        use iced::{Event, Rectangle, Size};
        let renderer = <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            None,
        )
        .await
        .unwrap();
        // Canvas widths at 1040 and 1280 with the default inspector.
        for (width, selected) in [(636., true), (636., false), (876., true)] {
            let mut app = fixture();
            if !selected {
                app.tab.selected.clear();
            }
            let size = Size::new(width, 80.);
            let left = 14. + (width - 28.) - GROUP_WIDTH;
            let click = |x: f32| {
                let mut bar = app.context_bar();
                let mut tree = iced::advanced::widget::Tree::new(bar.as_widget());
                let node = bar.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(Size::ZERO, size),
                );
                assert_eq!(node.size().height, 46., "Row height is fixed");
                let mut messages = Vec::new();
                for event in [mouse::Event::ButtonPressed, mouse::Event::ButtonReleased] {
                    bar.as_widget_mut().update(
                        &mut tree,
                        &Event::Mouse(event(mouse::Button::Left)),
                        Layout::new(&node),
                        mouse::Cursor::Available(iced::Point::new(x, 23.)),
                        &renderer,
                        &mut clipboard::Null,
                        &mut Shell::new(&mut messages),
                        &Rectangle::with_size(size),
                    );
                }
                messages
            };
            let align = click(left + MENU_BUTTON / 2.);
            let flip = click(left + 3. * (MENU_BUTTON + GAP) + 11. + GAP + ICON_BUTTON / 2.);
            if !selected {
                assert!(
                    align.is_empty() && flip.is_empty(),
                    "Disabled without a selection"
                );
                continue;
            }
            assert!(matches!(
                align.as_slice(),
                [Message::ContextMenu(context_menu::Action::Open(Page::AlignObjects, x))]
                    if (x - left).abs() < 0.5
            ));
            assert!(matches!(
                flip.as_slice(),
                [Message::Transform(Transform::FlipHorizontal)]
            ));
        }
    }
}
