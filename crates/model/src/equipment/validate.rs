//! Checks the schema cannot express: references, limits and joint geometry.
//! Every message names the field so an author can fix it.
use super::format::*;
use super::geometry::{
    Seg, add, area, distance_to_outline, flatten, inside, mul, normalize, parse_path, perp,
};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::overlay::FloatOverlay;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_FILE_BYTES: usize = 1 << 20;
pub const MAX_SHAPES: usize = 256;
pub const MAX_JOINTS: usize = 32;
pub const MAX_HOLDS: usize = 16;
pub const MAX_PARAMS: usize = 16;
pub const MAX_PARTS: usize = 32;
pub const MAX_LIQUIDS: usize = 8;
pub const MAX_PATH_COMMANDS: usize = 20_000;
pub const MAX_COORDINATE: f64 = 2000.;
pub const MAX_STRING: usize = 200;
pub const MAX_KEYWORDS: usize = 64;
pub const MAX_STATES: usize = 64;
pub const MAX_PART_DEPTH: usize = 4;
/// How far a joint may sit from the glass it belongs to.
const ATTACH_MM: f64 = 0.5;

pub fn validate(d: &Definition) -> Result<(), String> {
    if d.format != FORMAT {
        return Err("This is not a ReShiki piece file.".into());
    }
    if d.format_version > FORMAT_VERSION {
        return Err(format!(
            "This piece was made with a newer version of ReShiki (piece format {}). Update ReShiki to open it.",
            d.format_version
        ));
    }
    if !valid_id(&d.id) {
        return Err(format!(
            "The ID \"{}\" must be namespace/name, each 1–64 lowercase letters, digits or hyphens.",
            d.id
        ));
    }
    if d.revision == 0 {
        return Err("The revision must be 1 or more.".into());
    }
    if !d.names.contains_key("en") {
        return Err("The piece needs an English name in \"names\": {\"en\": …}.".into());
    }
    strings(d)?;
    if d.keywords.len() > MAX_KEYWORDS {
        return Err(format!(
            "The piece has {} keywords; the limit is {MAX_KEYWORDS}.",
            d.keywords.len()
        ));
    }
    for (what, n, limit) in [
        ("shapes", d.shapes.len(), MAX_SHAPES),
        ("joints", d.joints.len(), MAX_JOINTS),
        ("holds", d.holds.len(), MAX_HOLDS),
        ("parameters", d.params.len(), MAX_PARAMS),
        ("parts", d.parts.len(), MAX_PARTS),
        ("liquid regions", d.liquids.len(), MAX_LIQUIDS),
    ] {
        if n > limit {
            return Err(format!("The piece has {n} {what}; the limit is {limit}."));
        }
    }
    unique("shape", d.shapes.iter().map(|s| s.id.as_str()))?;
    unique("joint", d.joints.iter().map(Joint::id))?;
    unique("hold", d.holds.iter().map(|h| h.id.as_str()))?;
    unique("parameter", d.params.iter().map(Param::id))?;
    unique("part", d.parts.iter().map(|p| p.id.as_str()))?;
    unique("liquid region", d.liquids.iter().map(|l| l.id.as_str()))?;
    unique("label anchor", d.labels.iter().map(|l| l.id.as_str()))?;

    let params: BTreeMap<&str, &Param> = d.params.iter().map(|p| (p.id(), p)).collect();
    let parts: BTreeMap<&str, &Part> = d.parts.iter().map(|p| (p.id.as_str(), p)).collect();
    let liquids: BTreeSet<&str> = d.liquids.iter().map(|l| l.id.as_str()).collect();
    for p in &d.params {
        param(p, &liquids)?;
    }
    for p in &d.parts {
        part(p, &params, &parts)?;
    }

    // Shapes: paths, states, parts, limits.
    let mut commands = 0;
    let mut outlines = Vec::new();
    for s in &d.shapes {
        let segs = parse_path(&s.path)
            .map_err(|e| format!("Shape \"{}\" has a path ReShiki cannot read: {e}.", s.id))?;
        commands += segs.len();
        if commands > MAX_PATH_COMMANDS {
            return Err(format!(
                "Shape \"{}\" brings the piece to {} path points; the limit is {}. Simplify it.",
                s.id,
                group(commands),
                group(MAX_PATH_COMMANDS)
            ));
        }
        coordinates(&format!("Shape \"{}\"", s.id), &segs)?;
        if let Some(k) = &s.part
            && !parts.contains_key(k.as_str())
        {
            return Err(format!("Shape \"{}\" names a missing part \"{k}\".", s.id));
        }
        when(&format!("Shape \"{}\"", s.id), &s.when, &params)?;
        if !(-8..=8).contains(&s.depth) {
            return Err(format!(
                "Shape \"{}\" has depth {}; use −8 to 8.",
                s.id, s.depth
            ));
        }
        outlines.push((s, flatten(&segs)));
    }
    let glass: Vec<_> = outlines
        .iter()
        .filter(|(s, _)| s.role == Role::Glass)
        .map(|(_, o)| o)
        .collect();
    let defaults: BTreeMap<&str, &str> = d
        .params
        .iter()
        .filter_map(|p| match p {
            Param::States { id, default, .. } => Some((id.as_str(), default.as_str())),
            _ => None,
        })
        .collect();

    if let Some(t) = &d.tube
        && t.ends[0] == t.ends[1]
    {
        return Err("The tube's two ends must be different joints.".into());
    }
    for j in &d.joints {
        let tube_end = d
            .tube
            .as_ref()
            .is_some_and(|t| t.ends.iter().any(|e| e == j.id()));
        joint(j, &params, &parts, &outlines, &defaults, tube_end)?;
    }
    for h in &d.holds {
        let [lo, hi] = h.diameter_mm.range();
        if !(lo.is_finite() && hi.is_finite() && lo > 0. && lo <= hi) {
            return Err(format!(
                "Hold \"{}\" needs a positive diameter, or a range from the smaller to the larger.",
                h.id
            ));
        }
        point(&format!("Hold \"{}\"", h.id), h.from)?;
        point(&format!("Hold \"{}\"", h.id), h.to)?;
        if h.from == h.to {
            return Err(format!("Hold \"{}\" needs two different ends.", h.id));
        }
        // A seat names a joint or a hold by ID, so one ID cannot be both.
        if d.joints.iter().any(|j| j.id() == h.id) {
            return Err(format!("Hold \"{}\" has the ID of a joint.", h.id));
        }
        if let Some(k) = &h.part
            && !parts.contains_key(k.as_str())
        {
            return Err(format!("Hold \"{}\" names a missing part \"{k}\".", h.id));
        }
        when(&format!("Hold \"{}\"", h.id), &h.when, &params)?;
        if let Some(k) = &h.fit {
            let mm_range = d
                .params
                .iter()
                .any(|p| matches!(p, Param::Range { id, unit: Unit::Mm, .. } if id == k));
            if h.kind.held() || !mm_range {
                return Err(format!(
                    "Hold \"{}\" fits \"{k}\": only a jaw or slider fits, and only a millimetre range parameter.",
                    h.id
                ));
            }
        }
    }
    for l in &d.liquids {
        let segs = parse_path(&l.region).map_err(|e| {
            format!(
                "The liquid region \"{}\" has a path ReShiki cannot read: {e}.",
                l.id
            )
        })?;
        commands += segs.len();
        if commands > MAX_PATH_COMMANDS {
            return Err(format!(
                "The liquid region \"{}\" brings the piece to {} path points; the limit is {}. Simplify it.",
                l.id,
                group(commands),
                group(MAX_PATH_COMMANDS)
            ));
        }
        coordinates(&format!("The liquid region \"{}\"", l.id), &segs)?;
        let region = flatten(&segs);
        if region
            .iter()
            .any(|(points, closed)| !closed || points.len() < 3)
        {
            return Err(format!(
                "The liquid region \"{}\" must be closed: end each outline with Z.",
                l.id
            ));
        }
        // Whatever of the region lies outside the glass, by area.
        // One winding for every contour, so overlapping glass shapes unite.
        let wound = |p: &Vec<Xy>| {
            let mut p = p.clone();
            if area(&p) < 0. {
                p.reverse();
            }
            p
        };
        let subject: Vec<Vec<Xy>> = region.iter().map(|(p, _)| wound(p)).collect();
        let clip: Vec<Vec<Xy>> = glass
            .iter()
            .flat_map(|o| o.iter().map(|(p, _)| wound(p)))
            .collect();
        let outside: f64 = FloatOverlay::with_subj_and_clip(&subject, &clip)
            .overlay(OverlayRule::Difference, FillRule::NonZero)
            .iter()
            .flatten()
            .map(|c| area(c))
            .sum::<f64>()
            .abs();
        let total: f64 = subject.iter().map(|c| area(c)).sum::<f64>().abs();
        if outside > (0.01 * total).max(0.5) {
            return Err(format!(
                "The liquid region \"{}\" is outside the glass.",
                l.id
            ));
        }
    }
    for l in &d.labels {
        point(&format!("Label anchor \"{}\"", l.id), l.at)?;
    }
    if let Some(c) = &d.clip
        && taper(&c.size).is_none()
    {
        return Err(format!(
            "The clip size \"{}\" is not a standard taper size.",
            c.size
        ));
    }
    if let Some(t) = &d.tube {
        if !(t.width_mm.is_finite() && t.width_mm > 0.) {
            return Err("The tube needs a positive width.".into());
        }
        for end in &t.ends {
            if !d
                .joints
                .iter()
                .any(|j| j.id() == end && matches!(j, Joint::Plug { .. }))
            {
                return Err(format!(
                    "The tube end \"{end}\" must be a plug joint of this piece."
                ));
            }
        }
    }
    if d.shapes.is_empty() && d.tube.is_none() {
        return Err("The piece has no shapes.".into());
    }
    Ok(())
}

