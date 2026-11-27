use super::{CANCELED, FPS, Options, PATH_FRAME_SECONDS, PlaybackMode, RENDERING, SAVING, Work};
use std::cmp::Ordering;
use std::path::Path;
use std::sync::atomic::Ordering::Relaxed;

const FRAME_DELAY: u16 = 100 / FPS as u16;

pub(super) fn file_name(stem: &str) -> String {
    let mut name = String::new();
    for c in stem.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            name.push(c);
        } else if !name.is_empty() && !name.ends_with('-') {
            name.push('-');
        }
    }
    name.truncate(name.floor_char_boundary(80));
    let name = name.trim_end_matches('-');
    format!("{}.gif", if name.is_empty() { "animation" } else { name })
}

pub(super) fn irc_frames(
    len: usize,
    ts_index: usize,
    mode: PlaybackMode,
    speed: f32,
) -> Vec<usize> {
    if len == 0 {
        return Vec::new();
    }
    if len == 1 {
        return vec![0; 3 * FPS];
    }
    let reverse = mode == PlaybackMode::BackAndForth;
    let steps = len - 1;
    let seconds = steps as f32 * PATH_FRAME_SECONDS / speed;
    let fps = FPS as f32;
    let count = if reverse {
        1 + (seconds * fps).round().clamp(1.5 * fps, 3.0 * fps) as usize
    } else {
        (seconds * fps).round().clamp(3.0 * fps, 6.0 * fps) as usize
    };
    let last = count - 1;
    // Reserve the nearest slot for the saddle, keeping both endpoints in the pass.
    let ts_frame = match ts_index {
        0 => 0,
        i if i == steps => last,
        i => ((i * last + steps / 2) / steps).clamp(1, last - 1),
    };
    let sample = |i: usize| {
        let index = (i * steps + last / 2) / last;
        match i.cmp(&ts_frame) {
            Ordering::Less => index.min(ts_index - 1),
            Ordering::Equal => ts_index,
            Ordering::Greater => index.max(ts_index + 1),
        }
    };
    let mut frames: Vec<_> = (0..count).map(sample).collect();
    if reverse {
        frames.extend((1..last).rev().map(sample));
    }
    frames
}

pub(super) fn save(
    path: &Path,
    options: Options,
    mode: PlaybackMode,
    sequence: &[usize],
    work: &Work,
    render: impl FnMut(usize, usize) -> Result<Vec<u8>, String>,
) -> Result<bool, String> {
    let Some(bytes) = encode(options, mode, sequence, work, render)? else {
        return Ok(false);
    };
    // A cancel before this transition wins, even after the last frame.
    if work
        .state
        .compare_exchange(RENDERING, SAVING, Relaxed, Relaxed)
        .is_err()
    {
        return Ok(false);
    }
    crate::app::files::write(path, &bytes)?;
    Ok(true)
}

fn encode(
    options: Options,
    mode: PlaybackMode,
    sequence: &[usize],
    work: &Work,
    mut render: impl FnMut(usize, usize) -> Result<Vec<u8>, String>,
) -> Result<Option<Vec<u8>>, String> {
    if sequence.is_empty() {
        return Err("There are no frames to export.".into());
    }
    let side = options.size;
    let mut encoder = gif::Encoder::new(Vec::new(), side, side, &[]).map_err(encoding_error)?;
    if mode != PlaybackMode::Once {
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .map_err(encoding_error)?;
    }
    let mut previous = None;
    for (i, &index) in sequence.iter().enumerate() {
        if work.state.load(Relaxed) == CANCELED {
            return Ok(None);
        }
        // Plot readouts show the encoded frame ordinal, even for repeated geometries.
        if options.plot || previous.as_ref().is_none_or(|(last, _)| *last != index) {
            let mut rgba = render(i, index)?;
            if work.state.load(Relaxed) == CANCELED {
                return Ok(None);
            }
            let frame = quantize(options, &mut rgba)?;
            previous = Some((index, frame));
        }
        if let Some((_, frame)) = &previous {
            encoder.write_frame(frame).map_err(encoding_error)?;
        }
        work.completed.store(i + 1, Relaxed);
    }
    encoder.into_inner().map(Some).map_err(encoding_error)
}

fn quantize(options: Options, rgba: &mut [u8]) -> Result<gif::Frame<'static>, String> {
    let side = options.size;
    if rgba.len() != usize::from(side).pow(2) * 4 {
        return Err("The renderer returned an incomplete frame.".into());
    }
    if options.transparent {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            if pixel[3] < 128 {
                *pixel = [0; 4];
            }
        }
    }
    let mut frame = gif::Frame::from_rgba_speed(side, side, rgba, 20);
    frame.delay = FRAME_DELAY;
    if options.transparent {
        frame.dispose = gif::DisposalMethod::Background;
    }
    Ok(frame)
}

