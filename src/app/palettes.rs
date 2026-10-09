//! Compact visual flyouts for toolbar families.
use super::{App, InspectorTab, Message};
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
                option.style.fill = filled.then_some(reshiki::palette::Color::Ink);
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

// Hover hints show one key that selects this tool on empty, unselected canvas.
// Contextual atom/bond actions belong in Help, not in tool-selection hints.
pub(super) fn element_hint(symbol: &str) -> String {
    use iced::keyboard::Modifiers;
    let shift = |key| super::shortcuts::keys(Modifiers::SHIFT, key);
    let key = match symbol {
        "C" => "c".into(),
        "N" => "n".into(),
        "O" => "o".into(),
        "S" => "s".into(),
        "P" => "p".into(),
        "F" => "f".into(),
        "H" => "h".into(),
        "B" => shift("B"),
        "Cl" => shift("C"),
        "I" => "i".into(),
        "Li" => shift("L"),
        "Si" => shift("S"),
        _ => return symbol.into(),
    };
    format!("{symbol} · {key}")
}

pub(super) fn bond_hint(preset: BondPreset, label: &str) -> String {
    let key = match preset {
        BondPreset::Single => "1",
        BondPreset::Double => "2",
        BondPreset::Triple => "3",
        BondPreset::Quadruple => "4",
        _ => return label.into(),
    };
    format!("{label} · {key}")
}

pub(super) fn ring_hint(label: &str, action: &Action) -> String {
    use iced::keyboard::Modifiers;
    let key = match action {
        Action::Ring(size, false) => {
            let message = Message::Shortcut(super::shortcuts::Action::SelectRing(*size));
            let Some(key) = super::shortcuts::label(&message) else {
                return label.into();
            };
            key
        }
        Action::RingPreset(RingPreset::Benzene) => "j".into(),
        Action::RingPreset(RingPreset::Cyclopentadiene) => {
            super::shortcuts::keys(Modifiers::SHIFT, "J")
        }
        _ => return label.into(),
    };
    format!("{label} · {key}")
}

