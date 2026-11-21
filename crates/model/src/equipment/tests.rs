use super::eval::{Value, Values, evaluate, resolve};
use super::format::{Definition, Material, Param, content_hash, parse, taper};
use super::geometry::{Affine, Seg, area, flatten, len, sub};
use super::plan::{Ink, Instance, Pass, PlanPath, Settings, Weight, mating_transform, paint_plan};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/equipment/fixtures");
const GOLDENS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/equipment/fixtures/paint-plans.json"
);
/// 0.4 pt per mm in world units.
const SCALE: f64 = 0.4 / (14.4 / 42.);

fn fixtures() -> Vec<(String, Definition)> {
    let mut out: Vec<_> = std::fs::read_dir(FIXTURES)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "reshiki-piece"))
        .map(|p| {
            let text = std::fs::read_to_string(&p).unwrap();
            let name = p.file_stem().unwrap().to_string_lossy().into_owned();
            let def = parse(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            (name, def)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn fixture(name: &str) -> Definition {
    let text = std::fs::read_to_string(format!("{FIXTURES}/{name}.reshiki-piece")).unwrap();
    parse(&text).unwrap()
}

/// Every state of every states parameter, and both ends of every range.
fn cases(def: &Definition) -> Vec<(String, Values)> {
    let mut out = vec![("default".to_string(), Values::new())];
    for p in &def.params {
        let id = p.id().to_string();
        match p {
            Param::States { states, .. } => out.extend(states.iter().map(|s| {
                (
                    format!("{id}={}", s.id),
                    Values::from([(id.clone(), Value::State(s.id.clone()))]),
                )
            })),
            Param::Range { min, max, .. } => out.extend([*min, *max].map(|v| {
                (
                    format!("{id}={v}"),
                    Values::from([(id.clone(), Value::Number(v))]),
                )
            })),
            Param::Fill { .. } => out.extend([0., 0.5, 1.].map(|v| {
                (
                    format!("{id}={v}"),
                    Values::from([(id.clone(), Value::Number(v))]),
                )
            })),
            Param::Color { .. } => {}
        }
    }
    out
}

fn plan(def: &Definition, values: Values) -> Vec<PlanPath> {
    let mut instance = Instance::new(def, Affine::scale(SCALE, [0., 0.]));
    instance.values = values;
    if def.tube.is_some() {
        instance.centre_line = vec![[0., 0.], [30., 10.], [60., 0.]];
    }
    paint_plan(&[instance], Settings::default()).unwrap()
}

/// A compact fingerprint of a paint plan: exact counts per pass, and point
/// sums and bounds compared within a tolerance, so libm differences between
/// platforms do not matter but any change in geometry does.
fn summary(plan: &[PlanPath]) -> Json {
    let mut passes: BTreeMap<String, usize> = BTreeMap::new();
    let (mut points, mut sx, mut sy, mut mx, mut my) = (0usize, 0f64, 0f64, 0f64, 0f64);
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in plan {
        *passes
            .entry(format!("{:?}/{:?}/{:?}", p.pass, p.material, p.ink))
            .or_default() += 1;
        for s in &p.path {
            let pts: &[[f64; 2]] = match s {
                Seg::Move(p) | Seg::Line(p) => std::slice::from_ref(p),
                Seg::Cubic(a, _, _) => std::slice::from_ref(a),
                Seg::Close => &[],
            };
            let all: Vec<[f64; 2]> = match s {
                Seg::Cubic(a, c, p) => vec![*a, *c, *p],
                _ => pts.to_vec(),
            };
            for q in all {
                points += 1;
                sx += q[0];
                sy += q[1];
                // Weighted by position, so moving a point within a path shows.
                mx += q[0] * points as f64;
                my += q[1] * points as f64;
                b = [
                    b[0].min(q[0]),
                    b[1].min(q[1]),
                    b[2].max(q[0]),
                    b[3].max(q[1]),
                ];
            }
        }
    }
    let r = |v: f64| (v * 100.).round() / 100.;
    // Paint order, exactly: pass, material, ink and size of every path in turn.
    let order: String = plan
        .iter()
        .map(|p| {
            format!(
                "{}/{:?}/{:?}/{:?}/{:?}/{:?}/{};",
                p.owner,
                p.pass,
                p.material,
                p.color,
                p.style,
                p.ink,
                p.path.len()
            )
        })
        .collect();
    let order = {
        use sha2::{Digest, Sha256};
        Sha256::digest(order.as_bytes())
            .iter()
            .take(8)
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    json!({"passes": passes, "points": points, "order": order, "sum": [r(sx), r(sy)], "moment": [r(mx), r(my)], "bounds": b.map(r)})
}

#[test]
fn every_fixture_matches_its_paint_plan_golden() {
    let mut actual = BTreeMap::new();
    for (name, def) in fixtures() {
        for (case, values) in cases(&def) {
            actual.insert(format!("{name} {case}"), summary(&plan(&def, values)));
        }
    }
    actual.insert("setup reflux".into(), summary(&reflux()));
    if std::env::var("RESHIKI_UPDATE_PIECE_GOLDENS").as_deref() == Ok("1") {
        std::fs::write(
            GOLDENS,
            serde_json::to_string_pretty(&actual).unwrap() + "\n",
        )
        .unwrap();
        return;
    }
    let expected: BTreeMap<String, Json> =
        serde_json::from_str(&std::fs::read_to_string(GOLDENS).unwrap()).unwrap();
    assert_eq!(
        expected.keys().collect::<Vec<_>>(),
        actual.keys().collect::<Vec<_>>(),
        "cases changed; rerun with RESHIKI_UPDATE_PIECE_GOLDENS=1 and review the diff"
    );
    for (key, want) in &expected {
        let got = &actual[key];
        assert_eq!(want["passes"], got["passes"], "{key}");
        assert_eq!(want["points"], got["points"], "{key}");
        assert_eq!(want["order"], got["order"], "{key}");
        let close = |a: &Json, b: &Json| {
            a.as_array()
                .unwrap()
                .iter()
                .zip(b.as_array().unwrap())
                .all(|(x, y)| (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() <= 0.05)
        };
        assert!(
            close(&want["sum"], &got["sum"])
                && close(&want["bounds"], &got["bounds"])
                && want["moment"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .zip(got["moment"].as_array().unwrap())
                    .all(|(x, y)| {
                        let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
                        (x - y).abs() <= 0.05 + 1e-9 * x.abs()
                    }),
            "{key}: {want} != {got}"
        );
    }
}

/// A flask with a condenser seated on it, a clamp on its neck and a clip on the joint.
fn reflux() -> Vec<PlanPath> {
    let (flask, condenser, clamp, clip) = (
        fixture("rbf-300-29"),
        fixture("liebig-29"),
        fixture("clamp-3finger"),
        fixture("keck-clip-29"),
    );
    let base = Affine::scale(SCALE, [0., 0.]);
    let mut f = Instance::new(&flask, base);
    f.values.insert("fill".into(), Value::Number(0.35));
    f.seated.insert("neck".into());
    let neck = flask.joints.iter().find(|j| j.id() == "neck").unwrap();
    let bottom = condenser
        .joints
        .iter()
        .find(|j| j.id() == "bottom")
        .unwrap();
    let exposure = taper("29/42").unwrap().exposure() * SCALE;
    let mut c = Instance::new(
        &condenser,
        mating_transform(
            base.apply(neck.at()),
            neck.dir(),
            exposure,
            bottom.at(),
            bottom.dir(),
            SCALE,
        ),
    );
    c.seated.insert("bottom".into());
    let mut k = Instance::new(&clamp, Affine::translate(base.apply([0., -62.])).then(base));
    k.values.insert("grip".into(), Value::Number(26.));
    let mouth = base.apply(neck.at());
    let g = Instance::new(&clip, Affine::translate(mouth).then(base));
    paint_plan(&[f, c, k, g], Settings::default()).unwrap()
}

#[test]
fn the_architecture_example_parses_exactly_as_printed() {
    let def = fixture("architecture-example");
    assert_eq!(def.id, "reshiki/jy-schlenk-flask-100-15");
    assert_eq!(def.format_version, 1);
    assert_eq!(
        (
            def.shapes.len(),
            def.joints.len(),
            def.holds.len(),
            def.params.len()
        ),
        (10, 2, 1, 3)
    );
    let open = evaluate(
        &def,
        &resolve(
            &def,
            &Values::from([("valve".into(), Value::State("open".into()))]),
        )
        .unwrap(),
    )
    .unwrap();
    let plug = open.shapes.iter().find(|d| d.shape.id == "plug").unwrap();
    // The PTFE plug lifts 5 mm when the valve opens.
    assert!(matches!(plug.path[0], Seg::Move([36., y]) if (y + 103.).abs() < 1e-9));
}

#[test]
fn serde_round_trip_and_hash_are_stable() {
    for (name, def) in fixtures() {
        let text = serde_json::to_string(&def).unwrap();
        let again = parse(&text).unwrap();
        assert_eq!(def, again, "{name}");
        assert_eq!(content_hash(&def), content_hash(&again), "{name}");
    }
    // Key order and whitespace do not change the identity; a value does.
    let def = fixture("architecture-example");
    let mut value = serde_json::to_value(&def).unwrap();
    let pretty = serde_json::to_string_pretty(&value).unwrap();
    assert_eq!(content_hash(&parse(&pretty).unwrap()), content_hash(&def));
    value["revision"] = json!(2);
    assert_ne!(
        content_hash(&serde_json::from_value(value).unwrap()),
        content_hash(&def)
    );
}

fn example() -> Json {
    serde_json::to_value(fixture("architecture-example")).unwrap()
}

fn rejects(change: impl FnOnce(&mut Json)) -> String {
    let mut value = example();
    change(&mut value);
    parse(&value.to_string()).expect_err("should be refused")
}

#[test]
fn each_validation_message_names_the_problem() {
    let cases: Vec<(String, &str)> = vec![
        (
            rejects(|v| v["joints"][0]["at"] = json!([3.2, -100])),
            "Joint \"top\" is 3.2 mm from the outline. Place it on the rim of the opening.",
        ),
        (
            rejects(|v| v["joints"][1]["dir"] = json!([-1, 0])),
            "Joint \"barb\" points into the glass. Flip its direction or click the other side of the opening.",
        ),
        (
            rejects(|v| v["joints"][1]["id"] = json!("top")),
            "Two joints are named \"top\". Rename one.",
        ),
        (
            rejects(|v| v["parts"][0]["param"] = json!("spin")),
            "Part \"plug\" names a missing parameter \"spin\".",
        ),
        (
            rejects(|v| v["liquids"][0]["region"] = json!("M 200 0 L 210 0 L 210 10 Z")),
            "The liquid region \"bulb\" is outside the glass.",
        ),
        (
            rejects(|v| v["params"][1]["default"] = json!(1.4)),
            "Liquid runs from 0 to 1; the default 1.4 is outside it.",
        ),
        (
            rejects(|v| {
                v["params"].as_array_mut().unwrap().push(json!({"id": "t", "names": {"en": "Tilt"}, "kind": "range", "min": 5, "max": 2, "default": 3, "unit": "deg"}))
            }),
            "Tilt runs from 5 to 2; the minimum must be below the maximum.",
        ),
        (
            rejects(|v| v["shapes"][0]["path"] = json!("L 0 0")),
            "Shape \"bulb\" has a path ReShiki cannot read: unexpected data at position 1.",
        ),
        (
            rejects(|v| {
                v["shapes"][0]["path"] = json!(format!("M 0 0 {}", "L 1 1 ".repeat(20_001)))
            }),
            "Shape \"bulb\" brings the piece to 20,002 path points; the limit is 20,000. Simplify it.",
        ),
        (
            rejects(|v| v["shapes"][1]["path"] = json!("M 2001 0 L 0 0")),
            "Shape \"arm\" has a coordinate beyond ±2000 mm.",
        ),
        (
            rejects(|v| v["id"] = json!("Tanaka/flask")),
            "The ID \"Tanaka/flask\" must be namespace/name, each 1–64 lowercase letters, digits or hyphens.",
        ),
        (
            rejects(|v| v["format"] = json!("theme")),
            "This is not a ReShiki piece file.",
        ),
        (
            rejects(|v| v["format_version"] = json!(2)),
            "This piece was made with a newer version of ReShiki (piece format 2). Update ReShiki to open it.",
        ),
        (
            rejects(|v| v["joints"][0]["size"] = json!("16/30")),
            "Joint \"top\" has size \"16/30\", which is not a standard taper size.",
        ),
        (
            rejects(|v| v["joints"][0]["draw"] = json!("none")),
            "Joint \"top\" is ground glass, so ReShiki must draw it: use \"draw\": \"standard\".",
        ),
        (
            rejects(|v| v["shapes"][6]["part"] = json!("lid")),
            "Shape \"plug\" names a missing part \"lid\".",
        ),
        (
            rejects(|v| v["shapes"][0]["when"] = json!({"valve": ["ajar"]})),
            "Shape \"bulb\" names a missing state \"ajar\" of \"valve\".",
        ),
        (
            rejects(|v| v["parts"][0]["motion"] = json!({"translate": [0, 1]})),
            "Part \"plug\" moves by a number, but \"valve\" is not a number.",
        ),
        (
            rejects(|v| v["parts"][0]["parent"] = json!("plug")),
            "Part \"plug\" is nested more than 4 deep, or its parents form a loop.",
        ),
        (
            rejects(|v| v["names"] = json!({"ja": "フラスコ"})),
            "The piece needs an English name in \"names\": {\"en\": …}.",
        ),
        (
            rejects(|v| v["keywords"] = json!(vec!["k"; 65])),
            "The piece has 65 keywords; the limit is 64.",
        ),
        (
            rejects(|v| v["holds"][0]["diameter_mm"] = json!([20, 10])),
            "Hold \"neck\" needs a positive diameter, or a range from the smaller to the larger.",
        ),
    ];
    let more: Vec<(String, &str)> = vec![
        (
            rejects(|v| v["holds"][0]["to"] = json!([0, -70])),
            "Hold \"neck\" needs two different ends.",
        ),
        (
            rejects(|v| v["holds"][0]["id"] = json!("top")),
            "Hold \"top\" has the ID of a joint.",
        ),
        (
            rejects(|v| v["holds"][0]["fit"] = json!("fill")),
            "Hold \"neck\" fits \"fill\": only a jaw or slider fits, and only a millimetre range parameter.",
        ),
        (
            rejects(|v| {
                v["holds"][0]["kind"] = json!("jaw");
                v["holds"][0]["fit"] = json!("valve");
            }),
            "Hold \"neck\" fits \"valve\": only a jaw or slider fits, and only a millimetre range parameter.",
        ),
        (
            rejects(|v| {
                v["joints"][0]["poses"] =
                    json!({"valve": {"open": {"at": [0, -100], "dir": [0, 0]}}})
            }),
            "Joint \"top\" needs a direction in every pose.",
        ),
        (
            rejects(|v| {
                v["joints"][0]["poses"] =
                    json!({"valve": {"open": {"at": [-1500, -100], "dir": [0, -1]}}})
            }),
            "Joint \"top\" is 1479.2 mm from the outline. Place it on the rim of the opening.",
        ),
        (
            rejects(|v| v["liquids"][0]["region"] = json!("M -10 0 L 10 0 L 10 10")),
            "The liquid region \"bulb\" must be closed: end each outline with Z.",
        ),
        (
            rejects(|v| v["liquids"][0]["region"] = json!("M -40 -5 L 40 -5 L 40 5 L -40 5 Z")),
            "The liquid region \"bulb\" is outside the glass.",
        ),
        (
            rejects(|v| {
                v["params"].as_array_mut().unwrap().push(json!({"id": "s", "names": {"en": "Size"}, "kind": "range", "min": 0, "max": 10, "default": 5, "unit": "mm"}));
                v["parts"].as_array_mut().unwrap().push(json!({"id": "grow", "param": "s", "motion": {"scale": {"pivot": [0, 0], "per_unit": 0.5}}}));
            }),
            "Part \"grow\" shrinks to nothing within the range of \"s\".",
        ),
        (
            rejects(|v| v["parts"][0]["motion"]["per_state"]["open"] = json!([0, 0, 0, 0, 0, 0])),
            "Part \"plug\" has a motion with a missing or infinite number.",
        ),
    ];
    let schema: Vec<(String, &str)> = vec![
        (
            rejects(|v| v["shapes"][0]["depth"] = json!(100)),
            "Shape \"bulb\" has depth 100; use −8 to 8.",
        ),
        (
            rejects(|v| v["params"][0]["names"] = json!({"ja": "バルブ"})),
            "Parameter \"valve\" needs an English name.",
        ),
        (
            rejects(|v| v["params"][0]["states"][1]["names"] = json!({"ja": "開"})),
            "State \"open\" of \"valve\" needs an English name.",
        ),
        (
            rejects(|v| {
                v["params"].as_array_mut().unwrap().push(json!({"id": "s", "names": {"en": "Slide"}, "kind": "range", "min": 0, "max": 10, "default": 0, "unit": "mm"}));
                v["parts"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id": "far", "param": "s", "motion": {"translate": [1e308, 0]}}));
            }),
            "Part \"far\" moves too far over the range of \"s\".",
        ),
        // Every corner lies in the glass, but the middle crosses the gap beside the neck.
        (
            rejects(|v| v["liquids"][0]["region"] = json!("M -20 -20 L 50 -20 L 50 10 L -20 10 Z")),
            "The liquid region \"bulb\" is outside the glass.",
        ),
        // The barb's joint would sit on glass drawn only while the valve is open.
        (
            rejects(|v| v["shapes"][4]["when"] = json!({"valve": ["open"]})),
            "Joint \"barb\" is 12.0 mm from the outline. Place it on the rim of the opening.",
        ),
    ];
    for (got, want) in cases.into_iter().chain(more).chain(schema) {
        assert_eq!(got, want);
    }
    // A repeated field is refused, not silently overwritten.
    let text = example()
        .to_string()
        .replacen("\"revision\":1", "\"revision\":0,\"revision\":1", 1);
    assert!(
        parse(&text)
            .unwrap_err()
            .contains("duplicate field `revision`")
    );
    // Unknown fields name themselves.
    let typo = rejects(|v| v["shapes"][0]["layr"] = json!("body"));
    assert!(
        typo.starts_with("The piece file is not valid: unknown field `layr`"),
        "{typo}"
    );
}

#[test]
fn limits_hold_at_the_boundary() {
    let mut value = example();
    value["keywords"] = json!(vec!["k"; 64]);
    value["names"]["en"] = json!("x".repeat(200));
    parse(&value.to_string()).unwrap();
    value["names"]["en"] = json!("x".repeat(201));
    assert!(
        parse(&value.to_string())
            .unwrap_err()
            .contains("at most 200 characters")
    );
}

#[test]
fn a_standard_socket_leaves_the_side_view_mouth_open() {
    let def = fixture("rbf-300-29");
    let mut instance = Instance::new(&def, Affine::IDENTITY);
    let free = paint_plan(std::slice::from_ref(&instance), Settings::default()).unwrap();
    instance.seated.insert("neck".into());
    let seated = paint_plan(&[instance], Settings::default()).unwrap();
    assert_eq!(
        free, seated,
        "the socket rim does not change when a cone seats"
    );

    let mouth = -99.528;
    let taper = taper("29/42").unwrap();
    let radius = taper.d / 2.;
    assert!(free.iter().any(|p| {
        p.pass == Pass::Outline
            && flatten(&p.path).iter().any(|(points, closed)| {
                !closed
                    && points.first().zip(points.last()).is_some_and(|(a, b)| {
                        (a[1] - mouth).abs() < 1e-6
                            && (b[1] - mouth).abs() < 1e-6
                            && (a[0] + b[0]).abs() < 1e-6
                            && (a[0].abs() - radius).abs() < 1e-6
                    })
            })
    }));
    assert!(
        free.iter()
            .filter(|p| p.material == Material::Glass)
            .all(|p| {
                p.color.is_none()
                    && flatten(&p.path)
                        .iter()
                        .flat_map(|(points, _)| points)
                        .all(|p| p[1] >= mouth - 1e-6)
            })
    );
    assert!(
        free.iter()
            .any(|p| { p.material == Material::Frosted && p.ink == Ink::Stroke(Weight::Hatch) })
    );
}

#[test]
fn a_standard_socket_opens_the_nearby_authored_rim_without_changing_its_fill() {
    let mut def = fixture("rbf-300-29");
    let mut glass = def.shapes[0].clone();
    glass.id = "source-rim".into();
    def.shapes.push(glass);
    let scale = Affine::scale(SCALE, [0., 0.]);
    let expected =
        super::geometry::parse_path("M16 -99.36L15 -80C5 -76 -5 -76 -15 -80L-16 -99.36").unwrap();
    for source in [
        "M-16 -99.36L16 -99.36L15 -80C5 -76 -5 -76 -15 -80Z",
        "M-16 -99.36L0 -99.36L16 -99.36L15 -80C5 -76 -5 -76 -15 -80Z",
        "M0 -99.36L16 -99.36L15 -80C5 -76 -5 -76 -15 -80L-16 -99.36Z",
    ] {
        def.shapes.last_mut().unwrap().path = source.into();
        let path = super::geometry::parse_path(source).unwrap();
        let painted = plan(&def, Values::new());
        assert!(
            painted
                .iter()
                .any(|p| { p.pass == Pass::Glass && p.path == scale.map(&path) })
        );
        assert!(
            painted
                .iter()
                .any(|p| { p.pass == Pass::Outline && p.path == scale.map(&expected) })
        );
        assert!(
            !painted
                .iter()
                .any(|p| { p.pass == Pass::Outline && p.path == scale.map(&path) })
        );
    }
}

#[test]
fn a_seated_cone_is_cut_at_the_socket_mouth() {
    let def = fixture("liebig-29");
    let mut free = Instance::new(&def, Affine::IDENTITY);
    let free_plan = paint_plan(std::slice::from_ref(&free), Settings::default()).unwrap();
    free.seated.insert("bottom".into());
    let seated = paint_plan(&[free], Settings::default()).unwrap();
    let lowest = |plan: &[PlanPath]| {
        plan.iter()
            .filter(|p| p.pass == Pass::Glass)
            .flat_map(|p| flatten(&p.path))
            .flat_map(|(pts, _)| pts)
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let t = taper("29/42").unwrap();
    // Free: the cone reaches its tip, L + 1.4 below the shoulder at y = 0.
    assert!((lowest(&free_plan) - (t.l + 1.4)).abs() < 1e-6);
    // Seated: only the 0.06 L band outside the mouth is drawn.
    assert!((lowest(&seated) - t.exposure()).abs() < 1e-6);
    // No stroke runs along the cut, 0.06 L below the shoulder.
    let e = t.exposure();
    for p in seated
        .iter()
        .filter(|p| matches!(p.ink, Ink::Stroke(_) | Ink::FillStroke(_)))
    {
        for (pts, closed) in flatten(&p.path) {
            for (a, b) in super::geometry::edges(&pts, closed) {
                let along_cut =
                    (a[1] - e).abs() < 1e-6 && (b[1] - e).abs() < 1e-6 && (a[0] - b[0]).abs() > 1.;
                assert!(!along_cut, "{:?} strokes the cut", p.pass);
            }
        }
    }
    // The cut edge is filled but has no outline: the outline stays open.
    let outline = seated.iter().filter(|p| p.pass == Pass::Outline).find(|p| {
        flatten(&p.path)
            .iter()
            .any(|(pts, closed)| !closed && pts.len() > 2)
    });
    assert!(outline.is_some());
}

#[test]
fn liquid_level_leaves_the_asked_share_of_the_area_and_stays_level_when_turned() {
    let def = fixture("rbf-300-29");
    for degrees in [0., 35.] {
        let mut instance = Instance::new(&def, Affine::rotate(degrees, [0., 0.]));
        instance.values.insert("fill".into(), Value::Number(0.3));
        let plan = paint_plan(&[instance], Settings::default()).unwrap();
        let pools: Vec<_> = plan
            .iter()
            .filter(|p| p.pass == Pass::Liquid && p.ink == Ink::Fill)
            .collect();
        assert_eq!(pools.len(), 1);
        let pool: f64 = flatten(&pools[0].path)
            .iter()
            .map(|(p, _)| area(p).abs())
            .sum();
        let region: f64 = flatten(&super::geometry::parse_path(&def.liquids[0].region).unwrap())
            .iter()
            .map(|(p, _)| area(p).abs())
            .sum();
        assert!(
            (pool / region - 0.3).abs() < 0.001,
            "{degrees}°: {}",
            pool / region
        );
        // The surface is horizontal on the page.
        let top = flatten(&pools[0].path)[0]
            .0
            .iter()
            .map(|p| p[1])
            .fold(f64::INFINITY, f64::min);
        let on_top = flatten(&pools[0].path)[0]
            .0
            .iter()
            .filter(|p| (p[1] - top).abs() < 1e-6)
            .count();
        assert!(on_top >= 2, "{degrees}°");
    }
}

#[test]
fn seating_turns_and_places_the_moving_piece() {
    let flask = fixture("rbf-300-29");
    let condenser = fixture("liebig-29");
    let base = Affine::translate([40., 10.])
        .then(Affine::rotate(30., [0., 0.]))
        .then(Affine::scale(SCALE, [0., 0.]));
    let f = Instance::new(&flask, base);
    let e = evaluate(&flask, &resolve(&flask, &Values::new()).unwrap()).unwrap();
    let neck = e.joints.iter().find(|j| j.joint.id() == "neck").unwrap();
    let (at, dir) = (f.transform.apply(neck.at), f.transform.apply_dir(neck.dir));
    let bottom = condenser
        .joints
        .iter()
        .find(|j| j.id() == "bottom")
        .unwrap();
    let t = mating_transform(
        at,
        dir,
        taper("29/42").unwrap().exposure() * SCALE,
        bottom.at(),
        bottom.dir(),
        SCALE,
    );
    let shoulder = t.apply(bottom.at());
    let pointing = t.apply_dir(bottom.dir());
    assert!(
        len(sub(
            shoulder,
            [at[0] + dir[0] * 2.52 * SCALE, at[1] + dir[1] * 2.52 * SCALE]
        )) < 1e-9
    );
    assert!((pointing[0] + dir[0]).abs() < 1e-9 && (pointing[1] + dir[1]).abs() < 1e-9);
    assert!((t.scale_factor() - SCALE).abs() < 1e-12);
}

#[test]
fn parts_move_shapes_joints_and_handles() {
    let thermometer = fixture("thermometer");
    let at = |depth: f64| {
        let v = resolve(
            &thermometer,
            &Values::from([("depth".into(), Value::Number(depth))]),
        )
        .unwrap();
        evaluate(&thermometer, &v).unwrap().joints[0].at
    };
    assert_eq!(at(70.), [0., 70.]);
    assert_eq!(at(120.), [0., 120.]);
    // Out of range values are clamped to the parameter's range.
    assert_eq!(at(1000.), [0., 230.]);
    let clamp = fixture("clamp-3finger");
    let v = resolve(&clamp, &Values::from([("tilt".into(), Value::Number(90.))])).unwrap();
    let e = evaluate(&clamp, &v).unwrap();
    let jaw = e.holds.iter().find(|h| h.hold.id == "jaw").unwrap();
    // The head turns about the boss: the jaw swings from (0, ±12) to below the boss.
    assert!(len(sub(jaw.from, [-108., 120.])) < 1e-9 && len(sub(jaw.to, [-132., 120.])) < 1e-9);
    let nan = resolve(
        &thermometer,
        &Values::from([("depth".into(), Value::Number(f64::NAN))]),
    )
    .unwrap_err();
    assert_eq!(nan, "\"depth\" needs a finite number.");
    let unknown = resolve(&clamp, &Values::from([("spin".into(), Value::Number(1.))])).unwrap_err();
    assert_eq!(unknown, "The piece has no parameter \"spin\".");
}

#[test]
fn shapes_drawn_per_state_follow_the_state() {
    let def = fixture("schlenk-tube-15");
    let count = |state: &str| {
        let v = resolve(
            &def,
            &Values::from([("stopcock".into(), Value::State(state.into()))]),
        )
        .unwrap();
        evaluate(&def, &v)
            .unwrap()
            .shapes
            .iter()
            .filter(|d| !d.shape.when.is_empty())
            .count()
    };
    assert!(count("closed") > 0 && count("all") > count("closed"));
    let bad = resolve(
        &def,
        &Values::from([("stopcock".into(), Value::State("ajar".into()))]),
    )
    .unwrap_err();
    assert_eq!(bad, "\"stopcock\" has no state \"ajar\".");
}

#[test]
fn signed_zero_does_not_change_the_identity() {
    let def = fixture("architecture-example");
    let mut value = serde_json::to_value(&def).unwrap();
    value["labels"][0]["at"] = json!([-0.0, 8.0]);
    let negative: Definition = serde_json::from_value(value).unwrap();
    value = serde_json::to_value(&def).unwrap();
    value["labels"][0]["at"] = json!([0.0, 8.0]);
    let positive: Definition = serde_json::from_value(value).unwrap();
    assert_eq!(content_hash(&negative), content_hash(&positive));
}

#[test]
fn a_per_state_motion_is_measured_from_the_default_state() {
    let mut value = serde_json::to_value(fixture("architecture-example")).unwrap();
    // The default state moves the plug 10 mm; the drawn shapes are that pose.
    value["parts"][0]["motion"]["per_state"]["closed"] = json!([1, 0, 0, 1, 0, 10]);
    let def: Definition = serde_json::from_value(value).unwrap();
    let plug = |state: &str| {
        let v = resolve(
            &def,
            &Values::from([("valve".into(), Value::State(state.into()))]),
        )
        .unwrap();
        evaluate(&def, &v)
            .unwrap()
            .shapes
            .iter()
            .find(|d| d.shape.id == "plug")
            .unwrap()
            .path[0]
    };
    assert!(matches!(plug("closed"), Seg::Move([36., y]) if (y + 98.).abs() < 1e-9));
    assert!(matches!(plug("open"), Seg::Move([36., y]) if (y + 113.).abs() < 1e-9));
}

#[test]
fn tube_ends_are_two_different_plugs() {
    let mut value: Json = serde_json::from_str(
        &std::fs::read_to_string(format!("{FIXTURES}/hose-7.reshiki-piece")).unwrap(),
    )
    .unwrap();
    value["tube"]["ends"] = json!(["a", "a"]);
    assert_eq!(
        parse(&value.to_string()).unwrap_err(),
        "The tube's two ends must be different joints."
    );
    value["tube"]["ends"] = json!(["a", "b"]);
    value["joints"][1]["size_mm"] = json!(-7);
    assert_eq!(
        parse(&value.to_string()).unwrap_err(),
        "Joint \"b\" needs a positive size."
    );
}
