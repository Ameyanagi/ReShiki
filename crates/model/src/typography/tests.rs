use super::{
    Script, ScriptRun, effective_label_runs, label_runs, normalize_label_scripts,
    validate_label_scripts,
};

#[test]
fn small_caps_name_fragments_share_the_normal_text_baseline() {
    use super::{TextFormat, layout, name_markup};
    let markup = name_markup("L-alanine");
    let mut format = TextFormat::default();
    markup.apply_to(&mut format);
    let laid_out = layout(&markup.text, &format);
    assert_eq!(laid_out.fragments.len(), 2);
    let small_caps = &laid_out.fragments[0];
    let normal = &laid_out.fragments[1];
    assert_eq!(small_caps.text, "L");
    assert_eq!(normal.text, "-alanine");
    let baseline = |fragment: &super::TextFragment| {
        fragment.position.y + crate::style::text_ascent(&fragment.style)
    };
    assert!((baseline(small_caps) - baseline(normal)).abs() < 1e-3);
}

#[test]
fn uniform_size_normal_fragments_stay_at_line_top() {
    use super::{StyleChange, TextFormat, layout};
    let text = "normal text";
    let mut format = TextFormat::default();
    format.apply(text, Some(0..6), &StyleChange::Bold(true));
    let laid_out = layout(text, &format);
    assert_eq!(laid_out.fragments.len(), 2);
    for fragment in &laid_out.fragments {
        assert_eq!(fragment.style.script, Script::Normal);
        assert_eq!(fragment.position.y, 0.);
    }
}

#[test]
fn unicode_subscript_digits_store_ascii_with_the_manual_script_layout() {
    use super::{TextFormat, layout, normalize_subscript_digits};
    for (input, expected, start, end) in
        [("H₂O", "H2O", 1, 2), ("x₀₁₂₃₄₅₆₇₈₉", "x0123456789", 1, 11)]
    {
        let mut text = input.to_owned();
        let mut format = TextFormat::default();
        normalize_subscript_digits(&mut text, &mut format);
        assert_eq!(text, expected);
        assert_eq!(format.spans.len(), 1);
        assert_eq!((format.spans[0].start, format.spans[0].end), (start, end));
        assert_eq!(format.spans[0].style.script, Script::Subscript);
        format.validate(&text).unwrap();

        let mut manual = TextFormat::default();
        manual.apply(
            expected,
            Some(start..end),
            &super::StyleChange::Script(Script::Subscript),
        );
        let actual = layout(&text, &format);
        let expected = layout(expected, &manual);
        assert_eq!(
            (actual.width, actual.height),
            (expected.width, expected.height)
        );
        assert_eq!(actual.fragments.len(), expected.fragments.len());
        for (actual, expected) in actual.fragments.iter().zip(&expected.fragments) {
            assert_eq!(actual.text, expected.text);
            assert_eq!(actual.position, expected.position);
            assert_eq!(actual.style, expected.style);
        }
        let normalized = (text.clone(), format.clone());
        normalize_subscript_digits(&mut text, &mut format);
        assert_eq!((text, format), normalized);
    }
}

#[test]
fn unicode_subscript_digits_remap_utf8_spans_and_preserve_effective_styles() {
    use super::{StyleChange, TextFormat, normalize_subscript_digits};
    let mut text = "触媒 H₂O, CO₃ 尾".to_owned();
    let mut format = TextFormat::default();
    format.style.family = "Helvetica".into();
    format.style.size_pt = 14.;
    format.style.formula = true;
    // A styled span includes the digit, and another starts after both digits.
    let water = text.find('H').unwrap();
    let water_end = text.find(',').unwrap();
    let tail = text.find('尾').unwrap();
    format.apply(&text, Some(water..water_end), &StyleChange::Bold(true));
    format.apply(&text, Some(tail..text.len()), &StyleChange::Italic(true));
    let original = text.clone();
    let original_styles: Vec<_> = original
        .char_indices()
        .map(|(offset, _)| format.at(offset).clone())
        .collect();
    normalize_subscript_digits(&mut text, &mut format);
    assert_eq!(text, "触媒 H2O, CO3 尾");
    format.validate(&text).unwrap();
    for (((offset, c), source), original) in text
        .char_indices()
        .zip(original.chars())
        .zip(original_styles)
    {
        let mut expected = original;
        if matches!(source, '₀'..='₉') {
            expected.script = Script::Subscript;
            expected.formula = false;
            assert!(c.is_ascii_digit());
        }
        assert_eq!(format.at(offset), &expected, "{c} at {offset}");
    }
    let tail_span = format.spans.iter().find(|span| span.style.italic).unwrap();
    assert_eq!((tail_span.start, tail_span.end), (tail - 4, text.len()));
}

