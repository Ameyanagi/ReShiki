use super::*;

fn measured(work: [i32; 4], dpi: u32) -> Measurements {
    let scale = f64::from(dpi) / 96.;
    let [width, height] = INITIAL.map(|v| (v * scale).round() as i32);
    let frame_width = (16. * scale).ceil() as i32;
    let frame_height = (39. * scale).ceil() as i32;
    Measurements {
        work,
        outer: [0, 0, width + frame_width, height + frame_height],
        client: [width, height],
        dpi,
    }
}

fn assert_fits(measured: Measurements, fit: Fit) {
    let [left, top, right, bottom] = measured.work;
    let [old_x, old_y, old_right, old_bottom] = measured.outer;
    let [old_width, old_height] = measured.client;
    let frame_width = old_right - old_x - old_width;
    let frame_height = old_bottom - old_y - old_height;
    let [width, height] = fit.client;
    let [minimum_width, minimum_height] = fit.minimum;
    let [x, y] = fit.position;
    let scale = f64::from(measured.dpi) / 96.;
    let outer_width = (f64::from(width) * scale).round() as i32 + frame_width;
    let outer_height = (f64::from(height) * scale).round() as i32 + frame_height;
    assert!(x >= left && y >= top, "{measured:?}: {fit:?}");
    assert!(
        x + outer_width <= right && y + outer_height <= bottom,
        "{measured:?}: {fit:?}"
    );
    assert!(minimum_width > 0. && minimum_width <= width);
    assert!(minimum_height > 0. && minimum_height <= height);
}

#[test]
fn reported_200_percent_screen_reproduces_overflow_and_fits_with_decorations() {
    // The report's full screen is 2256 x 1504. Its work area cannot be larger;
    // this case also reserves a 96-pixel taskbar at the same 200% DPI.
    let measured = measured([0, 0, 2256, 1408], 192);
    let [fixed_width, fixed_height] = measured.client;
    assert_eq!([fixed_width, fixed_height], [2560, 1640]);
    assert!(fixed_width > 2256 && fixed_height > 1504);
    let fit = policy(measured, true).unwrap();
    assert_eq!(fit.client, [1096., 649.]);
    assert_eq!(fit.minimum, [1040., 649.]);
    assert!(fit.resize);
    assert_fits(measured, fit);
}

#[test]
fn ordinary_roomy_desktop_keeps_the_established_sizes() {
    let measured = measured([0, 0, 1920, 1040], 96);
    let fit = policy(measured, true).unwrap();
    assert_eq!(fit.client, [1280., 820.]);
    assert_eq!(fit.minimum, [1040., 680.]);
    assert!(!fit.resize);
    assert_fits(measured, fit);
}

#[test]
fn fractional_dpi_and_small_work_areas_fit_outer_frame_and_lower_minimum() {
    for dpi in [96, 120, 144, 168, 192, 240] {
        for work in [[0, 0, 1024, 720], [0, 0, 2256, 1408], [48, 0, 1920, 1080]] {
            let measured = measured(work, dpi);
            assert_fits(measured, policy(measured, true).unwrap());
        }
    }
    let fit = policy(measured([0, 0, 1024, 720], 192), true).unwrap();
    assert!(fit.minimum[0] < MINIMUM[0] as f32);
    assert!(fit.minimum[1] < MINIMUM[1] as f32);
}

#[test]
fn negative_monitor_origin_and_side_taskbar_use_physical_screen_coordinates() {
    let measured = measured([-1880, -200, 0, 840], 144);
    let fit = policy(measured, true).unwrap();
    assert!(fit.position[0] < 0 && fit.position[1] < 0);
    assert_fits(measured, fit);
}

#[test]
fn dpi_change_clamps_existing_window_without_resetting_to_initial_size() {
    let mut measured = measured([0, 0, 2256, 1408], 192);
    measured.outer = [-100, -100, 2492, 1618];
    let fit = policy(measured, false).unwrap();
    assert_fits(measured, fit);

    let measured = Measurements {
        work: [0, 0, 1920, 1040],
        outer: [20, 30, 1236, 769],
        client: [1200, 700],
        dpi: 96,
    };
    let fit = policy(measured, false).unwrap();
    assert_eq!(fit.client, [1200., 700.]);
    assert_eq!(fit.position, [20, 30]);
    assert!(!fit.resize);
    assert_fits(measured, fit);
}

#[test]
fn invalid_native_measurements_are_rejected_without_panicking() {
    let mut measured = measured([0, 0, 1920, 1040], 96);
    measured.dpi = 0;
    assert!(policy(measured, true).is_none());
    measured.dpi = 96;
    measured.work = [0, 0, 1, 1];
    assert!(policy(measured, true).is_none());
    measured.work = [100, 100, 0, 0];
    assert!(policy(measured, true).is_none());
    measured.work = [0, 0, 1920, 1040];
    measured.client = [0, 0];
    assert!(policy(measured, true).is_none());
}
