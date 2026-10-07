use super::*;
#[test]
fn reference_colors_round_trip_and_gamut_mapping_preserves_hue() {
    for r in (0..=255).step_by(17) {
        for g in (0..=255).step_by(17) {
            for b in (0..=255).step_by(17) {
                let rgb = [r, g, b];
                assert_eq!(Oklch::from_rgb(rgb).to_rgb(), rgb);
            }
        }
    }
    let red = Oklch::from_rgb([255, 0, 0]);
    assert!((red.l - 0.62795536).abs() < 1e-7);
    assert!((red.c - 0.2576833).abs() < 1e-7);
    let mapped = Oklch::from_rgb(
        Oklch {
            l: 0.8,
            c: 0.4,
            ..red
        }
        .to_rgb(),
    );
    assert!((mapped.l - 0.8).abs() < 0.003);
    assert!((mapped.h - red.h).abs() < 0.025);
    assert!(mapped.c < 0.4);
    assert_eq!(contrast([0; 3], [255; 3]), 21.);
    assert!(
        contrast([119; 3], [255; 3]) < TEXT_MIN,
        "never round 4.478 to a pass"
    );
}
#[test]
fn solves_multiple_backgrounds_and_reports_conflicts() {
    for seed in [[255, 255, 0], [10, 20, 230], [128; 3], [255, 0, 0]] {
        for backgrounds in [
            vec![[255; 3]],
            vec![[0; 3]],
            vec![[255; 3], [230, 245, 235]],
        ] {
            let ink = ensure_contrast(seed, &backgrounds, TEXT_TARGET).unwrap();
            assert!(meets(ink, &backgrounds, TEXT_TARGET));
            assert_eq!(ensure_contrast(ink, &backgrounds, TEXT_TARGET), Some(ink));
        }
    }
    let opposing = [[0; 3], [255; 3]];
    let gray = ensure_contrast([128; 3], &opposing, TEXT_MIN).unwrap();
    assert!(meets(gray, &opposing, TEXT_MIN));
    assert_eq!(ensure_contrast([128; 3], &opposing, TEXT_TARGET), None);
    assert_eq!(
        ensure_contrast([0; 3], &[[0; 3], [120; 3], [255; 3]], TEXT_MIN),
        None
    );
}
