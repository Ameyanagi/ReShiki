//! Windows work-area sizing; UI scale stays under Iced's DPI handling.

const INITIAL: [f64; 2] = [1280., 820.];
const MINIMUM: [f64; 2] = [1040., 680.];
const MARGIN: f64 = 8.;

#[derive(Debug, Clone, Copy)]
struct Measurements {
    work: [i32; 4],
    outer: [i32; 4],
    client: [i32; 2],
    dpi: u32,
}

#[derive(Debug, Clone, Copy)]
struct Fit {
    client: [f32; 2],
    minimum: [f32; 2],
    position: [i32; 2],
    resize: bool,
}

fn policy(measured: Measurements, initial: bool) -> Option<Fit> {
    let Measurements {
        work: [left, top, right, bottom],
        outer: [x, y, outer_right, outer_bottom],
        client: [client_width, client_height],
        dpi,
    } = measured;
    if dpi == 0 || right <= left || bottom <= top || client_width <= 0 || client_height <= 0 {
        return None;
    }
    let scale = f64::from(dpi) / 96.;
    let frame_width = f64::from(outer_right) - f64::from(x) - f64::from(client_width);
    let frame_height = f64::from(outer_bottom) - f64::from(y) - f64::from(client_height);
    if frame_width < 0. || frame_height < 0. {
        return None;
    }
    let margin = (MARGIN * scale).ceil();
    let max_width =
        ((f64::from(right) - f64::from(left) - frame_width - 2. * margin) / scale).floor();
    let max_height =
        ((f64::from(bottom) - f64::from(top) - frame_height - 2. * margin) / scale).floor();
    if max_width < 1. || max_height < 1. {
        return None;
    }
    let [minimum_width, minimum_height] = MINIMUM;
    let minimum_width = minimum_width.min(max_width);
    let minimum_height = minimum_height.min(max_height);
    let [preferred_width, preferred_height] = if initial {
        INITIAL
    } else {
        [
            f64::from(client_width) / scale,
            f64::from(client_height) / scale,
        ]
    };
    let width = preferred_width.clamp(minimum_width, max_width);
    let height = preferred_height.clamp(minimum_height, max_height);
    let physical_width = (width * scale).round();
    let physical_height = (height * scale).round();
    let outer_width = physical_width + frame_width;
    let outer_height = physical_height + frame_height;
    let position = if initial {
        [
            (f64::from(left) + (f64::from(right) - f64::from(left) - outer_width) / 2.).floor()
                as i32,
            (f64::from(top) + (f64::from(bottom) - f64::from(top) - outer_height) / 2.).floor()
                as i32,
        ]
    } else {
        [
            f64::from(x)
                .clamp(
                    f64::from(left) + margin,
                    f64::from(right) - margin - outer_width,
                )
                .floor() as i32,
            f64::from(y)
                .clamp(
                    f64::from(top) + margin,
                    f64::from(bottom) - margin - outer_height,
                )
                .floor() as i32,
        ]
    };
    Some(Fit {
        client: [width as f32, height as f32],
        minimum: [minimum_width as f32, minimum_height as f32],
        position,
        resize: physical_width != f64::from(client_width)
            || physical_height != f64::from(client_height),
    })
}

#[cfg(windows)]
pub(crate) fn fit<T: Send + 'static>(id: iced::window::Id, initial: bool) -> iced::Task<T> {
    use iced::{Size, Task, window};
    use reshiki_windows::window as native;
    window::run(id, |window| native::measurements(window)).then(move |result| {
        let fit = match result.and_then(|measured| {
            policy(
                Measurements {
                    work: measured.work,
                    outer: measured.outer,
                    client: measured.client,
                    dpi: measured.dpi,
                },
                initial,
            )
            .ok_or_else(|| "Invalid window work-area measurements".to_owned())
        }) {
            Ok(fit) => fit,
            Err(error) => {
                eprintln!("Could not fit the Windows window: {error}");
                return Task::none();
            }
        };
        let [width, height] = fit.client;
        let [minimum_width, minimum_height] = fit.minimum;
        window::set_min_size(id, Some(Size::new(minimum_width, minimum_height)))
            .chain(if fit.resize {
                window::resize(id, Size::new(width, height))
            } else {
                Task::none()
            })
            .chain(
                window::run(id, move |window| native::position(window, fit.position))
                    .map(|result| {
                        if let Err(error) = result {
                            eprintln!("Could not position the Windows window: {error}");
                        }
                    })
                    .discard(),
            )
    })
}

#[cfg(test)]
mod tests;
