//! Recognize native closed ellipses owned by projected aromatic rings.
use super::*;

pub(super) fn remove(tree: &mut Tree, prepared: &PreparedCdxml) -> Result<Vec<usize>> {
    let mut document = Document {
        drawing_style: prepared.drawing_style.clone().into_document()?,
        ..Default::default()
    };
    for (&id, p) in prepared
        .molecule
        .ids
        .iter()
        .zip(&prepared.molecule.positions)
    {
        document.add_atom("C", Point::new((p.x * 28.) as f32, (-p.y * 28.) as f32));
        document.atoms.last_mut().ok_or(SceneError::Limit)?.id = id;
    }
    document.bonds = prepared
        .bonds
        .iter()
        .cloned()
        .map(|b| b.into_document().map_err(SceneError::from))
        .collect::<Result<_>>()?;
    let circles = crate::aromatic::circles(&document);
    let mut removed = Vec::new();
    for index in tree.descendants(0)? {
        let node = tree.node(index)?;
        if node.tag != "curve"
            || node.attr("Closed") != Some("yes")
            || node.attr("FillType").is_some_and(|v| v != "None")
            || ["ArrowheadHead", "ArrowheadTail"]
                .iter()
                .any(|key| node.attr(key).is_some_and(|v| v != "None"))
        {
            continue;
        }
        let values = node
            .attr("CurvePoints")
            .unwrap_or("")
            .split_whitespace()
            .map(super::super::numeric::float)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if values.len() != 24 {
            continue;
        }
        let points: Vec<_> = values
            .chunks_exact(2)
            .map(|xy| {
                let [x, y] = xy else {
                    return Err(SceneError::Limit);
                };
                Ok(Point::new(
                    (x * prepared.source_scale) as f32,
                    (y * prepared.source_scale) as f32,
                ))
            })
            .collect::<Result<_>>()?;
        let mut owned = false;
        for circle in &circles {
            tree.spend(12)?;
            let Some([u, v]) = circle.projected_axes else {
                continue;
            };
            let det = u.x * v.y - u.y * v.x;
            if det.abs() < 0.001 {
                continue;
            }
            let local: Vec<_> = points
                .iter()
                .map(|p| {
                    let x = (p.x - circle.center.x) / circle.radius;
                    let y = (p.y - circle.center.y) / circle.radius;
                    Point::new((x * v.y - y * v.x) / det, (y * u.x - x * u.y) / det)
                })
                .collect();
            // Four opposite anchors on the unit circle and tangent controls.
            // Native ellipses can be inset further along the short axis.
            // Require the same center and opposite anchors, within the ring.
            let anchors: Vec<_> = local.iter().skip(1).step_by(3).copied().collect();
            let center = Point::new(
                anchors.iter().map(|p| p.x).sum::<f32>() / 4.,
                anchors.iter().map(|p| p.y).sum::<f32>() / 4.,
            );
            if center.distance(Point::default()) > 0.2 {
                continue;
            }
            owned = local.iter().enumerate().all(|(i, p)| {
                let expected = if i % 3 == 1 { 1. } else { 1.142_374 };
                (p.distance(center) - expected).abs() < 0.35
            }) && anchors
                .iter()
                .zip(anchors.iter().cycle().skip(2))
                .all(|(a, b)| {
                    Point::new(a.x + b.x, a.y + b.y)
                        .distance(Point::new(2. * center.x, 2. * center.y))
                        < 0.02
                });
            if owned {
                break;
            }
        }
        if owned {
            removed.push(index);
            tree.detach(index)?;
        }
    }
    Ok(removed)
}
