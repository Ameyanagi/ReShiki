//! Opt-in pre-change capture and verification of the reshiki-io import stages:
//! abbreviation flattening, CDXML preparation and scene assembly, and V3000
//! MOL and RXN reading. Payloads use platform libm trigonometry and depend on
//! the optimization profile, so capture and verify on one machine with the
//! same toolchain and the default test profile. Never commit a baseline.
use anyhow::{Context, bail, ensure};
use reshiki::{
    chemistry::{cdxml, molfile, reaction},
    exchange,
};
use serde_json::{Value, json};
use std::{
    fmt::Write as _,
    io::Write as _,
    ops::Range,
    path::{Path, PathBuf},
};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// Seed from reference/cdxml_abbreviations.rs (OMe with one connection point).
const OME: &str = "<CDXML BondLength='14.4'><page><fragment id='1'><n id='2' p='0 0'/><n id='3' p='14.4 0' NodeType='Fragment'><fragment id='4'><n id='5' p='0 0' Element='8'/><n id='6' p='14.4 0'/><n id='7' p='-14.4 0' NodeType='ExternalConnectionPoint'/><b id='8' B='7' E='5'/><b id='9' B='5' E='6'/></fragment><t><s>Me</s><s>O</s></t></n><b id='10' B='2' E='3'/></fragment></page></CDXML>";

/// Mirror-symmetric two-connection seed from tests/internal_groups.rs.
const TWO_CONNECTIONS: &str = r#"<CDXML><page><fragment id="1">
<n id="2" p="0 0"/><n id="3" p="40 0"/>
<n id="4" p="20 -10" NodeType="Fragment" BondOrdering="5 6">
<fragment id="10" ConnectionOrder="11 12">
<n id="11" p="0 0" NodeType="ExternalConnectionPoint"/>
<n id="12" p="40 0" NodeType="ExternalConnectionPoint"/>
<n id="13" p="20 -10"/><n id="14" p="20 -30" Element="17"/>
<n id="15" p="30 -20" Element="17"/>
<b id="16" B="11" E="13"/><b id="17" B="12" E="13"/>
<b id="18" B="13" E="14"/><b id="19" B="13" E="15"/>
</fragment><t><s>CCl2</s></t></n>
<b id="5" B="2" E="4"/><b id="6" B="4" E="3"/>
</fragment></page></CDXML>"#;

/// Asymmetric two-connection seed: its two placement pairs differ in scale,
/// and the mixed orderings rotate the group by +/-pi.
const ASYMMETRIC_TWO_CONNECTIONS: &str = r"<CDXML><page><fragment id='1'><n id='2' p='-20 0'/><n id='3' p='10 0'/><n id='4' p='0 0' NodeType='Fragment' BondOrdering='5 6'><fragment id='10' ConnectionOrder='11 12'><n id='11' p='-10 0' NodeType='ExternalConnectionPoint'/><n id='12' p='10 0' NodeType='ExternalConnectionPoint'/><n id='13' p='0 0'/><n id='14' p='0 -10' Element='17'/><b id='16' B='11' E='13'/><b id='17' B='12' E='13'/><b id='18' B='13' E='14'/></fragment><t><s>CCl</s></t></n><b id='5' B='2' E='4'/><b id='6' B='4' E='3'/></fragment></page></CDXML>";

/// Attributes mutated inside abbreviation wrappers and their attachment bonds.
const MUTATED_ATTRIBUTES: [&str; 9] = [
    "p",
    "id",
    "B",
    "E",
    "Order",
    "BondOrdering",
    "ConnectionOrder",
    "NodeType",
    "highlightColor",
];
const ATTRIBUTE_VALUES: [&str; 4] = ["", "x", "nan 0", "1e400 0"];
const RECORD_TOKENS: [&str; 9] = ["", "x", "-1", "0", "2", "100001", "(1 1)", "A=B=C", "\""];
const V30: &str = "M  V30 ";

/// One baseline file: a tab-separated `case, stage, payload` line per stage.
#[derive(Default)]
struct Output {
    text: String,
    cases: usize,
}

