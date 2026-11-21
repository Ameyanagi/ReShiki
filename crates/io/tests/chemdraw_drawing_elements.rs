//! Drawings saved by ChemDraw itself import whole: a style ReShiki cannot draw
//! yet takes the nearest one it can, and the import says so once.
use base64::{Engine, engine::general_purpose::STANDARD};
use reshiki_io::{engine::LocalEngine, exchange, formats};
use reshiki_model::{
    arrows::{Body, Head, NoGo},
    document::{Document, Point},
    graphics::{Graphic, GraphicKind, LinePattern, PathCommand},
    palette::Color,
    purpose::Purpose,
    reactions::{self, Role},
    scientific::{OrbitalKind, Phase, SymbolKind},
};
use std::path::{Path, PathBuf};

const NEAREST: &str = "Nearest style used: ";
const LEFT_OUT: &str = "Left out: ";
const KEEP: &str =
    " ReShiki cannot draw these yet. Keep the original ChemDraw file if you need them.";

fn format(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()? {
        "cdx" => Some("cdx"),
        "cdxml" => Some("cdxml"),
        _ => None,
    }
}

async fn open(data: Vec<u8>, format: &str) -> Result<(Document, Vec<String>), String> {
    formats::read_with_warnings(data, format, &LocalEngine::in_process()).await
}

async fn fixture(name: &str) -> (Document, Vec<String>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/chemdraw-drawing-elements")
        .join(name);
    open(std::fs::read(&path).unwrap(), format(&path).unwrap())
        .await
        .unwrap_or_else(|error| panic!("{name}: {error}"))
}

/// Where a path command ends.
fn anchor(command: Option<&PathCommand>) -> Point {
    match command {
        Some(PathCommand::Move(p) | PathCommand::Line(p) | PathCommand::Cubic(_, _, p)) => *p,
        _ => panic!("{command:?}"),
    }
}

/// One page of objects with black, white, red and blue after the two
/// built-in colours, read as CDXML and again as CDX.
async fn sheet(objects: &str) -> Vec<(Document, Vec<String>)> {
    let xml = format!(
        "<CDXML LineWidth='1' BoldWidth='4'><colortable><color r='1' g='1' b='1'/>\
         <color r='0' g='0' b='0'/><color r='1' g='0' b='0'/><color r='0' g='0' b='1'/>\
         </colortable><page id='1'>{objects}</page></CDXML>"
    );
    let mut read = Vec::new();
    for (format, data) in [
        ("cdxml", xml.clone().into_bytes()),
        ("cdx", exchange::to_cdx(&xml).unwrap()),
    ] {
        read.push(
            open(data, format)
                .await
                .unwrap_or_else(|error| panic!("{format}: {error}\n{objects}")),
        );
    }
    read
}

/// The box around a path's anchors.
fn extent(graphic: &Graphic) -> (Point, Point) {
    let anchors: Vec<_> = graphic
        .path
        .iter()
        .filter_map(|command| match command {
            PathCommand::Move(p) | PathCommand::Line(p) | PathCommand::Cubic(_, _, p) => Some(*p),
            PathCommand::Close => None,
        })
        .collect();
    let fold = |pick: fn(f32, f32) -> f32, start: f32| {
        anchors.iter().fold(Point::new(start, start), |found, p| {
            Point::new(pick(found.x, p.x), pick(found.y, p.y))
        })
    };
    (fold(f32::min, f32::MAX), fold(f32::max, f32::MIN))
}

/// The one notice for `list`, or none when nothing was approximated.
fn nearest(list: &str) -> Vec<String> {
    if list.is_empty() {
        return Vec::new();
    }
    vec![format!("{NEAREST}{list}.{KEEP}")]
}
fn left_out(list: &str) -> Vec<String> {
    vec![format!("{LEFT_OUT}{list}.{KEEP}")]
}

#[tokio::test]
async fn the_owners_drawing_opens_whole_without_a_notice() {
    for name in ["drawing.cdxml", "drawing.cdx"] {
        let (document, notices) = fixture(name).await;
        assert_eq!(notices, Vec::<String>::new(), "{name}");
        // 10 ovals, 10 rectangles and 5 lines, the lines graphic lines as
        // a line drawn in ReShiki is.
        assert_eq!(document.graphics.len(), 25, "{name}");
        assert_eq!(
            document
                .graphics
                .iter()
                .filter(|g| g.kind == GraphicKind::Line)
                .count(),
            5,
            "{name}"
        );
    }
}

#[tokio::test]
async fn every_probe_sheet_opens_and_names_what_it_approximated() {
    let curves = "2 doubled curves as pairs of grouped curves";
    let ratio =
        "1 unbalanced equilibrium arrow with its shorter arrow lengthened to a fifth of the other";
    for (sheet, saved, source) in [
        ("p1-shapes-enum", String::new(), String::new()),
        // Shaded orbitals are read as they are.
        ("p2-orbitals-enum", String::new(), String::new()),
        (
            "p3-symbols-enum",
            "3 stereo flags as text in a rounded box".into(),
            "3 stereo flags as text in a rounded box".into(),
        ),
        (
            "p4-brackets-enum",
            "3 single brackets as curves".into(),
            "3 single brackets as curves".into(),
        ),
        // Block and arc arrows are read as they are.
        ("p5-arrows-enum", ratio.to_owned(), ratio.to_owned()),
        ("p6-curves-enum", curves.to_owned(), curves.to_owned()),
    ] {
        for (name, expected) in [
            (format!("{sheet}.chemdraw.cdxml"), &saved),
            (format!("{sheet}.chemdraw.cdx"), &saved),
            (format!("{sheet}.source.cdxml"), &source),
        ] {
            let (document, notices) = fixture(&name).await;
            assert_eq!(notices, nearest(expected), "{name}");
            assert!(
                document.graphics.len() + document.arrows.len() > 5,
                "{name}"
            );
        }
    }
}

#[tokio::test]
async fn the_binary_file_reads_what_the_text_file_reads() {
    for sheet in [
        "p1-shapes-enum",
        "p2-orbitals-enum",
        "p3-symbols-enum",
        "p4-brackets-enum",
        "p5-arrows-enum",
        "p6-curves-enum",
    ] {
        let (text, _) = fixture(&format!("{sheet}.chemdraw.cdxml")).await;
        let (binary, _) = fixture(&format!("{sheet}.chemdraw.cdx")).await;
        let kinds = |document: &Document| {
            let mut kinds: Vec<_> = document
                .graphics
                .iter()
                .map(|graphic| format!("{:?} {:?}", graphic.kind, graphic.phase))
                .collect();
            kinds.sort();
            kinds
        };
        assert_eq!(kinds(&binary), kinds(&text), "{sheet}");
        let marks = |document: &Document| {
            let mut marks: Vec<_> = document
                .arrows
                .iter()
                .map(|arrow| {
                    let style = arrow.style.as_ref().unwrap();
                    format!("{:?} {:?} {:?}", arrow.kind, style.no_go, style.pattern)
                })
                .collect();
            marks.sort();
            marks
        };
        assert_eq!(marks(&binary), marks(&text), "{sheet}");
        // Every arrow and shape stands where the text file has it: an arc
        // that turned the other way or a moved head would not.
        let places = |document: &Document| {
            let mut places: Vec<_> = document
                .arrows
                .iter()
                .map(|arrow| {
                    // The control tells which way a curved arrow bows; an
                    // arc's middle, however many segments draw it.
                    let bow = match (arrow.arc, arrow.arc_circle()) {
                        (Some(sweep), Some((center, radius, first))) => {
                            let middle = (first + sweep * 0.5).to_radians();
                            center.offset(middle.cos() * radius, middle.sin() * radius)
                        }
                        _ => arrow
                            .control
                            .or(arrow.cubic.map(|[first, _]| first))
                            .unwrap_or(arrow.start),
                    };
                    [arrow.start, arrow.end, bow]
                })
                .chain(document.graphics.iter().map(|graphic| {
                    let (low, high) = graphic.shape_bounds();
                    [low, high, low]
                }))
                .map(|[a, b, c]| [a.x, a.y, b.x, b.y, c.x, c.y])
                .collect();
            places.sort_by(|a, b| {
                let key = |place: &[f32; 6]| place.map(|value| (value / 4.).round() as i32);
                key(a).cmp(&key(b))
            });
            places
        };
        let (binary_places, text_places) = (places(&binary), places(&text));
        assert_eq!(binary_places.len(), text_places.len(), "{sheet}");
        for (binary, text) in binary_places.iter().zip(&text_places) {
            assert!(
                binary.iter().zip(text).all(|(a, b)| (a - b).abs() < 0.5),
                "{sheet}: {binary:?} != {text:?}"
            );
        }
        assert_eq!(binary.annotations.len(), text.annotations.len(), "{sheet}");
    }
    // ChemDraw's binary values for the no-go marks and the lone pair bar.
    let (arrows, _) = fixture("p5-arrows-enum.chemdraw.cdx").await;
    let no_go = |mark| {
        arrows
            .arrows
            .iter()
            .filter(|arrow| arrow.style.as_ref().unwrap().no_go == mark)
            .count()
    };
    assert_eq!((no_go(NoGo::Cross), no_go(NoGo::Hash)), (1, 1));
    let (symbols, _) = fixture("p3-symbols-enum.chemdraw.cdx").await;
    for kind in [
        SymbolKind::LonePairBar,
        SymbolKind::Dagger,
        SymbolKind::DoubleDagger,
    ] {
        assert!(
            symbols
                .graphics
                .iter()
                .any(|graphic| graphic.kind == GraphicKind::Symbol(kind)),
            "{kind:?}"
        );
    }
}

