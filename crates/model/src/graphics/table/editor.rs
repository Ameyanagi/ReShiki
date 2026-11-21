//! Original empty grids for the native table tool. Existing object ownership
//! is assigned separately; creating a grid never captures nearby chemistry.
use super::{Border, Cell, Color, Frame, Graphic, GraphicKind, LinePattern, Point, Side, Table};
use crate::{document::Annotation, typography::TextFormat};

pub const DEFAULT_GRID: [usize; 2] = [2, 3];

pub fn valid_grid([rows, columns]: [usize; 2]) -> bool {
    rows > 0
        && columns > 0
        && rows
            .checked_mul(columns)
            .is_some_and(|n| n <= super::MAX_CELLS)
}

/// New cell text stays upright. Fit its inset box inside the affine cell,
/// rather than assuming the cell's first corner is the world's top-left.
pub fn new_text(
    id: u64,
    frame: Frame,
    text: String,
    mut format: TextFormat,
) -> Result<Annotation, String> {
    if text.len() > crate::document::MAX_ANNOTATION_TEXT {
        return Err("A text box can hold at most 64 KB of text".into());
    }
    let (lo, hi) = text_bounds(frame).ok_or("Invalid cell frame")?;
    let width = (hi.x - lo.x) * crate::style::DEFAULT.points_per_world();
    if !ordinary_frame(frame) {
        if width < 10. {
            return Err("This cell is too narrow to add visible text.".into());
        }
        let width = width.min(2000.);
        format.width_pt = Some(format.width_pt.map_or(width, |old| old.min(width)));
    }
    format.validate(&text)?;
    let annotation = Annotation {
        id,
        position: lo,
        text,
        format,
        angle: 0.,
    };
    let layout = crate::scene::table_annotation_layout(&annotation, frame);
    // Near-singular or very small cells can contain no readable glyph even
    // though their geometry validates. Refuse instead of adding hidden text.
    let visible = layout.fragments.iter().any(|fragment| {
        let at = lo.offset(fragment.position.x, fragment.position.y);
        crate::style::text_ink_boxes(&fragment.text, fragment.style.size(), &fragment.style)
            .into_iter()
            .any(|(a, b)| {
                b.x > a.x
                    && b.y > a.y
                    && [a, Point::new(b.x, a.y), b, Point::new(a.x, b.y)]
                        .into_iter()
                        .all(|p| frame.contains(at.offset(p.x, p.y)))
            })
    });
    visible
        .then_some(annotation)
        .ok_or_else(|| "This cell is too narrow to add visible text.".into())
}

fn text_bounds(frame: Frame) -> Option<(Point, Point)> {
    frame.local(frame.origin)?;
    if ordinary_frame(frame) {
        return Some((
            frame.point(Point::new(0.05, 0.08)),
            frame.point(Point::new(0.95, 0.92)),
        ));
    }
    // Start with the safe inset's axis-aligned bounds, then shrink uniformly
    // until both inverse-frame rows enclose the complete upright rectangle.
    let (x, y) = (frame.axis_x, frame.axis_y);
    let (xx, xy, yx, yy) = (
        f64::from(x.x),
        f64::from(x.y),
        f64::from(y.x),
        f64::from(y.y),
    );
    let determinant = xx * yy - xy * yx;
    let (hx, hy) = (
        0.45 * xx.abs() + 0.42 * yx.abs(),
        0.45 * xy.abs() + 0.42 * yy.abs(),
    );
    let scale = (0.45 / ((yy * hx).abs() + (yx * hy).abs()) * determinant.abs())
        .min(0.42 / ((xy * hx).abs() + (xx * hy).abs()) * determinant.abs())
        .min(1.);
    let centre = frame.point(Point::new(0.5, 0.5));
    let (hx, hy) = ((hx * scale) as f32, (hy * scale) as f32);
    let (lo, hi) = (centre.offset(-hx, -hy), centre.offset(hx, hy));
    (lo.is_finite() && hi.is_finite() && lo.x < hi.x && lo.y < hi.y).then_some((lo, hi))
}

fn ordinary_frame(frame: Frame) -> bool {
    frame.axis_x.y == 0. && frame.axis_y.x == 0. && frame.axis_x.x > 0. && frame.axis_y.y > 0.
}

pub fn dragged(
    id: u64,
    start: Point,
    end: Point,
    grid: [usize; 2],
    width_pt: f32,
) -> Result<Graphic, String> {
    if !valid_grid(grid) {
        return Err("Use positive row and column counts with at most 4096 cells.".into());
    }
    let [rows, columns] = grid;
    let mut graphic = Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        start,
        end,
        Default::default(),
        Default::default(),
        false,
    );
    graphic.kind = GraphicKind::Table;
    graphic.table = Some(Box::new(Table {
        cells: (0..rows * columns)
            .map(|i| {
                let (row, column) = (i / columns, i % columns);
                Cell {
                    id: (i + 1) as u32,
                    lo: Point::new(column as f32 / columns as f32, row as f32 / rows as f32),
                    hi: Point::new(
                        (column + 1) as f32 / columns as f32,
                        (row + 1) as f32 / rows as f32,
                    ),
                    members: vec![],
                    borders: [Side::Left, Side::Top, Side::Bottom, Side::Right]
                        .map(|side| Border {
                            side,
                            width_pt,
                            color: Color::Ink,
                            pattern: LinePattern::Solid,
                        })
                        .to_vec(),
                }
            })
            .collect(),
    }));
    super::validate_graphic(&graphic)?;
    Ok(graphic)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upright_inset_box_stays_inside_rotated_mirrored_and_sheared_cells() {
        for (axis_x, axis_y) in [
            (Point::new(0., 600.), Point::new(-400., 0.)),
            (Point::new(-600., 0.), Point::new(0., -400.)),
            (Point::new(-600., 0.), Point::new(0., 400.)),
            (Point::new(600., 120.), Point::new(1500., 700.)),
            (Point::new(-600., 120.), Point::new(1500., 700.)),
            (Point::new(600., 0.), Point::new(5000., 1000.)),
        ] {
            let frame = Frame {
                origin: Point::new(2000., 2000.),
                axis_x,
                axis_y,
            };
            let (lo, hi) = text_bounds(frame).unwrap();
            assert!(hi.x > lo.x && hi.y > lo.y);
            for point in [lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)] {
                let local = frame.local(point).unwrap();
                assert!((0.049..=0.951).contains(&local.x), "{local:?}");
                assert!((0.079..=0.921).contains(&local.y), "{local:?}");
            }
        }
        assert!(
            text_bounds(Frame {
                origin: Point::default(),
                axis_x: Point::new(600., 0.),
                axis_y: Point::new(600., 0.)
            })
            .is_none()
        );
    }
    #[test]
    fn drawn_grid_has_ordered_empty_native_cells_and_rejects_invalid_sizes() {
        let graphic =
            dragged(1, Point::new(30., 20.), Point::new(330., 180.), [2, 3], 0.6).unwrap();
        let table = graphic.table.as_ref().unwrap();
        assert_eq!(table.cells.len(), 6);
        assert!(
            table
                .cells
                .iter()
                .all(|cell| cell.members.is_empty() && cell.borders.len() == 4)
        );
        assert_eq!(table.cells[4].lo, Point::new(1. / 3., 0.5));
        for grid in [[0, 3], [2, 0], [usize::MAX, 2], [65, 65]] {
            assert!(dragged(1, Point::default(), Point::new(100., 100.), grid, 0.6).is_err());
        }
    }
}