impl Output {
    fn line(&mut self, case: &str, stage: &str, payload: &Value) -> anyhow::Result<()> {
        ensure!(!case.contains(['\t', '\n']), "Unsafe case name {case:?}");
        let payload = serde_json::to_string(payload)?;
        writeln!(self.text, "{case}\t{stage}\t{payload}")?;
        Ok(())
    }

    fn flatten(&mut self, case: &str, text: &str) -> anyhow::Result<()> {
        let payload = match cdxml::flatten_abbreviations(text) {
            // The detached presentation is serde(skip); its attributes are a
            // BTreeMap, so the Debug text is ordered.
            Ok(flat) => json!({
                "ok": serde_json::to_value(&flat)?,
                "presentations": flat
                    .abbreviations
                    .iter()
                    .map(|abbreviation| format!("{:?}", abbreviation.presentation))
                    .collect::<Vec<_>>(),
            }),
            Err(error) => json!({ "err": error.to_string() }),
        };
        self.line(case, "flatten_abbreviations", &payload)
    }

    fn scene(&mut self, case: &str, text: &str) -> anyhow::Result<()> {
        let payload = match cdxml::prepare_cdxml(text) {
            Ok(prepared) => match cdxml::assemble_cdxml(&prepared) {
                Ok(scene) => json!({ "ok": serde_json::to_value(&scene)? }),
                Err(error) => json!({ "scene_err": error.to_string() }),
            },
            Err(error) => json!({ "preparation_err": error.to_string() }),
        };
        self.line(case, "prepare_assemble", &payload)
    }

    fn cdxml(&mut self, case: &str, text: &str) -> anyhow::Result<()> {
        self.cases += 1;
        self.flatten(case, text)?;
        self.scene(case, text)
    }

    fn molfile(&mut self, case: &str, text: &str) -> anyhow::Result<()> {
        self.cases += 1;
        let payload = match molfile::read(text) {
            Ok(imported) => json!({
                "ok": serde_json::to_value(&imported)?,
                "chemdraw_directions": directions(&imported),
            }),
            Err(error) => json!({ "err": error.to_string() }),
        };
        self.line(case, "molfile_read", &payload)
    }

    fn rxn(&mut self, case: &str, text: &str) -> anyhow::Result<()> {
        self.cases += 1;
        let payload = match reaction::read_rxn(text) {
            Ok(imported) => {
                let roles = [&imported.reactants, &imported.products, &imported.agents]
                    .map(|parts| parts.iter().map(directions).collect::<Vec<_>>());
                json!({
                    "ok": serde_json::to_value(&imported)?,
                    "chemdraw_directions": roles,
                })
            }
            Err(error) => json!({ "err": error.to_string() }),
        };
        self.line(case, "read_rxn", &payload)
    }
}

/// The serde(skip) ChemDraw directions retained for Haworth recognition.
fn directions(imported: &molfile::Imported) -> String {
    format!("{:?}", imported.annotations.chemdraw_directions)
}

fn fixtures(directory: &Path, extension: &str, found: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            fixtures(&path, extension, found)?;
        } else if path.extension().is_some_and(|e| e == extension) {
            found.push(path);
        }
    }
    Ok(())
}

