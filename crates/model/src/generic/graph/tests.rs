use super::*;
use crate::document::Point;
use std::cell::Cell;

#[test]
fn blocked_edge_reach_excludes_both_directions() -> Result<(), String> {
    let mut doc = Document::default();
    let [a, b, c, d] = std::array::from_fn(|_| doc.add_atom("C", Point::default()));
    let graph = IndexedGraph::from_edges(
        &doc,
        [(a, b), (b, c), (c, d)].into_iter(),
        "Missing test endpoint",
    )?;
    assert_eq!(graph.reach(0, None)?, vec![true; 4]);
    assert_eq!(
        graph.reach(0, Some((1, 2)))?,
        vec![true, true, false, false]
    );
    assert_eq!(
        graph.reach(3, Some((1, 2)))?,
        vec![false, false, true, true]
    );
    assert_eq!(
        graph.reach(0, Some((2, 1)))?,
        vec![true, true, false, false]
    );
    Ok(())
}

#[test]
fn components_keep_atom_scan_and_stack_pop_order() -> Result<(), String> {
    let mut doc = Document::default();
    let [a, b, c, _d, e, f, g, h] = std::array::from_fn(|_| doc.add_atom("C", Point::default()));
    let graph = IndexedGraph::from_edges(
        &doc,
        [(a, c), (a, f), (f, c), (f, h), (b, e), (b, g)].into_iter(),
        "Missing test endpoint",
    )?;
    assert_eq!(
        graph.components()?,
        (
            vec![0, 1, 0, 2, 1, 0, 1, 0],
            vec![vec![0, 5, 7, 2], vec![1, 6, 4], vec![3]],
        )
    );
    assert_eq!(
        graph.adjacent,
        vec![
            vec![2, 5],
            vec![4, 6],
            vec![0, 5],
            vec![],
            vec![1],
            vec![0, 2, 7],
            vec![1],
            vec![5],
        ]
    );
    Ok(())
}

#[test]
fn connect_resolves_both_endpoints_before_filtering() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let mut graph = IndexedGraph::index(&doc);
    let called = Cell::new(false);
    assert_eq!(
        graph.connect(
            std::iter::once((a, a + 1)),
            "Missing test endpoint",
            |_, _| {
                called.set(true);
                false
            }
        ),
        Err("Missing test endpoint".into())
    );
    assert!(!called.get());
    assert_eq!(graph.adjacent, vec![Vec::<usize>::new()]);
}
