//! Original empty grids for the native table tool. Existing object ownership
//! is assigned separately; creating a grid never captures nearby chemistry.
use super::{Border, Cell, Color, Graphic, GraphicKind, LinePattern, Point, Side, Table};

pub const DEFAULT_GRID: [usize; 2] = [2, 3];

pub fn valid_grid([rows, columns]: [usize; 2]) -> bool {
    rows > 0
        && columns > 0
        && rows
            .checked_mul(columns)
            .is_some_and(|n| n <= super::MAX_CELLS)
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