#[tokio::test]
async fn a_curve_keeps_dashed_and_bold_together_and_a_shadow_takes_the_nearest_size() {
    let points = "20 20 20 20 30 0 50 40 60 20 60 20";
    for (document, notices) in sheet(&format!(
        "<curve id='2' CurvePoints='{points}' LineType='Dashed Bold'/>\
         <curve id='3' CurvePoints='{points}' LineType='Dashed Wavy'/>\
         <graphic id='4' GraphicType='Rectangle' BoundingBox='10 50 60 80' \
         RectangleType='Shadow' ShadowSize='9000'/>",
    ))
    .await
    {
        assert_eq!(
            notices,
            nearest("1 line, arrowhead or shadow size at the nearest size ReShiki draws")
        );
        let [both, dashed, shadowed] = document.graphics.as_slice() else {
            panic!("{:?}", document.graphics);
        };
        assert_eq!(
            (both.style.pattern, both.style.width_pt),
            (LinePattern::Dashed, 4.)
        );
        assert_eq!(
            (dashed.style.pattern, dashed.style.width_pt),
            (LinePattern::Dashed, 1.)
        );
        assert_eq!(shadowed.style.effects.shadow_pt, Some(36.));
    }
}

#[tokio::test]
async fn faded_graphics_import_opaque_and_block_arrows_keep_their_body() {
    // ReShiki's own binary writer has no fade, so this one is read as text.
    let faded = "<CDXML><page id='1'>\
         <graphic id='2' GraphicType='Rectangle' BoundingBox='10 10 60 40' FadePercent='50'/>\
         <graphic id='3' GraphicType='Rectangle' BoundingBox='10 50 60 80' FadePercent='300'/>\
         <graphic id='4' GraphicType='Rectangle' BoundingBox='10 90 60 120' alpha='0.5'/>\
         </page></CDXML>";
    let (document, notices) = open(faded.as_bytes().to_vec(), "cdxml").await.unwrap();
    // FadePercent is read as a see-through fill; alpha is drawn opaque.
    assert_eq!(notices, nearest("1 see-through object opaque"));
    assert_eq!(document.graphics.len(), 3);
    assert!(document.graphics[1].style.effects.opacity.is_some());
    assert!(document.graphics[2].style.effects.opacity.is_none());
    for (document, notices) in sheet(
        "<arrow id='4' Tail3D='10 100 0' Head3D='70 100 0' ArrowheadHead='Full' \
         ArrowheadType='Hollow' FillType='Solid' HeadSize='2000' ArrowheadCenterSize='1500' \
         ArrowheadWidth='600' ArrowShaftSpacing='800'/>\
         <arrow id='5' Tail3D='10 120 0' Head3D='70 120 0' ArrowheadHead='Full' \
         ArrowheadType='Solid' HeadSize='9000'/>",
    )
    .await
    {
        // The block arrow is read as one; only the head out of range changes.
        assert_eq!(
            notices,
            nearest("1 line, arrowhead or shadow size at the nearest size ReShiki draws")
        );
        assert_eq!(document.arrows[0].appearance().body, Body::Block);
        assert_eq!(
            document.arrows[1].style.clone().unwrap().head_length_pt,
            24.
        );
    }
}

#[tokio::test]
async fn arc_arrows_are_arcs_and_older_arcs_are_curves() {
    // ChemDraw's 90° arc arrow, its 270° arc arrow, its headless Arc tool,
    // an arc arrow without its axes, and two arcs from an older file.
    for (document, notices) in sheet(
        "<arrow id='2' ArrowheadHead='Full' ArrowheadType='Solid' AngularSize='90' \
         Head3D='375.61 301.39 0' Tail3D='375.61 322.61 0' Center3D='365 312 0' \
         MajorAxisEnd3D='380 312 0' MinorAxisEnd3D='365 327 0'/>\
         <arrow id='3' ArrowheadHead='Full' ArrowheadType='Solid' AngularSize='270' \
         Head3D='75.61 364.61 0' Tail3D='75.61 343.39 0' Center3D='65 354 0' \
         MajorAxisEnd3D='80 354 0' MinorAxisEnd3D='65 369 0'/>\
         <arrow id='4' AngularSize='-180' Head3D='83 340 0' Tail3D='47 340 0' \
         Center3D='65 340 0' MajorAxisEnd3D='83 340 0' MinorAxisEnd3D='65 358 0'/>\
         <arrow id='7' ArrowheadHead='Full' AngularSize='90' Head3D='10 400 0' \
         Tail3D='60 400 0'/>\
         <graphic id='5' GraphicType='Arc' BoundingBox='183 285 165 285' AngularSize='90'/>\
         <graphic id='6' GraphicType='Arc' BoundingBox='475.61 301.39 465 312' AngularSize='90' \
         ArrowType='FullHead'/>",
    )
    .await
    {
        assert_eq!(notices, nearest("1 arc arrow straight"));
        let [quarter, long, straight, legacy] = document.arrows.as_slice() else {
            panic!("{:?}", document.arrows);
        };
        // The arrow reader draws ChemDraw's arcs as arcs.
        assert!([quarter, long].iter().all(|arrow| arrow.arc.is_some()));
        assert!(straight.arc.is_none() && straight.cubic.is_none());
        // The arc arrow of an older file ends where ChemDraw's own arrow for
        // it does: both run up the right of their centres.
        assert!((legacy.start.x - legacy.end.x).abs() < 0.01 && legacy.start.y > legacy.end.y);
        assert!((legacy.start.y - quarter.start.y).abs() < 0.01);
        assert!((legacy.end.y - quarter.end.y).abs() < 0.01);
        // The Arc tool's half turn is a path from its tail to its head, and
        // an older headless arc a path of one quarter turn.
        let [headless, arc] = document.graphics.as_slice() else {
            panic!("{:?}", document.graphics);
        };
        assert_eq!((headless.kind, headless.path.len()), (GraphicKind::Path, 3));
        let (start, end) = (anchor(headless.path.first()), anchor(headless.path.last()));
        assert!((start.y - end.y).abs() < 0.01 && start.x < end.x);
        // Half a turn: as high as its radius.
        let (low, high) = extent(headless);
        assert!(((high.y - low.y) - (end.x - start.x) / 2.).abs() < 0.1);
        assert_eq!((arc.kind, arc.path.len()), (GraphicKind::Path, 2));
    }
}

#[tokio::test]
async fn orbitals_take_chemdraws_names_and_shaded_phases() {
    for (document, notices) in sheet(
        "<graphic id='2' GraphicType='Orbital' BoundingBox='20 40 20 10' OrbitalType='hybridPlus'/>\
         <graphic id='3' GraphicType='Orbital' BoundingBox='50 40 50 10' OrbitalType='hybridMinus'/>\
         <graphic id='4' GraphicType='Orbital' BoundingBox='80 40 80 10' OrbitalType='sFilled'/>\
         <graphic id='5' GraphicType='Orbital' BoundingBox='110 40 110 10' OrbitalType='lobeShaded'/>\
         <graphic id='6' GraphicType='Orbital' BoundingBox='140 40 140 10' OrbitalType='pFilled'/>\
         <graphic id='7' GraphicType='Orbital' BoundingBox='170 40 170 10' OrbitalType='dz2MinusFilled'/>\
         <graphic id='8' GraphicType='Orbital' BoundingBox='200 40 200 10' OrbitalType='p'/>\
         <graphic id='9' GraphicType='Orbital' BoundingBox='230 40 230 10' OrbitalType='dz2Plus'/>\
         <graphic id='10' GraphicType='Orbital' BoundingBox='260 40 260 10' OrbitalType='s'/>\
         <graphic id='11' GraphicType='Orbital' BoundingBox='290 40 290 10' OrbitalType='sShaded'/>",
    )
    .await
    {
        assert_eq!(notices, nearest(""));
        let read: Vec<_> = document
            .graphics
            .iter()
            .map(|graphic| (graphic.kind, graphic.phase, graphic.phase_flipped))
            .collect();
        use GraphicKind::Orbital;
        assert_eq!(
            read,
            [
                (Orbital(OrbitalKind::Hybrid), Phase::Gradient, true),
                (Orbital(OrbitalKind::Hybrid), Phase::Gradient, false),
                (Orbital(OrbitalKind::S), Phase::Solid, false),
                (Orbital(OrbitalKind::Lobe), Phase::Gradient, false),
                (Orbital(OrbitalKind::P), Phase::Solid, false),
                (Orbital(OrbitalKind::Dz2), Phase::Solid, false),
                (Orbital(OrbitalKind::P), Phase::Gradient, false),
                (Orbital(OrbitalKind::Dz2), Phase::Gradient, true),
                (Orbital(OrbitalKind::S), Phase::Open, false),
                (Orbital(OrbitalKind::S), Phase::Gradient, false),
            ]
        );
        // ChemDraw's p shades its lower lobe, toward the first point.
        let p = &document.graphics[6];
        let shaded: Vec<_> = p.parts().into_iter().filter(|part| part.filled).collect();
        let [lobe] = shaded.as_slice() else {
            panic!("{shaded:?}");
        };
        assert!(lobe.style.effects.shaded);
        let (lo, hi) = extent(&Graphic {
            path: lobe.commands.clone(),
            ..p.clone()
        });
        assert!(lo.y >= p.origin.y - 0.001 && hi.y > p.origin.y, "{lo:?} {hi:?}");
    }
}

