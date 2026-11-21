//! Explicit cycle shapes and their dimensions in Properties and Make review.
use super::{Action, App, Field, Inputs, Message, Proposal, ProposalKind, Shape};
use crate::canvas::layered::canvas;
use iced::widget::{column, row, text};
use iced::{Alignment, Element, Length};
use reshiki::{
    document::Document,
    graphics::{Graphic, GraphicKind},
};

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct MakeLayout {
    shape: Shape,
    size: Option<[f32; 2]>,
    corner: Option<f32>,
}

impl MakeLayout {
    pub(super) fn apply(self, doc: &mut Document, id: u64) -> Result<(), String> {
        let g = doc
            .graphics
            .iter_mut()
            .find(|g| g.id == id)
            .ok_or("Cycle is unavailable")?;
        g.cycle.as_mut().ok_or("Cycle is unavailable")?.shape = self.shape;
        g.style.corner_pt = self.corner;
        if let Some([width, height]) = self.size {
            if self.shape == Shape::Circle {
                reshiki::cycles::set_radius(doc, id, (width + height) * 0.25);
            } else {
                let current = dimensions(g);
                if current != [width, height]
                    && !reshiki::cycles::set_dimensions(doc, id, width, height)
                {
                    return Err("Could not resize the cycle with those dimensions".into());
                }
            }
        }
        let g = doc
            .graphics
            .iter()
            .find(|g| g.id == id)
            .ok_or("Cycle is unavailable")?;
        if reshiki::cycles::outline(g).is_none() {
            return Err("Resize the cycle before choosing this shape".into());
        }
        if self.shape != Shape::Circle {
            reshiki::cycles::tidy(doc, id);
        }
        doc.validate()
    }
}

pub(super) fn dimensions(g: &Graphic) -> [f32; 2] {
    [g.axis_x, g.axis_y].map(|p| f64::from(p.x).hypot(f64::from(p.y)) as f32)
}

fn input<'a>(
    inputs: &'a Inputs,
    id: &'static str,
    name: &'static str,
    field: Field,
) -> Element<'a, Message> {
    reshiki::accessibility::text_input(
        id,
        name,
        if field == Field::Corner { "Auto" } else { "" },
        inputs.get(field),
    )
    .style(crate::appearance::input_style)
    .on_input(move |v| Message::Cycle(Action::Edit(field, v)))
    .on_submit(Message::Cycle(Action::Apply(field)))
    .size(12)
    .padding(5)
    .width(56)
    .into()
}

pub(super) fn controls<'a>(g: &Graphic, inputs: &'a Inputs) -> Element<'a, Message> {
    let shape = g.cycle.as_ref().map_or(Shape::Circle, |c| c.shape);
    let choice = |selected, name: &'static str, id: &'static str, kind, width| {
        let preview = Graphic::dragged(
            1,
            kind,
            Default::default(),
            reshiki::document::Point::new(width, 60.),
            Default::default(),
            Default::default(),
            false,
        );
        reshiki::accessibility::button(
            id,
            name,
            row![
                canvas(crate::canvas::ScientificPreview(preview))
                    .width(48)
                    .height(36),
                text(name).size(11)
            ]
            .spacing(3)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([3, 5])
        .style(super::control(shape == selected))
        .checked(shape == selected)
        .on_press(Message::Cycle(Action::Shape(selected)))
    };
    let mut fields = column![
        text("Shape").size(11),
        row![
            choice(
                Shape::Circle,
                "Circle",
                "cycle.shape.circle",
                GraphicKind::Ellipse,
                60.
            ),
            choice(
                Shape::Ellipse,
                "Ellipse",
                "cycle.shape.ellipse",
                GraphicKind::Ellipse,
                100.
            ),
        ]
        .spacing(4),
        row![
            choice(
                Shape::RoundedRectangle,
                "Rounded rectangle",
                "cycle.shape.rounded-rectangle",
                GraphicKind::RoundedRectangle,
                100.
            ),
            choice(
                Shape::Rectangle,
                "Rectangle",
                "cycle.shape.rectangle",
                GraphicKind::Rectangle,
                100.
            ),
        ]
        .spacing(4),
    ]
    .spacing(5);
    if shape == Shape::Circle {
        fields = fields.push(super::labelled(
            "Radius (pt)",
            input(inputs, "cycle-radius", "Cycle radius (pt)", Field::Radius),
        ));
    } else {
        fields = fields.push(
            row![
                super::labelled(
                    "Width (pt)",
                    input(inputs, "cycle-width", "Cycle width (pt)", Field::Width)
                ),
                super::labelled(
                    "Height (pt)",
                    input(inputs, "cycle-height", "Cycle height (pt)", Field::Height)
                ),
            ]
            .spacing(10),
        );
        if shape == Shape::RoundedRectangle {
            fields = fields.push(super::hover_hint(
                super::labelled(
                    "Corner (pt)",
                    input(
                        inputs,
                        "cycle-corner",
                        "Cycle corner radius (pt)",
                        Field::Corner,
                    ),
                ),
                "Enter applies · at most half the shorter side · empty rounds automatically",
                iced::widget::tooltip::Position::Top,
            ));
        }
    }
    fields.into()
}

