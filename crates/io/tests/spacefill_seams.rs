//! Space-filling crystals are cut where their balls meet (#756).
use reshiki_model::{
    models3d::{Display, Style, primitives, set_display, turn},
    scene::Primitive,
};

fn spheres(doc: &reshiki_model::document::Document) -> (usize, usize) {
    let all = primitives(doc);
    let total = all
        .iter()
        .filter(|p| matches!(p, Primitive::Sphere { .. }))
        .count();
    let cut = all
        .iter()
        .filter(|p| {
            matches!(
                p,
                Primitive::Sphere {
                    contact: Some(_),
                    ..
                }
            )
        })
        .count();
    (total, cut)
}

#[test]
fn space_filling_crystals_are_cut_where_balls_meet() {
    for (name, text) in [
        (
            "nacl",
            include_str!("../../../tests/fixtures/crystal/nacl.cif"),
        ),
        (
            "co2",
            include_str!("../../../tests/fixtures/crystal/co2.cif"),
        ),
        (
            "cod",
            include_str!("../../../tests/fixtures/crystal/cod-7033930.cif"),
        ),
    ] {
        let (mut doc, _) = reshiki_io::crystal::read("cif", text).unwrap();
        assert!(set_display(&mut doc, 0, Display::ThreeD));
        doc.models[0].style = Style::SpaceFilling;
        for turning in [false, true] {
            doc.models[0].turning = turning;
            for _ in 0..3 {
                let (total, cut) = spheres(&doc);
                assert!(
                    total > 0 && cut * 2 > total,
                    "{name}, turning={turning}: {cut} of {total} cut"
                );
                doc.models[0].rotation = turn(doc.models[0].rotation, 12., 5., false);
            }
        }
        doc.models[0].turning = false;
        let still = format!("{:?}", primitives(&doc));
        doc.models[0].turning = true;
        let _ = primitives(&doc);
        doc.models[0].turning = false;
        assert_eq!(format!("{:?}", primitives(&doc)), still, "{name}");
    }
}

/// A P1 supercell of `text`'s crystal, `n` cells along each axis.
fn supercell(text: &str, n: [u8; 3]) -> reshiki_model::document::Document {
    let (doc, _) = reshiki_io::crystal::read("cif", text).unwrap();
    let model = &doc.models[0];
    let cell = model.cell.as_ref().unwrap();
    let len = |v: [f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let angle = |a: [f32; 3], b: [f32; 3]| {
        ((a[0] * b[0] + a[1] * b[1] + a[2] * b[2]) / len(a) / len(b))
            .acos()
            .to_degrees()
    };
    let [a, b, c] = cell.vectors;
    let mut cif = format!(
        "data_super\n_cell_length_a {}\n_cell_length_b {}\n_cell_length_c {}\n_cell_angle_alpha {}\n_cell_angle_beta {}\n_cell_angle_gamma {}\n_symmetry_space_group_name_H-M 'P 1'\nloop_\n_atom_site_label\n_atom_site_type_symbol\n_atom_site_fract_x\n_atom_site_fract_y\n_atom_site_fract_z\n",
        len(a) * f32::from(n[0]),
        len(b) * f32::from(n[1]),
        len(c) * f32::from(n[2]),
        angle(b, c),
        angle(a, c),
        angle(a, b),
    );
    let mut k = 0;
    for atom in &model.atoms {
        let element = &doc.atom(atom.id).unwrap().element;
        let f = cell.fractional(atom.xyz);
        for i in 0..n[0] {
            for j in 0..n[1] {
                for l in 0..n[2] {
                    k += 1;
                    let g = [
                        f[0] + f32::from(i),
                        f[1] + f32::from(j),
                        f[2] + f32::from(l),
                    ];
                    cif += &format!(
                        "{element}{k} {element} {} {} {}\n",
                        g[0] / f32::from(n[0]),
                        g[1] / f32::from(n[1]),
                        g[2] / f32::from(n[2])
                    );
                }
            }
        }
    }
    reshiki_io::crystal::read("cif", &cif).unwrap().0
}

/// Frame times of a space-filling crystal turned a little each frame, still
/// and turning; run with `--release --ignored --nocapture`.
#[test]
#[ignore]
fn turning_frame_times() {
    let cod = include_str!("../../../tests/fixtures/crystal/cod-7033930.cif");
    let nacl = include_str!("../../../tests/fixtures/crystal/nacl.cif");
    for (name, mut doc) in [
        ("cod 1x1x1", supercell(cod, [1, 1, 1])),
        ("cod 2x2x1", supercell(cod, [2, 2, 1])),
        ("cod 3x3x3", supercell(cod, [3, 3, 3])),
        ("nacl 3x3x3", supercell(nacl, [3, 3, 3])),
    ] {
        assert!(set_display(&mut doc, 0, Display::ThreeD));
        doc.models[0].style = Style::SpaceFilling;
        let balls = doc.models[0].atoms.len();
        for turning in [false, true] {
            doc.models[0].turning = turning;
            let frames = 20;
            let start = std::time::Instant::now();
            for _ in 0..frames {
                doc.models[0].rotation = turn(doc.models[0].rotation, 3., 1., false);
                std::hint::black_box(primitives(&doc));
            }
            let ms = start.elapsed().as_secs_f64() * 1e3 / f64::from(frames);
            eprintln!("{name} ({balls} balls) turning={turning}: {ms:.2} ms/frame");
        }
    }
}
