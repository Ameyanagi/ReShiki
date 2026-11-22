//! Nominal pipe thread identity is distinct from a tube's outside diameter.
use super::format::{self, Gender, PlugFamily};
use super::solve::{self, JointRef, Kind, Port};

fn port(family: PlugFamily, size: f64, gender: Gender) -> Port {
    Port {
        at: JointRef {
            piece: 1,
            joint: "process".into(),
        },
        kind: Kind::Plug(family, size),
        gender,
        point: [0., 0.],
        dir: [-1., 0.],
        mm: 1.,
        length: 0.,
        contact_length: 0.,
        insertion: 0.,
    }
}

#[test]
fn npt_requires_the_same_thread_family_nominal_size_and_opposite_gender() {
    let socket = port(PlugFamily::Npt, 6.35, Gender::F);
    assert!(solve::mates(
        &socket,
        &port(PlugFamily::Npt, 6.35, Gender::M)
    ));
    for other in [
        port(PlugFamily::Npt, 6.35, Gender::F),
        port(PlugFamily::Npt, 6., Gender::M),
        port(PlugFamily::Npt, 3.175, Gender::M),
        port(PlugFamily::Compression, 6.35, Gender::M),
        port(PlugFamily::Hose, 6.35, Gender::M),
    ] {
        assert!(!solve::mates(&socket, &other));
    }
    assert_eq!(socket.size(), "1/4 in NPT");
    assert_eq!(socket.diameter(), 13.716);
    assert_eq!(
        port(PlugFamily::Compression, 6.35, Gender::F).diameter(),
        6.35
    );
}

#[test]
fn npt_definition_serializes_its_type_and_rejects_unimplemented_nominal_sizes() {
    let text = r#"{
        "format":"reshiki-piece","format_version":0,"id":"test/thread",
        "revision":1,"family":"thread","variant":{},"names":{"en":"Thread"},
        "category":"other","units":"mm",
        "shapes":[{"id":"body","layer":"body","material":"metal","role":"solid",
                   "path":"M-10 -8H10V8H-10Z"}],
        "joints":[{"id":"process","kind":"plug","family":"npt","gender":"F",
                   "size_mm":6.35,"at":[-10,0],"dir":[-1,0]}]
    }"#;
    let definition = format::parse(text).unwrap();
    assert!(
        serde_json::to_string(&definition)
            .unwrap()
            .contains("\"family\":\"npt\"")
    );
    let error = format::parse(&text.replace("\"size_mm\":6.35", "\"size_mm\":3.175")).unwrap_err();
    assert!(error.contains("only 1/4 in NPT"), "{error}");
    let error =
        format::parse(&text.replace("\"size_mm\":6.35", "\"size_mm\":6.35,\"insertion_mm\":5"))
            .unwrap_err();
    assert!(error.contains("only on a female compression"), "{error}");
}
