//! Compact visual flyouts for toolbar families.
use super::{App, Message};
use crate::canvas::layered::canvas;
use crate::canvas::{ArrowPreview, OwnedDrawingPreview, Tool};
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, row, stack, text, tooltip,
};
use iced::{Alignment, Border, Color, Element, Length};
use reshiki::{
    arrows::{ArrowStyle, Preset as ArrowPreset},
    bonds::BondPreset,
    document::{Arrow, Document, Point},
    rings::Preset as RingPreset,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Atoms,
    Bonds,
    Rings,
    Arrows,
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
}
pub fn family(tool: Tool) -> Option<Family> {
    if tool.bond_preset().is_some() {
        return Some(Family::Bonds);
    }
    match tool {
        Tool::Atom => Some(Family::Atoms),
        Tool::Ring | Tool::RingPreset(_) => Some(Family::Rings),
        Tool::Arrow => Some(Family::Arrows),
        _ => None,
    }
}
impl App {
    pub(super) fn palette_action(&mut self, action: Action) -> iced::Task<Message> {
        match action {
            Action::Open(tool) => {
                self.assistant.menu = None;
                let chosen = family(tool);
                self.palette = if self.palette == chosen && self.tool == tool {
                    None
                } else {
                    chosen
                };
                self.tool = tool;
            }
            Action::Close => self.palette = None,
            Action::Atom(element) => {
                self.palette = None;
                return self.update(Message::Element(element));
            }
            Action::Bond(preset) => {
                self.palette = None;
                self.tool = match preset {
                    BondPreset::Single => Tool::Bond(1),
                    BondPreset::Double => Tool::Bond(2),
                    BondPreset::Triple => Tool::Bond(3),
                    _ => Tool::StyledBond(preset),
                };
            }
            Action::Ring(size, aromatic) => {
                self.palette = None;
                self.ring_size = size;
                self.aromatic_ring = aromatic;
                self.tool = Tool::Ring;
            }
            Action::RingPreset(preset) => {
                self.palette = None;
                return self.update(Message::Tool(Tool::RingPreset(preset)));
            }
            Action::Arrow(preset) => {
                self.selected.clear();
                self.palette = None;
                self.tool = Tool::Arrow;
                return self.update(Message::ArrowStyle(preset));
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
            Family::Bonds => "Bond styles",
            Family::Rings => "Rings",
            Family::Arrows => "Reaction & electron-flow arrows",
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
                for presets in BondPreset::ALL.chunks(4) {
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
                                column![canvas(OwnedDrawingPreview(doc)).width(68).height(38)]
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
                body = body.push(text("Choose a style, then draw or click an existing bond. Repeated Double clicks shift its position.").size(11));
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
                                    canvas(OwnedDrawingPreview(doc.clone()))
                                        .width(68)
                                        .height(54)
                                ]
                                .align_x(Alignment::Center),
                            )
                            .padding(4)
                            .style(super::workspace::control(false))
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
                for presets in ArrowPreset::ALL.chunks(3) {
                    let mut line = row![].spacing(8);
                    for preset in presets {
                        let arrow = Arrow::new(
                            1,
                            Point::new(0., 0.),
                            Point::new(100., 0.),
                            *preset,
                            ArrowStyle::preset(*preset),
                        );
                        line = line.push(super::workspace::hover_hint(
                            button(
                                column![canvas(ArrowPreview { arrow }).width(94).height(52)]
                                    .align_x(Alignment::Center),
                            )
                            .padding(6)
                            .style(super::workspace::control(self.arrow_style == *preset))
                            .on_press(Message::Palette(Action::Arrow(*preset))),
                            preset.to_string(),
                            tooltip::Position::Bottom,
                        ));
                    }
                    body = body.push(line);
                }
                body = body.push(text("Drag to draw. Select the arrow and drag its middle handle to adjust the bend.").size(11));
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
    fn toolbar_flyouts_choose_tools_without_mutating_the_drawing() {
        let (mut app, _) = App::new();
        app.doc.arrows.push(Arrow::new(
            1,
            Point::default(),
            Point::new(100., 0.),
            ArrowPreset::Forward,
            ArrowStyle::default(),
        ));
        app.selected = vec![1];
        let original = app.doc.clone();
        for tool in [Tool::Atom, Tool::Bond(1), Tool::Ring, Tool::Arrow] {
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