#[test]
fn text_without_unicode_subscript_digits_keeps_its_original_spans() {
    use super::{TextFormat, TextSpan, TextStyle, normalize_subscript_digits};
    let mut text = "H2O ² ₊₋⁺".to_owned();
    // Non-digit Unicode scripts remain literal; only U+2080..U+2089 convert.
    let mut format = TextFormat {
        spans: vec![TextSpan {
            start: 0,
            end: 1,
            style: TextStyle::default(),
        }],
        ..Default::default()
    };
    let original = (text.clone(), format.clone());
    normalize_subscript_digits(&mut text, &mut format);
    assert_eq!((text, format), original);
}

#[test]
fn free_label_formula_runs_cover_the_golden_table() {
    use Script::{Normal as N, Subscript as B, Superscript as P};
    type Run = (std::ops::Range<usize>, Script);
    let cases: &[(&str, &[Run])] = &[
        ("M2+", &[(0..1, N), (1..3, P)]),
        ("Pd2+", &[(0..2, N), (2..4, P)]),
        ("R1", &[(0..1, N), (1..2, B)]),
        ("L2", &[(0..1, N), (1..2, B)]),
        ("SO42-", &[(0..2, N), (2..3, B), (3..5, P)]),
        ("SO4 2-", &[(0..2, N), (2..3, B), (3..6, P)]),
        ("CO32-", &[(0..2, N), (2..3, B), (3..5, P)]),
        ("NR4+", &[(0..2, N), (2..3, B), (3..4, P)]),
        ("NMe4+", &[(0..3, N), (3..4, B), (4..5, P)]),
        ("PPh4+", &[(0..3, N), (3..4, B), (4..5, P)]),
        ("Me3Si+", &[(0..2, N), (2..3, B), (3..5, N), (5..6, P)]),
        ("X", &[(0..1, N)]),
        ("Ar", &[(0..2, N)]),
        ("L", &[(0..1, N)]),
        ("Cl,Br", &[(0..5, N)]),
        ("2 eq", &[(0..4, N)]),
        ("C12", &[(0..1, N), (1..3, B)]),
        ("[M]2+", &[(0..3, N), (3..5, P)]),
        ("+", &[(0..1, N)]),
        ("2-", &[(0..2, N)]),
        ("R₁", &[(0..4, N)]),
        ("custom ligand", &[(0..13, N)]),
        ("Fe 3+", &[(0..2, N), (2..5, P)]),
        ("M2−", &[(0..1, N), (1..5, P)]),
        ("M2⁺", &[(0..1, N), (1..5, P)]),
        ("M2⁻", &[(0..1, N), (1..5, P)]),
        ("(NR)4+", &[(0..4, N), (4..5, B), (5..6, P)]),
        ("C0+", &[(0..1, N), (1..2, B), (2..3, P)]),
        ("R,12", &[(0..4, N)]),
        ("R-12", &[(0..4, N)]),
        ("Fe  3+", &[(0..6, N)]),
        ("R1 ligand+", &[(0..10, N)]),
        ("", &[]),
    ];
    for (text, expected) in cases {
        let runs = label_runs(text);
        assert_eq!(runs, *expected, "{text}");
        let mut covered = 0;
        for (range, _) in &runs {
            assert_eq!(range.start, covered, "{text}");
            assert!(text.is_char_boundary(range.start));
            assert!(text.is_char_boundary(range.end));
            covered = range.end;
        }
        assert_eq!(covered, text.len(), "{text}");
        for pair in runs.windows(2) {
            assert_ne!(pair[0].1, pair[1].1, "Unmerged runs for {text}");
        }
    }
}

