//! Recover only explicitly owned polygons whose points still match their atoms.
use super::*;

pub(super) fn read(
    nodes: &[Node<'_, '_>],
    order: &[usize],
    tree: &Tree,
    prepared: &PreparedCdxml,
    association: &PreparedAtoms<'_>,
) -> Result<Vec<(usize, crate::ring_fills::RingFill)>> {
    let by_id: HashMap<_, _> = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.attribute("id").map(|id| (id, i)))
        .collect();
    let edges: BTreeSet<_> = prepared
        .bonds
        .iter()
        .map(|b| (b.a.min(b.b), b.a.max(b.b)))
        .collect();
    let mut result = vec![];
    for (source, node) in nodes.iter().enumerate().filter(|(_, n)| {
        n.has_tag_name("curve") && n.attribute("Name") == Some("ReShiki ring fill")
    }) {
        let closed = node.attribute("Closed") == Some("yes")
            || node
                .attribute("CurveType")
                .and_then(|s| s.parse::<u16>().ok())
                .is_some_and(|flags| flags & 1 != 0);
        if node.attribute("FillType") != Some("Solid") || !closed {
            continue;
        }
        let basis: Vec<_> = node
            .attribute("BasisObjects")
            .unwrap_or("")
            .split_whitespace()
            .collect();
        if !(3..=64).contains(&basis.len()) {
            continue;
        }
        let count = basis.len();
        let mut ids = vec![];
        let mut expected = vec![];
        for name in basis {
            let Some((ordinal, atom)) = by_id
                .get(name)
                .and_then(|&i| nodes.get(i).map(|n| (i, n)))
                .filter(|(_, n)| n.has_tag_name("n"))
            else {
                break;
            };
            let Some(position) = atom.attribute("p") else {
                break;
            };
            let p = point(position, prepared.source_scale)?;
            let id = association.identify_with_tolerance(
                tree,
                *order.get(ordinal).ok_or(SceneError::Limit)?,
                p,
                0.01,
                "Could not associate ring fill with its atom",
            )?;
            ids.push(id);
            expected.extend([p; 3]);
        }
        let values = node
            .attribute("CurvePoints")
            .unwrap_or("")
            .split_whitespace()
            .collect::<Vec<_>>();
        if ids.len() != count
            || values.len() != expected.len() * 2
            || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
            || !ids
                .iter()
                .zip(ids.iter().cycle().skip(1))
                .all(|(a, b)| edges.contains(&((*a).min(*b), (*a).max(*b))))
        {
            continue;
        }
        let mut matches = true;
        for (xy, expected) in values.chunks_exact(2).zip(expected) {
            let p = point(&xy.join(" "), prepared.source_scale)?;
            matches &= (p.x - expected.x).abs() < 0.02 && (p.y - expected.y).abs() < 0.02;
        }
        if !matches {
            continue;
        }
        let color_index = node
            .attribute("color")
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| SceneError::Invalid("Invalid ring fill color"))?;
        let color = prepared
            .palette
            .colors
            .get(color_index)
            .ok_or(SceneError::Invalid("Missing ring fill color"))?
            .into_document()?;
        result.push((source, crate::ring_fills::RingFill { atoms: ids, color }));
    }
    Ok(result)
}