fn valid_id(id: &str) -> bool {
    let part = |s: &str| {
        (1..=64).contains(&s.len())
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    matches!(id.split_once('/'), Some((ns, name)) if part(ns) && part(name))
}

fn strings(d: &Definition) -> Result<(), String> {
    let mut all: Vec<&str> = vec![d.family.as_str()];
    all.extend(d.names.values().map(String::as_str));
    all.extend(d.keywords.iter().map(String::as_str));
    all.extend(d.variant.iter().flat_map(|(k, v)| [k.as_str(), v.as_str()]));
    all.extend(d.credits.author.as_deref());
    all.extend(d.credits.license.as_deref());
    for p in &d.params {
        all.extend(p.names().values().map(String::as_str));
        if let Param::States { states, .. } = p {
            for s in states {
                all.extend(s.names.values().map(String::as_str));
            }
        }
    }
    for s in all {
        if s.chars().count() > MAX_STRING || s.chars().any(char::is_control) {
            return Err(format!(
                "The text \"{}\" must be printable and at most {MAX_STRING} characters.",
                s.chars().take(40).collect::<String>()
            ));
        }
    }
    Ok(())
}

fn unique<'a>(what: &str, ids: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if id.is_empty() || id.len() > 64 {
            return Err(format!("A {what} needs an ID of 1–64 characters."));
        }
        if !seen.insert(id) {
            return Err(format!("Two {what}s are named \"{id}\". Rename one."));
        }
    }
    Ok(())
}