fn encoding_error(error: gif::EncodingError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: Options = Options {
        size: 2,
        transparent: true,
        plot: false,
    };

    fn pixels(_: usize, frame: usize) -> Result<Vec<u8>, String> {
        let opaque = if frame == 0 {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 255]
        };
        Ok([opaque, [0; 4], opaque, [0; 4]]
            .into_iter()
            .flatten()
            .collect())
    }

    #[test]
    fn sequences_span_the_path_and_fit_three_to_six_seconds() {
        assert!(irc_frames(0, 0, PlaybackMode::Loop, 1.).is_empty());
        assert_eq!(irc_frames(1, 0, PlaybackMode::BackAndForth, 1.), [0; 60]);
        for mode in PlaybackMode::ALL {
            for (len, speed, count) in [(3, 2., 60), (51, 1., 80), (300, 0.5, 120)] {
                let sequence = irc_frames(len, len / 2, mode, speed);
                assert_eq!(
                    sequence.len(),
                    if mode == PlaybackMode::BackAndForth && len == 51 {
                        120
                    } else {
                        count
                    }
                );
                assert_eq!(sequence.first(), Some(&0));
                let end = if mode == PlaybackMode::BackAndForth {
                    sequence.len() / 2
                } else {
                    sequence.len() - 1
                };
                assert_eq!(sequence[end], len - 1);
                assert!(sequence[..=end].is_sorted());
                if mode == PlaybackMode::BackAndForth {
                    assert!(sequence[end..].is_sorted_by(|a, b| a >= b));
                    assert!(
                        sequence[end + 1..]
                            .iter()
                            .copied()
                            .eq(sequence[1..end].iter().rev().copied())
                    );
                }
            }
        }
    }

    #[test]
    fn sequences_include_the_transition_state_once_per_pass() {
        for mode in PlaybackMode::ALL {
            for (len, ts_index, count) in [(121, 60, 120), (3, 1, 60), (300, 1, 120)] {
                let sequence = irc_frames(len, ts_index, mode, 1.);
                assert_eq!(sequence.len(), count);
                assert_eq!(sequence[0], 0);
                let reverse = mode == PlaybackMode::BackAndForth;
                let end = if reverse { count / 2 } else { count - 1 };
                assert_eq!(sequence[end], len - 1);
                let (forward, backward) = sequence.split_at(end + 1);
                assert_eq!(
                    forward.iter().filter(|&&index| index == ts_index).count(),
                    1
                );
                assert_eq!(
                    backward.iter().filter(|&&index| index == ts_index).count(),
                    usize::from(reverse)
                );
            }
        }
    }

    #[test]
    fn names_use_the_reaction_or_molecule() {
        assert_eq!(file_name("Pent-4-enal-irc"), "pent-4-enal-irc.gif");
        assert_eq!(file_name("Water mode 2"), "water-mode-2.gif");
        assert_eq!(file_name("../../"), "animation.gif");
    }

    #[test]
    fn decoder_reads_size_count_delay_and_repeat() {
        for mode in PlaybackMode::ALL {
            let bytes = encode(SMALL, mode, &[0, 1, 0], &Work::default(), pixels)
                .unwrap()
                .unwrap();
            let mut options = gif::DecodeOptions::new();
            options.set_color_output(gif::ColorOutput::RGBA);
            let mut decoded = options.read_info(bytes.as_slice()).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (2, 2));
            let mut count = 0;
            while let Some(frame) = decoded.read_next_frame().unwrap() {
                assert_eq!(frame.delay, 5);
                assert_eq!(frame.dispose, gif::DisposalMethod::Background);
                assert_eq!(frame.buffer[7], 0);
                count += 1;
            }
            assert_eq!(count, 3);
            assert_eq!(
                decoded.repeat() == gif::Repeat::Infinite,
                mode != PlaybackMode::Once
            );
        }
    }

    #[test]
    fn a_single_saved_geometry_is_rendered_once_and_held_for_three_seconds() {
        let sequence = irc_frames(1, 0, PlaybackMode::Once, 1.);
        let mut rendered = 0;
        let bytes = encode(
            SMALL,
            PlaybackMode::Once,
            &sequence,
            &Work::default(),
            |frame, index| {
                rendered += 1;
                pixels(frame, index)
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(rendered, 1);
        let mut decoded = gif::DecodeOptions::new()
            .read_info(bytes.as_slice())
            .unwrap();
        let mut duration = 0;
        while let Some(frame) = decoded.read_next_frame().unwrap() {
            duration += frame.delay;
        }
        assert_eq!(duration, 300);
    }

    #[test]
    fn cancel_stops_rendering_without_creating_or_replacing_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("water-mode-2.gif");
        for existing in [false, true] {
            if existing {
                std::fs::write(&path, b"existing").unwrap();
            }
            let work = Work::default();
            let saved = save(
                &path,
                SMALL,
                PlaybackMode::Loop,
                &[0, 1, 2],
                &work,
                |frame, i| {
                    assert!(i < 2);
                    if i == 1 {
                        assert!(work.cancel());
                    }
                    pixels(frame, i)
                },
            )
            .unwrap();
            assert!(!saved);
            assert_eq!(work.completed.load(Relaxed), 1);
            if existing {
                assert_eq!(std::fs::read(&path).unwrap(), b"existing");
            } else {
                assert!(!path.exists());
            }
        }
    }

    #[test]
    fn plotted_frames_get_distinct_ordinals_for_repeated_and_reverse_samples() {
        let sequence = [0, 1, 1, 0];
        let mut rendered = Vec::new();
        let bytes = encode(
            Options {
                plot: true,
                ..SMALL
            },
            PlaybackMode::BackAndForth,
            &sequence,
            &Work::default(),
            |frame, index| {
                rendered.push((frame, index));
                pixels(frame, index)
            },
        )
        .unwrap()
        .unwrap();
        let mut decoded = gif::DecodeOptions::new()
            .read_info(bytes.as_slice())
            .unwrap();
        let decoded_count =
            std::iter::from_fn(|| decoded.read_next_frame().unwrap().map(|_| ())).count();
        assert_eq!(decoded_count, sequence.len());
        assert!(rendered.into_iter().eq(sequence.into_iter().enumerate()));
    }
}