#[tokio::test]
async fn shaded_orbitals_and_upright_signs_keep_their_look_through_reshikis_cdxml() {
    let engine = LocalEngine::in_process();
    for sheet in ["p2-orbitals-enum", "p3-symbols-enum"] {
        let (document, _) = fixture(&format!("{sheet}.chemdraw.cdxml")).await;
        let shaded = |document: &Document| {
            document
                .graphics
                .iter()
                .filter(|g| g.style.effects.shaded || g.phase == Phase::Gradient)
                .count()
        };
        for target in ["cdxml", "cdx"] {
            let bytes = formats::write_with_engine(document.clone(), target, &engine)
                .await
                .unwrap();
            let (restored, _) = open(bytes, target).await.unwrap();
            // Written as curves: each shaded lobe a ChemDraw shaded curve.
            let lobes: usize = document
                .graphics
                .iter()
                .map(|g| {
                    g.parts()
                        .iter()
                        .filter(|part| part.style.effects.shaded)
                        .count()
                })
                .sum();
            assert_eq!(shaded(&restored), lobes, "{sheet} {target}");
            assert_eq!(lobes > 0, sheet == "p2-orbitals-enum");
            if sheet == "p3-symbols-enum" {
                // Upright arms: the stems of the plus, circled plus, radical
                // cation and both daggers, and the bars of the four signs,
                // two ions and three dagger crossbars.
                let (mut stems, mut bars) = (0, 0);
                for graphic in &restored.graphics {
                    let (lo, hi) = extent(graphic);
                    let (w, h) = (hi.x - lo.x, hi.y - lo.y);
                    stems += usize::from(w < 0.001 && h > 0.001);
                    bars += usize::from(h < 0.001 && w > 0.001);
                }
                assert_eq!((stems, bars), (5, 9), "{target}");
            }
        }
    }
}

#[tokio::test]
async fn a_dxy_orbital_has_its_lobes_along_and_across_its_axis() {
    for (document, notices) in sheet(
        "<graphic id='2' GraphicType='Orbital' BoundingBox='20 40 20 10' OrbitalType='dxyFilled'/>\
         <graphic id='3' GraphicType='Orbital' BoundingBox='60 40 60 10' OrbitalType='dxy'/>",
    )
    .await
    {
        assert_eq!(notices, nearest(""));
        for (dxy, phase) in document
            .graphics
            .iter()
            .zip([Phase::Solid, Phase::Gradient])
        {
            assert_eq!(
                (dxy.kind, dxy.phase),
                (GraphicKind::Orbital(OrbitalKind::Dxy), phase)
            );
            // As ChemDraw draws it: the filled lobes above and below the
            // middle, the open ones left and right of it.
            let middle = |part: &reshiki_model::scientific::Part| {
                let (lo, hi) = extent(&Graphic {
                    path: part.commands.clone(),
                    ..dxy.clone()
                });
                Point::new(
                    (lo.x + hi.x) / 2. - dxy.origin.x,
                    (lo.y + hi.y) / 2. - dxy.origin.y,
                )
            };
            for part in dxy.parts() {
                let at = middle(&part);
                let (along, across) = if part.filled {
                    (at.y.abs(), at.x.abs())
                } else {
                    (at.x.abs(), at.y.abs())
                };
                assert!(across < 0.01 * along, "{at:?} {}", part.filled);
            }
        }
    }
}

/// Where a part's anchors sit on average, from the symbol's middle.
fn part_middle(graphic: &Graphic, part: &reshiki_model::scientific::Part) -> Point {
    let (lo, hi) = extent(&Graphic {
        path: part.commands.clone(),
        ..graphic.clone()
    });
    Point::new(
        (lo.x + hi.x) / 2. - graphic.origin.x,
        (lo.y + hi.y) / 2. - graphic.origin.y,
    )
}

#[tokio::test]
async fn signs_stand_upright_and_dots_sit_where_chemdraw_draws_them() {
    // As in ChemDraw's symbol sheet: each second point 6 pt up and left.
    for (document, notices) in sheet(
        "<graphic id='2' GraphicType='Symbol' BoundingBox='55 55 49 49' SymbolType='Plus'/>\
         <graphic id='3' GraphicType='Symbol' BoundingBox='85 55 79 49' SymbolType='Minus'/>\
         <graphic id='4' GraphicType='Symbol' BoundingBox='115 55 109 49' SymbolType='CirclePlus'/>\
         <graphic id='5' GraphicType='Symbol' BoundingBox='145 55 139 49' SymbolType='CircleMinus'/>\
         <graphic id='6' GraphicType='Symbol' BoundingBox='175 55 169 49' SymbolType='Dagger'/>\
         <graphic id='7' GraphicType='Symbol' BoundingBox='205 55 199 49' SymbolType='DoubleDagger'/>\
         <graphic id='8' GraphicType='Symbol' BoundingBox='235 55 229 49' SymbolType='Electron'/>\
         <graphic id='9' GraphicType='Symbol' BoundingBox='265 55 259 49' SymbolType='LonePair'/>\
         <graphic id='10' GraphicType='Symbol' BoundingBox='295 55 289 49' SymbolType='RadicalCation'/>\
         <graphic id='11' GraphicType='Symbol' BoundingBox='325 55 319 49' SymbolType='RadicalAnion'/>\
         <graphic id='12' GraphicType='Symbol' BoundingBox='355 55 349 49' SymbolType='LonePairBar'/>\
         <graphic id='13' GraphicType='Symbol' BoundingBox='385 40 391 46' SymbolType='RadicalCation'/>\
         <graphic id='14' GraphicType='Symbol' BoundingBox='415 40 415 46' SymbolType='RadicalAnion'/>",
    )
    .await
    {
        assert_eq!(notices, nearest(""));
        let g = &document.graphics;
        // Points 30 pt apart in the file, in the drawing's units.
        let pt = (g[1].origin.x - g[0].origin.x) / 30.;
        let size = 6. * std::f32::consts::SQRT_2 * pt;
        let near = |a: f32, b: f32| (a - b).abs() < 0.01 * size;
        // Signs and daggers: upright on the first point, as large as the
        // distance between the points.
        for (i, sign) in g[..6].iter().enumerate() {
            assert!(near(sign.origin.x, g[0].origin.x + 30. * pt * i as f32));
            assert!(near(sign.origin.y, g[0].origin.y));
            assert!(near(sign.axis_x.x, size) && near(sign.axis_x.y, 0.), "{i}");
            assert!(near(sign.axis_y.x, 0.) && near(sign.axis_y.y, size), "{i}");
        }
        // A lone electron is a small dot on its point.
        let [dot] = g[6].parts().try_into().unwrap();
        let (lo, hi) = extent(&Graphic {
            path: dot.commands.clone(),
            ..g[6].clone()
        });
        assert!(near(g[6].origin.y, g[0].origin.y));
        assert!((hi.x - lo.x) < 0.25 * size, "{lo:?} {hi:?}");
        // A lone pair has a dot on each point.
        let [first, second] = g[7].parts().try_into().unwrap();
        let (first, second) = (part_middle(&g[7], &first), part_middle(&g[7], &second));
        let (dx, dy) = (second.x - first.x, second.y - first.y);
        assert!(near(dx.abs(), 6. * pt) && near(dy.abs(), 6. * pt), "{dx} {dy}");
        assert!(near(g[7].origin.x, g[0].origin.x + 207. * pt));
        // A radical ion: the dot on the first point and the upright sign on
        // the second, whichever way the second lies.
        let at = |x: f32, y: f32, ion: &Graphic| {
            Point::new(
                g[0].origin.x + (x - 55.) * pt - ion.origin.x,
                g[0].origin.y + (y - 55.) * pt - ion.origin.y,
            )
        };
        for (ion, first, second) in [
            (&g[8], (295., 55.), (289., 49.)),
            (&g[9], (325., 55.), (319., 49.)),
            (&g[11], (385., 40.), (391., 46.)),
            (&g[12], (415., 40.), (415., 46.)),
        ] {
            let parts = ion.parts();
            let dot = part_middle(ion, parts.iter().find(|part| part.filled).unwrap());
            let arms = parts.iter().find(|part| !part.filled).unwrap();
            let sign = part_middle(ion, arms);
            let (a, b) = (at(first.0, first.1, ion), at(second.0, second.1, ion));
            assert!(near(dot.x, a.x) && near(dot.y, a.y), "{dot:?} {a:?}");
            assert!(near(sign.x, b.x) && near(sign.y, b.y), "{sign:?} {b:?}");
            // Each arm end lies level with the sign's middle or straight
            // above or below it.
            let middle = Point::new(sign.x + ion.origin.x, sign.y + ion.origin.y);
            for command in &arms.commands {
                if let PathCommand::Move(p) | PathCommand::Line(p) = command {
                    assert!(near(p.x, middle.x) || near(p.y, middle.y), "{p:?} {middle:?}");
                }
            }
        }
        // The bar lies between its points, along the line through them.
        assert!(near(g[10].origin.x, g[0].origin.x + 297. * pt));
        assert!(near(g[10].axis_x.x, g[10].axis_x.y));
    }
}

