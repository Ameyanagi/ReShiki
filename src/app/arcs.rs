use super::*;
use iced::Length;
use iced::widget::{column, container, row, text, tooltip};
use reshiki::graphics::{ArcGeometry, GraphicKind};

#[derive(Debug, Clone)]
pub enum Action {
    Start(String),
    Sweep(String),
    Apply,
    Preset(f32),
}

pub(super) struct Editor {
    pub geometry: ArcGeometry,
    start: String,
    sweep: String,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            geometry: ArcGeometry::default(),
            start: "180".into(),
            sweep: "180".into(),
        }
    }
}

impl Editor {
    fn set(&mut self, geometry: ArcGeometry) {
        self.geometry = geometry;
        self.start = geometry.start_degrees.to_string();
        self.sweep = geometry.sweep_degrees.to_string();
    }
}

impl App {
    pub(super) fn sync_arc(&mut self) {
        if let Some(graphic) = self
            .tab
            .doc
            .graphics
            .iter()
            .find(|g| self.tab.selected.contains(&g.id) && g.kind == GraphicKind::Arc)
        {
            self.tab.arc_editor.set(graphic.arc.unwrap_or_default());
        }
    }

    pub(super) fn update_arc(&mut self, action: Action) {
        let preset_only = matches!(action, Action::Preset(_));
        let geometry = match action {
            Action::Start(input) => {
                self.tab.arc_editor.start = input;
                return;
            }
            Action::Sweep(input) => {
                self.tab.arc_editor.sweep = input;
                return;
            }
            Action::Preset(sweep_degrees) => ArcGeometry {
                sweep_degrees,
                ..self.tab.arc_editor.geometry
            },
            Action::Apply => {
                let Ok(start_degrees) = self.tab.arc_editor.start.trim().parse::<f32>() else {
                    self.status = "Arc start must be a finite angle in degrees".into();
                    self.error = true;
                    return;
                };
                let Ok(sweep_degrees) = self.tab.arc_editor.sweep.trim().parse::<f32>() else {
                    self.status = "Arc sweep must be 0.1–360°".into();
                    self.error = true;
                    return;
                };
                ArcGeometry {
                    start_degrees,
                    sweep_degrees,
                }
            }
        };
        if let Err(error) = geometry.validate() {
            self.status = error;
            self.error = true;
            return;
        }
        let geometry = ArcGeometry {
            start_degrees: geometry.start_degrees.rem_euclid(360.),
            ..geometry
        };
        self.tab.arc_editor.set(geometry);
        let before = self.tab.doc.clone();
        for graphic in &mut self.tab.doc.graphics {
            if self.tab.selected.contains(&graphic.id) && graphic.kind == GraphicKind::Arc {
                graphic.set_arc(ArcGeometry {
                    start_degrees: if preset_only {
                        graphic.arc.unwrap_or_default().start_degrees
                    } else {
                        geometry.start_degrees
                    },
                    ..geometry
                });
            }
        }
        self.status = format!("Arc sweep: {:.1}°", geometry.sweep_degrees);
        self.error = false;
        self.changed(before);
        self.sync_arc();
    }

    /// Sweep presets as one segmented strip marking the current sweep.
    /// `fill` spreads it across the inspector.
    pub(super) fn arc_presets(&self, fill: bool) -> Element<'_, Message> {
        let sweep = self.tab.arc_editor.geometry.sweep_degrees;
        let strip = row(ArcGeometry::PRESETS.into_iter().map(|degrees| {
            reshiki::accessibility::button(
                format!(
                    "arc-preset-{}-{degrees:.0}",
                    if fill { "inspector" } else { "context" }
                ),
                format!("Arc sweep {degrees:.0} degrees"),
                text(format!("{degrees:.0}°")).size(12).center(),
            )
            .checked(sweep == degrees)
            .width(if fill { Length::Fill } else { Length::Shrink })
            .padding([4, 7])
            .style(move |theme: &iced::Theme, status| {
                if sweep == degrees {
                    crate::appearance::primary(theme, status)
                } else {
                    workspace::control(false)(theme, status)
                }
            })
            .on_press(Message::Arc(Action::Preset(degrees)))
            .into()
        }))
        .spacing(2);
        workspace::hover_hint(
            container(strip).padding(2).style(|theme| {
                let field =
                    crate::appearance::dropdown(theme, iced::widget::pick_list::Status::Active);
                container::Style {
                    background: Some(field.background),
                    border: field.border,
                    ..Default::default()
                }
            }),
            "Arc sweep",
            tooltip::Position::Bottom,
        )
        .into()
    }

    pub(super) fn arc_controls(&self) -> Element<'_, Message> {
        let field = |id, label, value, hint, input: fn(String) -> Message| {
            row![
                text(label).size(12),
                workspace::hover_hint(
                    workspace::accessible_unit_field(
                        reshiki::accessibility::text_input(
                            id,
                            format!("Arc {label} (degrees)"),
                            "180",
                            value
                        )
                        .style(crate::appearance::input_style)
                        .on_input(input)
                        .on_submit(Message::Arc(Action::Apply)),
                        "°",
                    ),
                    hint,
                    tooltip::Position::Top,
                ),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
            .width(Length::Fill)
        };
        column![
            workspace::section("ARC ANGLES"),
            self.arc_presets(true),
            row![
                field(
                    "arc-start",
                    "Start",
                    &self.tab.arc_editor.start,
                    "Clockwise from the right · Enter applies",
                    |s| Message::Arc(Action::Start(s))
                ),
                field(
                    "arc-sweep",
                    "Sweep",
                    &self.tab.arc_editor.sweep,
                    "0.1–360° · Enter applies",
                    |s| Message::Arc(Action::Sweep(s))
                ),
            ]
            .spacing(10),
        ]
        .spacing(8)
        .into()
    }
}

#[cfg(test)]
mod tests;
