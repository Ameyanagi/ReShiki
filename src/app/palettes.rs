//! Compact visual flyouts for toolbar families.
use super::{App, Message};
use crate::canvas::layered::canvas;
use crate::canvas::{PalettePreview, Tool};
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, row, stack, text, tooltip,
};
use iced::{Alignment, Border, Color, Element, Length};
use reshiki::{
    arrows::{ArrowStyle, Preset as ArrowPreset},
    bonds::BondPreset,
    document::{Arrow, Document, Point},
    graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, LinePattern},
    rings::Preset as RingPreset,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Atoms,
    Bonds,
    Rings,
    Arrows,
    Rectangles,
    Ellipses,
    Brackets,
    Symbols,
    Orbitals,
}
#[derive(Debug, Clone)]
pub enum Action {
    Open(Tool),
    Close,
    Atom(String),
    Bond(BondPreset),
    Ring(u8, bool),
    RingPreset(RingPreset),
    Arrow(ArrowPreset),
    ArrowVariant(ArrowPreset, ArrowStyle),
    Graphic(GraphicOption),
    Tool(Tool),
}
#[derive(Debug, Clone, PartialEq)]
pub struct GraphicOption {
    pub kind: GraphicKind,
    pub style: GraphicStyle,
    pub sides: BracketSides,
    pub constrain: bool,
}
impl GraphicOption {
    fn new(kind: GraphicKind) -> Self {
        Self {
            kind,
            style: GraphicStyle::default(),
            sides: BracketSides::Both,
            constrain: false,
        }
    }
    fn document(&self) -> Document {
        let mut doc = Document::default();
        doc.graphics.push(Graphic::dragged(
            1,
            self.kind,
            Point::default(),
            Point::new(64., 42.),
            self.style.clone(),
            self.sides,
            self.constrain,
        ));
        doc
    }
}
pub(super) struct Memory {
    pub bond: Tool,
    pub ring: Tool,
    pub rectangle: GraphicOption,
    pub ellipse: GraphicOption,
    pub bracket: GraphicOption,
    pub symbol: Tool,
    pub orbital: Tool,
}
impl Default for Memory {
    fn default() -> Self {
        Self {
            bond: Tool::Wedge,
            ring: Tool::Ring,
            rectangle: GraphicOption::new(GraphicKind::Rectangle),
            ellipse: GraphicOption::new(GraphicKind::Ellipse),
            bracket: GraphicOption::new(GraphicKind::Brackets),
            symbol: Tool::Graphic(GraphicKind::Symbol(
                reshiki::scientific::SymbolKind::CirclePlus,
            )),
            orbital: Tool::Graphic(GraphicKind::Orbital(reshiki::scientific::OrbitalKind::P)),
        }
    }
}
impl Memory {
    pub fn graphic(&self, tool: Tool) -> Option<&GraphicOption> {
        match family(tool) {
            Some(Family::Rectangles) => Some(&self.rectangle),
            Some(Family::Ellipses) => Some(&self.ellipse),
            Some(Family::Brackets) => Some(&self.bracket),
            _ => None,
        }
    }
    pub fn remember(&mut self, tool: Tool) {
        match family(tool) {
            Some(Family::Bonds) => self.bond = tool,
            Some(Family::Rings) => self.ring = tool,
            Some(Family::Symbols) => self.symbol = tool,
            Some(Family::Orbitals) => self.orbital = tool,
            Some(Family::Rectangles | Family::Ellipses | Family::Brackets) => {
                if let Tool::Graphic(kind) = tool {
                    match family(tool) {
                        Some(Family::Rectangles) => self.rectangle.kind = kind,
                        Some(Family::Ellipses) => self.ellipse.kind = kind,
                        _ => self.bracket.kind = kind,
                    }
                }
            }
            _ => {}
        }
    }
}
pub fn family(tool: Tool) -> Option<Family> {
    match tool {
        Tool::Bond(_) => None,
        Tool::StyledBond(_) | Tool::Wedge | Tool::Hash | Tool::Wavy => Some(Family::Bonds),
        Tool::Atom => Some(Family::Atoms),
        Tool::Ring | Tool::RingPreset(_) => Some(Family::Rings),
        Tool::Arrow => Some(Family::Arrows),
        Tool::Graphic(GraphicKind::Rectangle | GraphicKind::RoundedRectangle) => {
            Some(Family::Rectangles)
        }
        Tool::Graphic(GraphicKind::Ellipse) => Some(Family::Ellipses),
        Tool::Graphic(GraphicKind::Brackets | GraphicKind::Parentheses | GraphicKind::Braces) => {
            Some(Family::Brackets)
        }
        Tool::Graphic(GraphicKind::Symbol(_)) => Some(Family::Symbols),
        Tool::Graphic(GraphicKind::Orbital(_)) => Some(Family::Orbitals),
        _ => None,
    }
}
fn bond_tool(preset: BondPreset) -> Tool {
    match preset {
        BondPreset::Single => Tool::Bond(1),
        BondPreset::Double => Tool::Bond(2),
        BondPreset::Triple => Tool::Bond(3),
        BondPreset::Wedge => Tool::Wedge,
        BondPreset::HashedWedge => Tool::Hash,
        BondPreset::Wavy => Tool::Wavy,
        _ => Tool::StyledBond(preset),
    }
}
fn graphic_options(family: Family) -> Vec<(String, GraphicOption)> {
    use GraphicKind as G;
    let mut options = Vec::new();
    let kinds: &[G] = match family {
        Family::Rectangles => &[G::Rectangle, G::RoundedRectangle],
        Family::Ellipses => &[G::Ellipse],
        Family::Brackets => &[G::Brackets, G::Parentheses, G::Braces],
        _ => &[],
    };
    for &kind in kinds {
        if family == Family::Brackets {
            for sides in [BracketSides::Both, BracketSides::Left, BracketSides::Right] {
                let mut option = GraphicOption::new(kind);
                option.sides = sides;
                options.push((format!("{kind} · {sides}"), option));
            }
        } else {
            for (name, pattern, filled) in [
                ("Outline", LinePattern::Solid, false),
                ("Dashed", LinePattern::Dashed, false),
                ("Filled", LinePattern::Solid, true),
            ] {
                let mut option = GraphicOption::new(kind);
                option.style.pattern = pattern;
                option.style.fill = filled.then_some([0; 3]);
                options.push((format!("{name} {kind}"), option));
            }
        }
    }
    if matches!(family, Family::Rectangles | Family::Ellipses) {
        let mut option = GraphicOption::new(if family == Family::Rectangles {
            G::Rectangle
        } else {
            G::Ellipse
        });
        option.constrain = true;
        options.push((
            if family == Family::Rectangles {
                "Square"
            } else {
                "Circle"
            }
            .into(),
            option,
        ));
    }
    options
}
impl App {
    pub(super) fn palette_action(&mut self, action: Action) -> iced::Task<Message> {
        match action {
            Action::Open(tool) => {
                self.assistant.menu = None;
                let chosen = family(tool);
                self.palette = if self.palette == chosen { None } else { chosen };
            }
            Action::Tool(tool) => {
                self.palette = None;
                return self.update(Message::Tool(tool));
            }
            Action::Graphic(option) => {
                self.palette = None;
                let tool = Tool::Graphic(option.kind);
                match family(tool) {
                    Some(Family::Rectangles) => self.toolbar.rectangle = option,
                    Some(Family::Ellipses) => self.toolbar.ellipse = option,
                    Some(Family::Brackets) => self.toolbar.bracket = option,
                    _ => {}
                }
                return self.update(Message::Tool(tool));
            }
            Action::Close => self.palette = None,
            Action::Atom(element) => {
                self.palette = None;
                return self.update(Message::Element(element));
            }
            Action::Bond(preset) => {
                self.palette = None;
                return self.update(Message::Tool(bond_tool(preset)));
            }
            Action::Ring(size, aromatic) => {
                self.palette = None;
                self.ring_size = size;
                self.aromatic_ring = aromatic;
                self.toolbar.ring = Tool::Ring;
                self.tool = Tool::Ring;
            }
            Action::RingPreset(preset) => {
                self.palette = None;
                return self.update(Message::Tool(Tool::RingPreset(preset)));
            }
            Action::Arrow(preset) => {
                return self
                    .palette_action(Action::ArrowVariant(preset, ArrowStyle::preset(preset)));
            }
            Action::ArrowVariant(preset, style) => {
                self.selected.clear();
                self.palette = None;
                self.arrow_style = preset;
                self.arrows.style = style;
                self.arrows.refresh_inputs();
                return self.update(Message::Tool(Tool::Arrow));
            }
        }
        iced::Task::none()
    }
    pub(super) fn with_palette<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let Some(family) = self.palette else {
            return base;
        };
        let title = match family {
            Family::Atoms => "Choose an element",
            Family::Bonds => "Other bonds",
            Family::Rings => "Rings",
            Family::Arrows => "Reaction & electron-flow arrows",
            Family::Rectangles => "Rectangles",
            Family::Ellipses => "Ellipses & circles",
            Family::Brackets => "Brackets",
            Family::Symbols => "Chemical symbols",
            Family::Orbitals => "Orbitals",
        };
        let mut body = column![
            row![
                text(title).size(14),
                Space::new().width(Length::Fill),
                button(text("×").size(20))
                    .style(button::text)
                    .on_press(Message::Palette(Action::Close))
            ]
            .align_y(Alignment::Center)
        ]
        .spacing(8);
        match family {
            Family::Atoms => {
                // Positions follow the 18 periodic-table groups. f-blocks are separate.
                let rows = [
                    "H . . . . . . . . . . . . . . . . He",
                    "Li Be . . . . . . . . . . B C N O F Ne",
                    "Na Mg . . . . . . . . . . Al Si P S Cl Ar",
                    "K Ca Sc Ti V Cr Mn Fe Co Ni Cu Zn Ga Ge As Se Br Kr",
                    "Rb Sr Y Zr Nb Mo Tc Ru Rh Pd Ag Cd In Sn Sb Te I Xe",
                    "Cs Ba La Hf Ta W Re Os Ir Pt Au Hg Tl Pb Bi Po At Rn",
                    "Fr Ra Ac Rf Db Sg Bh Hs Mt Ds Rg Cn Nh Fl Mc Lv Ts Og",
                    ". . Ce Pr Nd Pm Sm Eu Gd Tb Dy Ho Er Tm Yb Lu . .",
                    ". . Th Pa U Np Pu Am Cm Bk Cf Es Fm Md No Lr . .",
                ];
                for symbols in rows {
                    let mut line = row![].spacing(2);
                    for symbol in symbols.split_whitespace() {
                        if symbol == "." {
                            line = line.push(Space::new().width(29).height(29));
                        } else {
                            let number = reshiki::editing::ELEMENTS
                                .iter()
                                .position(|e| *e == symbol)
                                .map(|n| n + 1)
                                .unwrap_or(0);
                            line = line.push(super::workspace::hover_hint(
                                button(text(symbol).size(12).center())
                                    .width(29)
                                    .height(29)
                                    .padding(1)
                                    .style(super::workspace::control(self.element == symbol))
                                    .on_press(Message::Palette(Action::Atom(symbol.into()))),
                                format!("{symbol} · Atomic number {number}"),
                                tooltip::Position::Bottom,
                            ));
                        }
                    }
                    body = body.push(line);
                }
                body = body.push(text("Choose an element, then click an atom to replace it or empty space to add it.").size(11));
            }
            Family::Bonds => {
                let presets: Vec<_> = BondPreset::ALL
                    .iter()
                    .copied()
                    .filter(|p| {
                        !matches!(
                            p,
                            BondPreset::Single | BondPreset::Double | BondPreset::Triple
                        )
                    })
                    .collect();
                for presets in presets.chunks(4) {
                    let mut line = row![].spacing(8);
                    for preset in presets {
                        let mut doc = Document::default();
                        let a = doc.add_atom("C", Point::new(0., 16.));
                        let b = doc.add_atom("C", Point::new(60., -16.));
                        let (order, display, _) = preset.parts();
                        doc.add_bond(a, b, order, display);
                        if let Some(bond) = doc.bonds.first_mut() {
                            preset.apply(bond);
                        }
                        line = line.push(super::workspace::hover_hint(
                            button(
                                column![
                                    canvas(PalettePreview(doc)).width(68).height(42),
                                    text(preset.name()).size(10).center().width(68)
                                ]
                                .align_x(Alignment::Center),
                            )
                            .padding(4)
                            .style(super::workspace::control(
                                self.tool.bond_preset() == Some(*preset),
                            ))
                            .on_press(Message::Palette(Action::Bond(*preset))),
                            preset.name(),
                            tooltip::Position::Bottom,
                        ));
                    }
                    body = body.push(line);
                }
                body = body.push(text("Choose a style, then draw or click an existing bond. Single, double and triple bonds also have direct toolbar buttons.").size(11));
            }
            Family::Rings => {
                let mut options = Vec::new();
                for size in 3..=8 {
                    let mut doc = Document::default();
                    reshiki::editing::ring(&mut doc, Point::default(), size, false, 42.);
                    options.push((doc, format!("{size}-membered"), Action::Ring(size, false)));
                }
                let mut aromatic = Document::default();
                reshiki::editing::ring(&mut aromatic, Point::default(), 6, true, 42.);
                options.push((aromatic, "Benzene".into(), Action::Ring(6, true)));
                for p in [
                    RingPreset::ChairUp,
                    RingPreset::ChairDown,
                    RingPreset::Cyclopentadiene,
                ] {
                    options.push((p.document(42., false), p.to_string(), Action::RingPreset(p)));
                }
                for group in options.chunks(4) {
                    let mut line = row![].spacing(8);
                    for (doc, label, action) in group {
                        line = line.push(super::workspace::hover_hint(
                            button(
                                column![
                                    canvas(PalettePreview(doc.clone())).width(68).height(48),
                                    text(label.clone()).size(10).center().width(68)
                                ]
                                .align_x(Alignment::Center),
                            )
                            .padding(4)
                            .style(super::workspace::control(match action {
                                Action::Ring(size, aromatic) => {
                                    self.tool == Tool::Ring
                                        && self.ring_size == *size
                                        && self.aromatic_ring == *aromatic
                                }
                                Action::RingPreset(preset) => {
                                    self.tool == Tool::RingPreset(*preset)
                                }
                                _ => false,
                            }))
                            .on_press(Message::Palette(action.clone())),
                            label.clone(),
                            tooltip::Position::Bottom,
                        ));
                    }
                    body = body.push(line);
                }
                body = body.push(text("Choose a ring, then click an atom or bond to attach. Templates offer more structures.").size(11));
            }
            Family::Arrows => {
                let mut options: Vec<_> = ArrowPreset::ALL
                    .iter()
                    .map(|&preset| (preset.to_string(), preset, ArrowStyle::preset(preset)))
                    .collect();
                for (label, preset, style) in [
                    (
                        "Bold",
                        ArrowPreset::Forward,
                        ArrowStyle {
                            width_pt: 1.4,
                            head_length_pt: 7.,
                            head_width_pt: 2.4,
                            ..ArrowStyle::default()
                        },
                    ),
                    (
                        "Dashed",
                        ArrowPreset::Forward,
                        ArrowStyle {
                            pattern: LinePattern::Dashed,
                            ..ArrowStyle::default()
                        },
                    ),
                    (
                        "Hollow",
                        ArrowPreset::Forward,
                        ArrowStyle {
                            shape: reshiki::arrows::HeadShape::Hollow,
                            head_length_pt: 6.,
                            head_width_pt: 2.,
                            ..ArrowStyle::default()
                        },
                    ),
                    (
                        "Unequal equilibrium",
                        ArrowPreset::Equilibrium,
                        ArrowStyle {
                            equilibrium_ratio: 0.6,
                            ..ArrowStyle::preset(ArrowPreset::Equilibrium)
                        },
                    ),
                    (
                        "Angled",
                        ArrowPreset::Forward,
                        ArrowStyle {
                            shape: reshiki::arrows::HeadShape::Open,
                            ..ArrowStyle::default()
                        },
                    ),
                    (
                        "Half arrow",
                        ArrowPreset::Forward,
                        ArrowStyle {
                            head: reshiki::arrows::Head::Left,
                            shape: reshiki::arrows::HeadShape::Open,
                            ..ArrowStyle::default()
                        },
                    ),
                ] {
                    options.push((label.into(), preset, style));
                }
                for presets in options.chunks(3) {
                    let mut line = row![].spacing(8);
                    for (label, preset, style) in presets {
                        let arrow = Arrow::new(
                            1,
                            Point::new(0., 0.),
                            Point::new(80., 0.),
                            *preset,
                            style.clone(),
                        );
                        line = line.push(super::workspace::hover_hint(
                            button(
                                column![
                                    canvas(PalettePreview(Document {
                                        arrows: vec![arrow],
                                        ..Document::default()
                                    }))
                                    .width(94)
                                    .height(50),
                                    text(label.clone()).size(10).center().width(94)
                                ]
                                .align_x(Alignment::Center),
                            )
                            .padding(6)
                            .style(super::workspace::control(
                                self.arrow_style == *preset && self.arrows.style == *style,
                            ))
                            .on_press(Message::Palette(
                                if style == &ArrowStyle::preset(*preset) {
                                    Action::Arrow(*preset)
                                } else {
                                    Action::ArrowVariant(*preset, style.clone())
                                },
                            )),
                            label.clone(),
                            tooltip::Position::Bottom,
                        ));
                    }
                    body = body.push(line);
                }
                body = body.push(text("Click to place a rightward arrow, or drag to set its length and direction. Drag the middle handle to bend it.").size(11));
            }
            Family::Rectangles | Family::Ellipses | Family::Brackets => {
                for options in graphic_options(family).chunks(3) {
                    let mut line = row![].spacing(8);
                    for (label, option) in options {
                        line = line.push(
                            button(
                                column![
                                    canvas(PalettePreview(option.document()))
                                        .width(94)
                                        .height(50),
                                    text(label.clone()).size(10).width(94).center(),
                                ]
                                .align_x(Alignment::Center),
                            )
                            .padding(6)
                            .style(super::workspace::control(
                                self.tool == Tool::Graphic(option.kind)
                                    && self.toolbar.graphic(self.tool) == Some(option),
                            ))
                            .on_press(Message::Palette(Action::Graphic(option.clone()))),
                        );
                    }
                    body = body.push(line);
                }
                body = body.push(
                    text(
                        "Choose a style, then drag to draw. Hold the toolbar button to change it.",
                    )
                    .size(11),
                );
            }
            Family::Symbols | Family::Orbitals => {
                let tools: Vec<Tool> = match family {
                    Family::Symbols => reshiki::scientific::SymbolKind::ALL
                        .iter()
                        .map(|k| Tool::Graphic(GraphicKind::Symbol(*k)))
                        .collect(),
                    _ => reshiki::scientific::OrbitalKind::ALL
                        .iter()
                        .map(|k| Tool::Graphic(GraphicKind::Orbital(*k)))
                        .collect(),
                };
                for choices in tools.chunks(3) {
                    let mut line = row![].spacing(8);
                    for &tool in choices {
                        let label = match tool {
                            Tool::Graphic(kind) => kind.to_string(),
                            _ => String::new(),
                        };
                        line = line.push(
                            button(
                                column![
                                    canvas(super::icons::Glyph(
                                        super::icons::Icon::Tool(tool),
                                        true
                                    ))
                                    .width(24)
                                    .height(24),
                                    text(label.clone()).size(10).width(94).center(),
                                ]
                                .align_x(Alignment::Center),
                            )
                            .padding(6)
                            .style(super::workspace::control(self.tool == tool))
                            .on_press(Message::Palette(Action::Tool(tool))),
                        );
                    }
                    body = body.push(line);
                }
            }
        }
        let popup = container(body)
            .width(if family == Family::Atoms { 590 } else { 360 })
            .padding(14)
            .style(|_| container::Style {
                background: Some(Color::WHITE.into()),
                border: Border {
                    color: Color::from_rgb8(192, 204, 201),
                    width: 1.,
                    radius: 10.into(),
                },
                shadow: iced::Shadow {
                    color: Color::from_rgba8(20, 40, 35, 0.18),
                    offset: iced::Vector::new(0., 5.),
                    blur_radius: 18.,
                },
                ..Default::default()
            });
        stack![
            base,
            mouse_area(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
            )
            .on_press(Message::Palette(Action::Close)),
            container(opaque(popup)).padding(iced::Padding {
                top: 146.,
                right: 12.,
                bottom: 12.,
                left: 112.
            })
        ]
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remembered_tools_and_shape_presets_do_not_change_selected_objects() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::Palette(Action::RingPreset(RingPreset::ChairDown)));
        let _ = app.update(Message::Tool(Tool::Bond(2)));
        assert_eq!(app.toolbar.ring, Tool::RingPreset(RingPreset::ChairDown));
        let _ = app.update(Message::Tool(app.toolbar.ring));
        assert!(app.palette.is_none());
        assert_eq!(app.tool, Tool::RingPreset(RingPreset::ChairDown));
        let circle = graphic_options(Family::Ellipses)
            .into_iter()
            .find(|(label, _)| label == "Circle")
            .unwrap()
            .1;
        let _ = app.update(Message::Palette(Action::Graphic(circle)));
        app.edit(crate::canvas::Edit::Graphic(
            Point::default(),
            Point::new(100., 30.),
            true,
        ));
        let drawing = app.doc.clone();
        let g = &app.doc.graphics[0];
        assert!(
            (g.axis_x.distance(Point::default()) - g.axis_y.distance(Point::default())).abs()
                < 0.001
        );
        let filled = graphic_options(Family::Rectangles)
            .into_iter()
            .find(|(_, option)| option.style.fill.is_some())
            .unwrap()
            .1;
        let _ = app.update(Message::Palette(Action::Graphic(filled)));
        assert_eq!(app.doc, drawing);
        assert!(app.graphic_style.fill.is_some());
        let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::Ellipse)));
        assert!(app.toolbar.ellipse.constrain);
        assert!(app.graphic_style.fill.is_none());
        for family in [
            Family::Rectangles,
            Family::Ellipses,
            Family::Brackets,
            Family::Arrows,
            Family::Symbols,
            Family::Orbitals,
        ] {
            app.palette = Some(family);
            let _ = app.view();
        }
    }
    #[test]
    fn an_eraser_drag_is_one_undo_step_and_empty_strokes_leave_history_unchanged() {
        use crate::canvas::Edit;
        let (mut app, _) = App::new();
        app.tool = Tool::Erase;
        let a = app.doc.add_atom("C", Point::new(0., 0.));
        app.doc.add_atom("O", Point::new(0., 50.));
        let keep = app.doc.add_atom("N", Point::new(50., 50.));
        let original = app.doc.clone();
        app.edit(Edit::EraseStart(Point::new(0., -20.)));
        app.edit(Edit::EraseTo(Point::new(0., -20.), Point::new(0., 20.)));
        assert!(app.doc.atom(a).is_none());
        app.edit(Edit::EraseTo(Point::new(0., 20.), Point::new(0., 80.)));
        app.edit(Edit::EraseEnd);
        let erased = app.doc.clone();
        assert_eq!(erased.atoms.len(), 1);
        assert!(erased.atom(keep).is_some());
        assert!(app.history.undo(&mut app.doc));
        assert_eq!(app.doc, original);
        assert!(!app.history.can_undo());
        assert!(app.history.redo(&mut app.doc));
        assert_eq!(app.doc, erased);
        app.edit(Edit::EraseStart(Point::new(1000., 1000.)));
        app.edit(Edit::EraseEnd);
        assert!(app.history.undo(&mut app.doc));
        assert_eq!(app.doc, original);
    }
    #[test]
    fn toolbar_flyouts_choose_tools_without_mutating_the_drawing() {
        let (mut app, _) = App::new();
        app.doc.arrows.push(Arrow::new(
            1,
            Point::default(),
            Point::new(80., 0.),
            ArrowPreset::Forward,
            ArrowStyle::default(),
        ));
        app.selected = vec![1];
        let original = app.doc.clone();
        for tool in [Tool::Atom, Tool::Wedge, Tool::Ring, Tool::Arrow] {
            let _ = app.update(Message::Palette(Action::Open(tool)));
            assert!(app.palette.is_some());
            let _ = app.view();
            let _ = app.update(Message::Tool(Tool::Select));
            assert!(app.palette.is_none());
            assert_eq!(app.doc, original);
        }
        let _ = app.update(Message::Palette(Action::Atom("Br".into())));
        assert_eq!(app.element, "Br");
        let _ = app.update(Message::Palette(Action::Bond(BondPreset::HollowWedge)));
        assert_eq!(app.tool.bond_preset(), Some(BondPreset::HollowWedge));
        let _ = app.update(Message::Palette(Action::Ring(7, false)));
        assert_eq!(app.ring_size, 7);
        let _ = app.update(Message::Palette(Action::Arrow(ArrowPreset::Bent)));
        assert_eq!(app.arrow_style, ArrowPreset::Bent);
        assert_eq!(app.doc, original);
    }
    #[test]
    fn undo_during_an_eraser_drag_does_not_merge_later_motion_into_older_edits() {
        use crate::canvas::Edit;
        let (mut app, _) = App::new();
        let original = app.doc.clone();
        app.doc.add_atom("C", Point::default());
        app.changed(original.clone());
        app.tool = Tool::Erase;
        app.edit(Edit::EraseStart(Point::default()));
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc.atoms.len(), 1);
        app.edit(Edit::EraseTo(Point::default(), Point::new(10., 0.)));
        app.edit(Edit::EraseEnd);
        assert_eq!(app.doc.atoms.len(), 1);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
    }
    #[test]
    fn crossing_depth_changes_are_undoable_and_preserve_chemistry() {
        let (mut app, _) = App::new();
        let a = app.doc.add_atom("C", Point::default());
        let b = app.doc.add_atom("C", Point::new(42., 0.));
        app.doc.add_bond(a, b, 1, "plain");
        app.selected = vec![a, b];
        let before = app.doc.clone();
        let _ = app.update(Message::BondDepth(true));
        assert_eq!(app.doc.bonds[0].z_order, 1);
        assert!(!super::super::chemistry_changed(&before, &app.doc));
        assert!(app.history.undo(&mut app.doc));
        assert_eq!(app.doc, before);
    }
}