#[tokio::test]
async fn daggers_and_the_bar_are_symbols_and_stereo_flags_are_boxed_text() {
    for (document, notices) in sheet(
        "<graphic id='2' GraphicType='Symbol' BoundingBox='55 105 49 99' SymbolType='Dagger'/>\
         <graphic id='3' GraphicType='Symbol' BoundingBox='85 105 79 99' SymbolType='DoubleDagger'/>\
         <graphic id='4' GraphicType='Symbol' BoundingBox='115 105 109 99' SymbolType='LonePairBar'/>\
         <graphic id='5' GraphicType='Symbol' BoundingBox='395 105 389 99' SymbolType='Racemic'/>\
         <graphic id='6' GraphicType='Symbol' BoundingBox='425 105 431 111' SymbolType='Absolute'/>\
         <graphic id='7' GraphicType='Symbol' BoundingBox='455 105 449 99' SymbolType='Relative'/>",
    )
    .await
    {
        assert_eq!(notices, nearest("3 stereo flags as text in a rounded box"));
        let kinds: Vec<_> = document.graphics.iter().map(|g| g.kind).collect();
        use GraphicKind::{RoundedRectangle, Symbol};
        assert_eq!(
            kinds,
            [
                Symbol(SymbolKind::Dagger),
                Symbol(SymbolKind::DoubleDagger),
                Symbol(SymbolKind::LonePairBar),
                RoundedRectangle,
                RoundedRectangle,
                RoundedRectangle
            ]
        );
        let mut flags: Vec<_> = document.annotations.iter().map(|a| a.text.as_str()).collect();
        flags.sort_unstable();
        assert_eq!(flags, ["Abs", "Rac", "Rel"]);
        // Each box is centered on its symbol's first point, wherever the second
        // lies, so 30 pt apart; as wide as ChemDraw draws it around the
        // caption-sized label, and grouped with its label.
        let boxes = &document.graphics[3..];
        let size = |axis: Point| axis.x.hypot(axis.y);
        let scale = size(boxes[0].axis_y) / (1.32 * 10.);
        assert!((size(boxes[0].axis_x) / scale - 2.3 * 10.).abs() < 0.01);
        let center = |g: &Graphic| g.origin.x + (g.axis_x.x + g.axis_y.x) / 2.;
        assert!(((center(&boxes[1]) - center(&boxes[0])) / scale - 30.).abs() < 0.01);
        // The label starts at the box's top half and is centered across it.
        let racemic = document.annotations.iter().find(|a| a.text == "Rac").unwrap();
        let middle = boxes[0].origin.y + (boxes[0].axis_x.y + boxes[0].axis_y.y) / 2.;
        assert!(racemic.position.y < middle);
        assert!(((center(&boxes[0]) - racemic.position.x) / scale - 0.89 * 10.).abs() < 0.01);
        assert!(document.graphics[3].style.corner_pt.is_some());
        assert_eq!(document.groups.len(), 3);
        assert!(document.groups.iter().all(|group| group.members.len() == 2));
    }
    // A flag keeps its colour, and a hidden one stays hidden.
    for (document, _) in sheet(
        "<graphic id='2' GraphicType='Symbol' BoundingBox='55 105 49 99' SymbolType='Absolute' \
         color='4'/>\
         <graphic id='3' GraphicType='Symbol' BoundingBox='85 105 79 99' SymbolType='Relative' \
         Visible='no'/>",
    )
    .await
    {
        let [flag] = document.annotations.as_slice() else {
            panic!("{:?}", document.annotations);
        };
        assert_eq!(flag.text, "Abs");
        assert_eq!(flag.format.style.color, Color::Custom([255, 0, 0]));
    }
}

#[tokio::test]
async fn facing_single_brackets_are_a_pair_and_a_lone_one_is_a_curve() {
    // As ChemDraw saves a drawn pair: the right bracket runs down, the left up.
    for (document, notices) in sheet(
        "<graphic id='2' GraphicType='Bracket' BracketType='Square' BoundingBox='170 45 170 90'/>\
         <graphic id='3' GraphicType='Bracket' BracketType='Square' BoundingBox='120 90 120 45'/>\
         <graphic id='4' GraphicType='Bracket' BracketType='Round' BoundingBox='300 90 300 45'/>\
         <graphic id='5' GraphicType='Bracket' BracketType='Curly' BoundingBox='400 45 440 90'/>",
    )
    .await
    {
        assert_eq!(notices, nearest("2 single brackets as curves"));
        let kinds: Vec<_> = document.graphics.iter().map(|g| g.kind).collect();
        assert_eq!(
            kinds,
            [GraphicKind::Brackets, GraphicKind::Path, GraphicKind::Path]
        );
        let pair = &document.graphics[0];
        let scale = pair.axis_y.y / 45.;
        assert!((pair.axis_x.x / scale - 50.).abs() < 0.01);
        assert!(((pair.origin.x + pair.axis_x.x) / pair.origin.x - 170. / 120.).abs() < 0.001);
        // The lone parenthesis is as tall as drawn and bulges left, away
        // from what it would enclose.
        let (low, high) = extent(&document.graphics[1]);
        assert!(((high.y - low.y) / scale - 45.).abs() < 0.01);
        assert_eq!(document.graphics.len(), 3);
        let [PathCommand::Move(lip), PathCommand::Cubic(bulge, _, _)] =
            document.graphics[1].path.as_slice()
        else {
            panic!("{:?}", document.graphics[1].path);
        };
        assert!(bulge.x < lip.x && (lip.x - bulge.x) / scale < 8.);
        assert!((high.x - low.x).abs() < 0.01);
    }
}

#[tokio::test]
async fn brackets_that_differ_or_hide_are_not_made_a_pair() {
    for (document, notices) in sheet(
        "<graphic id='2' GraphicType='Bracket' BracketType='Square' BoundingBox='170 45 170 90' \
         color='4'/>\
         <graphic id='3' GraphicType='Bracket' BracketType='Square' BoundingBox='120 90 120 45'/>\
         <graphic id='4' GraphicType='Bracket' BracketType='Round' BoundingBox='270 45 270 90'/>\
         <graphic id='5' GraphicType='Bracket' BracketType='Round' BoundingBox='220 90 220 45' \
         Visible='no'/>",
    )
    .await
    {
        assert_eq!(notices[0], nearest("4 single brackets as curves")[0]);
        let strokes: Vec<_> = document.graphics.iter().map(|g| g.style.stroke).collect();
        assert_eq!(strokes.len(), 3);
        assert_eq!(strokes[0], Color::Custom([255, 0, 0]));
    }
}

#[tokio::test]
async fn doubled_and_older_curves_import_as_plain_curves() {
    let points = "20 20 20 20 30 0 50 40 60 20 60 20";
    for (document, notices) in sheet(&format!(
        "<curve id='2' CurvePoints='{points}' CurveType='257' color='4'/>\
         <curve id='3' CurvePoints='{points}' CurveType='512'/>\
         <curve id='4' CurvePoints='{points}' FillType='Shaded' LineType='Dashed Wavy'/>\
         <curve id='5' CurvePoints='{points}' CurveType='8'/>\
         <curve id='6' CurvePoints='20 20 30 0 50 40 60 20'/>\
         <curve id='7' CurvePoints='20 20'/><curve id='8'/>",
    ))
    .await
    {
        assert_eq!(notices, nearest("1 doubled curve as two grouped curves"));
        // The headed curve is an arrow; the two without a segment draw nothing.
        assert_eq!(document.arrows.len(), 1);
        let [closed, outer, inner, dashed, padded] = document.graphics.as_slice() else {
            panic!("{:?}", document.graphics);
        };
        assert!(closed.style.effects.shaded && dashed.style.effects.shaded);
        assert_eq!((outer.style.fill, inner.style.fill), (None, None));
        assert_eq!(dashed.style.pattern, LinePattern::Dashed);
        // Each line runs half the bold width and half its own width from
        // the curve, (4 + 1) / 2 pt, on either side.
        let ends = |graphic: &Graphic| match graphic.path.as_slice() {
            [PathCommand::Move(start), .., PathCommand::Cubic(_, _, end)] => [*start, *end],
            path => panic!("{path:?}"),
        };
        let [start, end] = ends(padded);
        let scale = (end.x - start.x) / 40.;
        let [
            PathCommand::Move(_),
            PathCommand::Cubic(leaving, arriving, _),
        ] = padded.path[..]
        else {
            panic!("{:?}", padded.path);
        };
        let [[a, b], [c, d]] = [outer, inner].map(ends);
        for (one, other, at, toward) in [(a, c, start, leaving), (b, d, end, arriving)] {
            // The two ends sit either side of the curve's, square to it.
            let (x, y) = ((one.x - at.x) / scale, (one.y - at.y) / scale);
            let (u, v) = (toward.x - at.x, toward.y - at.y);
            assert!((x.hypot(y) - 2.5).abs() < 0.01, "{one:?}");
            assert!((x * u + y * v).abs() / u.hypot(v) < 0.01, "{one:?}");
            assert!((one.x + other.x - 2. * at.x).abs() < 0.01);
            assert!((one.y + other.y - 2. * at.y).abs() < 0.01);
        }
        let [group] = document.groups.as_slice() else {
            panic!("{:?}", document.groups);
        };
        let mut members = group.members.clone();
        members.sort_unstable();
        assert_eq!(members, [outer.id, inner.id]);
    }
}