/// Every fixture with the extension, sorted by path for a deterministic order.
fn sorted_fixtures(extension: &str) -> anyhow::Result<Vec<(String, PathBuf)>> {
    let root = Path::new(ROOT);
    let mut found = Vec::new();
    fixtures(&root.join("tests/fixtures"), extension, &mut found)?;
    let mut named = found
        .into_iter()
        .map(|path| {
            let name = path
                .strip_prefix(root)?
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            Ok((name, path))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    named.sort();
    Ok(named)
}

fn cdxml_corpus() -> anyhow::Result<Output> {
    let mut out = Output::default();
    for (name, path) in sorted_fixtures("cdxml")? {
        out.cdxml(&name, &std::fs::read_to_string(&path)?)?;
    }
    for (name, path) in sorted_fixtures("cdx")? {
        match exchange::from_cdx(&std::fs::read(&path)?) {
            Ok(xml) => out.cdxml(&name, &xml)?,
            Err(error) => {
                out.cases += 1;
                out.line(&name, "from_cdx", &json!({ "err": error.to_string() }))?;
            }
        }
    }
    Ok(out)
}

fn many_abbreviations(count: usize) -> anyhow::Result<String> {
    let mut text = String::from("<CDXML><page><fragment id='1'>");
    for i in 0..count {
        let outer = 100 + 3 * i;
        let inner = outer + 1;
        let atom = outer + 2;
        write!(
            text,
            "<n id='{outer}' p='{i} 0' NodeType='Fragment'><fragment id='{inner}'><n id='{atom}' p='0 0'/></fragment><t><s>Me</s></t></n>"
        )?;
    }
    text.push_str("</fragment></page></CDXML>");
    Ok(text)
}

fn replace_once(text: &str, from: &str, to: &str) -> anyhow::Result<String> {
    ensure!(text.matches(from).count() == 1, "Expected one {from}");
    Ok(text.replace(from, to))
}

/// Both orderings of BondOrdering against the written, reversed and absent
/// ConnectionOrder: as written, both reversed, each reversed alone, and each
/// BondOrdering without a ConnectionOrder.
fn ordering_variants(
    name: &str,
    source: &str,
    quote: char,
) -> anyhow::Result<Vec<(String, String)>> {
    let bond_ordering = format!("BondOrdering={quote}5 6{quote}");
    let connection_order = format!(" ConnectionOrder={quote}11 12{quote}");
    let mut variants = Vec::new();
    for (bond_name, bonds) in [("bonds_written", "5 6"), ("bonds_reversed", "6 5")] {
        let text = replace_once(
            source,
            &bond_ordering,
            &format!("BondOrdering={quote}{bonds}{quote}"),
        )?;
        for (connection_name, connections) in [
            ("connections_written", Some("11 12")),
            ("connections_reversed", Some("12 11")),
            ("connections_removed", None),
        ] {
            let replacement = connections
                .map(|order| format!(" ConnectionOrder={quote}{order}{quote}"))
                .unwrap_or_default();
            variants.push((
                format!("seed/{name}/{bond_name}/{connection_name}"),
                replace_once(&text, &connection_order, &replacement)?,
            ));
        }
    }
    Ok(variants)
}

fn is_wrapper(node: &roxmltree::Node<'_, '_>) -> bool {
    node.has_tag_name("n") && matches!(node.attribute("NodeType"), Some("Fragment" | "Nickname"))
}

/// Value replacements and removals of each listed attribute on a wrapper,
/// anything inside a wrapper, or a bond attached to a wrapper.
fn attribute_mutations(name: &str, text: &str) -> anyhow::Result<Vec<(String, String)>> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };
    let document = roxmltree::Document::parse_with_options(text, options)
        .with_context(|| format!("Parsing {name}"))?;
    let wrappers = document
        .descendants()
        .filter(is_wrapper)
        .collect::<Vec<_>>();
    let ids = wrappers
        .iter()
        .filter_map(|wrapper| wrapper.attribute("id"))
        .collect::<Vec<_>>();
    let mut ranges = wrappers
        .iter()
        .map(roxmltree::Node::range)
        .collect::<Vec<Range<usize>>>();
    ranges.extend(
        document
            .descendants()
            .filter(|node| {
                node.has_tag_name("b")
                    && [node.attribute("B"), node.attribute("E")]
                        .into_iter()
                        .flatten()
                        .any(|end| ids.contains(&end))
            })
            .map(|node| node.range()),
    );
    let mut variants = Vec::new();
    for attribute in document.descendants().flat_map(|node| node.attributes()) {
        let range = attribute.range();
        if !MUTATED_ATTRIBUTES.contains(&attribute.name())
            || !ranges
                .iter()
                .any(|outer| outer.start <= range.start && range.end <= outer.end)
        {
            continue;
        }
        let value = attribute.range_value();
        let case = format!("{name}/@{}/{}", range.start, attribute.name());
        for (i, replacement) in ATTRIBUTE_VALUES.into_iter().enumerate() {
            variants.push((
                format!("{case}/value{i}"),
                format!(
                    "{}{replacement}{}",
                    &text[..value.start],
                    &text[value.end..]
                ),
            ));
        }
        variants.push((
            format!("{case}/removed"),
            format!("{}{}", &text[..range.start], &text[range.end..]),
        ));
    }
    Ok(variants)
}

