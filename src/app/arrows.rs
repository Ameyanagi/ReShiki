use super::*;
use crate::canvas::layered::canvas;
use reshiki::arrows::{ArrowStyle, Head, HeadShape, NoGo, Preset};
use reshiki::graphics::LinePattern;

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
}
impl Default for State {
    fn default() -> Self {
        let style = ArrowStyle::default();
        Self {
            numbers: FIELDS.map(|f| f.get(&style).to_string()),
            style,
            color: "#000000".into(),
        }
    }
}
impl State {
    pub(super) fn refresh_inputs(&mut self) {
        self.numbers = FIELDS.map(|f| f.get(&self.style).to_string());
        let [r, g, b] = self.style.color;
        self.color = format!("#{r:02X}{g:02X}{b:02X}");
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
    Reverse,
    Flip,
    Straighten,
    Reset,
}
impl App {
    pub(super) fn sync_arrows(&mut self) {
        if let Some(a) = self
            .doc
            .arrows
            .iter()
            .find(|a| self.selected.contains(&a.id))
        {
            self.arrow_style = Preset::from_kind(&a.kind);
            self.arrows.style = a.appearance();
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
        self.arrows.refresh_inputs();
    }
    pub(super) fn arrow_action(&mut self, action: Action) {
        match action {
            Action::Number(field, s) => {
                if let Some(value) = self.arrows.numbers.get_mut(field as usize) {
                    *value = s;
                }
                return;
            }
            Action::Color(s) => {
                self.arrows.color = s;
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
                    s.color = graphics::parse_color(&self.arrows.color)
                        .ok_or("Use a six-digit hex color, for example #205091")?
                }
                Action::ApplyNumber(f) => f.set(
                    s,
                    self.arrows
                        .numbers
                        .get(f as usize)
                        .ok_or("Unknown arrow property")?
                        .parse::<f32>()
                        .map_err(|_| "Enter a number and press Return")?,
                ),
                Action::Reset => {
                    *s = ArrowStyle::preset(self.arrow_style);
                    s.width_pt = self.doc.drawing_style.line_width_pt;
                }
                _ => {}
            }
            s.validate()
        };
        let mut style = self.arrows.style.clone();
        if let Err(error) = change(&mut style) {
            self.status = error;
            self.error = true;
            return;
        }
        let mut next = self.doc.clone();
        for a in next
            .arrows
            .iter_mut()
            .filter(|a| self.selected.contains(&a.id))
        {
            match action {
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
                if self.selected.contains(&reaction.arrow) {
                    std::mem::swap(&mut reaction.reactants, &mut reaction.products);
                }
            }
        }
        let before = std::mem::replace(&mut self.doc, next);
        self.arrows.style = style;
        self.changed(before);
        self.sync_arrows();
        self.error = false;
        self.status = "Arrow updated · Drag the middle handle to bend".into();
    }
    pub(super) fn arrow_panel(&self) -> Element<'_, Message> {
        use super::workspace::{muted, section};
        use iced::widget::{button, checkbox, column, pick_list, row, text, text_input};
        use iced::{Alignment, Length};
        let s = &self.arrows.style;
        let has_selection = self
            .doc
            .arrows
            .iter()
            .any(|a| self.selected.contains(&a.id));
        let number = |label: &'static str, field: Field| {
            row![
                text(label).size(11).width(Length::Fill),
                text_input(
                    "",
                    self.arrows
                        .numbers
                        .get(field as usize)
                        .map(String::as_str)
                        .unwrap_or("")
                )
                .size(12)
                .padding(5)
                .width(70)
                .on_input(move |v| Message::ArrowAction(Action::Number(field, v)))
                .on_submit(Message::ArrowAction(Action::ApplyNumber(field)))
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
                    .doc
                    .arrows
                    .iter()
                    .find(|a| self.selected.contains(&a.id))
                    .cloned()
                    .unwrap_or_else(|| Arrow::new(
                        1,
                        Point::new(-65., 0.),
                        Point::new(65., 0.),
                        self.arrow_style,
                        s.clone()
                    ))
            })
            .width(Length::Fill)
            .height(70),
            pick_list(Preset::ALL, Some(self.arrow_style), Message::ArrowStyle)
                .text_size(12)
                .padding(6)
                .width(Length::Fill),
            row![
                text("End").size(11).width(44),
                pick_list(Head::ALL, Some(s.head), |v| Message::ArrowAction(
                    Action::Head(v)
                ))
                .text_size(12)
                .padding(5)
                .width(Length::Fill)
            ]
            .align_y(Alignment::Center),
            row![
                text("Start").size(11).width(44),
                pick_list(Head::ALL, Some(s.tail), |v| Message::ArrowAction(
                    Action::Tail(v)
                ))
                .text_size(12)
                .padding(5)
                .width(Length::Fill)
            ]
            .align_y(Alignment::Center),
            row![
                pick_list(
                    [LinePattern::Solid, LinePattern::Dashed, LinePattern::Dotted],
                    Some(s.pattern),
                    |v| Message::ArrowAction(Action::Pattern(v))
                )
                .text_size(12)
                .padding(5),
                text_input("#RRGGBB", &self.arrows.color)
                    .on_input(|v| Message::ArrowAction(Action::Color(v)))
                    .on_submit(Message::ArrowAction(Action::ApplyColor))
                    .size(12)
                    .padding(5)
                    .width(Length::Fill)
            ]
            .spacing(6),
            number("Line width (pt)", Field::Line),
        ]
        .spacing(8);
        let mut geometry = column![
            text("Arrowhead shape").size(11).color(muted()),
            pick_list(HeadShape::ALL, Some(s.shape), |v| Message::ArrowAction(
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
        if self.arrow_style == Preset::Equilibrium {
            geometry = geometry.push(number("Reverse length (0.2–1)", Field::Ratio));
        }
        if matches!(self.arrow_style, Preset::Equilibrium | Preset::Retro) {
            geometry = geometry.push(number("Shaft separation (pt)", Field::Gap));
        }
        geometry = geometry
            .push(
                row![
                    text("No reaction").size(11),
                    pick_list(NoGo::ALL, Some(s.no_go), |v| Message::ArrowAction(
                        Action::NoGo(v)
                    ))
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
        if has_selection {
            panel = panel.push(
                row![
                    button(text("Reverse").size(11))
                        .on_press(Message::ArrowAction(Action::Reverse)),
                    button(text("Flip bend").size(11)).on_press(Message::ArrowAction(Action::Flip)),
                    button(text("Straighten").size(11))
                        .on_press(Message::ArrowAction(Action::Straighten))
                ]
                .spacing(4),
            );
        }
        geometry = geometry.push(
            button(text("Reset arrow style").size(11))
                .on_press(Message::ArrowAction(Action::Reset)),
        );
        panel.push(self.inspector_section(super::inspector::Section::ArrowGeometry, "Arrowhead & markers", "", false, geometry))
            .push(text("Drag endpoints to resize; drag the square handle to bend. Return applies numeric and color fields.").size(11).color(muted())).into()
    }
}