#[tokio::test]
async fn unbalanced_equilibria_keep_their_lengths_and_retro_heads_take_both_shafts() {
    // ChemDraw stores the forward arrow's length against the reverse one in
    // hundredths; ReShiki keeps the reverse one's against the forward one.
    let equilibrium = |y: u32, ratio: &str| {
        format!(
            "<arrow id='{y}' ArrowheadHead='HalfLeft' ArrowheadTail='HalfLeft' \
             ArrowheadType='Solid' ArrowShaftSpacing='400' {ratio} \
             Head3D='105 {y} 0' Tail3D='30 {y} 0'/>"
        )
    };
    let objects = [
        equilibrium(20, ""),
        equilibrium(40, "ArrowEquilibriumRatio='50'"),
        equilibrium(60, "ArrowEquilibriumRatio='300'"),
        equilibrium(80, "ArrowEquilibriumRatio='2'"),
        "<arrow id='100' ArrowheadHead='Full' ArrowheadType='Angle' ArrowShaftSpacing='600' \
         HeadSize='1000' ArrowheadCenterSize='875' ArrowheadWidth='250' \
         Head3D='105 100 0' Tail3D='30 100 0'/>\
         <graphic id='101' SupersededBy='100' BoundingBox='105 100 30 100' GraphicType='Line' \
         ArrowType='RetroSynthetic' HeadSize='1200'/>"
            .into(),
    ]
    .concat();
    for (document, notices) in sheet(&objects).await {
        assert_eq!(
            notices,
            nearest(
                "1 unbalanced equilibrium arrow with its shorter arrow lengthened to a fifth \
                 of the other"
            )
        );
        let style = |i: usize| document.arrows[i].style.clone().unwrap();
        let ratios: Vec<_> = (0..4).map(|i| style(i).equilibrium_ratio).collect();
        assert_eq!(ratios, [1., 2., 1. / 3., 5.]);
        // The arrows still run from tail to head.
        assert!(document.arrows.iter().all(|a| a.end.x > a.start.x));
        // A retrosynthesis head is as wide as its center size, so the shafts,
        // 3 pt either side, meet it. ChemDraw saves the older form beside it.
        assert_eq!(document.arrows[4].kind, "retro");
        assert_eq!(style(4).head_width_pt, 8.75);
        // A ChemDraw save keeps each ratio.
        let saved = formats::write(document.clone(), "cdxml").await.unwrap();
        let text = String::from_utf8(saved.clone()).unwrap();
        for value in [
            "ArrowEquilibriumRatio=\"50\"",
            "ArrowEquilibriumRatio=\"300\"",
        ] {
            assert!(text.contains(value), "{value}");
        }
        let (again, _) = open(saved, "cdxml").await.unwrap();
        let ratios_again: Vec<_> = again
            .arrows
            .iter()
            .map(|a| a.style.as_ref().unwrap().equilibrium_ratio)
            .collect();
        assert_eq!(ratios_again[..4], ratios);
        assert_eq!(again.arrows[4].style.as_ref().unwrap().head_width_pt, 8.75);
    }
}

#[tokio::test]
async fn a_hairline_curve_with_a_head_takes_the_thinnest_arrow_line() {
    for (document, notices) in sheet(
        "<curve id='2' CurvePoints='20 20 20 20 30 0 50 40 60 20 60 20' ArrowheadHead='Full' \
         LineWidth='0.05'/>\
         <curve id='3' CurvePoints='20 50 20 50 30 30 50 70 60 50 60 50' ArrowheadHead='Full' \
         ArrowheadType='Hollow' LineWidth='0.05'/>",
    )
    .await
    {
        assert_eq!(
            notices,
            nearest("1 line, arrowhead or shadow size at the nearest size ReShiki draws")
        );
        // A hollow head makes a path, which draws the hairline itself.
        assert_eq!(document.arrows[0].appearance().width_pt, 0.1);
        assert_eq!(document.graphics.len(), 1);
    }
}

#[tokio::test]
async fn objects_without_an_equivalent_are_left_out_and_named() {
    let read = |objects: &'static str| async move {
        let xml = format!("<CDXML><page id='1'>{objects}</page></CDXML>");
        open(xml.into_bytes(), "cdxml").await.unwrap()
    };
    let caption = "<t id='9' p='10 200'><s>kept</s></t>";
    let (document, notices) = read(
        "<bioshape id='2' BoundingBox='10 10 50 50' BioShapeType='Mitochondrion'/>\
         <bioshape id='3' BoundingBox='60 10 90 50' BioShapeType='Ribosome'/>\
         <gepplate id='4' BoundingBox='10 60 90 160'><geplane id='5'><gepband id='6'/></geplane>\
         </gepplate><tlcplate id='7' BoundingBox='100 60 150 160'><tlclane id='8'/></tlcplate>\
         <spectrum id='10'/><t id='9' p='10 200'><s>kept</s></t>",
    )
    .await;
    assert_eq!(
        notices,
        left_out("2 BioDraw shapes; 1 gel plate; 1 TLC plate; 1 other ChemDraw object")
    );
    assert_eq!(document.annotations.len(), 1);
    // A table's cells and a plasmid map's ring, regions and labels remain.
    let (document, notices) = read(
        "<table id='2' BoundingBox='10 10 200 100'><page id='3'><border id='4' Side='left'/>\
         <t id='5' p='20 30'><s>cell</s></t></page></table>\
         <plasmidmap id='6' p='300 300' RingRadius='3932160' LineWidth='2'>\
         <plasmidregion id='7' AngularSize='90' Head3D='300 240 0' Tail3D='360 300 0' \
         Center3D='300 300 0' MajorAxisEnd3D='360 300 0' MinorAxisEnd3D='300 360 0' \
         ArrowheadHead='Full' ArrowheadType='Hollow' HeadSize='3978' ArrowheadCenterSize='3978' \
         ArrowheadWidth='692' ArrowShaftSpacing='2767' FadePercent='300'/>\
         <plasmidmarker id='8'><t id='9' p='380 300'><s>ori</s></t></plasmidmarker></plasmidmap>",
    )
    .await;
    assert_eq!(
        notices,
        nearest(
            "1 line, arrowhead or shadow size at the nearest size ReShiki draws; \
             1 plasmid map as its ring, regions and labels; 1 table as its cell contents"
        )
    );
    let mut texts: Vec<_> = document
        .annotations
        .iter()
        .map(|a| a.text.as_str())
        .collect();
    texts.sort_unstable();
    assert_eq!(texts, ["cell", "ori"]);
    let kinds: Vec<_> = document.graphics.iter().map(|g| g.kind).collect();
    assert_eq!(kinds, [GraphicKind::Ellipse]);
    // A region is an arc arrow around the ring.
    assert!(document.arrows[0].arc.is_some());
    // A template file's grid, a picture without a bitmap and an oval without
    // its axes.
    let (document, notices) = read(
        "<embeddedobject id='2' BoundingBox='180 80 260 130' MacPICT='00000000'/>\
         <graphic id='3' GraphicType='Oval' BoundingBox='180 80 260 130' OvalType='Circle'/>\
         <graphic id='4' GraphicType='Undefined' BoundingBox='180 80 260 130'/>\
         <t id='9' p='10 200'><s>kept</s></t>",
    )
    .await;
    assert_eq!(
        notices,
        left_out("1 picture without a bitmap copy; 2 incomplete or unknown graphics")
    );
    assert_eq!(
        (document.graphics.len(), document.annotations.len()),
        (0, 1)
    );
    let grid = format!("<CDXML><templategrid id='2'/><page id='1'>{caption}</page></CDXML>");
    let (document, notices) = open(grid.into_bytes(), "cdxml").await.unwrap();
    assert_eq!((document.annotations.len(), notices.len()), (1, 0));
}

