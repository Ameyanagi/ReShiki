use super::*;

#[test]
fn explicit_units_and_bare_values_have_one_canonical_size() {
    for inputs in [
        ["0.5 cm", "5 mm", "14.173228346456693 pt"],
        ["0.508 cm", "5.08 mm", "14.4 pt"],
    ] {
        let expected = parse(inputs[0], Unit::Points).unwrap().points;
        for unit in Unit::ALL {
            for input in inputs {
                assert_eq!(parse(input, unit).unwrap().points, expected);
            }
            assert_eq!(
                parse(&format(expected, unit), unit).unwrap().points,
                expected
            );
        }
    }
    for input in ["+0.5CM", ".5 cm", "5e-1 cm", "  0.5\u{a0}cm\t"] {
        assert_eq!(
            parse(input, Unit::Points).unwrap().points,
            parse("5mm", Unit::Points).unwrap().points
        );
    }
    assert_eq!(parse("5", Unit::Millimetres), parse("5 mm", Unit::Points));
}

#[test]
fn partial_and_invalid_edits_never_supply_a_value() {
    for input in [
        "", "+", "-", ".", "5.", "1e", "1e+", "1e-", "5 c", "5 m", "5 p",
    ] {
        assert!(
            matches!(parse(input, Unit::Points), Err(InputError::Incomplete(_))),
            "{input}"
        );
    }
    for input in [
        "NaN", "inf", "INFINITY", "1e309", "1e39 cm", "1e-999", "1e-49", "-0", "-1 mm", "0,5 cm",
        "1 000 mm", "5 px", "5mm2", "5 m m", "１cm", "−1pt",
    ] {
        assert!(parse(input, Unit::Points).is_err(), "{input}");
    }
    assert_eq!(parse("0e999", Unit::Points).unwrap().points, 0.);
}

#[test]
fn physical_limits_are_checked_after_conversion() {
    for dimension in Dimension::ALL {
        let (min, max) = dimension.range();
        for unit in Unit::ALL {
            for value in [min, max] {
                let parsed = parse(&format(value, unit), unit).unwrap().points;
                assert!(dimension.validate(parsed, unit).is_ok());
            }
            assert!(dimension.validate(max + 1., unit).is_err());
        }
    }
    assert!(Dimension::Margin.validate(0., Unit::Points).is_ok());
    assert!(Dimension::Line.validate(0., Unit::Points).is_err());
}

#[test]
fn adjacent_physical_boundaries_have_the_same_result_in_every_unit() {
    // The public point limits are an independent oracle. Exercise the
    // neighboring representable values through real unit-bearing input,
    // without using the editor's formatter or conversion helper.
    for (dimension, min, max) in [
        (Dimension::Bond, 5.0_f32, 100.0_f32),
        (Dimension::Line, 0.1, 6.0),
        (Dimension::Bold, 0.1, 12.0),
        (Dimension::Margin, 0.0, 12.0),
        (Dimension::Hash, 0.3, 12.0),
    ] {
        for (points, valid) in [
            (min.next_down(), false),
            (min, true),
            (min.next_up(), true),
            (max.next_down(), true),
            (max, true),
            (max.next_up(), false),
        ] {
            for (suffix, units_per_point) in [("pt", 1.0), ("mm", 25.4 / 72.0), ("cm", 2.54 / 72.0)]
            {
                let value = f64::from(points) * units_per_point;
                let text = format!("{value:.17e} {suffix}");
                let parsed = parse(&text, Unit::Points);
                if let Ok(parsed) = parsed {
                    assert_eq!(parsed.points.to_bits(), points.to_bits(), "{text}");
                }
                let accepted = parsed
                    .is_ok_and(|parsed| dimension.validate(parsed.points, parsed.unit).is_ok());
                assert_eq!(accepted, valid, "{dimension:?}: {text}");
            }
        }
    }
}