fn has_wrapper(text: &str) -> bool {
    ['"', '\''].into_iter().any(|quote| {
        ["Fragment", "Nickname"]
            .into_iter()
            .any(|kind| text.contains(&format!("NodeType={quote}{kind}{quote}")))
    })
}

fn abbreviation_corpus() -> anyhow::Result<Output> {
    let mut out = Output::default();
    let mut seeds = vec![("seed/ome".to_owned(), OME.to_owned())];
    seeds.extend(ordering_variants("two_connections", TWO_CONNECTIONS, '"')?);
    seeds.extend(ordering_variants(
        "asymmetric_two_connections",
        ASYMMETRIC_TWO_CONNECTIONS,
        '\'',
    )?);
    seeds.push((
        "seed/many_abbreviations_3".to_owned(),
        many_abbreviations(3)?,
    ));
    for (name, text) in &seeds {
        out.cdxml(name, text)?;
        for (case, variant) in attribute_mutations(name, text)? {
            out.cdxml(&case, &variant)?;
        }
    }
    // Mutated fixtures run through flattening only, bounding the runtime.
    for (name, path) in sorted_fixtures("cdxml")? {
        let text = std::fs::read_to_string(&path)?;
        if !has_wrapper(&text) {
            continue;
        }
        out.cdxml(&name, &text)?;
        for (case, variant) in attribute_mutations(&name, &text)? {
            out.cases += 1;
            out.flatten(&case, &variant)?;
        }
    }
    Ok(out)
}

/// A V3000 MOL file around `M  V30` records.
fn mol(info: &str, records: &[String]) -> String {
    let mut text = format!("\n{info}\n\n  0  0  0  0  0  0  0  0  0  0999 V3000\n");
    for record in records {
        text.push_str(V30);
        text.push_str(record);
        text.push('\n');
    }
    text.push_str("M  END\n");
    text
}

fn records(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| (*line).to_owned()).collect()
}

/// One atom carrying an atom property.
fn atom_property(property: &str) -> Vec<String> {
    let mut lines = records(&["BEGIN CTAB", "COUNTS 1 0 0 0 0", "BEGIN ATOM"]);
    lines.push(format!("1 C 0 0 0 0 {property}"));
    lines.extend(records(&["END ATOM", "END CTAB"]));
    lines
}

/// One bond carrying bond properties, between a dummy R atom and a metal.
fn bond_property(order: u8, property: &str) -> Vec<String> {
    let mut lines = records(&[
        "BEGIN CTAB",
        "COUNTS 4 2 0 0 0",
        "BEGIN ATOM",
        "1 C 0 0 0 0",
        "2 C 1.4 0 0 0",
        "3 R 0.7 1 0 0",
        "4 Pt 0.7 2.5 0 0",
        "END ATOM",
        "BEGIN BOND",
        "1 2 1 2",
    ]);
    lines.push(format!("2 {order} 3 4 {property}"));
    lines.extend(records(&["END BOND", "END CTAB"]));
    lines
}

/// A wedged stereocenter followed by the blocks after the bond table.
fn chiral_center(counts: &str, tail: &[&str]) -> Vec<String> {
    let mut lines = records(&["BEGIN CTAB", counts]);
    lines.extend(records(&[
        "BEGIN ATOM",
        "1 C 0 0 0 0",
        "2 F 0 1.5 0 0",
        "3 Cl 1.3 -0.75 0 0",
        "4 Br -1.3 -0.75 0 0",
        "5 O 0 -1.5 0 0",
        "END ATOM",
        "BEGIN BOND",
        "1 1 1 2 CFG=1",
        "2 1 1 3",
        "3 1 1 4",
        "4 1 1 5",
        "END BOND",
    ]));
    lines.extend(records(tail));
    lines.push("END CTAB".to_owned());
    lines
}

const INFO: &str = "  ReShiki           2D";
const CHEMDRAW_INFO: &str = "  ChemDraw          2D";