#[tokio::test]
async fn captions_with_unknown_settings_import_plain() {
    for (document, notices) in sheet(
        "<t id='2' p='10 20' RotationAngle='5898240'><s>turned</s></t>\
         <t id='3' p='10 40'><s face='9'>outlined</s></t>\
         <t id='4' p='10 60'><s size='3'>tiny</s></t>\
         <t id='5' p='10 80' Justification='Auto' LineHeight='400'><s>spaced</s></t>\
         <fragment id='6'><n id='7' p='100 100' Element='8' NumHydrogens='1'>\
         <t id='8' p='96 104'><s face='96'>O</s><s face='97' color='4'>H</s></t></n>\
         <n id='9' p='120 100'/><b id='10' B='7' E='9'/></fragment>\
         <t id='11' p='10 120' CaptionLineHeight='3'><s size='1'>tiny, tight</s></t>",
    )
    .await
    {
        assert_eq!(
            notices,
            nearest(
                "1 outlined or shadowed text in the plain face; \
                 2 text sizes at the nearest size ReShiki draws; \
                 1 atom label with mixed text styles in the style of its first character; \
                 2 captions with automatic line spacing or width"
            )
        );
        assert_eq!(document.annotations.len(), 5);
        // Turned without a box: about its anchor, as in ChemDraw.
        assert_eq!(document.annotations[0].angle, 90.);
        let outlined = &document.annotations[1].format.style;
        assert!(outlined.bold);
        assert_eq!(document.annotations[2].format.style.size_pt, 4.);
        assert_eq!(document.atoms.len(), 2);
    }
}

#[tokio::test]
async fn abbreviation_labels_with_mixed_fonts_open_in_their_first_characters_style() {
    // As ChemDraw writes "D-Trp" with a small D: an abbreviation label, not
    // an element label, with runs in two sizes.
    for (document, notices) in sheet(
        "<fragment id='11'><n id='12' p='70 100'/><n id='13' p='100 100' NodeType='Fragment'>\
         <fragment id='14'><n id='15' p='100 100' Element='8'/><n id='16' p='115 100'/>\
         <n id='17' p='85 100' NodeType='ExternalConnectionPoint'/>\
         <b id='18' B='17' E='15'/><b id='19' B='15' E='16'/></fragment>\
         <t p='96 104'><s size='7'>O</s><s size='10'>Me</s></t></n>\
         <b id='20' B='12' E='13'/></fragment>",
    )
    .await
    {
        assert_eq!(
            notices,
            nearest("1 atom label with mixed text styles in the style of its first character")
        );
        let [abbreviation] = document.abbreviations.as_slice() else {
            panic!("one abbreviation: {:?}", document.abbreviations);
        };
        assert_eq!(abbreviation.label_style.as_ref().unwrap().size_pt, 7.);
    }
}

#[tokio::test]
async fn captions_take_the_nearest_size_of_their_page_or_document() {
    for (xml, notices, size) in [
        (
            "<CDXML><page id='1' CaptionSize='3'><t id='2' p='10 20'><s>page</s></t>\
             <t id='3' p='10 40'><s>page</s></t></page></CDXML>",
            nearest("2 text sizes at the nearest size ReShiki draws"),
            4.,
        ),
        (
            "<CDXML CaptionSize='200'><page id='1'><t id='2' p='10 20'><s>document</s></t>\
             </page></CDXML>",
            nearest("1 text size at the nearest size ReShiki draws"),
            144.,
        ),
        // A caption with its own size takes nothing from the document, and
        // the document's label size only sets its drawing style.
        (
            "<CDXML CaptionSize='200' LabelSize='200'><page id='1'><t id='2' p='10 20'>\
             <s size='10'>own</s></t></page></CDXML>",
            Vec::new(),
            10.,
        ),
    ] {
        let (document, found) = open(xml.as_bytes().to_vec(), "cdxml").await.unwrap();
        assert_eq!(found, notices, "{xml}");
        assert_eq!(
            formats::chemdraw_loss("cdxml", xml).unsupported,
            document.annotations.len() * usize::from(size != 10.),
            "{xml}"
        );
        for caption in &document.annotations {
            assert_eq!(caption.format.style.size_pt, size, "{xml}");
        }
    }
    let xml = "<CDXML LabelSize='200'><page id='1'><t id='2' p='10 20'><s>plain</s></t>\
               </page></CDXML>";
    let (document, _) = open(xml.as_bytes().to_vec(), "cdxml").await.unwrap();
    assert_eq!(document.drawing_style.font_size_pt, 144.);
}

#[tokio::test]
async fn captions_that_inherit_an_outline_are_named() {
    for xml in [
        "<CDXML CaptionFace='8'><page id='1'><t id='2' p='10 20'><s>outlined</s></t>\
         <t id='3' p='10 40'><s face='0'>own</s></t></page></CDXML>",
        "<CDXML><page id='1' CaptionFace='9'><t id='2' p='10 20'><s>outlined</s></t>\
         <t id='3' p='10 40'><s face='0'>own</s></t></page></CDXML>",
    ] {
        let (document, notices) = open(xml.as_bytes().to_vec(), "cdxml").await.unwrap();
        assert_eq!(
            notices,
            nearest("1 outlined or shadowed text in the plain face"),
            "{xml}"
        );
        assert_eq!(formats::chemdraw_loss("cdxml", xml).unsupported, 1, "{xml}");
        assert_eq!(document.annotations.len(), 2, "{xml}");
    }
}

#[tokio::test]
async fn a_hidden_object_left_out_keeps_its_contents_hidden() {
    let xml = "<CDXML><page id='1'>\
               <bioshape id='2' Visible='no'><t id='3' p='10 20'><s>hidden</s></t></bioshape>\
               <bioshape id='4'><t id='5' p='10 40'><s>shown</s></t></bioshape></page></CDXML>";
    let (document, notices) = open(xml.as_bytes().to_vec(), "cdxml").await.unwrap();
    assert_eq!(notices[..1], left_out("2 BioDraw shapes"));
    assert!(notices[1].starts_with("Hidden drawing objects were omitted."));
    let texts: Vec<_> = document
        .annotations
        .iter()
        .map(|a| a.text.as_str())
        .collect();
    assert_eq!(texts, ["shown"]);
}

#[tokio::test]
async fn an_older_half_arrow_takes_its_side_from_the_sign_of_its_size() {
    let xml = "<CDXML><page id='1'>\
               <graphic id='2' GraphicType='Line' ArrowType='HalfHead' HeadSize='1000' \
               BoundingBox='70 20 10 20'/>\
               <graphic id='3' GraphicType='Line' ArrowType='HalfHead' HeadSize='-1000' \
               BoundingBox='70 40 10 40'/>\
               <graphic id='4' GraphicType='Line' ArrowType='HalfHead' HeadSize='-600' \
               BoundingBox='70 60 10 60'/></page></CDXML>";
    let (document, notices) = open(xml.as_bytes().to_vec(), "cdxml").await.unwrap();
    assert_eq!(notices, Vec::<String>::new());
    let [left, right, small] = document.arrows.as_slice() else {
        panic!("{:?}", document.arrows);
    };
    let [left, right, small] = [left, right, small].map(|arrow| arrow.appearance());
    assert_eq!(
        [left.head, right.head, small.head],
        [Head::Left, Head::Right, Head::Right]
    );
    assert_eq!(
        [
            left.head_length_pt,
            right.head_length_pt,
            small.head_length_pt
        ],
        [10., 10., 6.]
    );
}

#[test]
fn a_save_counts_each_changed_object_once() {
    // An arrowhead out of size, and a rotated caption in a tiny outlined face.
    let xml = "<CDXML><page id='1'><arrow id='2' Tail3D='10 20 0' Head3D='70 20 0' \
               ArrowheadHead='Full' HeadSize='9000'/>\
               <t id='3' p='10 60' RotationAngle='5898240'><s size='3' face='9'>turned</s></t>\
               </page></CDXML>";
    assert_eq!(formats::chemdraw_loss("cdxml", xml).unsupported, 2);
}

#[tokio::test]
async fn chemdraws_binary_extras_do_not_stop_a_file() {
    let xml = "<CDXML><colortable><color r='1' g='1' b='1'/><color r='0' g='0' b='0'/>\
               <color r='1' g='0' b='0'/></colortable><page id='1'>\
               <graphic id='2' GraphicType='Rectangle' BoundingBox='10 10 60 40' color='4'/>\
               </page></CDXML>";
    let plain = exchange::to_cdx(xml).unwrap();
    let color = [0x01, 0x03, 0x02, 0x00, 0x04, 0x00];
    let at = plain
        .windows(color.len())
        .position(|window| window == color)
        .unwrap();
    let with = |replacement: &[u8]| {
        let mut data = plain[..at].to_vec();
        data.extend_from_slice(replacement);
        data.extend_from_slice(&plain[at + color.len()..]);
        data
    };
    // A colour index in four bytes, a property and an object ReShiki does not
    // know, and an identifier used twice.
    let wide = with(&[0x01, 0x03, 0x04, 0x00, 0x04, 0x00, 0x00, 0x00]);
    let property = with(&[
        0x01, 0x03, 0x02, 0x00, 0x04, 0x00, 0x00, 0x7f, 0x02, 0x00, 0x2c, 0x01,
    ]);
    let object = with(&[
        0x01, 0x03, 0x02, 0x00, 0x04, 0x00, 0x25, 0x80, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]);
    let repeated = with(&[
        0x01, 0x03, 0x02, 0x00, 0x04, 0x00, 0x08, 0x80, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]);
    for (name, data, notice) in [
        ("four-byte colour", wide, None),
        ("unknown property", property, None),
        ("unknown object", object, Some(left_out("1 BioDraw shape"))),
        ("repeated identifier", repeated, None),
    ] {
        assert!(exchange::from_cdx(&data).is_err(), "{name}: strict reader");
        let (document, notices) = open(data, "cdx")
            .await
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(notices, notice.unwrap_or_default(), "{name}");
        assert_eq!(document.graphics.len(), 1, "{name}");
        assert_eq!(
            document.graphics[0].style.stroke,
            Color::Custom([255, 0, 0]),
            "{name}"
        );
    }
}