pub(super) fn graphic_hint(kind: GraphicKind, label: &str) -> String {
    use iced::keyboard::Modifiers;
    let key = match kind {
        GraphicKind::Brackets => "T",
        GraphicKind::Symbol(reshiki::scientific::SymbolKind::CirclePlus) => "E",
        GraphicKind::Orbital(reshiki::scientific::OrbitalKind::P) => "G",
        _ => return label.into(),
    };
    format!(
        "{label} · {}",
        super::shortcuts::keys(Modifiers::SHIFT, key)
    )
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
                self.tab.selected.clear();
                self.palette = None;
                self.tab.arrow_style = preset;
                self.tab.arrows.style = style;
                self.tab
                    .arrows
                    .refresh_inputs(&reshiki::palette::Palette::of(&self.tab.doc));
                return self.update(Message::Tool(Tool::Arrow));
            }
        }
        iced::Task::none()
    }
    pub(super) fn with_palette<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let Some(family) = self.palette else {
            return base;
        };
        let base = reshiki::accessibility::inert(base);
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
        body = match family {
            Family::Atoms => self.atom_palette(body),
            Family::Bonds => self.bond_palette(body),
            Family::Rings => self.ring_palette(body),
            Family::Arrows => self.arrow_palette(body),
            Family::Rectangles | Family::Ellipses | Family::Brackets => {
                self.graphic_palette(body, family)
            }
            Family::Symbols | Family::Orbitals => self.scientific_palette(body, family),
        };
        let popup = container(body)
            .width(if family == Family::Atoms { 590 } else { 360 })
            .padding(14)
            .style(|theme| {
                crate::appearance::container(
                    theme,
                    container::Style {
                        background: Some(Color::WHITE.into()),
                        border: Border {
                            color: Color::from_rgb8(192, 204, 201),
                            width: 1.,
                            radius: 10.into(),
                        },
                        shadow: crate::appearance::surface_shadow(iced::Shadow {
                            color: Color::from_rgba8(20, 40, 35, 0.18),
                            offset: iced::Vector::new(0., 5.),
                            blur_radius: 18.,
                        }),
                        ..Default::default()
                    },
                )
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
    fn atom_palette<'a>(
        &'a self,
        mut body: iced::widget::Column<'a, Message>,
    ) -> iced::widget::Column<'a, Message> {
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
                    line = line.push(super::workspace::hover_hint(
                        button(text(symbol).size(12).center())
                            .width(29)
                            .height(29)
                            .padding(1)
                            .style(super::workspace::element_control(
                                self.element == symbol,
                                &self.tab.doc,
                                symbol,
                            ))
                            .on_press(Message::Palette(Action::Atom(symbol.into()))),
                        element_hint(symbol),
                        tooltip::Position::Bottom,
                    ));
                }
            }
            body = body.push(line);
        }
        body = body.push(
            text("Choose an element, then click an atom to replace it or empty space to add it.")
                .size(11),
        );
        body
    }
    fn bond_palette<'a>(
        &'a self,
        mut body: iced::widget::Column<'a, Message>,
    ) -> iced::widget::Column<'a, Message> {
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
                    bond_hint(*preset, preset.name()),
                    tooltip::Position::Bottom,
                ));
            }
            body = body.push(line);
        }
        body = body.push(text("Choose a style, then draw or click an existing bond. Single, double and triple bonds also have direct toolbar buttons.").size(11));
        body
    }
    fn ring_palette<'a>(
        &'a self,
        mut body: iced::widget::Column<'a, Message>,
    ) -> iced::widget::Column<'a, Message> {
        let mut options = Vec::new();
        for size in 3..=8 {
            let mut doc = Document::default();
            reshiki::editing::ring(&mut doc, Point::default(), size, false, 42.);
            options.push((doc, format!("{size}-membered"), Action::Ring(size, false)));
        }
        let mut aromatic = Document::default();
        reshiki::editing::ring(&mut aromatic, Point::default(), 6, true, 42.);
        options.push((
            RingPreset::Benzene.document(42., false),
            "Benzene".into(),
            Action::RingPreset(RingPreset::Benzene),
        ));
        options.push((aromatic, "Aromatic circle".into(), Action::Ring(6, true)));
        for p in [
            RingPreset::ChairUp,
            RingPreset::ChairDown,
            RingPreset::Cyclopentadiene,
            RingPreset::HaworthFive,
            RingPreset::HaworthSix,
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
                        Action::RingPreset(preset) => self.tool == Tool::RingPreset(*preset),
                        _ => false,
                    }))
                    .on_press(Message::Palette(action.clone())),
                    ring_hint(label, action),
                    tooltip::Position::Bottom,
                ));
            }
            body = body.push(line);
        }
        body = body.push(text("Choose a ring, then click an atom or bond to attach. Templates offer more structures.").size(11));
        body
    }
    fn arrow_palette<'a>(
        &'a self,
        mut body: iced::widget::Column<'a, Message>,
    ) -> iced::widget::Column<'a, Message> {
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
                        self.tab.arrow_style == *preset && self.tab.arrows.style == *style,
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
        body = body.push(text("Click to place or change an arrow. Click the same type again to switch direction or half-head side. Drag to draw; drag the middle handle to bend.").size(11));
        body
    }
    fn graphic_palette<'a>(
        &'a self,
        mut body: iced::widget::Column<'a, Message>,
        family: Family,
    ) -> iced::widget::Column<'a, Message> {
        for options in graphic_options(family).chunks(3) {
            let mut line = row![].spacing(8);
            for (label, option) in options {
                line = line.push(super::workspace::hover_hint(
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
                    // T keeps the remembered bracket style and sides;
                    // it cannot choose this specific palette variant.
                    label.clone(),
                    tooltip::Position::Bottom,
                ));
            }
            body = body.push(line);
        }
        body = body.push(
            text("Choose a style, then drag to draw. Hold the toolbar button to change it.")
                .size(11),
        );
        body
    }
    fn scientific_palette<'a>(
        &'a self,
        mut body: iced::widget::Column<'a, Message>,
        family: Family,
    ) -> iced::widget::Column<'a, Message> {
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
                line = line.push(super::workspace::hover_hint(
                    button(
                        column![
                            canvas(super::icons::Glyph(super::icons::Icon::Tool(tool), true))
                                .width(24)
                                .height(24),
                            text(label.clone()).size(10).width(94).center(),
                        ]
                        .align_x(Alignment::Center),
                    )
                    .padding(6)
                    .style(super::workspace::control(self.tool == tool))
                    .on_press(Message::Palette(Action::Tool(tool))),
                    if let Tool::Graphic(kind) = tool {
                        graphic_hint(kind, &label)
                    } else {
                        label
                    },
                    tooltip::Position::Bottom,
                ));
            }
            body = body.push(line);
        }
        body
    }
    pub(super) fn select_tool(&mut self, tool: Tool) {
        self.tab.arrow_source = None;
        self.tab.erase_stroke = false;
        self.palette = None;
        self.toolbar.remember(tool);
        if let Some(option) = self.toolbar.graphic(tool) {
            self.tab.graphic_style = option.style.clone();
            self.tab.bracket_sides = option.sides;
            self.tab.graphic_width_input = self.tab.graphic_style.width_pt.to_string();
        }
        self.tool = tool;
        self.error = false;
        if matches!(tool, Tool::Graphic(_) | Tool::RingPreset(_)) {
            self.tab.selected.clear();
        }
        if matches!(
            tool,
            Tool::Arrow | Tool::Graphic(_) | Tool::EditPoints | Tool::RingPreset(_)
        ) {
            self.inspector_open = true;
            self.inspector_tab = InspectorTab::Properties;
        }
        if matches!(tool, Tool::Graphic(_)) {
            self.sync_graphics();
        }
    }
}

#[cfg(test)]
mod tests;