fn molecule_blocks() -> Vec<(String, String)> {
    let mut blocks = vec![
        (
            "atom_and_bond_properties".to_owned(),
            mol(
                CHEMDRAW_INFO,
                &records(&[
                    "BEGIN CTAB",
                    "COUNTS 6 5 0 0 0",
                    "BEGIN ATOM",
                    "1 C 0 0 0 1 CHG=0 RAD=0 MASS=13 VAL=0 CFG=0 HCOUNT=0 RBCNT=0 SUBST=0 UNSAT=0 STBOX=0 EXACHG=0 INVRET=0 SEQID=1 CLASS=X",
                    "2 C 1.5 0 0 0 MASS=12.5 RGROUPS=(0) ATTCHORD=1",
                    "3 N 2.25 1.299 0 2 CHG=1 VAL=4",
                    "4 C 3.75 1.299 0 0 RAD=2",
                    "5 O -0.75 1.299 0 0 CHG=-1",
                    "6 C -0.75 -1.299 0 0 ATTCHPT=1 ATTCHORD=(2 2 Al)",
                    "END ATOM",
                    "BEGIN BOND",
                    "1 1 1 2 CFG=1 RXCTR=0 TOPO=0 STBOX=0",
                    "2 1 2 3 CFG=3",
                    "3 1 3 4 CFG=2",
                    "4 1 1 5 CFG=0",
                    "5 2 1 6 CFG=2",
                    "END BOND",
                    "END CTAB",
                ]),
            ),
        ),
        (
            "multicenter_and_variable_attachments".to_owned(),
            mol(
                INFO,
                &records(&[
                    "BEGIN CTAB",
                    "COUNTS 6 3 0 0 0",
                    "BEGIN ATOM",
                    "1 C 0 0 0 0",
                    "2 C 1.4 0 0 0",
                    "3 R 0.7 1 0 0",
                    "4 Pt 0.7 2.5 0 0",
                    "5 R 3 0 0 0",
                    "6 Cl 4.5 0 0 0",
                    "END ATOM",
                    "BEGIN BOND",
                    "1 2 1 2",
                    "2 9 3 4 CFG=2 ENDPTS=(2 1 2) ATTACH=ALL",
                    "3 1 5 6 ATTACH=ANY ENDPTS=(2 1 2)",
                    "END BOND",
                    "END CTAB",
                ]),
            ),
        ),
        (
            "linknode_collection_sgroup_obj3d_unknown".to_owned(),
            mol(
                INFO,
                &chiral_center(
                    "COUNTS 5 4 1 1 0",
                    &[
                        "LINKNODE 1 3 2 1 2 1 5",
                        "BEGIN COLLECTION",
                        "MDLV30/HILITE ATOMS=(1 2)",
                        "END COLLECTION",
                        "BEGIN SGROUP",
                        "1 DAT 0 ATOMS=(1 5) FIELDNAME=note FIELDDATA=hydroxy",
                        "END SGROUP",
                        "BEGIN OBJ3D",
                        "1 POINT 0 0 0",
                        "END OBJ3D",
                        "BEGIN RGROUP 1",
                        "RLOGIC 0 0 \"\"",
                        "END RGROUP",
                    ],
                ),
            ),
        ),
        (
            // Stereo groups parse, then the finished molecule rejects them.
            "stereo_collection".to_owned(),
            mol(
                INFO,
                &chiral_center(
                    "COUNTS 5 4 0 0 0",
                    &[
                        "BEGIN COLLECTION",
                        "MDLV30/STEABS ATOMS=(1 1)",
                        "MDLV30/STEREL1 ATOMS=(1 1)",
                        "END COLLECTION",
                    ],
                ),
            ),
        ),
    ];
    for (name, property) in [
        ("atom_charge_excessive", "CHG=200"),
        ("atom_radical_unsupported", "RAD=4"),
        ("atom_mass_invalid", "MASS=-1"),
        ("atom_cfg_invalid", "CFG=4"),
        ("atom_hydrogen_query", "HCOUNT=2"),
        ("atom_ring_bond_query", "RBCNT=2"),
        ("atom_substitution_query", "SUBST=2"),
        ("atom_unsaturation_query", "UNSAT=1"),
        ("atom_rgroups_unsupported", "RGROUPS=(1 1)"),
        ("atom_rgroups_missing", "RGROUPS=(2 1)"),
        ("atom_rgroups_unbracketed", "RGROUPS=1"),
        ("atom_attachment_duplicate", "ATTCHPT=1 ATTCHPT=2"),
        ("atom_template_order_odd", "ATTCHORD=(1 1)"),
        ("atom_template_order_duplicate", "ATTCHORD=(4 1 Al 1 Br)"),
    ] {
        blocks.push((name.to_owned(), mol(INFO, &atom_property(property))));
    }
    for (name, order, property) in [
        ("bond_cfg_invalid", 1, "CFG=4"),
        ("bond_topology_query", 1, "TOPO=1"),
        (
            "bond_endpoints_duplicate",
            9,
            "ENDPTS=(2 1 2) ENDPTS=(2 1 2) ATTACH=ALL",
        ),
        (
            "bond_attach_duplicate",
            9,
            "ENDPTS=(2 1 2) ATTACH=ALL ATTACH=ANY",
        ),
        ("bond_attach_unknown", 9, "ENDPTS=(2 1 2) ATTACH=SOME"),
        ("bond_endpoints_without_attach", 9, "ENDPTS=(2 1 2)"),
        ("bond_attach_without_endpoints", 9, "ATTACH=ALL"),
        ("bond_endpoints_overflow", 9, "ENDPTS=(2 1 2 1) ATTACH=ALL"),
        ("bond_endpoints_unbracketed", 9, "ENDPTS=2 ATTACH=ALL"),
        ("bond_endpoints_count", 9, "ENDPTS=(1 1) ATTACH=ALL"),
    ] {
        blocks.push((name.to_owned(), mol(INFO, &bond_property(order, property))));
    }
    blocks.push((
        "bond_attach_without_dummy".to_owned(),
        mol(
            INFO,
            &records(&[
                "BEGIN CTAB",
                "COUNTS 4 2 0 0 0",
                "BEGIN ATOM",
                "1 C 0 0 0 0",
                "2 C 1.4 0 0 0",
                "3 C 0.7 1 0 0",
                "4 Pt 0.7 2.5 0 0",
                "END ATOM",
                "BEGIN BOND",
                "1 2 1 2",
                "2 9 3 4 ENDPTS=(2 1 2) ATTACH=ALL",
                "END BOND",
                "END CTAB",
            ]),
        ),
    ));
    blocks
}