#[tokio::test]
async fn atoms_kept_straight_in_a_group_open_as_one_molecule() {
    // Older ChemDraw files, such as its Anatomy template, keep a diagram's
    // atom labels in an integral group without a fragment around them.
    for (document, notices) in sheet(
        "<group id='10' Integral='yes'><n id='11' p='50 50' Element='8'/>\
         <n id='12' p='70 50'/><b id='13' B='11' E='12'/></group>",
    )
    .await
    {
        assert_eq!(notices, Vec::<String>::new());
        assert_eq!((document.atoms.len(), document.bonds.len()), (2, 1));
    }
}

#[tokio::test]
async fn native_and_legacy_object_tag_names_are_read() {
    // Native object tags use bare bytes. Read the style-run count prefix used
    // in older ReShiki files as well.
    let xml = "<CDXML><page id='1'><fragment id='2'>\
               <n id='3' p='10 10' ShowAtomNumber='yes' AtomNumber='1'>\
               <objecttag TagType='Unknown' Name='number'><t p='12 4'><s size='7.5'>1</s></t>\
               </objecttag></n><n id='4' p='30 10'/><b id='5' B='3' E='4'/></fragment>\
               </page></CDXML>";
    let native = exchange::to_cdx(xml).unwrap();
    let bare_name = b"\x08\x00\x06\x00number";
    let at = native
        .windows(bare_name.len())
        .position(|window| window == bare_name)
        .unwrap();
    let mut legacy = native.clone();
    legacy.splice(
        at..at + bare_name.len(),
        b"\x08\x00\x08\x00\x00\x00number".iter().copied(),
    );
    let (expected, _) = open(native, "cdx").await.unwrap();
    let (document, _) = open(legacy, "cdx").await.unwrap();
    assert_eq!(
        serde_json::to_value(&document).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
}

#[tokio::test]
async fn an_unknown_property_on_an_atom_is_named() {
    let xml = "<CDXML><page id='1'><fragment id='2'><n id='3' p='10 10'/><n id='4' p='30 10'/>\
               <b id='5' B='3' E='4'/></fragment></page></CDXML>";
    let plain = exchange::to_cdx(xml).unwrap();
    open(plain.clone(), "cdx").await.unwrap();
    let atom = [0x04, 0x80, 0x03, 0x00, 0x00, 0x00];
    let at = plain
        .windows(atom.len())
        .position(|window| window == atom)
        .unwrap()
        + atom.len();
    let mut data = plain[..at].to_vec();
    data.extend_from_slice(&[0x00, 0x7f, 0x02, 0x00, 0x2c, 0x01]);
    data.extend_from_slice(&plain[at..]);
    assert!(exchange::from_cdx(&data).is_err());
    assert_eq!(
        formats::chemdraw_loss("cdx", &STANDARD.encode(&data)).unsupported,
        1
    );
    let (document, notices) = open(data, "cdx").await.unwrap();
    assert_eq!(
        notices,
        left_out("1 atom or bond setting ReShiki does not know")
    );
    assert_eq!((document.atoms.len(), document.bonds.len()), (2, 1));
}

#[tokio::test]
async fn a_mark_on_an_atom_is_never_left_out() {
    // A radical without a graphic type, standing for the atom's charge: the
    // mark reader refuses it rather than the pass dropping chemistry.
    let xml = "<CDXML><page id='1'><fragment id='2'><n id='3' p='0 0' Element='8' Charge='-1'/>\
               <graphic id='4' SymbolType='Radical' BoundingBox='4 3 0 0'>\
               <represent object='3' attribute='Charge'/></graphic></fragment></page></CDXML>";
    assert!(open(xml.as_bytes().to_vec(), "cdxml").await.is_err());
}

#[tokio::test]
async fn text_and_arrows_chemdraw_stacks_over_a_shape_stay_over_it() {
    // A structure under a shaded box, a legend over the box, a box at the
    // legend's level, a box under an arrow, then a box and a line on top.
    for (document, _) in sheet(
        "<fragment id='2'><n id='3' p='20 20' Z='2'/><n id='4' p='40 20' Z='3'/>\
         <b id='5' B='3' E='4' Z='4'/></fragment>\
         <graphic id='6' Z='5' GraphicType='Rectangle' RectangleType='Shaded' \
         BoundingBox='0 0 100 60'/>\
         <t id='7' Z='6' p='10 50'><s>Legend</s></t>\
         <graphic id='8' Z='6' GraphicType='Rectangle' RectangleType='Filled' \
         BoundingBox='0 70 100 90'/>\
         <graphic id='9' Z='7' GraphicType='Rectangle' RectangleType='Filled' \
         BoundingBox='0 100 100 120'/>\
         <arrow id='10' Z='8' Tail3D='10 110 0' Head3D='90 110 0' ArrowheadHead='Full'/>\
         <graphic id='11' Z='9' GraphicType='Rectangle' RectangleType='Filled' \
         BoundingBox='200 0 300 60'/>\
         <arrow id='12' Z='10' Tail3D='200 80 0' Head3D='300 80 0'/>",
    )
    .await
    {
        let layers: Vec<_> = document.graphics.iter().map(|g| g.layer).collect();
        // Behind the chemistry in ChemDraw's order, and only the top box and
        // the line in front: the line is a graphic, not an arrow to stay under.
        assert_eq!(layers, [-4, -3, -2, 1, 2]);
        assert_eq!(document.graphics[4].kind, GraphicKind::Line);
    }
    // A curve with a head counts as an arrow: the box under it stays behind.
    for (document, _) in sheet(
        "<fragment id='2'><n id='3' p='20 20' Z='2'/></fragment>\
         <graphic id='4' Z='7' GraphicType='Rectangle' RectangleType='Filled' \
         BoundingBox='0 0 100 60'/>\
         <curve id='5' Z='8' ArrowheadHead='Full' CurvePoints='0 0 0 0 30 10 60 10 90 0 90 0'/>",
    )
    .await
    {
        assert!(document.graphics[0].layer < 0, "{:?}", document.graphics);
    }
}

#[tokio::test]
async fn chemdraw_lines_are_graphic_lines_and_reaction_arrows_stay_arrows() {
    // A reaction arrow between two structures, a dashed bold line, a wavy
    // line, a line a reaction step lists, an arc without heads and a double
    // shaft without heads.
    for (document, _) in sheet(
        "<fragment id='2'><n id='3' p='0 20'/><n id='4' p='10 20'/><b id='5' B='3' E='4'/>\
         </fragment><fragment id='6'><n id='7' p='90 20'/><n id='8' p='100 20'/>\
         <b id='9' B='7' E='8'/></fragment>\
         <arrow id='10' Tail3D='20 20 0' Head3D='70 20 0' ArrowheadHead='Full'/>\
         <arrow id='11' Tail3D='20 60 0' Head3D='70 60 0' LineType='Dashed Bold'/>\
         <arrow id='12' Tail3D='70 80 0' Head3D='20 80 0' LineType='Wavy'/>\
         <arrow id='13' Tail3D='20 100 0' Head3D='70 100 0'/>\
         <arrow id='14' AngularSize='-180' Head3D='83 340 0' Tail3D='47 340 0' \
         Center3D='65 340 0' MajorAxisEnd3D='83 340 0' MinorAxisEnd3D='65 358 0'/>\
         <arrow id='17' Tail3D='20 120 0' Head3D='70 120 0' ArrowShaftSpacing='800'/>\
         <scheme id='15'><step id='16' ReactionStepArrows='13'/></scheme>",
    )
    .await
    {
        let [dashed, wavy, arc] = document.graphics.as_slice() else {
            panic!("{:?}", document.graphics);
        };
        assert_eq!(arc.kind, GraphicKind::Path);
        assert_eq!(
            (dashed.kind, dashed.style.pattern, dashed.style.width_pt),
            (GraphicKind::Line, LinePattern::Dashed, 4.)
        );
        assert_eq!(
            (wavy.kind, wavy.style.pattern),
            (GraphicKind::Line, LinePattern::Wavy)
        );
        // Each runs from ChemDraw's tail to its head.
        assert!(dashed.axis_x.x > 0. && wavy.axis_x.x < 0.);
        assert_eq!((dashed.axis_x.y, wavy.axis_x.y), (0., 0.));
        // The reaction arrow, the listed line and a broad body without heads
        // stay arrows that take part in reactions.
        let purposes: Vec<_> = document.arrows.iter().map(|a| a.purpose).collect();
        assert_eq!(purposes, [Purpose::Chemistry; 3]);
        let reaction = reactions::preview(&document, document.arrows[0].id).unwrap();
        assert_eq!(
            [Role::Reactant, Role::Product].map(|role| reaction.participants(role).len()),
            [1, 1]
        );
    }
    // A lone page, without its CDXML document, keeps a line as an arrow.
    let page = "<page><arrow id='2' Tail3D='0 0 0' Head3D='20 0 0'/></page>";
    let (document, _) = open(page.into(), "cdxml").await.unwrap();
    assert_eq!((document.arrows.len(), document.graphics.len()), (1, 0));
}

#[tokio::test]
async fn a_reaction_step_forgets_an_arc_that_is_no_longer_an_arrow() {
    // ChemDraw lists a headless arc among a step's arrows.
    let xml = "<CDXML><page id='1'><arrow id='2' Tail3D='10 20 0' Head3D='70 20 0' \
               ArrowheadHead='Full'/><graphic id='3' GraphicType='Arc' \
               BoundingBox='183 285 165 285' AngularSize='90'/><scheme id='4'>\
               <step id='5' ReactionStepArrows='2 3'/></scheme></page></CDXML>";
    let (document, _) = open(xml.as_bytes().to_vec(), "cdxml").await.unwrap();
    assert_eq!((document.arrows.len(), document.graphics.len()), (1, 1));
}

#[tokio::test]
async fn files_that_opened_before_open_the_same_way() {
    // A lone pair on an atom as earlier ReShiki versions wrote it to CDX,
    // with the value ChemDraw uses for the bar.
    let pair = "<CDXML><page id='1'><fragment id='2'><n id='3' p='100 100' Element='8' \
                NumHydrogens='2'/></fragment><graphic id='4' GraphicType='Symbol' \
                SymbolType='LonePairBar' BoundingBox='100 92 100 88'>\
                <represent attribute='Radical' object='3'/></graphic></page></CDXML>";
    let (document, notices) = open(pair.as_bytes().to_vec(), "cdxml").await.unwrap();
    assert_eq!(notices, Vec::<String>::new());
    assert_eq!((document.atoms.len(), document.graphics.len()), (1, 0));
    // Hidden artwork of a kind ReShiki does not know stays out of the way,
    // and an arrowhead at the edge of the range keeps its size.
    for (document, notices) in sheet(
        "<graphic id='2' GraphicType='Undefined' BoundingBox='0 500 1 501' Visible='no'/>\
         <arrow id='3' Tail3D='0 0 0' Head3D='60 0 0' ArrowheadHead='Full' \
         HeadSize='50.0001' ArrowheadCenterSize='5.00001'/>",
    )
    .await
    {
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(notices[0].contains("Hidden drawing objects were omitted"));
        assert_eq!(document.arrows.len(), 1);
    }
    // More left-out objects than any figure has still leave the rest.
    let many = format!(
        "<CDXML><page id='1'><t id='2' p='10 20'><s>kept</s></t>{}</page></CDXML>",
        "<bioshape/>".repeat(7000)
    );
    let (document, notices) = open(many.into_bytes(), "cdxml").await.unwrap();
    assert_eq!(notices, left_out("7000 BioDraw shapes"));
    assert_eq!(document.annotations.len(), 1);
}

#[tokio::test]
async fn broken_files_are_still_refused() {
    for (name, data, format) in [
        (
            "cut-off text",
            b"<CDXML><page id='1'><graphic".to_vec(),
            "cdxml",
        ),
        (
            "no drawing",
            b"<CDXML><page id='1'><bioshape id='2'/></page></CDXML>".to_vec(),
            "cdxml",
        ),
        (
            "bond without an atom",
            b"<CDXML><page id='1'><fragment id='2'><n id='3' p='1 1'/><b id='4' B='3' E='9'/>\
              </fragment></page></CDXML>"
                .to_vec(),
            "cdxml",
        ),
        (
            "reaction step naming an object the file does not have",
            b"<CDXML><page id='1'><arrow id='2' Tail3D='10 20 0' Head3D='70 20 0' \
              ArrowheadHead='Full'/><graphic id='3' GraphicType='Rectangle' \
              BoundingBox='10 40 60 70' RectangleType='Shaded'/><scheme id='4'>\
              <step id='5' ReactionStepArrows='2 99'/></scheme></page></CDXML>"
                .to_vec(),
            "cdxml",
        ),
        (
            "bad coordinates",
            b"<CDXML><page id='1'><graphic id='2' GraphicType='Rectangle' \
              BoundingBox='1 2 x 4' RectangleType='Shaded'/></page></CDXML>"
                .to_vec(),
            "cdxml",
        ),
        (
            "cut-off binary",
            b"VjCD0100\x04\x03\x02\x01".to_vec(),
            "cdx",
        ),
        (
            "two atoms with one identifier",
            {
                let xml = "<CDXML><page id='1'><fragment id='2'><n id='3' p='10 10'/>\
                           <n id='4' p='24 10'/><b id='5' B='3' E='4'/></fragment></page></CDXML>";
                let mut data = exchange::to_cdx(xml).unwrap();
                let second = [0x04, 0x80, 0x04, 0x00, 0x00, 0x00];
                let at = data
                    .windows(second.len())
                    .position(|window| window == second)
                    .unwrap();
                data[at + 2] = 3;
                data
            },
            "cdx",
        ),
    ] {
        assert!(open(data, format).await.is_err(), "{name}");
    }
    let whole = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/chemdraw-drawing-elements/drawing.cdx"),
    )
    .unwrap();
    assert!(
        open(whole[..whole.len() / 2].to_vec(), "cdx")
            .await
            .is_err()
    );
}