impl App {
    pub(super) fn sync_cycle_proposal(&mut self) {
        let Some(p) = self.tab.cycle_inputs.proposal.as_ref() else {
            return;
        };
        let Some(g) = p.doc.graphics.iter().find(|g| g.id == p.cycle) else {
            return;
        };
        let [width, height] = dimensions(g);
        let (radius, width, height) = (
            super::points(g.cycle_circle().1, &p.doc),
            super::points(width, &p.doc),
            super::points(height, &p.doc),
        );
        let corner = g.style.corner_pt.map(|v| v.to_string()).unwrap_or_default();
        let inputs = &mut self.tab.cycle_inputs;
        (inputs.radius, inputs.width, inputs.height, inputs.corner) =
            (radius, width, height, corner);
    }

    fn change_make_layout(&mut self, edit: impl FnOnce(&mut MakeLayout)) -> bool {
        let Some(p) = self.tab.cycle_inputs.proposal.as_deref() else {
            return false;
        };
        let ProposalKind::Make {
            selected,
            reversed,
            layout,
            ..
        } = &p.kind
        else {
            return false;
        };
        let (selected, reversed, mut layout) = (selected.clone(), *reversed, *layout);
        edit(&mut layout);
        match super::proposed(
            &self.tab.doc,
            &selected,
            reversed,
            layout,
            self.appearance.two_decimal_weight,
        ) {
            Ok((doc, cycle, left)) => {
                self.tab.cycle_inputs.proposal = Some(Box::new(Proposal {
                    doc,
                    cycle,
                    kind: ProposalKind::Make {
                        left,
                        selected,
                        reversed,
                        layout,
                    },
                }));
                self.sync_cycle_proposal();
                self.error = false;
                self.status = "Review the catalytic cycle · Enter applies · Esc discards".into();
            }
            Err(error) => {
                self.error = true;
                self.status = error;
            }
        }
        true
    }

    pub(super) fn change_cycle_shape(&mut self, shape: Shape) {
        if self.change_make_layout(|layout| layout.shape = shape) {
            return;
        }
        let Some(g) = self.selected_cycle_graphic() else {
            return;
        };
        if g.cycle.as_ref().is_some_and(|c| c.shape == shape) {
            return;
        }
        let mut candidate = g.clone();
        if let Some(c) = candidate.cycle.as_mut() {
            c.shape = shape;
        }
        if reshiki::cycles::outline(&candidate).is_none() {
            self.error = true;
            self.status = "Resize the cycle before choosing this shape".into();
            return;
        }
        self.edit_cycle(|c| c.shape = shape);
        self.tab.cycle_inputs.layout_open = true;
    }