#[test]
fn manual_label_runs_override_inference_and_merge_normal_gaps() -> Result<(), String> {
    let mut display = crate::atom_labels::AtomDisplay {
        variable: Some("SO42-".into()),
        ..Default::default()
    };
    assert_eq!(effective_label_runs(&display), label_runs("SO42-"));
    display.label_scripts = vec![ScriptRun {
        start: 2,
        end: 3,
        script: Script::Superscript,
    }];
    assert_eq!(
        effective_label_runs(&display),
        vec![
            (0..2, Script::Normal),
            (2..3, Script::Superscript),
            (3..5, Script::Normal),
        ]
    );
    display.label_scripts = vec![ScriptRun {
        start: 1,
        end: 3,
        script: Script::Normal,
    }];
    assert_eq!(effective_label_runs(&display), vec![(0..5, Script::Normal)]);
    display.set_variable(Some("SO42-".into()));
    assert!(display.label_scripts.is_empty());
    assert_eq!(effective_label_runs(&display), label_runs("SO42-"));
    display.label_scripts.push(ScriptRun {
        start: 2,
        end: 3,
        script: Script::Subscript,
    });
    display.set_variable(None);
    assert!(display.label_scripts.is_empty());
    assert!(effective_label_runs(&display).is_empty());
    display.validate()?;
    Ok(())
}

#[test]
fn manual_label_run_normalization_preserves_literal_intent() -> Result<(), String> {
    let run = |start, end, script| ScriptRun { start, end, script };
    assert_eq!(
        normalize_label_scripts(
            "NH42+",
            &[
                run(0, 2, Script::Normal),
                run(2, 2, Script::Subscript),
                run(2, 3, Script::Subscript),
                run(3, 4, Script::Superscript),
                run(4, 5, Script::Superscript),
            ]
        )?,
        vec![run(2, 3, Script::Subscript), run(3, 5, Script::Superscript)]
    );
    assert_eq!(
        normalize_label_scripts("R1", &[run(1, 2, Script::Normal)])?,
        vec![run(0, 2, Script::Normal)]
    );
    assert!(normalize_label_scripts("R1", &[])?.is_empty());
    assert!(normalize_label_scripts("R1", &[run(1, 1, Script::Normal)])?.is_empty());
    Ok(())
}

#[test]
fn stored_manual_label_ranges_reject_invalid_offsets_and_missing_text() {
    let run = |start, end| ScriptRun {
        start,
        end,
        script: Script::Subscript,
    };
    for runs in [
        vec![run(0, 0)],
        vec![run(2, 1)],
        vec![run(0, 5)],
        vec![run(0, 2)],
        vec![run(2, 4)],
        vec![run(1, 4), run(0, 1)],
        vec![run(0, 4), run(1, 4)],
    ] {
        let display = crate::atom_labels::AtomDisplay {
            variable: Some("R₁".into()),
            label_scripts: runs.clone(),
            ..Default::default()
        };
        assert!(display.validate().is_err(), "{runs:?}");
        assert!(validate_label_scripts("R₁", &runs).is_err(), "{runs:?}");
    }
    for runs in [vec![run(2, 1)], vec![run(0, 2)], vec![run(1, 4), run(0, 1)]] {
        assert!(normalize_label_scripts("R₁", &runs).is_err());
    }
    let display = crate::atom_labels::AtomDisplay {
        label_scripts: vec![run(0, 1)],
        ..Default::default()
    };
    assert_eq!(
        display.validate(),
        Err("Atom label scripts require a variable label".into())
    );
    let display = crate::atom_labels::AtomDisplay {
        variable: Some("R₁".into()),
        label_scripts: vec![run(1, 4)],
        ..Default::default()
    };
    assert!(display.validate().is_ok());
    assert_eq!(
        effective_label_runs(&display),
        vec![(0..1, Script::Normal), (1..4, Script::Subscript)]
    );
}

