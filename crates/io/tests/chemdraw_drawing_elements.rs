//! Drawings saved by ChemDraw itself import whole: a style ReShiki cannot draw
//! yet takes the nearest one it can, and the import says so once.
use reshiki_io::{engine::LocalEngine, exchange, formats};
use reshiki_model::{
    arrows::{HeadShape, NoGo},
    document::{Document, Point},
    graphics::{Graphic, GraphicKind, LinePattern, PathCommand},
    palette::Color,
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
        // 10 ovals, 10 rectangles and 5 lines.
        assert_eq!(
            document.graphics.len() + document.arrows.len(),
            25,
            "{name}"
        );
    }
}

#[tokio::test]
async fn every_probe_sheet_opens_and_names_what_it_approximated() {
    let arrows = "2 block arrows as line arrows";
    let curves = "2 doubled curves as single lines";
    for (sheet, saved, source) in [
        ("p1-shapes-enum", String::new(), String::new()),
        (
            "p2-orbitals-enum",
            "3 shaded orbitals in flat grey".into(),
            "9 shaded orbitals in flat grey".into(),
        ),
        (
            "p3-symbols-enum",
            "3 stereo flags as text".into(),
            "3 stereo flags as text".into(),
        ),
        (
            "p4-brackets-enum",
            "3 single brackets as curves".into(),
            "3 single brackets as curves".into(),
        ),
        ("p5-arrows-enum", arrows.to_owned(), arrows.to_owned()),
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
                    // The control tells which way a curved arrow bows.
                    let bow = arrow
                        .control
                        .or(arrow.cubic.map(|[first, _]| first))
                        .unwrap_or(arrow.start);
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
async fn faded_graphics_and_block_arrows_import_plain() {
    // ReShiki's own binary writer has no fade, so this one is read as text.
    let faded = "<CDXML><page id='1'>\
         <graphic id='2' GraphicType='Rectangle' BoundingBox='10 10 60 40' FadePercent='50'/>\
         <graphic id='3' GraphicType='Rectangle' BoundingBox='10 50 60 80' FadePercent='300'/>\
         <graphic id='4' GraphicType='Rectangle' BoundingBox='10 90 60 120' alpha='0.5'/>\
         </page></CDXML>";
    let (document, notices) = open(faded.as_bytes().to_vec(), "cdxml").await.unwrap();
    // FadePercent only spreads a shaded fill; alpha is see-through.
    assert_eq!(notices, nearest("1 see-through object opaque"));
    assert_eq!(document.graphics.len(), 3);
    for (document, notices) in sheet(
        "<arrow id='4' Tail3D='10 100 0' Head3D='70 100 0' ArrowheadHead='Full' \
         ArrowheadType='Hollow' FillType='Solid' HeadSize='2000' ArrowheadCenterSize='1500' \
         ArrowheadWidth='600' ArrowShaftSpacing='800'/>\
         <arrow id='5' Tail3D='10 120 0' Head3D='70 120 0' ArrowheadHead='Full' \
         ArrowheadType='Solid' HeadSize='9000'/>",
    )
    .await
    {
        assert_eq!(
            notices,
            nearest(
                "1 block arrow as a line arrow; \
                 1 line, arrowhead or shadow size at the nearest size ReShiki draws"
            )
        );
        let block = document.arrows[0].style.clone().unwrap();
        assert_eq!(
            (block.shape, block.head_length_pt),
            (HeadShape::Hollow, 20.)
        );
        assert_eq!(
            document.arrows[1].style.clone().unwrap().head_length_pt,
            24.
        );
    }
}

#[tokio::test]
async fn arcs_and_arc_arrows_import_as_the_same_curve() {
    // ChemDraw's 90° arc arrow, its 270° arc arrow, its headless Arc tool
    // and an arc from an older file.
    for (document, notices) in sheet(
        "<arrow id='2' ArrowheadHead='Full' ArrowheadType='Solid' AngularSize='90' \
         Head3D='375.61 301.39 0' Tail3D='375.61 322.61 0' Center3D='365 312 0' \
         MajorAxisEnd3D='380 312 0' MinorAxisEnd3D='365 327 0'/>\
         <arrow id='3' ArrowheadHead='Full' ArrowheadType='Solid' AngularSize='270' \
         Head3D='75.61 364.61 0' Tail3D='75.61 343.39 0' Center3D='65 354 0' \
         MajorAxisEnd3D='80 354 0' MinorAxisEnd3D='65 369 0'/>\
         <arrow id='4' AngularSize='-180' Head3D='83 340 0' Tail3D='47 340 0' \
         Center3D='65 340 0' MajorAxisEnd3D='83 340 0' MinorAxisEnd3D='65 358 0'/>\
         <graphic id='5' GraphicType='Arc' BoundingBox='183 285 165 285' AngularSize='90'/>\
         <graphic id='6' GraphicType='Arc' BoundingBox='475.61 301.39 465 312' AngularSize='90' \
         ArrowType='FullHead'/>",
    )
    .await
    {
        assert_eq!(notices, Vec::<String>::new());
        // The arc arrow of an older file ends where ChemDraw's own arrow for
        // it does: both run up the right of their centres.
        let [modern, legacy] = document.arrows.as_slice() else {
            panic!("{:?}", document.arrows);
        };
        assert!((legacy.start.x - legacy.end.x).abs() < 0.01 && legacy.start.y > legacy.end.y);
        assert!((legacy.start.y - modern.start.y).abs() < 0.01);
        assert!((legacy.end.y - modern.end.y).abs() < 0.01);
        // One quarter turn is a curved arrow; longer arcs are paths.
        let arrow = &document.arrows[0];
        assert!(arrow.control.is_some() || arrow.cubic.is_some());
        assert!((arrow.start.x - arrow.end.x).abs() < 0.01);
        assert!(arrow.start.y > arrow.end.y);
        let [headed, half, quarter] = document.graphics.as_slice() else {
            panic!("{:?}", document.graphics);
        };
        assert!(
            [headed, half, quarter]
                .iter()
                .all(|graphic| graphic.kind == GraphicKind::Path)
        );
        assert!(headed.heads.is_some());
        // Three quarter turns, two and one.
        assert_eq!(
            [headed.path.len(), half.path.len(), quarter.path.len()],
            [4, 3, 2]
        );
        // The 270° arc runs the long way round, to the left of its ends.
        let (low, high) = extent(headed);
        assert!(high.x - low.x > 0.8 * (high.y - low.y));
        // The half turn rises above its ends, as ChemDraw draws it.
        let (low, high) = extent(half);
        let PathCommand::Move(start) = half.path[0] else {
            panic!("{:?}", half.path);
        };
        assert!((start.y - high.y).abs() < 0.01);
        assert!(((high.x - low.x) / (high.y - low.y) - 2.).abs() < 0.01);
    }
}

#[tokio::test]
async fn orbitals_take_chemdraws_names() {
    for (document, notices) in sheet(
        "<graphic id='2' GraphicType='Orbital' BoundingBox='20 40 20 10' OrbitalType='hybridPlus'/>\
         <graphic id='3' GraphicType='Orbital' BoundingBox='50 40 50 10' OrbitalType='hybridMinus'/>\
         <graphic id='4' GraphicType='Orbital' BoundingBox='80 40 80 10' OrbitalType='sFilled'/>\
         <graphic id='5' GraphicType='Orbital' BoundingBox='110 40 110 10' OrbitalType='lobeShaded'/>\
         <graphic id='6' GraphicType='Orbital' BoundingBox='140 40 140 10' OrbitalType='pFilled'/>\
         <graphic id='7' GraphicType='Orbital' BoundingBox='170 40 170 10' OrbitalType='dz2MinusFilled'/>",
    )
    .await
    {
        assert_eq!(notices, nearest("1 shaded orbital in flat grey"));
        let read: Vec<_> = document
            .graphics
            .iter()
            .map(|graphic| (graphic.kind, graphic.phase, graphic.phase_flipped))
            .collect();
        use GraphicKind::Orbital;
        assert_eq!(
            read,
            [
                (Orbital(OrbitalKind::Hybrid), Phase::Open, true),
                (Orbital(OrbitalKind::Hybrid), Phase::Open, false),
                (Orbital(OrbitalKind::S), Phase::Solid, false),
                (Orbital(OrbitalKind::Lobe), Phase::Shaded, false),
                (Orbital(OrbitalKind::P), Phase::Solid, false),
                (Orbital(OrbitalKind::Dz2), Phase::Solid, false),
            ]
        );
    }
}

#[tokio::test]
async fn daggers_and_the_bar_are_symbols_and_stereo_flags_are_text() {
    for (document, notices) in sheet(
        "<graphic id='2' GraphicType='Symbol' BoundingBox='55 105 49 99' SymbolType='Dagger'/>\
         <graphic id='3' GraphicType='Symbol' BoundingBox='85 105 79 99' SymbolType='DoubleDagger'/>\
         <graphic id='4' GraphicType='Symbol' BoundingBox='115 105 109 99' SymbolType='LonePairBar'/>\
         <graphic id='5' GraphicType='Symbol' BoundingBox='395 105 389 99' SymbolType='Racemic'/>\
         <graphic id='6' GraphicType='Symbol' BoundingBox='425 105 419 99' SymbolType='Absolute'/>\
         <graphic id='7' GraphicType='Symbol' BoundingBox='455 105 449 99' SymbolType='Relative'/>",
    )
    .await
    {
        assert_eq!(notices, nearest("3 stereo flags as text"));
        let kinds: Vec<_> = document.graphics.iter().map(|g| g.kind).collect();
        use GraphicKind::Symbol;
        assert_eq!(
            kinds,
            [
                Symbol(SymbolKind::Dagger),
                Symbol(SymbolKind::DoubleDagger),
                Symbol(SymbolKind::LonePairBar)
            ]
        );
        let mut flags: Vec<_> = document.annotations.iter().map(|a| a.text.as_str()).collect();
        flags.sort_unstable();
        assert_eq!(flags, ["Abs", "Rac", "Rel"]);
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
        assert_eq!(notices, nearest("1 doubled curve as a single line"));
        // The headed curve is an arrow; the two without a segment draw nothing.
        assert_eq!(document.arrows.len(), 1);
        let [closed, doubled, dashed, padded] = document.graphics.as_slice() else {
            panic!("{:?}", document.graphics);
        };
        assert!(closed.style.effects.shaded && dashed.style.effects.shaded);
        assert_eq!(doubled.style.fill, None);
        assert_eq!(dashed.style.pattern, LinePattern::Dashed);
        assert_eq!(padded.path, doubled.path);
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
    assert_eq!(kinds, [GraphicKind::Ellipse, GraphicKind::Path]);
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
async fn captions_with_unknown_settings_import_upright_and_plain() {
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
                "1 rotated caption upright; 1 outlined or shadowed text in the plain face; \
                 2 text sizes at the nearest size ReShiki draws; \
                 1 atom label with mixed text styles in the style of its first character; \
                 2 captions with automatic line spacing or width"
            )
        );
        assert_eq!(document.annotations.len(), 5);
        let outlined = &document.annotations[1].format.style;
        assert!(outlined.bold);
        assert_eq!(document.annotations[2].format.style.size_pt, 4.);
        assert_eq!(document.atoms.len(), 2);
    }
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
        0x01, 0x03, 0x02, 0x00, 0x04, 0x00, 0x03, 0x03, 0x02, 0x00, 0x2c, 0x01,
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
