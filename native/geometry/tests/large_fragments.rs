//! Large nano-fragments that the desktop 3D model used to give up on.
//! Fixture provenance is in fixtures/README.md.
use reshiki_geometry::{Request, Response, solve};

fn request(json: &str) -> Request {
    let request: Request = serde_json::from_str(json).unwrap();
    request.validate().unwrap();
    request
}

fn distance(response: &Response, a: usize, b: usize) -> f64 {
    let [p, q] = [a, b].map(|i| response.coordinates[i]);
    (0..3).map(|i| (p[i] - q[i]).powi(2)).sum::<f64>().sqrt()
}

/// Every drawn bond within `lengths`, every H on its parent, no atom overlaps.
fn assert_geometry(request: &Request, response: &Response, lengths: std::ops::Range<f64>) {
    for bond in &request.bonds {
        let length = distance(response, bond.a, bond.b);
        assert!(lengths.contains(&length), "bond {bond:?}: {length} A");
    }
    for (offset, &parent) in response.hydrogen_parents.iter().enumerate() {
        let length = distance(response, parent, response.original_count + offset);
        assert!((1.0..1.15).contains(&length), "H on {parent}: {length} A");
    }
    for a in 0..response.coordinates.len() {
        for b in a + 1..response.coordinates.len() {
            assert!(distance(response, a, b) > 0.9, "atoms {a} and {b} overlap");
        }
    }
}

#[test]
fn symmetric_c96h24_graphene_flake_generates() {
    // ETKDGv3 embeds this flake in about a second, but RMS pruning's symmetry
    // search over its 120 atoms did not finish within the worker deadline.
    // Fail at the longest worker deadline instead of hanging the test run.
    let request = request(include_str!("fixtures/graphene-c96h24-request.json"));
    let (sender, receiver) = std::sync::mpsc::channel();
    let solving = request.clone();
    std::thread::spawn(move || {
        let _ = sender.send(solve(&solving));
    });
    let response = receiver
        .recv_timeout(std::time::Duration::from_secs(120))
        .expect("generation outlasted the 120-second worker deadline")
        .unwrap();
    assert_eq!(response.hydrogen_parents.len(), 24);
    assert_geometry(&request, &response, 1.36..1.46);
    assert!(response.converged, "{:?}", response.diagnostics);
}

#[test]
fn flexible_dab_dendrimer_retries_from_random_coordinates() {
    // Every eigenvector start fails for this 150-atom poly(propylene imine)
    // dendrimer, so generation needs random starting coordinates.
    let request = request(include_str!("fixtures/dab-am-8-dendrimer-request.json"));
    let response = solve(&request).unwrap();
    assert_eq!(response.hydrogen_parents.len(), 96);
    assert_geometry(&request, &response, 1.40..1.60);
    assert!(
        response
            .diagnostics
            .iter()
            .any(|line| line.starts_with("initialization=random-coordinates")),
        "{:?}",
        response.diagnostics
    );
}
