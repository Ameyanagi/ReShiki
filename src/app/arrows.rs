use super::*;
mod attachments;
use crate::canvas::layered::canvas;
use reshiki::arrows::{ArrowStyle, Body, Head, HeadShape, NoGo, Preset};
use reshiki::graphics::LinePattern;

/// The arrow card's one line of gesture help; the full text is in the guide.
const GESTURE_HINT: &str = "Drag the diamond to bend, the squares to aim each end";
const ARC_HINT: &str = "Drag the diamond to change the sweep";
const ATTACH_LABEL: &str = "Attach ends to atoms and bonds";

#[derive(Debug, Clone, Copy)]
pub enum Field {
    Line,
    Length,
    Width,
    Gap,
    Ratio,
    Notch,
}
impl Field {
    fn get(self, s: &ArrowStyle) -> f32 {
        match self {
            Self::Line => s.width_pt,
            Self::Length => s.head_length_pt,
            Self::Width => s.head_width_pt,
            Self::Gap => s.gap_pt,
            Self::Ratio => s.equilibrium_ratio,
            Self::Notch => s.head_notch,
        }
    }
    fn set(self, s: &mut ArrowStyle, v: f32) {
        match self {
            Self::Line => s.width_pt = v,
            Self::Length => s.head_length_pt = v,
            Self::Width => s.head_width_pt = v,
            Self::Gap => s.gap_pt = v,
            Self::Ratio => s.equilibrium_ratio = v,
            Self::Notch => s.head_notch = v,
        }
    }
}
const FIELDS: [Field; 6] = [
    Field::Line,
    Field::Length,
    Field::Width,
    Field::Gap,
    Field::Ratio,
    Field::Notch,
];
pub struct State {
    pub style: ArrowStyle,
    pub numbers: [String; 6],
    pub color: String,
    pub attach_targets: bool,
    /// The sweep new arrows take as arcs; None draws them straight.
    pub arc: Option<f32>,
    pub sweep: String,
    source: Option<reshiki::arrow_anchors::Pick>,
}
impl Default for State {
    fn default() -> Self {
        let style = ArrowStyle::default();
        Self {
            numbers: FIELDS.map(|f| f.get(&style).to_string()),
            style,
            color: "#000000".into(),
            attach_targets: true,
            arc: None,
            sweep: String::new(),
            source: None,
        }
    }
}
impl State {
    pub(super) fn begin_source(&mut self, pick: reshiki::arrow_anchors::Pick) {
        self.source = Some(pick);
    }

    pub(super) fn cancel_source(&mut self) {
        self.source = None;
    }

    pub(super) fn source(&self) -> Option<&reshiki::arrow_anchors::Pick> {
        self.source.as_ref()
    }

    pub(super) fn refresh_inputs(&mut self, palette: &reshiki::palette::Palette) {
        self.numbers = FIELDS.map(|f| f.get(&self.style).to_string());
        self.color = reshiki::palette::hex(palette.rgb(self.style.color));
    }
}
#[derive(Debug, Clone)]
pub enum Action {
    Head(Head),
    Tail(Head),
    Shape(HeadShape),
    Pattern(LinePattern),
    NoGo(NoGo),
    Dipole(bool),
    Body(Body),
    MakeArc,
    Sweep(String),
    ApplySweep,
    Number(Field, String),
    ApplyNumber(Field),
    Color(String),
    ApplyColor,
    AttachTargets(bool),
    DetachStart,
    DetachEnd,
    Reverse,
    Flip,
    Straighten,
    Reset,
    /// Change the selected interior node's type.
    NodeType(reshiki::graphics::PathNodeType),
    DeleteNode,
}
impl App {
    fn selected_arrow_anchors(&self) -> (bool, bool) {
        let mut start = false;
        let mut end = false;
        for arrow in self
            .tab
            .doc
            .arrows
            .iter()
            .filter(|arrow| self.tab.selected.contains(&arrow.id))
        {
            start |= arrow.start_anchor.is_some();
            end |= arrow.end_anchor.is_some();
            if start && end {
                break;
            }
        }
        (start, end)
    }

    pub(super) fn arrow_attach_mode(&self) -> bool {
        self.tab.arrows.attach_targets && self.tab.arrow_style.supports_attachments()
    }

