use super::*;

fn row(y: f32) -> Rectangle {
    Rectangle {
        x: 0.,
        y,
        width: 100.,
        height: 20.,
    }
}

#[test]
fn no_rows_use_natural_body_height() {
    assert_eq!(
        heights(10., 100., &[]),
        Heights {
            minimum: 110.,
            preferred: 110.,
        }
    );
}

#[test]
fn one_row_sets_minimum_and_preferred_height() {
    assert_eq!(
        heights(10., 100., &[row(5.5)]),
        Heights {
            minimum: 37.5,
            preferred: 37.5,
        }
    );
}

#[test]
fn several_rows_use_the_last_row() {
    assert_eq!(
        heights(10., 100., &[row(0.), row(24.5)]),
        Heights {
            minimum: 32.,
            preferred: 56.5,
        }
    );
}

#[test]
fn every_measured_row_sets_the_preferred_height() {
    assert_eq!(
        heights(
            10.,
            200.,
            &[row(0.), row(24.5), row(49.), row(73.5), row(98.)]
        ),
        Heights {
            minimum: 32.,
            preferred: 130.,
        }
    );
}

#[test]
fn no_requested_height_uses_preferred_height() {
    assert_eq!(
        floating_height(
            None,
            Heights {
                minimum: 100.,
                preferred: 200.
            },
            500.
        ),
        200.
    );
}

#[test]
fn requested_height_below_minimum_uses_minimum() {
    assert_eq!(
        floating_height(
            Some(50.),
            Heights {
                minimum: 100.,
                preferred: 200.
            },
            500.
        ),
        100.
    );
}

#[test]
fn floating_height_is_capped_by_available_height() {
    assert_eq!(
        floating_height(
            Some(400.),
            Heights {
                minimum: 100.,
                preferred: 200.
            },
            150.
        ),
        150.
    );
}

#[test]
fn floating_height_is_capped_by_maximum_content_height() {
    assert_eq!(
        floating_height(
            Some(800.),
            Heights {
                minimum: 100.,
                preferred: 200.
            },
            1000.
        ),
        620.
    );
}

#[test]
fn row_measurement_stops_once_a_row_reaches_the_height_cap() {
    let mut rows = Rows::default();
    let mut traversed = false;
    rows.traverse(&mut |_| traversed = true);
    assert!(traversed);

    // Rows every 100px: more than the old four are measured, and the
    // seventh, ending at 620 (the cap), is the last; later rows cannot
    // change the capped preferred height.
    for index in 0..9 {
        let mut node = reshiki::accessibility::Node {
            id: format!("{SITE_ID_PREFIX}13C.{index}"),
            name: "Result row".into(),
            role: reshiki::accessibility::Role::Button,
            enabled: true,
            focused: false,
            checked: None,
            expanded: None,
            value: None,
            bounds: row(index as f32 * 100.),
            visible_bounds: None,
        };
        rows.custom(None, Rectangle::default(), &mut node);
    }
    assert_eq!(
        rows.0,
        (0..7)
            .map(|index| row(index as f32 * 100.))
            .collect::<Vec<_>>()
    );
    rows.traverse(&mut |_| panic!("A full measurement must not descend"));
}

/// Rows, counting the accessibility nodes built while it measures.
#[derive(Default)]
struct Counted {
    rows: Rows,
    nodes: usize,
}
impl Operation for Counted {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let mut descend = false;
        self.rows.traverse(&mut |_| descend = true);
        if descend {
            operate(self);
        }
    }
    fn custom(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        state: &mut dyn Any,
    ) {
        self.nodes += usize::from(state.is::<reshiki::accessibility::Node>());
        self.rows.custom(id, bounds, state);
    }
}

/// Counts every accessibility node, traversing everything.
#[derive(Default)]
struct Nodes(usize);
impl Operation for Nodes {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn custom(
        &mut self,
        _: Option<&iced::advanced::widget::Id>,
        _: Rectangle,
        state: &mut dyn Any,
    ) {
        self.0 += usize::from(state.is::<reshiki::accessibility::Node>());
    }
}

#[tokio::test]
#[ignore = "Opt-in real renderer row measurement traversal check"]
async fn row_measurement_skips_later_rows_in_a_column() {
    use iced::advanced::renderer::Headless;
    use iced::widget::{Column, Space, text};
    let renderer = <Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    // 100px rows: the seventh is the first to end past the 620px content cap.
    let (count, measured_rows) = (9, 7);
    let mut table = Column::new();
    for index in 0..count {
        table = table.push(Space::new().height(1).width(Length::Fill));
        table = table.push(table_row(
            reshiki::accessibility::button(
                format!("{SITE_ID_PREFIX}13C.{index}"),
                "Result row",
                text("Row"),
            )
            .width(Length::Fill)
            .height(100)
            .on_press(Message::Nmr(super::super::Action::SelectRow(index))),
        ));
    }
    let mut table = Element::from(table);
    let mut tree = Tree::new(&table);
    let node = table.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(200., 2000.)),
    );
    let bounds: Vec<Rectangle> = Layout::new(&node)
        .children()
        .skip(1)
        .step_by(2)
        .map(|row| row.bounds())
        .collect();
    assert_eq!(bounds.len(), count);

    let mut measured = Counted::default();
    table
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut measured);
    assert_eq!(measured.rows.0, bounds[..measured_rows]);
    assert_eq!(measured.nodes, measured_rows);

    let mut all = Nodes::default();
    table
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut all);
    assert_eq!(all.0, count);
}
