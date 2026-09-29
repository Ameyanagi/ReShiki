use super::*;
use iced::widget::{button, column, row, text};
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
            .doc
            .graphics
            .iter()
            .find(|g| self.selected.contains(&g.id) && g.kind == GraphicKind::Arc)
        {
            self.arc_editor.set(graphic.arc.unwrap_or_default());
        }
    }

    pub(super) fn update_arc(&mut self, action: Action) {
        let preset_only = matches!(action, Action::Preset(_));
        let geometry = match action {
            Action::Start(input) => {
                self.arc_editor.start = input;
                return;
            }
            Action::Sweep(input) => {
                self.arc_editor.sweep = input;
                return;
            }
            Action::Preset(sweep_degrees) => ArcGeometry {
                sweep_degrees,
                ..self.arc_editor.geometry
            },
            Action::Apply => {
                let Ok(start_degrees) = self.arc_editor.start.trim().parse::<f32>() else {
                    self.status = "Arc start must be a finite angle in degrees".into();
                    self.error = true;
                    return;
                };
                let Ok(sweep_degrees) = self.arc_editor.sweep.trim().parse::<f32>() else {
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
        self.arc_editor.set(geometry);
        let before = self.doc.clone();
        for graphic in &mut self.doc.graphics {
            if self.selected.contains(&graphic.id) && graphic.kind == GraphicKind::Arc {
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

    pub(super) fn arc_controls(&self) -> Element<'_, Message> {
        let preset = |degrees| {
            button(text(format!("{degrees:.0}°")).size(12))
                .padding(5)
                .on_press(Message::Arc(Action::Preset(degrees)))
        };
        let field = |placeholder, value, input: fn(String) -> Message| {
            crate::appearance::text_input(placeholder, value)
                .on_input(input)
                .on_submit(Message::Arc(Action::Apply))
                .padding(5)
                .size(12)
                .width(78)
        };
        column![
            text("ARC ANGLES").size(11),
            row(ArcGeometry::PRESETS.into_iter().map(|p| preset(p).into())).spacing(5),
            row![
                column![text("Start (°)").size(11), field("180", &self.arc_editor.start, |s| Message::Arc(Action::Start(s)))].spacing(4),
                column![text("Sweep (°)").size(11), field("180", &self.arc_editor.sweep, |s| Message::Arc(Action::Sweep(s)))].spacing(4),
            ].spacing(8),
            row![
                button(text("Apply angles").size(12)).padding(5).on_press(Message::Arc(Action::Apply)),
                preset(360.),
            ].spacing(5),
            text("Sweep: 0.1–360°. Angles run clockwise from the right. Drag an ellipse frame; Shift makes it circular. Edit arc endpoints to adjust the curve.").size(11),
        ].spacing(7).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawing() -> App {
        let (mut app, _) = App::new();
        app.tool = Tool::Graphic(GraphicKind::Arc);
        app.update_arc(Action::Preset(270.));
        app.edit(Edit::Graphic(
            Point::new(20., 30.),
            Point::new(180., 110.),
            false,
        ));
        app
    }

    #[test]
    fn arc_presets_apply_to_new_drawings_and_numeric_edits_undo_as_one_action() {
        let mut app = drawing();
        assert_eq!(app.tool, Tool::Select);
        assert_eq!(app.doc.graphics[0].arc.unwrap().sweep_degrees, 270.);
        let before = app.doc.clone();
        app.update_arc(Action::Start("-45.5".into()));
        app.update_arc(Action::Sweep("123.4".into()));
        assert_eq!(app.doc, before, "typing must not change the drawing");
        app.update_arc(Action::Apply);
        let after = app.doc.clone();
        assert_eq!(after.graphics[0].arc.unwrap().start_degrees, 314.5);
        assert_eq!(after.graphics[0].arc.unwrap().sweep_degrees, 123.4);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        assert_eq!(app.arc_editor.geometry.sweep_degrees, 270.);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, after);
    }

    #[test]
    fn invalid_arc_input_is_rejected_atomically() {
        let mut app = drawing();
        let before = app.doc.clone();
        let parameters = app.arc_editor.geometry;
        for (start, sweep) in [
            ("NaN", "90"),
            ("0", "NaN"),
            ("0", "361"),
            ("0", "0"),
            ("oops", "90"),
        ] {
            app.update_arc(Action::Start(start.into()));
            app.update_arc(Action::Sweep(sweep.into()));
            app.update_arc(Action::Apply);
            assert!(app.error);
            assert_eq!(app.doc, before);
            assert_eq!(app.arc_editor.geometry, parameters);
        }
    }

    #[test]
    fn applying_a_preset_to_multiple_arcs_preserves_each_start_angle() {
        let mut app = drawing();
        let mut second = app.doc.graphics[0].clone();
        second.id = app.doc.next_id();
        second.set_arc(ArcGeometry {
            start_degrees: 37.,
            sweep_degrees: 120.,
        });
        app.doc.graphics.push(second);
        app.selected = app.doc.all_ids();
        app.update_arc(Action::Preset(90.));
        assert_eq!(app.doc.graphics[0].arc.unwrap().start_degrees, 180.);
        assert_eq!(app.doc.graphics[1].arc.unwrap().start_degrees, 37.);
        assert!(
            app.doc
                .graphics
                .iter()
                .all(|g| g.arc.unwrap().sweep_degrees == 90.)
        );
    }

    #[test]
    fn endpoint_drag_and_undo_keep_arc_type_selection_and_inspector_values() {
        let mut app = drawing();
        let id = app.doc.graphics[0].id;
        let before = app.doc.clone();
        let _ = app.update(Message::Tool(Tool::EditPoints));
        app.edit(Edit::GraphicPoint(id, 1, Point::new(180., 70.)));
        assert_eq!(app.doc.graphics[0].kind, GraphicKind::Arc);
        assert_eq!(app.arc_editor.geometry.sweep_degrees, 180.);
        assert_eq!(app.selected, vec![id]);
        let after = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        assert_eq!(app.arc_editor.geometry.sweep_degrees, 270.);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, after);
    }
}
