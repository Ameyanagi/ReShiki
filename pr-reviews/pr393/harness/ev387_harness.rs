//! Evidence harness for #387 (temporary; copied into tests/ of each tree, run, then removed).
//! Builds methoxide (CH3-O, circled minus, 3 lone pairs) where every mark is placed by
//! the symbol tool's own drag code (`Drawing::place`, start on O, release at an angle),
//! identical inputs on origin/main and on the branch. Writes .rsk files to $EV387_OUT.
use reshiki::{
    document::Point,
    editing::{self, Transform},
    engine::{LocalEngine, Request},
    graphics::{GraphicKind, GraphicStyle},
    scientific::{Drawing, Phase, SymbolKind},
};

fn drawing(kind: SymbolKind) -> Drawing {
    Drawing {
        kind: GraphicKind::Symbol(kind),
        style: GraphicStyle::default(),
        phase: Phase::Solid,
        flipped: false,
        attach: true,
        snap_orbitals: true,
    }
}

#[tokio::test]
async fn ev387_methoxide_drags() {
    let out = std::path::PathBuf::from(std::env::var("EV387_OUT").expect("EV387_OUT"));
    std::fs::create_dir_all(&out).unwrap();
    let engine = LocalEngine::default();
    let base = engine
        .request(Request::import_smiles("CO"))
        .await
        .unwrap()
        .document
        .unwrap();
    let o = base.atoms.iter().position(|a| a.element == "O").unwrap();
    let c = 1 - o;
    let pos = base.atoms[o].position;
    let bond = base.atoms[c].position.distance(pos);
    // Release a typical distance from O: 0.6 bond lengths.
    let reach = 0.6 * bond;
    // Screen angle, counter-clockwise from +x (world y points down).
    let at = |deg: f32| {
        let (s, co) = deg.to_radians().sin_cos();
        pos.offset(reach * co, -reach * s)
    };
    let mut log = format!(
        "O at ({:.2},{:.2}); C at ({:.2},{:.2}); bond {:.2}; release reach {:.2}\n",
        pos.x, pos.y, base.atoms[c].position.x, base.atoms[c].position.y, bond, reach
    );
    let mut placed = Vec::new();
    for theta in [0_f32, 45., 90., 135., 200.] {
        let mut doc = base.clone();
        let drags = [
            (SymbolKind::CircleMinus, theta + 180.),
            (SymbolKind::LonePair, theta),
            (SymbolKind::LonePair, theta + 120.),
            (SymbolKind::LonePair, theta + 240.),
        ];
        for (kind, deg) in drags {
            drawing(kind)
                .place(&mut doc, pos, at(deg), false, 10.)
                .unwrap();
        }
        log += &format!("theta {theta}:\n");
        for m in &doc.atoms[o].marks {
            let dir = (-m.offset.y).atan2(m.offset.x).to_degrees().rem_euclid(360.);
            log += &format!(
                "  {:?} offset ({:.2},{:.2}) |r| {:.2} dir {:.1} angle {:.1}\n",
                m.kind,
                m.offset.x,
                m.offset.y,
                (m.offset.x.powi(2) + m.offset.y.powi(2)).sqrt(),
                dir,
                m.angle
            );
        }
        std::fs::write(out.join(format!("methoxide_{theta:03}.rsk")), doc.file_json().unwrap())
            .unwrap();
        placed.push((theta, doc));
    }
    // Same molecule (theta = 45 panel) rotated 30 degrees through the Rotate command path.
    let mut doc = placed[1].1.clone();
    let ids: Vec<u64> = doc
        .atoms
        .iter()
        .map(|a| a.id)
        .collect();
    editing::transform(&mut doc, &ids, Transform::Rotate(30.));
    log += "theta 45 rotated 30:\n";
    let oa = doc.atoms.iter().find(|a| a.element == "O").unwrap();
    for m in &oa.marks {
        let dir = (-m.offset.y).atan2(m.offset.x).to_degrees().rem_euclid(360.);
        log += &format!(
            "  {:?} offset ({:.2},{:.2}) dir {:.1} angle {:.1}\n",
            m.kind, m.offset.x, m.offset.y, dir, m.angle
        );
    }
    std::fs::write(out.join("methoxide_045_rot30.rsk"), doc.file_json().unwrap()).unwrap();
    std::fs::write(out.join("marks.txt"), log).unwrap();
}