    pub(super) fn place_or_restyle_arrow(&mut self, p: Point, hit: Option<u64>) {
        if let Some(id) = hit.filter(|id| self.tab.doc.arrows.iter().any(|a| a.id == *id)) {
            self.apply_arrow_tool(id);
        } else {
            let length = self.tab.doc.drawing_style.bond_length_world * 2.;
            self.place_arrow(p, p.offset(length, 0.));
        }
    }

    pub(super) fn apply_arrow_tool(&mut self, id: u64) {
        self.tab.arrows.cancel_source();
        let Some(arrow) = self.tab.doc.arrows.iter_mut().find(|a| a.id == id) else {
            return;
        };
        if arrow.apply_tool(self.tab.arrow_style, &self.tab.arrows.style) {
            for reaction in self.tab.doc.reactions.iter_mut().filter(|r| r.arrow == id) {
                std::mem::swap(&mut reaction.reactants, &mut reaction.products);
            }
        }
        self.tab.selected = vec![id];
        self.sync_arrows();
    }
    pub(super) fn sync_arrows(&mut self) {
        if let Some(a) = self
            .tab
            .doc
            .arrows
            .iter()
            .find(|a| self.tab.selected.contains(&a.id))
        {
            self.tab.arrow_style = Preset::from_kind(&a.kind);
            self.tab.arrows.style = a.appearance();
            self.tab.arrows.sweep = a.arc.map_or_else(String::new, |s| format!("{s:.0}"));
            if !matches!(
                self.inspector_tab,
                InspectorTab::Templates
                    | InspectorTab::Reactions
                    | InspectorTab::Assistant
                    | InspectorTab::Pages
            ) {
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Properties;
            }
        }
        self.tab
            .arrows
            .refresh_inputs(&reshiki::palette::Palette::of(&self.tab.doc));
    }
    pub(super) fn arrow_action(&mut self, action: Action) {
        match action {
            Action::AttachTargets(value) => {
                self.tab.arrows.attach_targets = value;
                self.tab.arrows.cancel_source();
                self.error = false;
                self.status = if value {
                    "Attached arrows · Click a source, then a destination"
                } else {
                    "Free arrows · Click or drag"
                }
                .into();
                return;
            }
            Action::Number(field, s) => {
                if let Some(value) = self.tab.arrows.numbers.get_mut(field as usize) {
                    *value = s;
                }
                return;
            }
            Action::Color(s) => {
                self.tab.arrows.color = s;
                return;
            }
            Action::Sweep(s) => {
                self.tab.arrows.sweep = s;
                return;
            }
            Action::MakeArc | Action::ApplySweep => {
                self.arrow_geometry_action(action);
                return;
            }
            Action::NodeType(_) | Action::DeleteNode => {
                let Some((id, index, _)) = self.selected_arrow_node() else {
                    self.status = "Select an arrow node".into();
                    self.error = true;
                    return;
                };
                let before = self.tab.doc.clone();
                let Some(a) = self.tab.doc.arrows.iter_mut().find(|a| a.id == id) else {
                    return;
                };
                let result = match action {
                    Action::NodeType(kind) => a.set_node_type(index, kind),
                    _ => a.delete_node(index),
                };
                if let Err(error) = result {
                    self.status = error;
                    self.error = true;
                    return;
                }
                if matches!(action, Action::DeleteNode) {
                    self.tab.path_point = None;
                }
                self.changed(before);
                return;
            }
            _ => {}
        }
        let change = |s: &mut ArrowStyle| -> Result<(), String> {
            match action {
                Action::Head(h) => s.head = h,
                Action::Tail(h) => s.tail = h,
                Action::Shape(v) => s.shape = v,
                Action::Pattern(v) => s.pattern = v,
                Action::NoGo(v) => s.no_go = v,
                Action::Dipole(v) => s.dipole = v,
                Action::Body(v) => s.set_body(v),
                Action::ApplyColor => {
                    s.color = reshiki::palette::Color::Custom(
                        graphics::parse_color(&self.tab.arrows.color)
                            .ok_or("Use a six-digit hex color, for example #205091")?,
                    )
                }
                Action::ApplyNumber(f) => f.set(
                    s,
                    self.tab
                        .arrows
                        .numbers
                        .get(f as usize)
                        .ok_or("Unknown arrow property")?
                        .parse::<f32>()
                        .map_err(|_| "Type a number, then press Enter")?,
                ),
                Action::Reset => {
                    *s = ArrowStyle::preset(self.tab.arrow_style);
                    s.width_pt = self.tab.doc.drawing_style.line_width_pt;
                }
                _ => {}
            }
            s.validate()
        };
        let mut style = self.tab.arrows.style.clone();
        if let Err(error) = change(&mut style) {
            self.status = error;
            self.error = true;
            return;
        }
        let mut next = self.tab.doc.clone();
        for a in next
            .arrows
            .iter_mut()
            .filter(|a| self.tab.selected.contains(&a.id))
        {
            match action {
                Action::DetachStart => a.start_anchor = None,
                Action::DetachEnd => a.end_anchor = None,
                Action::Reverse => a.reverse(),
                Action::Flip => a.flip_bend(),
                Action::Straighten => a.straighten(),
                _ => {
                    let mut appearance = a.appearance();
                    if let Err(error) = change(&mut appearance) {
                        self.status = error;
                        self.error = true;
                        return;
                    }
                    a.style = Some(appearance);
                }
            }
        }
        if matches!(action, Action::Reverse) {
            for reaction in &mut next.reactions {
                if self.tab.selected.contains(&reaction.arrow) {
                    std::mem::swap(&mut reaction.reactants, &mut reaction.products);
                }
            }
        }
        let before = std::mem::replace(&mut self.tab.doc, next);
        if matches!(action, Action::ApplyColor) {
            self.remember_custom(Some(style.color), &before);
        }
        self.tab.arrows.style = style;
        self.changed(before);
        self.sync_arrows();
        self.error = false;
        self.status = "Arrow updated · Drag the diamond to bend".into();
    }
    /// Arc sweeps belong to each selected arrow, not to its style.
    fn arrow_geometry_action(&mut self, action: Action) {
        let mut next = self.tab.doc.clone();
        let mut changed = 0;
        let sweep = match action {
            Action::ApplySweep => match self
                .tab
                .arrows
                .sweep
                .trim()
                .trim_end_matches('°')
                .parse::<f32>()
            {
                Ok(v)
                    if v.is_finite()
                        && (reshiki::arrows::MIN_SWEEP..=reshiki::arrows::MAX_SWEEP)
                            .contains(&v.abs()) =>
                {
                    Some(v)
                }
                _ => {
                    self.status = "Type a sweep of 1–359°; minus turns it the other way".into();
                    self.error = true;
                    return;
                }
            },
            _ => None,
        };
        for a in next
            .arrows
            .iter_mut()
            .filter(|a| self.tab.selected.contains(&a.id))
        {
            match action {
                Action::MakeArc if a.kind != "bent" && !a.is_attached() && !a.is_arc() => {
                    a.set_arc(180.);
                    // A node picked before is gone: the arc's nodes are its own.
                    self.tab.path_point = None;
                }
                Action::ApplySweep if a.is_arc() => a.set_arc(sweep.unwrap_or(180.)),
                _ => continue,
            }
            if let Err(error) = a.validate() {
                self.status = error;
                self.error = true;
                return;
            }
            changed += 1;
        }
        if changed == 0 {
            self.status = match action {
                Action::MakeArc => "Select a free arrow without a corner to make an arc",
                _ => "Select an arrow first",
            }
            .into();
            self.error = true;
            return;
        }
        let before = std::mem::replace(&mut self.tab.doc, next);
        self.changed(before);
        self.sync_arrows();
        self.error = false;
        self.status = match action {
            Action::MakeArc => "Arc arrow · Drag the diamond to change its sweep",
            _ => "Arrow updated",
        }
        .into();
    }
    pub(super) fn arrow_panel(&self) -> Element<'_, Message> {
        use super::workspace::{accessible_command, command, hover_hint, muted_text};
        use iced::widget::{Space, checkbox, column, container, row, text, tooltip};
        use iced::{Alignment, Length};
        let s = &self.tab.arrows.style;
        let selected_arrows = self
            .tab
            .doc
            .arrows
            .iter()
            .filter(|a| self.tab.selected.contains(&a.id))
            .count();
        let has_selection = selected_arrows > 0;
        let heading = match (selected_arrows, self.tab.selected.len()) {
            (0, _) => "Arrow tool".into(),
            (1, 1) => "Arrow properties".into(),
            (1, _) => "Arrow properties · 1 arrow".into(),
            (count, _) => format!("Arrow properties · {count} arrows"),
        };
        let number = |label: &'static str, field: Field| {
            let value = self
                .tab
                .arrows
                .numbers
                .get(field as usize)
                .map(String::as_str)
                .unwrap_or("");
            let input: Element<'_, Message> = if matches!(field, Field::Line) {
                reshiki::accessibility::text_input(
                    "arrow-line-width",
                    "Arrow line width (pt)",
                    "",
                    value,
                )
                .style(crate::appearance::input_style)
                .size(12)
                .padding(5)
                .width(70)
                .on_input(move |v| Message::ArrowAction(Action::Number(field, v)))
                .on_submit(Message::ArrowAction(Action::ApplyNumber(field)))
                .into()
            } else {
                crate::appearance::text_input("", value)
                    .size(12)
                    .padding(5)
                    .width(70)
                    .on_input(move |v| Message::ArrowAction(Action::Number(field, v)))
                    .on_submit(Message::ArrowAction(Action::ApplyNumber(field)))
                    .into()
            };
            row![
                text(label).size(11).width(Length::Fill),
                hover_hint(input, "Enter applies", tooltip::Position::Top),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
        };
        // One chemistry arrow: its roles, colours by origin and atom maps are
        // one click away instead of under Labels & chemistry.
        let roles = matches!(
            self.tab
                .doc
                .arrows
                .iter()
                .filter(|a| self.tab.selected.contains(&a.id))
                .collect::<Vec<_>>()
                .as_slice(),
            [arrow] if arrow.purpose.is_chemistry()
                && Preset::from_kind(&arrow.kind).reaction_role_refusal().is_none()
        );
        let header: Element<'_, Message> = if roles {
            row![
                container(super::workspace::section(&heading)).width(Length::Fill),
                accessible_command("arrow.reaction-roles", "Reaction roles", "Reaction roles…")
                    .on_press(Message::Reaction(super::reactions::Action::Open)),
            ]
            .align_y(Alignment::Center)
            .into()
        } else {
            super::workspace::section(&heading).into()
        };
        let mut panel = column![
            header,
            canvas(crate::canvas::ArrowPreview {
                arrow: self
                    .tab
                    .doc
                    .arrows
                    .iter()
                    .find(|a| self.tab.selected.contains(&a.id))
                    .cloned()
                    .unwrap_or_else(|| {
                        let mut arrow = Arrow::new(
                            1,
                            Point::new(-65., 0.),
                            Point::new(65., 0.),
                            self.tab.arrow_style,
                            s.clone(),
                        );
                        if let Some(sweep) = self.tab.arrows.arc {
                            arrow.set_arc(sweep);
                        }
                        arrow
                    })
            })
            .width(Length::Fill)
            .height(70),
            crate::appearance::pick_list(
                Preset::ALL,
                Some(self.tab.arrow_style),
                Message::ArrowStyle
            )
            .text_size(12)
            .padding(6)
            .width(Length::Fill),
            row![
                text("End").size(11).width(44),
                crate::appearance::pick_list(Head::ALL, Some(s.head), |v| Message::ArrowAction(
                    Action::Head(v)
                ))
                .text_size(12)
                .padding(5)
                .width(Length::Fill)
            ]
            .align_y(Alignment::Center),
            row![
                text("Start").size(11).width(44),
                crate::appearance::pick_list(Head::ALL, Some(s.tail), |v| Message::ArrowAction(
                    Action::Tail(v)
                ))
                .text_size(12)
                .padding(5)
                .width(Length::Fill)
            ]
            .align_y(Alignment::Center),
            row![
                crate::appearance::pick_list(LinePattern::ALL, Some(s.pattern), |v| {
                    Message::ArrowAction(Action::Pattern(v))
                })
                .text_size(12)
                .padding(5),
                hover_hint(
                    crate::appearance::text_input("#RRGGBB", &self.tab.arrows.color)
                        .on_input(|v| Message::ArrowAction(Action::Color(v)))
                        .on_submit(Message::ArrowAction(Action::ApplyColor))
                        .size(12)
                        .padding(5)
                        .width(Length::Fill),
                    "Hex color · Enter applies",
                    tooltip::Position::Top,
                )
            ]
            .spacing(6),
            number("Line width (pt)", Field::Line),
        ]
        .spacing(8);
        let selected: Vec<_> = self
            .tab
            .doc
            .arrows
            .iter()
            .filter(|a| self.tab.selected.contains(&a.id))
            .collect();
        if !matches!(
            self.tab.arrow_style,
            Preset::Equilibrium | Preset::Retro | Preset::Bent
        ) {
            panel = panel.push(
                row![
                    text("Body").size(11).width(44),
                    crate::appearance::pick_list(Body::ALL, Some(s.body), |v| {
                        Message::ArrowAction(Action::Body(v))
                    })
                    .text_size(12)
                    .padding(5)
                    .width(Length::Fill)
                ]
                .align_y(Alignment::Center),
            );
            if !s.body.is_line() {
                panel = panel.push(number("Body width (pt)", Field::Gap));
            }
        }
        if selected.first().is_some_and(|first| first.is_arc()) {
            panel = panel.push(
                row![
                    text("Sweep (°)").size(11).width(Length::Fill),
                    hover_hint(
                        crate::appearance::text_input("180", &self.tab.arrows.sweep)
                            .on_input(|v| Message::ArrowAction(Action::Sweep(v)))
                            .on_submit(Message::ArrowAction(Action::ApplySweep))
                            .size(12)
                            .padding(5)
                            .width(70),
                        "1–359 · Minus turns it the other way · Enter applies",
                        tooltip::Position::Top,
                    )
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }
        let mut geometry = column![
            text("Arrowhead shape").size(11).style(muted_text),
            crate::appearance::pick_list(HeadShape::ALL, Some(s.shape), |v| Message::ArrowAction(
                Action::Shape(v)
            ))
            .text_size(12)
            .padding(6)
            .width(Length::Fill),
            number("Head length (pt)", Field::Length),
            number("Head half-width (pt)", Field::Width),
            number("Head notch (0–0.9)", Field::Notch),
        ]
        .spacing(8);
        if self.tab.arrow_style == Preset::Equilibrium {
            geometry = geometry.push(number("Reverse length (0.2–5)", Field::Ratio));
        }
        if matches!(self.tab.arrow_style, Preset::Equilibrium | Preset::Retro) {
            geometry = geometry.push(number("Shaft separation (pt)", Field::Gap));
        }
        geometry = geometry
            .push(
                row![
                    text("No reaction").size(11),
                    crate::appearance::pick_list(NoGo::ALL, Some(s.no_go), |v| {
                        Message::ArrowAction(Action::NoGo(v))
                    })
                    .text_size(12)
                    .padding(5)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .push(
                checkbox(s.dipole)
                    .label("Dipole cross at start")
                    .on_toggle(|v| Message::ArrowAction(Action::Dipole(v)))
                    .size(14)
                    .text_size(12),
            );
        if self.tab.arrow_style.supports_attachments() {
            panel = panel.push(hover_hint(
                checkbox(self.tab.arrows.attach_targets)
                    .label(ATTACH_LABEL)
                    .on_toggle(|value| Message::ArrowAction(Action::AttachTargets(value)))
                    .size(14)
                    .text_size(12),
                format!(
                    "Two clicks: a source, then a destination atom, bond or lone pair · {}-click or turn this off for a free arrow",
                    crate::platform::alt_key()
                ),
                tooltip::Position::Top,
            ));
        }
        if has_selection {
            let (start, end) = self.selected_arrow_anchors();
            let action = |id, name, message| {
                accessible_command(id, name, name)
                    .width(Length::Fill)
                    .on_press(Message::ArrowAction(message))
            };
            let curved = self.tab.doc.arrows.iter().any(|arrow| {
                self.tab.selected.contains(&arrow.id) && arrow.bezier_controls().is_some()
            });
            let mut straighten =
                row![action("arrow.straighten", "Straighten", Action::Straighten)].spacing(4);
            let arcs = selected.iter().any(|a| a.is_arc());
            let can_arc = !arcs
                && selected
                    .iter()
                    .any(|a| a.kind != "bent" && !a.is_attached());
            let make_arc = || {
                hover_hint(
                    action("arrow.make-arc", "Make arc", Action::MakeArc),
                    "A circular arc through both ends · Drag the diamond to change its sweep",
                    tooltip::Position::Top,
                )
            };
            if can_arc && !curved {
                straighten = straighten.push(make_arc());
            } else if curved && !arcs {
                straighten = straighten.push(hover_hint(
                    accessible_command("arrow.smooth", "Smooth bow", "Smooth")
                        .width(Length::Fill)
                        .on_press(Message::Graphics(super::graphics::Action::Cleanup(
                            super::graphics::Cleanup::Smooth,
                        ))),
                    "Even out the bow's tangent angles and lengths",
                    tooltip::Position::Top,
                ));
            } else {
                straighten = straighten.push(Space::new().width(Length::Fill));
            }
            let mut commands = column![
                row![
                    action("arrow.reverse", "Reverse", Action::Reverse),
                    action("arrow.flip-bend", "Flip bend", Action::Flip),
                ]
                .spacing(4),
                straighten,
            ]
            .spacing(2);
            if curved && !arcs {
                commands = commands.push(
                    row![
                        hover_hint(
                            accessible_command("arrow.symmetrize", "Symmetrize", "Symmetrize")
                                .width(Length::Fill)
                                .on_press(Message::Graphics(super::graphics::Action::Cleanup(
                                    super::graphics::Cleanup::Symmetrise { upright: false },
                                ))),
                            "Average the bow across its perpendicular bisector",
                            tooltip::Position::Top,
                        ),
                        if can_arc {
                            Element::from(make_arc())
                        } else {
                            Space::new().width(Length::Fill).into()
                        },
                    ]
                    .spacing(4),
                );
            }
            // Interior nodes are edited with the shared Edit points tool.
            if let Some(arrow) = self.selected_node_arrow() {
                commands = commands.push(if self.tool == Tool::EditPoints {
                    let nodes = arrow.nodes.len();
                    column![
                        text(format!(
                            "{nodes} node{} · Click the shaft to add one",
                            if nodes == 1 { "" } else { "s" }
                        ))
                        .size(11)
                        .style(muted_text),
                        row![
                            accessible_command(
                                "arrow.delete-node",
                                "Delete arrow node",
                                "Delete node"
                            )
                            .width(Length::Fill)
                            .on_press_maybe(
                                self.selected_arrow_node()
                                    .map(|_| Message::ArrowAction(Action::DeleteNode))
                            ),
                            Space::new().width(Length::Fill),
                        ]
                        .spacing(4),
                    ]
                    .spacing(4)
                } else {
                    column![
                        row![
                            hover_hint(
                                accessible_command(
                                    "arrow.edit-points",
                                    "Edit arrow points",
                                    "Edit points"
                                )
                                .width(Length::Fill)
                                .on_press(Message::Tool(Tool::EditPoints)),
                                "Add nodes so the arrow can follow a path",
                                tooltip::Position::Top,
                            ),
                            Space::new().width(Length::Fill),
                        ]
                        .spacing(4)
                    ]
                });
            }
            // Unavailable Detach buttons say why on hover.
            let detach = |id, name, label, message: Option<Action>, why| {
                let available = message.is_some();
                let button = accessible_command(id, name, label)
                    .width(Length::Fill)
                    .on_press_maybe(message.map(Message::ArrowAction));
                if available {
                    Element::from(button)
                } else {
                    hover_hint(button, why, tooltip::Position::Top).into()
                }
            };
            panel = panel.push(
                commands.push(
                    row![
                        detach(
                            "arrow.detach-start",
                            "Detach arrow start",
                            "Detach start",
                            start.then_some(Action::DetachStart),
                            "Start is not attached",
                        ),
                        detach(
                            "arrow.detach-end",
                            "Detach arrow end",
                            "Detach end",
                            end.then_some(Action::DetachEnd),
                            "End is not attached",
                        ),
                    ]
                    .spacing(4),
                ),
            );
        }
        geometry = geometry.push(command(
            "Reset arrow style",
            Message::ArrowAction(Action::Reset),
        ));
        panel
            .push(self.inspector_section(
                super::inspector::Section::ArrowGeometry,
                "Arrowhead & markers",
                "",
                false,
                geometry,
            ))
            .push(
                text(if selected.iter().any(|a| a.is_arc()) {
                    ARC_HINT
                } else {
                    GESTURE_HINT
                })
                .size(11)
                .style(muted_text),
            )
            .into()
    }
    pub(super) fn set_arrow_style(&mut self, style: Preset) {
        self.tab.arrows.cancel_source();
        self.tab.arrow_style = style;
        self.tab.arrows.style = reshiki::arrows::ArrowStyle::preset(style);
        self.tab.arrows.style.width_pt = self.tab.doc.drawing_style.line_width_pt;
        self.tool = Tool::Arrow;
        self.inspector_open = true;
        self.inspector_tab = InspectorTab::Properties;
        let before = self.tab.doc.clone();
        for a in &mut self.tab.doc.arrows {
            if self.tab.selected.contains(&a.id) {
                a.set_preset(style, &self.tab.arrows.style);
            }
        }
        self.changed(before);
        self.sync_arrows();
    }
}

/// Text a new callout starts with, selected so typing replaces it.
const CALLOUT_TEXT: &str = "Note";
impl App {
    /// What a callout would point at: one atom, one bond (its two atoms), one
    /// whole molecule, or one text or shape.
    pub(super) fn callout_target(&self) -> Option<reshiki::leaders::Target> {
        use reshiki::leaders::Target;
        let doc = &self.tab.doc;
        let selected = &self.tab.selected;
        let atoms: Vec<u64> = selected
            .iter()
            .copied()
            .filter(|id| doc.atom(*id).is_some())
            .collect();
        if atoms.len() != selected.len() {
            let [id] = selected.as_slice() else {
                return None;
            };
            let object = doc.annotations.iter().any(|a| a.id == *id)
                || doc
                    .graphics
                    .iter()
                    .any(|g| g.id == *id && g.leader.is_none());
            return object.then_some(Target::Object { id: *id });
        }
        match atoms.as_slice() {
            [] => None,
            [atom] => Some(Target::Atom { atom: *atom }),
            [a, b]
                if doc
                    .bonds
                    .iter()
                    .any(|bond| (bond.a, bond.b) == (*a, *b) || (bond.a, bond.b) == (*b, *a)) =>
            {
                Some(Target::Bond {
                    a: *a,
                    b: *b,
                    fraction: 0.5,
                })
            }
            _ => {
                let molecules = reshiki::reactions::molecules(doc, &atoms);
                let [molecule] = molecules.as_slice() else {
                    return None;
                };
                let atom = *atoms.first()?;
                (molecule.len() == atoms.len()).then_some(Target::Molecule { atom })
            }
        }
    }
    /// Start typing a callout's text beside the selection. Applying the text
    /// adds it with its leader in one change; Esc adds nothing.
    pub(super) fn add_callout(&mut self) -> iced::Task<Message> {
        let position = self
            .callout_target()
            .ok_or_else(|| "Select an atom, bond, molecule or text first".to_owned())
            .and_then(|target| {
                reshiki::leaders::callout_position(&self.tab.doc, &target).map(|p| (target, p))
            });
        let (target, position) = match position {
            Ok(found) => found,
            Err(error) => {
                self.status = error;
                self.error = true;
                return iced::Task::none();
            }
        };
        let task = self.inline_action(super::inline_text::Action::Begin(None, position));
        if let Some(state) = self.tab.inline_text.as_mut() {
            state.callout = Some(target);
            self.tab.caption = CALLOUT_TEXT.into();
            self.tab.caption_editor = iced::widget::text_editor::Content::with_text(CALLOUT_TEXT);
            self.tab
                .caption_editor
                .perform(iced::widget::text_editor::Action::SelectAll);
        }
        task
    }
}

#[cfg(test)]
mod tests;