fn reaction_block() -> String {
    let participant = |order: u8| {
        [
            "BEGIN CTAB".to_owned(),
            "COUNTS 2 1 0 0 0".to_owned(),
            "BEGIN ATOM".to_owned(),
            "1 C 0 0 0 1".to_owned(),
            "2 O 1.5 0 0 2".to_owned(),
            "END ATOM".to_owned(),
            "BEGIN BOND".to_owned(),
            format!("1 {order} 1 2"),
            "END BOND".to_owned(),
            "END CTAB".to_owned(),
        ]
    };
    let mut text = String::from("$RXN V3000\n\n  ReShiki\n\n");
    let mut lines = vec!["COUNTS 1 1".to_owned(), "BEGIN REACTANT".to_owned()];
    lines.extend(participant(1));
    lines.extend(records(&["END REACTANT", "BEGIN PRODUCT"]));
    lines.extend(participant(2));
    lines.push("END PRODUCT".to_owned());
    for line in lines {
        text.push_str(V30);
        text.push_str(&line);
        text.push('\n');
    }
    text.push_str("M  END\n");
    text
}

/// For every `M  V30` record: drop it, duplicate it, lowercase its content,
/// and replace each of its whitespace tokens with each listed token.
fn record_mutations(name: &str, text: &str) -> Vec<(String, String)> {
    let lines = text.lines().collect::<Vec<_>>();
    let rebuild = |index: usize, replacement: &[&str]| {
        let mut output = String::new();
        for (i, line) in lines.iter().enumerate() {
            let current: &[&str] = if i == index {
                replacement
            } else {
                std::slice::from_ref(line)
            };
            for line in current {
                output.push_str(line);
                output.push('\n');
            }
        }
        output
    };
    let mut variants = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(content) = line.strip_prefix(V30) else {
            continue;
        };
        let case = format!("{name}/L{}", index + 1);
        variants.push((format!("{case}/drop"), rebuild(index, &[])));
        variants.push((format!("{case}/duplicate"), rebuild(index, &[line, line])));
        let lowercase = format!("{V30}{}", content.to_ascii_lowercase());
        variants.push((format!("{case}/lowercase"), rebuild(index, &[&lowercase])));
        let tokens = content.split_whitespace().collect::<Vec<_>>();
        for position in 0..tokens.len() {
            for (i, token) in RECORD_TOKENS.into_iter().enumerate() {
                let mut replaced = tokens.clone();
                replaced[position] = token;
                let record = format!("{V30}{}", replaced.join(" "));
                variants.push((
                    format!("{case}/token{position}/value{i}"),
                    rebuild(index, &[&record]),
                ));
            }
        }
    }
    variants
}

