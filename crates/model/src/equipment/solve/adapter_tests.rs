//! Indexed adapter choices must preserve the original catalogue-first scan.
use super::*;
use crate::equipment::format::Category;

fn scanned_adapter(s: &Port, m: &Port) -> Option<(Arc<Definition>, String, String)> {
    let probe = |j: &Joint, like: &Port| Port {
        kind: match j {
            Joint::Taper { size, .. } => Kind::Taper(size.clone()),
            Joint::Plug {
                family, size_mm, ..
            } => Kind::Plug(*family, *size_mm),
        },
        gender: j.gender(),
        contact_length: 0.,
        insertion: 0.,
        ..like.clone()
    };
    builtin::catalogue()
        .iter()
        .filter(|d| d.category == Category::Adapter)
        .find_map(|d| {
            d.joints.iter().find_map(|a| {
                let b = d
                    .joints
                    .iter()
                    .find(|b| b.id() != a.id() && mates(&probe(b, m), m))?;
                mates(&probe(a, s), s).then(|| (d.clone(), a.id().to_owned(), b.id().to_owned()))
            })
        })
}

fn probe(kind: Kind, gender: Gender) -> Port {
    Port {
        at: JointRef {
            piece: 0,
            joint: String::new(),
        },
        kind,
        gender,
        point: [0., 0.],
        dir: [1., 0.],
        mm: 1.,
        length: 0.,
        contact_length: 0.,
        insertion: 0.,
    }
}
fn choice(found: Option<(Arc<Definition>, String, String)>) -> Option<(String, String, String)> {
    found.map(|(definition, near, far)| (definition.id.clone(), near, far))
}

#[test]
fn indexed_adapters_preserve_every_catalogue_interface_and_legacy_tolerance() {
    let mut probes: Vec<Port> = Vec::new();
    for d in builtin::catalogue() {
        for joint in &d.joints {
            let kind = match joint {
                Joint::Taper { size, .. } => Kind::Taper(size.clone()),
                Joint::Plug {
                    family, size_mm, ..
                } => Kind::Plug(*family, *size_mm),
            };
            if !probes
                .iter()
                .any(|p| p.kind == kind && p.gender == joint.gender())
            {
                probes.push(probe(kind, joint.gender()));
            }
        }
    }
    // Values on both sides of legacy 10% tolerance, and near exact nominal ODs.
    for family in [
        PlugFamily::Hose,
        PlugFamily::Stopper,
        PlugFamily::Compression,
        PlugFamily::WorkTube,
        PlugFamily::Npt,
    ] {
        for size in [8.9999, 9., 9.0001, 10., 6., 6.35] {
            for gender in [Gender::M, Gender::F] {
                probes.push(probe(Kind::Plug(family, size), gender));
            }
        }
    }
    probes.push(probe(Kind::Hold(HoldKind::TubeSpan, [6.35; 2]), Gender::F));
    for s in &probes {
        for m in &probes {
            assert_eq!(
                choice(adapter(s, m)),
                choice(scanned_adapter(s, m)),
                "{:?} {:?}",
                s.kind,
                m.kind
            );
        }
    }
}

#[test]
#[ignore = "isolated requested-Rust-allocation measurement; run with --test-threads=1"]
fn indexed_negative_adapter_lookup_allocates_nothing() {
    let s = probe(Kind::Plug(PlugFamily::Npt, 6.35), Gender::M);
    let m = probe(Kind::Plug(PlugFamily::WorkTube, 15.), Gender::M);
    assert!(adapter(&s, &m).is_none()); // initialize metadata outside measurement
    let (_, allocations) = crate::parity_tests::measured(|| {
        for _ in 0..1_000 {
            assert!(std::hint::black_box(adapter(&s, &m)).is_none());
        }
    });
    assert_eq!(allocations.count, 0);
}