#[test]
fn wrapping_breaks_between_words_never_inside_one() {
    use super::{TextFormat, layout};
    let lines = |text: &str, width_pt: f32| {
        let format = TextFormat {
            width_pt: Some(width_pt),
            ..TextFormat::default()
        };
        let layout = layout(text, &format);
        let mut lines: Vec<(f32, String)> = vec![];
        for fragment in &layout.fragments {
            match lines.last_mut() {
                Some((y, line)) if (*y - fragment.position.y).abs() < 0.001 => {
                    line.push_str(&fragment.text);
                }
                _ => lines.push((fragment.position.y, fragment.text.clone())),
            }
        }
        lines
            .into_iter()
            .map(|(_, line)| line.trim_end().to_owned())
            .collect::<Vec<_>>()
    };
    // #313 and #329: a word wider than the box overflows it instead of
    // being cut at an arbitrary character.
    assert_eq!(lines("4-Bromoanisole", 30.), ["4-Bromoanisole"]);
    assert_eq!(
        lines("(2R)-2-hydroxypropanoic acid", 40.),
        ["(2R)-2-hydroxypropanoic", "acid"]
    );
    // Measured, not hard-coded: glyph advances come from the system fonts.
    let alpha_beta = layout("alpha beta ", &TextFormat::default()).width
        * crate::style::DEFAULT.points_per_world();
    assert_eq!(
        lines("alpha beta alpha beta", alpha_beta + 0.5),
        ["alpha beta", "alpha beta"]
    );
    // Text without spaces between words still wraps at CJK characters.
    let cjk = lines("日本語の長い説明文です", 30.);
    assert!(cjk.len() > 1);
    assert_eq!(cjk.concat(), "日本語の長い説明文です");
    // A voicing mark stays with its kana.
    let kana = lines("ﾊﾞﾊﾞﾊﾞﾊﾞﾊﾞﾊﾞﾊﾞﾊﾞ", 12.);
    assert!(kana.len() > 1);
    assert!(kana.iter().all(|line| !line.starts_with('ﾞ')), "{kana:?}");
}

#[test]
fn generated_names_wrap_only_at_chemical_punctuation() {
    use super::{TextAlign, TextFormat, layout};
    let name = "(1R,2S,5R)-5-methyl-2-(propan-2-yl)cyclohexan-1-ol";
    let width = crate::style::DEFAULT.bond_length_world * 6.;
    let format = TextFormat {
        alignment: TextAlign::Center,
        width_pt: Some(width * crate::style::DEFAULT.points_per_world()),
        name_breaks: true,
        ..Default::default()
    };
    let laid_out = layout(name, &format);
    let mut lines: Vec<(f32, String)> = vec![];
    for fragment in &laid_out.fragments {
        match lines.last_mut() {
            Some((y, text)) if (*y - fragment.position.y).abs() < 0.001 => {
                text.push_str(&fragment.text);
            }
            _ => lines.push((fragment.position.y, fragment.text.clone())),
        }
    }
    assert!(lines.len() > 1, "{lines:?}");
    assert_eq!(
        lines
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<String>(),
        name
    );
    for (index, (_, text)) in lines.iter().enumerate() {
        assert!(
            layout(text, &TextFormat::default()).width <= width + 0.001,
            "{text}"
        );
        if index + 1 < lines.len() {
            assert!(text.ends_with(['-', ',', ')', ']']), "{text}");
        }
    }
    for word in ["methyl", "propan", "cyclohexan"] {
        assert!(
            lines.iter().any(|(_, text)| text.contains(word)),
            "{word}: {lines:?}"
        );
    }
    // The caption-specific rule is absent in old documents and ordinary text.
    let plain = TextFormat {
        name_breaks: false,
        ..format.clone()
    };
    assert!(layout(name, &plain).width > width);
    let encoded = serde_json::to_value(&plain).unwrap();
    assert!(encoded.get("name_breaks").is_none());
    assert!(
        !serde_json::from_value::<TextFormat>(encoded)
            .unwrap()
            .name_breaks
    );
    let encoded = serde_json::to_value(&format).unwrap();
    assert!(
        serde_json::from_value::<TextFormat>(encoded)
            .unwrap()
            .name_breaks
    );
    let mut edited = format;
    edited.edited(name, "My sample", 0..name.len());
    assert!(!edited.name_breaks);
}
