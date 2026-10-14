use super::*;
mod attachments;
use crate::canvas::layered::canvas;
use reshiki::arrows::{ArrowStyle, Head, HeadShape, NoGo, Preset};
use reshiki::graphics::LinePattern;

/// The arrow card's one line of gesture help; the full text is in the guide.
const GESTURE_HINT: &str = "Drag the diamond to bend, the squares to aim each end";
const ATTACH_LABEL: &str = "Attach ends to atoms and bonds";
const ATTACH_HINT: &str = concat!(
    "Two clicks: a source, then a destination atom, bond or lone pair · ",
    "Option/Alt-click or turn this off for a free arrow"
);

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
    pub(super) fn arrow_panel(&self) -> Element<'_, Message> {
        use super::workspace::{accessible_command, command, hover_hint, muted_text, section};
        use iced::widget::{Space, checkbox, column, row, text, tooltip};
        use iced::{Alignment, Length};
        let s = &self.tab.arrows.style;
        let has_selection = self
            .tab
            .doc
            .arrows
            .iter()
            .any(|a| self.tab.selected.contains(&a.id));
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
        let mut panel = column![
            section(if has_selection {
                "ARROW PROPERTIES"
            } else {
                "ARROW TOOL"
            }),
            canvas(crate::canvas::ArrowPreview {
                arrow: self
                    .tab
                    .doc
                    .arrows
                    .iter()
                    .find(|a| self.tab.selected.contains(&a.id))
                    .cloned()
                    .unwrap_or_else(|| Arrow::new(
                        1,
                        Point::new(-65., 0.),
                        Point::new(65., 0.),
                        self.tab.arrow_style,
                        s.clone()
                    ))
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
                crate::appearance::pick_list(
                    [LinePattern::Solid, LinePattern::Dashed, LinePattern::Dotted],
                    Some(s.pattern),
                    |v| Message::ArrowAction(Action::Pattern(v))
                )
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
            geometry = geometry.push(number("Reverse length (0.2–1)", Field::Ratio));
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
                ATTACH_HINT,
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
                column![
                    row![
                        action("arrow.reverse", "Reverse", Action::Reverse),
                        action("arrow.flip-bend", "Flip bend", Action::Flip),
                    ]
                    .spacing(4),
                    row![
                        action("arrow.straighten", "Straighten", Action::Straighten),
                        Space::new().width(Length::Fill),
                    ]
                    .spacing(4),
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
                ]
                .spacing(2),
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
            .push(text(GESTURE_HINT).size(11).style(muted_text))
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

#[cfg(test)]
mod tests;