fn v3000_corpus() -> anyhow::Result<Output> {
    let mut out = Output::default();
    for (name, text) in molecule_blocks() {
        out.molfile(&name, &text)?;
        for (case, variant) in record_mutations(&name, &text) {
            out.molfile(&case, &variant)?;
        }
    }
    Ok(out)
}

fn rxn_corpus() -> anyhow::Result<Output> {
    let mut out = Output::default();
    let text = reaction_block();
    out.rxn("rxn_two_participants", &text)?;
    for (case, variant) in record_mutations("rxn_two_participants", &text) {
        out.rxn(&case, &variant)?;
    }
    Ok(out)
}

/// The first line that differs, with its case name, or None when equal.
fn first_difference<'a>(
    expected: &'a str,
    actual: &'a str,
) -> Option<(usize, Option<&'a str>, Option<&'a str>)> {
    let (mut expected, mut actual) = (expected.lines(), actual.lines());
    let mut line = 0;
    loop {
        line += 1;
        match (expected.next(), actual.next()) {
            (None, None) => return None,
            (left, right) if left != right => return Some((line, left, right)),
            _ => (),
        }
    }
}

fn check(mode: &str, directory: &Path, group: &str, output: &Output) -> anyhow::Result<()> {
    let path = directory.join(format!("{group}.tsv"));
    if mode == "capture" {
        // Refuse to replace the original output during a later run.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| format!("Creating {}", path.display()))?;
        file.write_all(output.text.as_bytes())?;
        return Ok(());
    }
    let expected = std::fs::read(&path).with_context(|| format!("Reading {}", path.display()))?;
    if expected == output.text.as_bytes() {
        return Ok(());
    }
    let expected = String::from_utf8(expected)?;
    let (line, left, right) = first_difference(&expected, &output.text)
        .context("Baselines differ only in their final newline")?;
    let case = left
        .or(right)
        .and_then(|text| text.split('\t').next())
        .unwrap_or_default();
    let artifacts = Path::new(ROOT).join("artifacts/io-stage");
    std::fs::create_dir_all(&artifacts)?;
    std::fs::write(
        artifacts.join(format!("{group}-expected.tsv")),
        left.unwrap_or_default(),
    )?;
    std::fs::write(
        artifacts.join(format!("{group}-actual.tsv")),
        right.unwrap_or_default(),
    )?;
    bail!(
        "{group} differs at line {line}, case {case}; see {}",
        artifacts.display()
    )
}

#[test]
#[ignore = "Pre-change io stage capture/verification; set mode and baseline directory explicitly"]
fn io_stages_match_prechange_baseline() -> anyhow::Result<()> {
    let mode = std::env::var("RESHIKI_IO_STAGE_MODE")
        .context("Set RESHIKI_IO_STAGE_MODE=capture or verify")?;
    ensure!(matches!(mode.as_str(), "capture" | "verify"));
    let directory = PathBuf::from(
        std::env::var_os("RESHIKI_IO_STAGE_BASELINE_DIR")
            .context("Set RESHIKI_IO_STAGE_BASELINE_DIR")?,
    );
    if mode == "capture" {
        std::fs::create_dir_all(&directory)?;
    }
    let groups = [
        ("cdxml", cdxml_corpus()?),
        ("abbreviations", abbreviation_corpus()?),
        ("v3000", v3000_corpus()?),
        ("rxn", rxn_corpus()?),
    ];
    let mut failures = Vec::new();
    for (group, output) in &groups {
        if let Err(error) = check(&mode, &directory, group, output) {
            failures.push(format!("{error:#}"));
        }
    }
    let cases = groups.iter().map(|(_, output)| output.cases).sum::<usize>();
    println!("IO_STAGE_BASELINE,{mode},{},{cases}", directory.display());
    ensure!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}
