//! Six-coordinate drawings the desktop 3D model view now builds in relaxed
//! mode. Fixture provenance is in fixtures/README.md.
use reshiki_geometry::{ForceField, Request, Response, solve};

fn request(json: &str) -> Request {
    let request: Request = serde_json::from_str(json).unwrap();
    request.validate().unwrap();
    request
}

fn distance(response: &Response, a: usize, b: usize) -> f64 {
    let [p, q] = [a, b].map(|i| response.coordinates[i]);
    (0..3).map(|i| (p[i] - q[i]).powi(2)).sum::<f64>().sqrt()
}

/// The six ligand angles at `centre`: twelve near 90° and three near 180°.
fn assert_octahedral(request: &Request, response: &Response, centre: usize) {
    let ligands: Vec<_> = request
        .bonds
        .iter()
        .filter_map(|b| {
            (b.a == centre)
                .then_some(b.b)
                .or((b.b == centre).then_some(b.a))
        })
        .collect();
    assert_eq!(ligands.len(), 6);
    let mut trans = 0;
    for (index, &a) in ligands.iter().enumerate() {
        for &b in &ligands[index + 1..] {
            let [u, v] = [a, b].map(|i| {
                let [p, c] = [response.coordinates[i], response.coordinates[centre]];
                [p[0] - c[0], p[1] - c[1], p[2] - c[2]]
            });
            let cosine = (0..3).map(|i| u[i] * v[i]).sum::<f64>()
                / (distance(response, a, centre) * distance(response, b, centre));
            let degrees = cosine.clamp(-1., 1.).acos().to_degrees();
            let expected = if degrees > 135. {
                trans += 1;
                180.
            } else {
                90.
            };
            assert!(
                (degrees - expected).abs() < 5.,
                "{a}-{centre}-{b}: {degrees}°"
            );
        }
    }
    assert_eq!(trans, 3);
}

#[test]
fn flat_uranium_hexafluoride_drawing_becomes_an_octahedron() {
    // On main two fluorines collapsed to 30° (F···F 1.19 Å): UFF's octahedral
    // angle term has a minimum at 0° and UFF has no 1-3 repulsion.
    // The view asks for MMFF94s and gets UFF; the Optimization panel asks for UFF.
    for field in [ForceField::UFF, ForceField::MMFF94s] {
        let request = Request {
            field,
            ..request(include_str!("fixtures/uf6-request.json"))
        };
        let response = solve(&request).unwrap();
        assert_eq!(response.field, ForceField::UFF);
        assert_octahedral(&request, &response, 1);
        for fluorine in [0, 2, 3, 4, 5, 6] {
            let length = distance(&response, 1, fluorine);
            assert!((2.1..2.25).contains(&length), "U-F {length} A");
        }
    }
}

#[test]
fn hexanitratocerate_keeps_its_nitrates_intact() {
    // Lifting only the donor oxygens tore the planar nitrates apart (N-O
    // 0.71 Å); on main they collapsed into each other (0.74 Å contacts).
    let request = Request {
        field: ForceField::UFF,
        ..request(include_str!("fixtures/ce-nitrate6-request.json"))
    };
    let response = solve(&request).unwrap();
    assert_octahedral(&request, &response, 0);
    for bond in &request.bonds {
        let length = distance(&response, bond.a, bond.b);
        let range = if bond.a == 0 || bond.b == 0 {
            2.1..2.6
        } else {
            1.15..1.45
        };
        assert!(range.contains(&length), "{bond:?}: {length} A");
    }
    let n = response.coordinates.len();
    for a in 0..n {
        for b in a + 1..n {
            assert!(distance(&response, a, b) > 1.1, "atoms {a} and {b} overlap");
        }
    }
}
