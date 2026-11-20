//! Exact compound fills joined by cancelling bridges for editable exchange.
use super::{Error, P, PathCommand, Result, Writer, invalid};

/// SVG closes the current contour before a following draw command. Make that
/// implicit new subpath explicit for both the fill and CDX control triples.
pub(super) fn explicit_subpaths(commands: &[PathCommand]) -> std::borrow::Cow<'_, [PathCommand]> {
    let mut first = None;
    let mut closed = false;
    let mut out: Option<Vec<PathCommand>> = None;
    for (i, command) in commands.iter().enumerate() {
        match command {
            PathCommand::Move(p) => {
                first = Some(*p);
                closed = false;
            }
            PathCommand::Close => closed = true,
            PathCommand::Line(_) | PathCommand::Cubic(..) if closed => {
                if let Some(first) = first {
                    out.get_or_insert_with(|| commands.iter().take(i).cloned().collect())
                        .push(PathCommand::Move(first));
                }
                closed = false;
            }
            _ => {}
        }
        if let Some(out) = &mut out {
            out.push(command.clone());
        }
    }
    out.map_or(
        std::borrow::Cow::Borrowed(commands),
        std::borrow::Cow::Owned,
    )
}

impl Writer<'_> {
    /// ChemDraw fills one closed curve by NonZero winding. Each extra contour
    /// returns to the same base anchor along the reverse of its entry bridge.
    /// These line segments cancel, even when they cross another contour.
    /// Duplicate closing anchors retain each original incoming Bézier control.
    pub(super) fn joined_fill(&mut self, paths: &[(Vec<P>, bool)]) -> Result<Vec<P>> {
        let Some((base, true)) = paths.first() else {
            return Err(invalid("A compound fill needs closed contours"));
        };
        let count = paths.iter().try_fold(0usize, |n, (p, closed)| {
            if !closed || p.len() < 6 || !p.len().is_multiple_of(3) {
                return Err(Error::Limit);
            }
            n.checked_add(p.len()).ok_or(Error::Limit)
        })?;
        let extra = paths
            .len()
            .saturating_sub(1)
            .checked_mul(6)
            .ok_or(Error::Limit)?;
        let count = count.checked_add(extra).ok_or(Error::Limit)?;
        // One imported native Path has at most 20,000 commands. Reserve its
        // Move, implicit closing cubic and Close; CDX also fits its u16 count.
        if count / 3 > 19_998 {
            return Err(Error::Limit);
        }
        self.spend(count)?;
        let hub = *base.get(1).ok_or(Error::Limit)?;
        let mut points = Vec::with_capacity(count);
        points.extend_from_slice(base);
        if paths.len() == 1 {
            return Ok(points);
        }
        if let Some(incoming) = points.first_mut() {
            *incoming = hub;
        }
        points.extend([*base.first().ok_or(Error::Limit)?, hub, hub]);
        for (i, (path, _)) in paths.iter().enumerate().skip(1) {
            let at = *path.get(1).ok_or(Error::Limit)?;
            points.extend([at, at, *path.get(2).ok_or(Error::Limit)?]);
            points.extend_from_slice(path.get(3..).ok_or(Error::Limit)?);
            points.extend([*path.first().ok_or(Error::Limit)?, at, at]);
            if i + 1 < paths.len() {
                points.extend([hub; 3]);
            }
        }
        Ok(points)
    }
}
