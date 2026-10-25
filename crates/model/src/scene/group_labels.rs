//! Decisions made when a chemical label is placed, using its actual glyph ink.
use super::*;
use crate::abbreviations::LabelAlignment;

/// Change the bond-based side only when it collides and the other side is clear.
/// This is an edit-time decision; rendering and hit tests never recursively
/// choose new directions for neighboring groups.
pub(crate) fn clearer_orientation(doc: &Document, id: u64) -> Option<LabelAlignment> {
    clearer_orientation_with_reach(doc, id, LabelReach::new(doc))
}

fn clearer_orientation_with_reach(
    doc: &Document,
    id: u64,
    reach: LabelReach,
) -> Option<LabelAlignment> {
    let atom = doc.atom(id)?;
    let group = doc.abbreviation(id)?;
    if group.reverse_label.trim().is_empty() {
        return None;
    }
    let mut forward = group.clone();
    forward.alignment = LabelAlignment::Left;
    let mut reverse = group.clone();
    reverse.alignment = LabelAlignment::Right;
    let forward = abbreviation_label_runs(atom, doc, &forward);
    let reverse = abbreviation_label_runs(atom, doc, &reverse);
    let (lo, hi) = label_ink_boxes(&forward)
        .into_iter()
        .chain(label_ink_boxes(&reverse))
        .reduce(|(lo, hi), (a, b)| {
            (
                Point::new(lo.x.min(a.x), lo.y.min(a.y)),
                Point::new(hi.x.max(b.x), hi.y.max(b.y)),
            )
        })?;
    let mut nearby = Vec::new();
    for other in &doc.atoms {
        if other.id == id || !doc.atom_visible(other.id) || crate::models3d::hides(doc, other.id) {
            continue;
        }
        if let Some(limit) = reach.of(other)
            && (other.position.x + limit < lo.x
                || other.position.x - limit > hi.x
                || other.position.y + limit < lo.y
                || other.position.y - limit > hi.y)
        {
            continue;
        }
        // Keep crowded or very large drawings responsive. An incomplete
        // obstacle set must never justify changing the established side.
        if nearby.len() == 64 {
            return None;
        }
        nearby.push(other.id);
    }
    // Automatic hydrogens on neighboring atoms must respond to each proposed
    // fixed group side just as they do in the real scene. Their own measuring
    // guard bounds recursion; an outer guard would freeze them on the old
    // bond-based side and falsely reject both choices after label refresh.
    let obstacles = |alignment| {
        let mut context = doc.clone();
        context
            .abbreviations
            .iter_mut()
            .find(|g| g.anchor == id)?
            .alignment = alignment;
        Some(
            nearby
                .iter()
                .filter_map(|id| context.atom(*id))
                .flat_map(|other| label_ink_boxes(&atom_label_runs(other, &context)))
                .collect::<Vec<_>>(),
        )
    };
    let bonds = unrelated_bonds(atom, doc);
    let size = STYLE.world(group.text_style(doc).size_pt);
    let (preferred, alternative, current, alignment) = if group.faces_left(doc) {
        (
            &reverse,
            &forward,
            LabelAlignment::Right,
            LabelAlignment::Left,
        )
    } else {
        (
            &forward,
            &reverse,
            LabelAlignment::Left,
            LabelAlignment::Right,
        )
    };
    (!clear_of(preferred, &obstacles(current)?, &bonds, size)
        && clear_of(alternative, &obstacles(alignment)?, &bonds, size))
    .then_some(alignment)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn unbounded_font_reach_measures_a_complete_small_drawing_but_caps_large_ones() {
        // Runtime font registration is process-global. Isolate this variable
        // fallback face from other font-generation and layout tests.
        const CHILD: &str = "RESHIKI_UNBOUNDED_LABEL_FONT_TEST";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "scene::group_labels::tests::unbounded_font_reach_measures_a_complete_small_drawing_but_caps_large_ones", "--nocapture"])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        crate::style::add_font(
            include_bytes!(
                "../../../../tests/fixtures/font-export-103/variable-default-100.subset.ttf"
            )
            .to_vec(),
        )
        .unwrap();
        let mut doc = Document::from_json(include_bytes!(
            "../../../../tests/fixtures/stereo-repeat/pentaerythritol-input.rsk"
        ))
        .unwrap();
        for id in [7, 1, 9] {
            doc = crate::atom_text::apply(&doc, id, "OAc", crate::atom_text::Mode::Auto).unwrap();
        }
        doc.abbreviations
            .iter_mut()
            .find(|g| g.anchor == 9)
            .unwrap()
            .alignment = LabelAlignment::Auto;
        assert!(
            LabelReach::new(&doc).em.is_none(),
            "A real variable fallback face has no conservative reach"
        );
        assert_eq!(clearer_orientation(&doc, 9), Some(LabelAlignment::Left));
        for index in 0..65 {
            doc.add_atom("O", Point::new(1000. + index as f32 * 42., 1000.));
        }
        assert_eq!(clearer_orientation(&doc, 9), None);
    }
}

/// Only the displayed attachment element gets core-glyph priority. Nicknames
/// such as Ac, hydrogens and charges still belong to their complete label.
pub(super) fn element_glyph_hit(
    atom: &Atom,
    doc: &Document,
    runs: &[Primitive],
    point: Point,
) -> bool {
    let (content, range) = if let Some(group) = doc.abbreviation(atom.id) {
        let content = group.text(doc);
        (
            content,
            crate::abbreviations::anchor_range(content, group.faces_left(doc)),
        )
    } else if atom.display.variable.is_none() {
        (atom.element.as_str(), 0..atom.element.len())
    } else {
        return false;
    };
    if content.get(range.clone()) != Some(atom.element.as_str()) {
        return false;
    }
    let start = content[..range.start].chars().count();
    let end = start + atom.element.chars().count();
    let mut index = 0;
    let mut picked = String::new();
    let mut hit = false;
    for run in runs {
        let Primitive::Text {
            position,
            text,
            size,
            style,
            ..
        } = run
        else {
            continue;
        };
        let mut x = position.x;
        for character in text.chars() {
            if (start..end).contains(&index) {
                picked.push(character);
                hit |= crate::style::text_ink_boxes(&character.to_string(), *size, style)
                    .into_iter()
                    .any(|(lo, hi)| {
                        point.x >= x + lo.x
                            && point.x <= x + hi.x
                            && point.y >= position.y + lo.y
                            && point.y <= position.y + hi.y
                    });
            }
            x += crate::style::glyph_metrics(character, style).1 * size;
            index += 1;
        }
    }
    hit && picked == atom.element
}
