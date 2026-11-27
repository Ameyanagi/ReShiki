use super::*;
use crate::app::profile_animation::tests::claisen;

fn source() -> Source {
    let (doc, id) = claisen();
    let profile = doc.graphics[0].energy_profile.as_deref().unwrap();
    let motion = profile_animation::Motion::Steps;
    let timeline = profile_animation::Timeline::new(profile, &[], motion);
    Source {
        name: "Claisen rearrangement".into(),
        doc,
        model: 0,
        motion: Motion::Profile {
            id,
            timeline,
            motion,
        },
        mode: PlaybackMode::Loop,
        speed: 1.,
    }
}

#[test]
fn profile_export_sequence_and_format_follow_the_current_settings() {
    let mut source = source();
    assert_eq!(
        source.sequence(),
        [vec![0; 16], vec![1; 16], vec![2; 16]].concat()
    );
    source.mode = PlaybackMode::BackAndForth;
    assert_eq!(
        source.sequence(),
        [vec![0; 16], vec![1; 16], vec![2; 16], vec![1; 16]].concat()
    );
    source.speed = 2.;
    assert_eq!(source.sequence().len(), 32);
    assert_eq!(
        Format::available(true).len(),
        if cfg!(target_os = "macos") { 2 } else { 1 }
    );
    assert_eq!(Format::available(false), [Format::Gif]);
}

#[test]
fn profile_gif_encodes_every_state_at_twenty_frames_per_second() {
    let mut source = source();
    source.speed = 2.;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("claisen.gif");
    let options = Options {
        transparent: true,
        ..Options::default()
    };
    assert!(
        render(
            &source,
            options,
            Format::Gif,
            &source.sequence(),
            &Work::default(),
            &path
        )
        .unwrap()
    );
    let bytes = std::fs::read(path).unwrap();
    let mut decode = gif::DecodeOptions::new();
    decode.set_color_output(gif::ColorOutput::RGBA);
    let mut gif = decode.read_info(bytes.as_slice()).unwrap();
    assert_eq!((gif.width(), gif.height()), (480, 480));
    let mut previous = None;
    let (mut frames, mut unique) = (0, 0);
    while let Some(frame) = gif.read_next_frame().unwrap() {
        assert_eq!(frame.delay, 5);
        assert!(
            frame
                .buffer
                .as_chunks::<4>()
                .0
                .iter()
                .any(|rgba| rgba[3] == 0)
        );
        if previous.as_deref() != Some(frame.buffer.as_ref()) {
            unique += 1;
            previous = Some(frame.buffer.to_vec());
        }
        frames += 1;
    }
    assert_eq!((frames, unique), (24, 3));
}

#[test]
#[ignore = "writes sample files to RESHIKI_PROFILE_SAMPLE_DIR"]
fn write_claisen_samples() {
    let source = source();
    let directory = PathBuf::from(std::env::var_os("RESHIKI_PROFILE_SAMPLE_DIR").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let sequence = source.sequence();
    for format in Format::available(true) {
        let path = directory.join(encode::file_name(&source.name, format.extension()));
        let start = std::time::Instant::now();
        match render(
            &source,
            Options::default(),
            *format,
            &sequence,
            &Work::default(),
            &path,
        ) {
            Ok(true) => eprintln!(
                "{}: {} bytes, {} frames, {:.2} s at {FPS} fps, encoded in {:.2} s",
                path.display(),
                path.metadata().unwrap().len(),
                sequence.len(),
                sequence.len() as f64 / FPS as f64,
                start.elapsed().as_secs_f64(),
            ),
            failure => panic!("{}: {failure:?}", path.display()),
        }
    }
}