    /// Return whether this is a dimension edit, including Make review edits.
    pub(super) fn apply_cycle_layout_field(
        &mut self,
        field: Field,
        value: &str,
        number: Option<f32>,
    ) -> bool {
        let making = self
            .tab
            .cycle_inputs
            .proposal
            .as_ref()
            .is_some_and(|p| matches!(p.kind, ProposalKind::Make { .. }));
        let g = if making {
            self.tab
                .cycle_inputs
                .proposal
                .as_ref()
                .and_then(|p| p.doc.graphics.iter().find(|g| g.id == p.cycle))
        } else {
            self.selected_cycle_graphic()
        };
        let Some(g) = g else {
            return true;
        };
        let shape = g.cycle.as_ref().map_or(Shape::Circle, |c| c.shape);
        if matches!(field, Field::Width | Field::Height) && shape == Shape::Circle
            || field == Field::Radius && shape != Shape::Circle
            || field == Field::Corner && shape != Shape::RoundedRectangle
        {
            return true;
        }
        if field == Field::Radius && !making {
            return false;
        }
        let id = g.id;
        let [width, height] = dimensions(g);
        if !making && matches!(field, Field::Width | Field::Height) {
            let current = if field == Field::Width { width } else { height };
            if super::points(current, &self.tab.doc).parse::<f32>().ok() == number {
                self.sync_cycle();
                self.error = false;
                self.status.clear();
                return true;
            }
        }
        let valid = match field {
            Field::Radius => number.is_some_and(|v| (10. ..=2000.).contains(&v)),
            Field::Width | Field::Height => number.is_some_and(|v| (10. ..=4000.).contains(&v)),
            Field::Corner => {
                value.is_empty()
                    || number.is_some_and(|v| (0. ..=reshiki::graphics::MAX_CORNER_PT).contains(&v))
            }
            _ => return false,
        };
        if !valid {
            self.error = true;
            self.status = match field {
                Field::Radius => "Enter a radius of 10–2000 pt",
                Field::Width | Field::Height => "Enter a width or height of 10–4000 pt",
                _ => "Corner radius must be 0–1000 pt",
            }
            .into();
            return true;
        }
        let world = number.map(|v| reshiki::style::DEFAULT.world(v));
        let corner = (!value.is_empty()).then_some(number).flatten();
        if making {
            self.change_make_layout(|layout| match field {
                Field::Radius => layout.size = world.map(|r| [2. * r, 2. * r]),
                Field::Width => layout.size = world.map(|w| [w, height]),
                Field::Height => layout.size = world.map(|h| [width, h]),
                Field::Corner => layout.corner = corner,
                _ => {}
            });
            return true;
        }
        if field == Field::Corner
            && self
                .selected_cycle_graphic()
                .is_some_and(|g| g.style.corner_pt == corner)
        {
            self.sync_cycle();
            self.error = false;
            self.status.clear();
            return true;
        }
        let mut candidate = self.tab.doc.clone();
        if field == Field::Corner {
            if let Some(g) = candidate.graphics.iter_mut().find(|g| g.id == id) {
                g.style.corner_pt = corner;
            }
        } else if let Some(world) = world {
            let (w, h) = if field == Field::Width {
                (world, height)
            } else {
                (width, world)
            };
            if !reshiki::cycles::set_dimensions(&mut candidate, id, w, h)
                && (w != width || h != height)
            {
                self.error = true;
                self.status = "Could not resize the cycle with those dimensions".into();
                return true;
            }
        }
        self.finish_cycle_edit(candidate, id, None);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_configuration_propagates_a_rejected_size_instead_of_using_the_old_frame() {
        use reshiki::{cycles::Cycle, document::Point};
        let mut doc = Document::default();
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(a, b, 1, "plain");
        let id = doc.next_id();
        let mut g = Graphic::dragged(
            id,
            GraphicKind::Ellipse,
            Point::default(),
            Point::new(1., 1.),
            Default::default(),
            Default::default(),
            false,
        );
        g.axis_x = Point::new(6e-5, 0.);
        g.axis_y = Point::new(0., 6e-5);
        g.cycle = Some(Box::new(Cycle {
            shape: Shape::Ellipse,
            intermediates: vec![vec![a, b]],
            steps: vec![Default::default()],
            ..Default::default()
        }));
        doc.graphics.push(g);
        doc.validate().unwrap();
        let before = doc.clone();
        let layout = MakeLayout {
            shape: Shape::Ellipse,
            size: Some([600., 200.]),
            corner: None,
        };
        assert!(layout.apply(&mut doc, id).is_err());
        assert_eq!(doc, before);
    }

    #[tokio::test]
    #[ignore = "Opt-in cycle controls with the app renderer"]
    async fn shape_controls_follow_the_explicit_choice_and_show_only_its_dimensions() {
        use iced::advanced::{renderer::Headless, widget::operation};
        use iced_runtime::{UserInterface, user_interface::Cache};
        let mut renderer =
            <iced::Renderer as Headless>::new(iced::Font::default(), iced::Pixels(16.), None)
                .await
                .unwrap();
        let (mut app, _) = App::new();
        let id = reshiki::cycles::insert(&mut app.tab.doc, Default::default(), 150.);
        app.tab.selected = vec![id];
        for (shape, selected) in [
            (Shape::Circle, "cycle.shape.circle"),
            (Shape::Ellipse, "cycle.shape.ellipse"),
            (Shape::RoundedRectangle, "cycle.shape.rounded-rectangle"),
            (Shape::Rectangle, "cycle.shape.rectangle"),
        ] {
            app.tab.doc.graphics[0].cycle.as_mut().unwrap().shape = shape;
            app.sync_cycle();
            let size = iced::Size::new(340., 250.);
            let mut ui = UserInterface::build(
                controls(&app.tab.doc.graphics[0], &app.tab.cycle_inputs),
                size,
                Cache::new(),
                &mut renderer,
            );
            let mut collect =
                reshiki::accessibility::Collect::new(iced::Rectangle::with_size(size));
            ui.operate(&renderer, &mut operation::black_box(&mut collect));
            let snapshot = collect.snapshot();
            assert!(snapshot.duplicate_ids.is_empty());
            let has = |id: &str| snapshot.nodes.iter().any(|n| n.id == id);
            let checked: Vec<_> = snapshot
                .nodes
                .iter()
                .filter(|n| n.checked == Some(true))
                .map(|n| n.id.as_str())
                .collect();
            assert_eq!(checked, [selected]);
            assert_eq!(has("cycle-radius"), shape == Shape::Circle);
            assert_eq!(has("cycle-width"), shape != Shape::Circle);
            assert_eq!(has("cycle-height"), shape != Shape::Circle);
            assert_eq!(has("cycle-corner"), shape == Shape::RoundedRectangle);
            assert!(snapshot.nodes.iter().all(|n| n.visible_bounds.is_some()));
        }
    }
}
