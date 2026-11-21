//! Native ChemDraw TLC objects, never a flattened collection of ellipses.
use super::*;
use crate::graphics::{Graphic, LinePattern, TlcPlate};
impl Writer<'_> {
    pub(super) fn tlc(&mut self, g: &Graphic, tlc: &TlcPlate, z: usize) -> Result<Key> {
        g.validate().map_err(invalid)?;
        if !g.style.effects.is_plain()
            || g.style.fill.is_some()
            || g.style.pattern != LinePattern::Solid
            || g.style.corner_pt.is_some()
        {
            return Err(invalid(
                "This TLC plate appearance requires native or image export",
            ));
        }
        self.spend(tlc.lanes.len() + tlc.lanes.iter().map(|l| l.spots.len()).sum::<usize>())?;
        let id = self.id()?;
        let color = self.color(g.style.stroke)?;
        let (lo, hi) = g.bounds();
        let plate = self.tree.add(
            Some(self.page),
            "tlcplate",
            [
                ("id", id),
                ("Z", z.to_string()),
                ("color", color),
                ("LineWidth", number(real(g.style.width_pt))),
                (
                    "BoundingBox",
                    format!("{} {}", self.position(lo), self.position(hi)),
                ),
                ("TopLeft", self.position(tlc.point(g, 0., 0.))),
                ("TopRight", self.position(tlc.point(g, 1., 0.))),
                ("BottomRight", self.position(tlc.point(g, 1., 1.))),
                ("BottomLeft", self.position(tlc.point(g, 0., 1.))),
                ("OriginFraction", number(tlc.origin_fraction)),
                ("SolventFrontFraction", number(tlc.solvent_front_fraction)),
                ("ShowOrigin", yes(tlc.show_origin).into()),
                ("ShowSolventFront", yes(tlc.show_solvent_front).into()),
                ("ShowBorders", yes(tlc.show_borders).into()),
            ],
        )?;
        for (index, lane) in tlc.lanes.iter().enumerate() {
            let id = self.id()?;
            let lane_node = self.tree.add(Some(plate), "tlclane", [("id", id)])?;
            for spot in &lane.spots {
                if spot
                    .style
                    .fill
                    .is_some_and(|fill| fill != spot.style.stroke)
                {
                    return Err(invalid("A native TLC spot has one stroke/fill color"));
                }
                let dimension = |v: f64| -> Result<String> {
                    let raw = (v * 65536.).round_ties_even();
                    if !(1. ..=f64::from(i32::MAX)).contains(&raw) {
                        return Err(invalid("TLC spot dimensions exceed signed 16.16 storage"));
                    }
                    Ok(format!("{raw:.0}"))
                };
                let id = self.id()?;
                let color = self.color(spot.style.stroke)?;
                let flags = usize::from(spot.style.fill.is_some()) * 128
                    + usize::from(spot.style.pattern == LinePattern::Dashed) * 2;
                let n = self.tree.add(
                    Some(lane_node),
                    "tlcspot",
                    [
                        ("id", id),
                        ("Rf", number(spot.rf)),
                        ("Width", dimension(spot.width_pt)?),
                        ("Height", dimension(spot.height_pt)?),
                        ("Tail", "0".into()),
                        ("color", color),
                        ("LineWidth", number(real(spot.style.width_pt))),
                        ("CurveType", flags.to_string()),
                        ("ShowRf", yes(spot.show_rf).into()),
                    ],
                )?;
                if let Some(label) = tlc.label_annotation(g, index, spot) {
                    let id = self.id()?;
                    let tag = self.tree.add(
                        Some(n),
                        "objecttag",
                        [
                            ("id", id),
                            ("Name", "rf".into()),
                            ("TagType", "Unknown".into()),
                            ("Visible", "yes".into()),
                        ],
                    )?;
                    let layout = crate::typography::layout(&label.text, &label.format);
                    let scale = crate::style::DEFAULT.points_per_world();
                    let baseline = layout
                        .fragments
                        .first()
                        .map(|run| run.position.y + crate::style::text_ascent(&run.style))
                        .unwrap_or_else(|| crate::style::text_ascent(&label.format.style));
                    let measured = TextMetrics {
                        width: layout.width * scale,
                        height: layout.height * scale,
                        baseline: baseline * scale,
                    };
                    let text = self.annotation(&label, Some(&measured))?;
                    self.tree.attach(tag, text)?;
                }
            }
        }
        Ok(plate)
    }
}
