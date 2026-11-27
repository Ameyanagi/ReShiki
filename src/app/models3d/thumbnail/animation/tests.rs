use super::*;
use reshiki::models3d::{ForceField, Hydrogen, Model3d, Placed};

fn molecule() -> (Document, Vec<[f32; 3]>) {
    let mut doc = Document::default();
    let positions = vec![
        [-1.2, 0., 0.1],
        [0., 0.3, -0.1],
        [1.2, 0., 0.2],
        [-1.6, 0.8, 0.],
    ];
    let atoms: Vec<_> = ["C", "C", "O"]
        .into_iter()
        .zip(&positions)
        .map(|(symbol, &xyz)| Placed {
            id: doc.add_atom(symbol, Point::new(xyz[0] * 30., xyz[1] * 30.)),
            xyz,
        })
        .collect();
    for pair in atoms.windows(2) {
        doc.add_bond(pair[0].id, pair[1].id, 1, "plain");
    }
    let parent = atoms.first().unwrap().id;
    let mut model = Model3d::new(
        &doc,
        atoms,
        vec![Hydrogen {
            parent,
            xyz: positions[3],
        }],
        ForceField::Mmff94s,
    );
    model.display = Display::ThreeD;
    model.rotation = [1., 0., 0., 0.];
    doc.models.push(model);
    (doc, positions)
}

#[test]
fn geometry_and_graph_validation_include_added_hydrogens() {
    let (doc, positions) = molecule();
    let mut renderer = AnimationRenderer::new(&doc, 0, 480, true).unwrap();
    assert!(renderer.positions(&positions[..3]).is_err());
    let mut invalid = positions.clone();
    invalid[3][1] = f32::NAN;
    assert!(renderer.positions(&invalid).is_err());
    let mut transferred = positions.clone();
    transferred[3] = [1.2, 0., 1.];
    let angstrom: Vec<_> = transferred
        .iter()
        .flatten()
        .copied()
        .map(f64::from)
        .collect();
    assert!(renderer.set_irc_topology(&[6, 6, 8], &angstrom).is_err());
    renderer.set_irc_topology(&[6, 6, 8, 1], &angstrom).unwrap();
    renderer.positions(&transferred).unwrap();
    let model = renderer.doc.models.first().unwrap();
    assert_eq!(model.atoms.len(), positions.len());
    assert!(model.hydrogens.is_empty());
    assert_eq!(
        model.generated.as_ref().unwrap().bonds.last(),
        Some(&[2, 3])
    );
    assert_eq!(doc.models.first().unwrap().hydrogens.len(), 1);
    assert_eq!(
        model.points(&renderer.doc).unwrap()[3],
        ("H", transferred[3])
    );
}

#[test]
fn svg_labels_use_round_ticks_including_zero() {
    let mut svg = String::new();
    let plot = AnimationPlot {
        points: &[[-8.8, 0.], [0., 27.3], [8.7, -4.3]],
        marker: [0., 27.3],
        x_label: "s",
        y_label: "ΔE",
        readout: "frame 31 / 120",
        direction: "at the transition state",
    };
    let doc = Document::default();
    plot_svg(&mut svg, &plot, Palette::in_mode(&doc, doc.canvas_theme)).unwrap();
    let labels: Vec<_> = svg
        .split_terminator("</text>")
        .filter(|s| s.contains("<text "))
        .filter_map(|s| s.rsplit_once('>').map(|(_, text)| text))
        .collect();
    assert_eq!(
        labels,
        [
            "frame 31 / 120",
            "at the transition state",
            "-10",
            "0",
            "10",
            "20",
            "30",
            "-10",
            "-5",
            "0",
            "5",
            "10",
            "s",
            "ΔE"
        ]
    );
}

#[test]
fn offscreen_movie_frames_have_square_nonempty_pixels_and_a_moving_plot_marker() {
    let (doc, positions) = molecule();
    let mut displaced = positions.clone();
    displaced[2][1] += 0.35;
    let frames = vec![positions.clone(), displaced.clone()];
    let mut plot = AnimationPlot {
        points: &[[-2.4, 0.], [0., 18.6], [2.4, -2.]],
        marker: [0., 18.6],
        x_label: "s / amu½·bohr",
        y_label: "ΔE / kcal/mol",
        readout: "s = 0.0 amu½·bohr · ΔE 18.6 kcal/mol · frame 2 / 3",
        direction: "Reactant → product",
    };
    for side in [480, 720, 1080] {
        for transparent in [false, true] {
            let mut renderer = AnimationRenderer::new(&doc, 0, side, transparent).unwrap();
            renderer.fit(&frames).unwrap();
            let first = renderer.render(&positions, Some(&plot)).unwrap();
            assert_eq!(first.len(), side as usize * side as usize * 4);
            assert!(
                first
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[3] > 0 && p[..3] != [255; 3])
            );
            assert_eq!(first[3], if transparent { 0 } else { 255 });
            if !transparent {
                assert!(first.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
            }
            plot.marker = [2.4, -2.];
            let second = renderer.render(&positions, Some(&plot)).unwrap();
            assert_ne!(first, second);
            let moved = renderer.render(&displaced, Some(&plot)).unwrap();
            assert_ne!(second, moved);
            plot.marker = [0., 18.6];
        }
    }
    plot.points = &[[0., 18.6]];
    let mut renderer = AnimationRenderer::new(&doc, 0, 480, true).unwrap();
    assert!(renderer.render(&positions, Some(&plot)).is_ok());
}