fn group(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn english(names: &BTreeMap<String, String>, id: &str) -> String {
    names.get("en").cloned().unwrap_or_else(|| id.to_string())
}

fn param(p: &Param, liquids: &BTreeSet<&str>) -> Result<(), String> {
    if !p.names().contains_key("en") {
        return Err(format!("Parameter \"{}\" needs an English name.", p.id()));
    }
    if let Param::States { states, .. } = p
        && let Some(s) = states.iter().find(|s| !s.names.contains_key("en"))
    {
        return Err(format!(
            "State \"{}\" of \"{}\" needs an English name.",
            s.id,
            p.id()
        ));
    }
    let name = english(p.names(), p.id());
    match p {
        Param::Range {
            min, max, default, ..
        } => {
            if !(min.is_finite() && max.is_finite() && min < max) {
                return Err(format!(
                    "{name} runs from {min} to {max}; the minimum must be below the maximum."
                ));
            }
            if !(min..=max).contains(&default) {
                return Err(format!(
                    "{name} runs from {min} to {max}; the default {default} is outside it."
                ));
            }
        }
        Param::States {
            states, default, ..
        } => {
            if states.is_empty() || states.len() > MAX_STATES {
                return Err(format!("{name} needs 1–{MAX_STATES} states."));
            }
            unique("state", states.iter().map(|s| s.id.as_str()))?;
            if !states.iter().any(|s| &s.id == default) {
                return Err(format!("{name} has no state \"{default}\" to start in."));
            }
        }
        Param::Fill {
            liquid, default, ..
        } => {
            if !liquids.contains(liquid.as_str()) {
                return Err(format!(
                    "{name} names a missing liquid region \"{liquid}\"."
                ));
            }
            if !(0. ..=1.).contains(default) {
                return Err(format!(
                    "{name} runs from 0 to 1; the default {default} is outside it."
                ));
            }
        }
        Param::Color { liquid, .. } => {
            if !liquids.contains(liquid.as_str()) {
                return Err(format!(
                    "{name} names a missing liquid region \"{liquid}\"."
                ));
            }
        }
    }
    Ok(())
}

fn part(
    p: &Part,
    params: &BTreeMap<&str, &Param>,
    parts: &BTreeMap<&str, &Part>,
) -> Result<(), String> {
    let Some(param) = params.get(p.param.as_str()) else {
        return Err(format!(
            "Part \"{}\" names a missing parameter \"{}\".",
            p.id, p.param
        ));
    };
    match (&p.motion, param) {
        (Motion::PerState(states), Param::States { states: known, .. }) => {
            for s in states.keys() {
                if !known.iter().any(|k| &k.id == s) {
                    return Err(format!(
                        "Part \"{}\" moves in a missing state \"{s}\".",
                        p.id
                    ));
                }
            }
        }
        (Motion::PerState(_), _) => {
            return Err(format!(
                "Part \"{}\" moves per state, but \"{}\" has no states.",
                p.id, p.param
            ));
        }
        (_, Param::Range { .. } | Param::Fill { .. }) => {}
        _ => {
            return Err(format!(
                "Part \"{}\" moves by a number, but \"{}\" is not a number.",
                p.id, p.param
            ));
        }
    }
    let finite = match &p.motion {
        Motion::Translate(v) => v.iter().all(|x| x.is_finite()),
        Motion::Rotate {
            pivot,
            degrees_per_unit,
        } => pivot.iter().all(|x| x.is_finite()) && degrees_per_unit.is_finite(),
        Motion::Scale { pivot, per_unit } => {
            pivot.iter().all(|x| x.is_finite()) && per_unit.is_finite()
        }
        Motion::Stretch {
            pivot,
            axis,
            per_unit,
        } => {
            pivot.iter().chain(axis).all(|x| x.is_finite())
                && per_unit.is_finite()
                && axis != &[0., 0.]
        }
        Motion::PerState(m) => m
            .values()
            .all(|m| crate::equipment::geometry::Affine(*m).inverse().is_some()),
    };
    if !finite {
        return Err(format!(
            "Part \"{}\" has a motion with a missing or infinite number.",
            p.id
        ));
    }
    // Bound what the motion can reach over the whole range, not only its numbers.
    let span = number_range(param).map_or(1., |(lo, hi, d)| (lo - d).abs().max((hi - d).abs()));
    let reach = 2. * MAX_COORDINATE;
    let bounded = match &p.motion {
        Motion::Translate(v) => v.iter().all(|x| (x * span).abs() <= reach),
        Motion::Rotate {
            pivot,
            degrees_per_unit,
        } => pivot.iter().all(|x| x.abs() <= reach) && (degrees_per_unit * span).abs() <= 3600.,
        Motion::Scale { pivot, per_unit }
        | Motion::Stretch {
            pivot, per_unit, ..
        } => pivot.iter().all(|x| x.abs() <= reach) && 1. + (per_unit * span).abs() <= 100.,
        Motion::PerState(m) => m.values().all(|[a, b, c, d, e, f]| {
            [a, b, c, d].iter().all(|x| x.abs() <= 100.) && e.abs() <= reach && f.abs() <= reach
        }),
    };
    if !bounded {
        return Err(format!(
            "Part \"{}\" moves too far over the range of \"{}\".",
            p.id, p.param
        ));
    }
    let shrink = match &p.motion {
        Motion::Scale { per_unit, .. } | Motion::Stretch { per_unit, .. } => Some(*per_unit),
        _ => None,
    };
    if let (Some(k), Some(range)) = (shrink, number_range(param)) {
        let (lo, hi, default) = range;
        if [lo, hi].iter().any(|v| 1. + k * (v - default) <= 0.) {
            return Err(format!(
                "Part \"{}\" shrinks to nothing within the range of \"{}\".",
                p.id, p.param
            ));
        }
    }
    let mut depth = 1;
    let mut parent = p.parent.as_deref();
    while let Some(k) = parent {
        let Some(next) = parts.get(k) else {
            return Err(format!("Part \"{}\" names a missing parent \"{k}\".", p.id));
        };
        depth += 1;
        if depth > MAX_PART_DEPTH {
            return Err(format!(
                "Part \"{}\" is nested more than {MAX_PART_DEPTH} deep, or its parents form a loop.",
                p.id
            ));
        }
        parent = next.parent.as_deref();
    }
    Ok(())
}

fn when(
    owner: &str,
    when: &BTreeMap<String, Vec<String>>,
    params: &BTreeMap<&str, &Param>,
) -> Result<(), String> {
    for (k, states) in when {
        let Some(Param::States { states: known, .. }) = params.get(k.as_str()) else {
            return Err(format!(
                "{owner} names states of \"{k}\", which has no states."
            ));
        };
        for s in states {
            if !known.iter().any(|x| &x.id == s) {
                return Err(format!("{owner} names a missing state \"{s}\" of \"{k}\"."));
            }
        }
    }
    Ok(())
}

/// A number parameter's minimum, maximum and default.
fn number_range(p: &Param) -> Option<(f64, f64, f64)> {
    match p {
        Param::Range {
            min, max, default, ..
        } => Some((*min, *max, *default)),
        Param::Fill { default, .. } => Some((0., 1., *default)),
        _ => None,
    }
}

fn point(owner: &str, p: Xy) -> Result<(), String> {
    if p.iter().all(|x| x.is_finite() && x.abs() <= MAX_COORDINATE) {
        Ok(())
    } else {
        Err(format!(
            "{owner} has a coordinate beyond ±{MAX_COORDINATE} mm."
        ))
    }
}

fn coordinates(owner: &str, segs: &[Seg]) -> Result<(), String> {
    for s in segs {
        match *s {
            Seg::Move(p) | Seg::Line(p) => point(owner, p)?,
            Seg::Cubic(a, b, p) => {
                point(owner, a)?;
                point(owner, b)?;
                point(owner, p)?;
            }
            Seg::Close => {}
        }
    }
    Ok(())
}

/// A joint position and direction, and the state it belongs to (none for the default).
type Pose<'a> = (Xy, Xy, Option<(&'a str, &'a str)>);

type Outline = Vec<(Vec<Xy>, bool)>;

fn joint(
    j: &Joint,
    params: &BTreeMap<&str, &Param>,
    parts: &BTreeMap<&str, &Part>,
    all: &[(&Shape, Outline)],
    defaults: &BTreeMap<&str, &str>,
    tube_end: bool,
) -> Result<(), String> {
    let id = j.id();
    let owner = format!("Joint \"{id}\"");
    point(&owner, j.at())?;
    let dir = j.dir();
    if !(dir.iter().all(|x| x.is_finite()) && dir != [0., 0.]) {
        return Err(format!("{owner} needs a direction."));
    }
    if let Some(k) = j.part()
        && !parts.contains_key(k)
    {
        return Err(format!("{owner} names a missing part \"{k}\"."));
    }
    let mut poses: Vec<Pose> = vec![(j.at(), dir, None)];
    for (k, by_state) in j.poses() {
        let states = BTreeMap::from([(k.clone(), by_state.keys().cloned().collect())]);
        when(&owner, &states, params)?;
        for (state, pose) in by_state {
            point(&owner, pose.at)?;
            if !(pose.dir.iter().all(|x| x.is_finite()) && pose.dir != [0., 0.]) {
                return Err(format!("{owner} needs a direction in every pose."));
            }
            poses.push((pose.at, pose.dir, Some((k.as_str(), state.as_str()))));
        }
    }
    if let Joint::Taper {
        size, draw, gender, ..
    } = j
    {
        if taper(size).is_none() {
            return Err(format!(
                "{owner} has size \"{size}\", which is not a standard taper size."
            ));
        }
        // A cone may draw its own end in rubber or PTFE, as a septum does;
        // a socket is always ground glass.
        if *draw != JointDraw::Standard && *gender == Gender::F {
            return Err(format!(
                "{owner} is ground glass, so ReShiki must draw it: use \"draw\": \"standard\"."
            ));
        }
    }
    if let Joint::Plug { size_mm, .. } = j
        && !(size_mm.is_finite() && *size_mm > 0.)
    {
        return Err(format!("{owner} needs a positive size."));
    }
    // A tube's ends follow the centre line stored on each placed copy.
    if tube_end {
        return Ok(());
    }
    for (at, dir, state) in poses {
        // Check each pose against the shapes drawn in its state.
        let shown: Vec<(&Shape, Outline)> = all
            .iter()
            .filter(|(s, _)| {
                s.when.iter().all(|(k, states)| {
                    let current = match state {
                        Some((param, value)) if param == k => Some(value),
                        _ => defaults.get(k.as_str()).copied(),
                    };
                    current.is_some_and(|c| states.iter().any(|x| x == c))
                })
            })
            .map(|(s, o)| (*s, o.clone()))
            .collect();
        let glass: Vec<&Outline> = shown
            .iter()
            .filter(|(s, _)| s.role == Role::Glass)
            .map(|(_, o)| o)
            .collect();
        let filled: Vec<&Outline> = shown
            .iter()
            .filter(|(s, _)| s.role != Role::Detail)
            .map(|(_, o)| o)
            .collect();
        attach(
            j,
            &owner,
            at,
            normalize(dir),
            parts,
            &glass,
            &filled,
            &shown,
        )?;
    }
    Ok(())
}

/// Where a joint meets its own glass, at one pose.
#[allow(clippy::too_many_arguments)]
fn attach(
    j: &Joint,
    owner: &str,
    at: Xy,
    u: Xy,
    parts: &BTreeMap<&str, &Part>,
    glass: &[&Outline],
    filled: &[&Outline],
    all: &[(&Shape, Outline)],
) -> Result<(), String> {
    let near = |p: Xy, outlines: &[&Outline]| {
        outlines
            .iter()
            .map(|o| {
                if inside(p, o) {
                    0.
                } else {
                    distance_to_outline(p, o)
                }
            })
            .fold(f64::INFINITY, f64::min)
    };
    let on_glass = |p: Xy| {
        glass
            .iter()
            .map(|o| distance_to_outline(p, o))
            .fold(f64::INFINITY, f64::min)
    };
    let far = |gap: f64| {
        format!("{owner} is {gap:.1} mm from the outline. Place it on the rim of the opening.")
    };
    let into = || {
        format!(
            "{owner} points into the glass. Flip its direction or click the other side of the opening."
        )
    };
    // Attachment points on the joint's own drawing. A point attachment repeats
    // its anchor; an open mouth needs both rim edges.
    let (anchors, rim_attachment) = match j {
        // A cone the piece draws itself sits on its own drawing, like a plug.
        Joint::Taper {
            draw: JointDraw::None,
            ..
        } => {
            let outlines: Vec<&Outline> = all.iter().map(|(_, o)| o).collect();
            let gap = near(at, &outlines);
            if gap > ATTACH_MM {
                return Err(far(gap));
            }
            ([at; 2], false)
        }
        Joint::Taper { size, gender, .. } => {
            let Some(t) = taper(size) else {
                return Ok(());
            };
            match gender {
                // The generated socket's base meets the author's neck.
                Gender::F => {
                    let base = add(at, mul(u, -(t.l + 4.)));
                    let v = perp(u);
                    let gap = on_glass(add(base, mul(v, t.neck / 2.)))
                        .max(on_glass(add(base, mul(v, -t.neck / 2.))));
                    if gap > ATTACH_MM {
                        return Err(far(gap));
                    }
                    if near(add(at, u), filled) == 0. {
                        return Err(into());
                    }
                    ([base; 2], false)
                }
                // The generated cone's collar meets the author's tube.
                Gender::M => {
                    let collar = add(at, mul(u, -4.1));
                    let gap = near(collar, glass);
                    if gap > ATTACH_MM {
                        return Err(far(gap));
                    }
                    ([collar; 2], false)
                }
            }
        }
        Joint::Plug { size_mm, .. } => {
            let outlines: Vec<&Outline> = all.iter().map(|(_, o)| o).collect();
            let gap = near(at, &outlines);
            let mut anchors = [at; 2];
            if gap > ATTACH_MM {
                // The mouth's centre can lie in open air between two walls.
                // Its diameter and axis reconstruct both physical rim edges.
                let v = mul(perp(u), size_mm / 2.);
                anchors = [add(at, v), add(at, mul(v, -1.))];
                let rim_gap = anchors
                    .iter()
                    .map(|p| {
                        outlines
                            .iter()
                            .map(|outline| distance_to_outline(*p, outline))
                            .fold(f64::INFINITY, f64::min)
                    })
                    .fold(0_f64, f64::max);
                if rim_gap > ATTACH_MM {
                    return Err(far(gap.min(rim_gap)));
                }
            }
            let embedded = filled
                .iter()
                .any(|o| inside(at, o) && distance_to_outline(at, o) > ATTACH_MM);
            if !embedded && near(add(at, u), filled) == 0. {
                return Err(into());
            }
            (anchors, gap > ATTACH_MM)
        }
    };
    // A joint that moves with a part must sit on that part's shapes.
    if let Some(k) = j.part() {
        let moving: Vec<&Outline> = all
            .iter()
            .filter(|(s, _)| s.part.as_deref().is_some_and(|p| descends(p, k, parts)))
            .map(|(_, o)| o)
            .collect();
        let attachment_gap = |anchor| {
            if rim_attachment {
                moving
                    .iter()
                    .map(|outline| distance_to_outline(anchor, outline))
                    .fold(f64::INFINITY, f64::min)
            } else {
                near(anchor, &moving)
            }
        };
        if !moving.is_empty()
            && anchors
                .iter()
                .any(|anchor| attachment_gap(*anchor) > ATTACH_MM)
        {
            let param = parts.get(k).map(|p| p.param.as_str()).unwrap_or(k);
            return Err(format!(
                "{owner} moves with \"{param}\" but sits on the fixed body. Add it to the part or move it."
            ));
        }
    }
    Ok(())
}

/// Whether part `p` is `ancestor` or nested inside it.
fn descends(p: &str, ancestor: &str, parts: &BTreeMap<&str, &Part>) -> bool {
    let mut current = Some(p);
    while let Some(k) = current {
        if k == ancestor {
            return true;
        }
        current = parts.get(k).and_then(|x| x.parent.as_deref());
    }
    false
}