fn files(root: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, found);
        } else if format(&path).is_some() {
            found.push(path);
        }
    }
}

/// Every file of a local ChemDraw corpus imports. Set
/// `RESHIKI_CHEMDRAW_CORPUS` to its folder. ChemDraw's own samples and
/// templates are not ours to commit, so this runs on a developer's machine.
#[tokio::test]
#[ignore = "needs a local ChemDraw corpus"]
async fn a_local_chemdraw_corpus_imports_every_file() {
    let root = PathBuf::from(std::env::var("RESHIKI_CHEMDRAW_CORPUS").unwrap());
    let mut found = Vec::new();
    files(&root, &mut found);
    found.sort();
    assert!(
        !found.is_empty(),
        "no ChemDraw file under {}",
        root.display()
    );
    let mut refused = Vec::new();
    for path in &found {
        let data = std::fs::read(path).unwrap();
        let name = path.strip_prefix(&root).unwrap().display().to_string();
        match open(data, format(path).unwrap()).await {
            Ok((_, warnings)) => println!("OK\t{name}\t{}", warnings.join(" | ")),
            Err(error) => {
                println!("REFUSED\t{name}\t{error}");
                refused.push(name);
            }
        }
    }
    assert!(
        refused.is_empty(),
        "{} of {} refused",
        refused.len(),
        found.len()
    );
}

#[tokio::test]
async fn a_label_written_hydrogens_first_keeps_them_on_the_left() {
    // As in ChemDraw's citric acid cycle sample: water written H2O.
    for (document, notices) in sheet(
        "<fragment id='10'><n id='11' p='245.59 363.45' Element='8' NumHydrogens='2'><t p='240.8 367.82' BoundingBox='240.8 359.07 263.81 370.42' LabelJustification='Left' LabelAlignment='Left'>\
         <s size='12'>H</s><s size='12' face='32'>2</s><s size='12'>O</s></t></n></fragment>\
         <fragment id='20'><n id='21' p='300 363' Element='8' NumHydrogens='2'><t p='296 367'>\
         <s size='12'>OH</s><s size='12' face='32'>2</s></t></n></fragment>\
         <fragment id='30'><n id='31' p='350 363' Element='8' NumHydrogens='3' Charge='1'>\
         <t p='340 367'><s size='12'>H</s><s size='12' face='32'>3</s><s size='12'>O</s>\
         <s size='12' face='64'>+</s></t></n></fragment>\
         <fragment id='40'><n id='41' p='400 363' Element='8'/></fragment>",
    )
    .await
    {
        assert_eq!(notices, nearest(""));
        let sides: Vec<_> = document
            .atoms
            .iter()
            .map(|atom| atom.display.hydrogen_position)
            .collect();
        use reshiki_model::atom_labels::HydrogenPosition::{Auto, Left};
        assert_eq!(sides, [Left, Auto, Left, Auto]);
        // ChemDraw anchors the lone H2O on its H; its O moves to where
        // ChemDraw draws it, 263.81 - (245.59 - 240.8) = 259.02.
        let x: Vec<_> = document.atoms.iter().map(|atom| atom.position.x).collect();
        assert!(((x[1] - x[0]) / (x[3] - x[1]) - (300. - 259.02) / 100.).abs() < 1e-3);
    }
}

#[tokio::test]
async fn a_doubled_curve_without_a_length_stays_one_line() {
    let points = "20 20 20 20 20 20 20 20 20 20 20 20";
    for (_, notices) in sheet(&format!(
        "<curve id='2' CurvePoints='{points}' CurveType='512'/>\
         <curve id='3' CurvePoints='20 20 20 20 30 0 50 40 60 20 60 20' CurveType='513' \
         FadePercent='500'/>"
    ))
    .await
    {
        assert!(
            notices[0].contains("2 doubled curves as single lines"),
            "{notices:?}"
        );
    }
}
