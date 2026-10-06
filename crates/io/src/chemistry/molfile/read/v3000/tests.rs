//! Pins the V3000 CTAB reader's exact diagnostics, the line each one reports
//! and which fault wins when one record holds several. Every expectation was
//! observed on the reader before its stage split.
use super::*;
use crate::attachments::Kind;

/// Lines 1-4 of the header in reference/molfile_import.rs: zero counts, V3000.
const MOL_HEADER: &str = "\n                    2D\n\n  0  0  0  0  0  0  0  0  0  0999 V3000\n";

/// Atoms 1 and 5 are dummy `R` atoms; atoms 2-4 are carbons.
const ATOMS: [&str; 5] = [
    "1 R 0 0 0 0",
    "2 C 1 0 0 0",
    "3 C 2 0 0 0",
    "4 C 3 0 0 0",
    "5 R 4 0 0 0",
];

/// `M  V30 BEGIN CTAB` (line 1), then each record from line 2, all prefixed.
fn ctab_text(records: &[&str]) -> String {
    std::iter::once("BEGIN CTAB")
        .chain(records.iter().copied())
        .map(|record| format!("M  V30 {record}\n"))
        .collect()
}

fn read_text(text: &str, expect_end: bool) -> Result<Parsed> {
    let mut r = Reader {
        lines: text.lines(),
        line: 0,
    };
    let mut p = Parsed::new(false);
    super::read(&mut r, &mut p, expect_end)?;
    Ok(p)
}

fn ctab(records: &[&str], expect_end: bool) -> Result<Parsed> {
    read_text(&ctab_text(records), expect_end)
}

/// The exact Display of the error, or `ok`.
fn outcome<T>(result: Result<T>) -> String {
    result.map_or_else(|error| error.to_string(), |_| "ok".to_owned())
}

/// The whole-file reader: the reference header, the CTAB and `M  END`.
fn mol(records: &[&str]) -> String {
    let text = format!("{MOL_HEADER}{}M  END\n", ctab_text(records));
    outcome(crate::chemistry::molfile::read(&text))
}

/// `ATOMS` at lines 4-8 after `counts`, then `tail` from line 10.
fn after_atoms(counts: &str, tail: &[&str]) -> Result<Parsed> {
    let mut records = vec![counts, "BEGIN ATOM"];
    records.extend(ATOMS);
    records.push("END ATOM");
    records.extend(tail);
    ctab(&records, false)
}

/// `ATOMS`, then the bond records from line 11.
fn bonds(records: &[&str]) -> Result<Parsed> {
    let counts = format!("COUNTS 5 {}", records.len());
    after_atoms(
        &counts,
        &[&["BEGIN BOND"], records, &["END BOND", "END CTAB"]].concat(),
    )
}

/// Three carbons, then the collection records from line 9.
fn stereo_groups(records: &[&str]) -> String {
    let atoms = ["COUNTS 3 0", "BEGIN ATOM", "1 C 0 0 0 0", "2 C 1 0 0 0"];
    let block = ["3 C 2 0 0 0", "END ATOM", "BEGIN COLLECTION"];
    let end = ["END COLLECTION", "END CTAB"];
    match ctab(&[&atoms[..], &block, records, &end].concat(), false) {
        Ok(p) => format!("{:?}", p.metadata.groups),
        Err(error) => error.to_string(),
    }
}

/// Run every `records => expected` row (records split on ` | `, `//` rows
/// are comments) and report all mismatches at once.
#[track_caller]
fn check(table: &str, run: impl Fn(&[&str]) -> String) {
    let failures: Vec<_> = table
        .lines()
        .map(str::trim)
        .filter(|row| !row.is_empty() && !row.starts_with("//"))
        .filter_map(|row| {
            let (records, expected) = row.split_once(" => ").expect("row without ` => `");
            let records: Vec<_> = records.trim_end().split(" | ").collect();
            let actual = run(&records);
            (actual != expected).then(|| format!("{row}\n    actual {actual}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[track_caller]
fn check_ctab(table: &str) {
    check(table, |records| outcome(ctab(records, false)));
}

/// One atom record at line 4 (a continuation adds line 5).
#[track_caller]
fn check_atom(table: &str) {
    check(table, |records| {
        let framed = [
            &["COUNTS 1 0", "BEGIN ATOM"],
            records,
            &["END ATOM", "END CTAB"],
        ];
        outcome(ctab(&framed.concat(), false))
    });
}

/// Bond records from line 11.
#[track_caller]
fn check_bond(table: &str) {
    check(table, |records| outcome(bonds(records)));
}

#[test]
fn ctab_header_and_counts() {
    for (text, expected) in [
        ("", "Invalid MOL input at line 1: Unexpected end of file"),
        (
            "M V30 BEGIN CTAB\n",
            "Invalid MOL input at line 1: Missing V3000 line prefix",
        ),
        (
            "M  V30 BEGIN ATOM\n",
            "Invalid MOL input at line 1: Missing BEGIN CTAB",
        ),
        (
            "M  V30 begin ctab\nM  V30 counts 0 0\nM  V30 end ctab\n",
            "ok",
        ),
        (
            "M  V30 BEGIN CTAB\n",
            "Invalid MOL input at line 2: Unexpected end of file",
        ),
        (
            "M  V30 BEGIN CTAB\nM  V30 \n",
            "Invalid MOL input at line 2: Missing V3000 field",
        ),
    ] {
        assert_eq!(outcome(read_text(text, false)), expected, "{text:?}");
    }
    // 64 nested lists are accepted; the 65th level hits the limit before the
    // keyword check.
    let nested = format!("COUNTS 0 0 0 0 X={}{}", "(".repeat(64), ")".repeat(64));
    assert_eq!(outcome(ctab(&[&nested, "END CTAB"], false)), "ok");
    let deep = format!("COUNT {}", "(".repeat(65));
    assert_eq!(
        outcome(ctab(&[&deep], false)),
        "MOL input exceeds the size or work limit"
    );
    check_ctab(
        r#"
        COUNTS 0 0 | END CTAB  => ok
        COUNTS "0" 0 | END CTAB => ok
        // Tokenizer faults win over the keyword check.
        COUNT )                => Invalid MOL input at line 2: Unbalanced property list
        COUNT "0 0             => Invalid MOL input at line 2: Unclosed quoted field or property list
        COUNT (0               => Invalid MOL input at line 2: Unclosed quoted field or property list
        COUNT 0 0              => Invalid MOL input at line 2: Missing V3000 counts
        COUNTS                 => Invalid MOL input at line 2: Missing V3000 field
        COUNTS 1               => Invalid MOL input at line 2: Missing V3000 field
        COUNTS 1.0 0           => Invalid MOL input at line 2: Invalid integer "1.0"
        // Both optional counts parse before either limit check.
        COUNTS 1 0 -1 x        => Invalid MOL input at line 2: Invalid integer "x"
        COUNTS 1 0 -1 0        => Invalid MOL input at line 2: Negative count
        COUNTS 0 0 1 x         => Invalid MOL input at line 2: Invalid integer "x"
        COUNTS 0 0 100001 -1   => MOL input exceeds the size or work limit
        COUNTS 0 0 -1 100001   => Invalid MOL input at line 2: Negative count
        COUNTS 0 0 0 100001    => MOL input exceeds the size or work limit
        // The atom limit runs before the bond count parses.
        COUNTS 100001 x        => MOL input exceeds the size or work limit
        COUNTS -1 x            => Invalid MOL input at line 2: Negative count
        COUNTS 0 300001 x      => MOL input exceeds the size or work limit
        COUNTS 0 300000 x      => Invalid MOL input at line 2: Invalid integer "x"
        "#,
    );
}

#[test]
fn atom_block_framing_and_ids() {
    check_ctab(
        r#"
        COUNTS 1 0 | ATOM => Invalid MOL input at line 3: Missing BEGIN ATOM
        COUNTS 1 0 | begin atom | 1 C 0 0 0 0 | end atom | END CTAB => ok
        COUNTS 2 0 | BEGIN ATOM | 1 C 0 0 0 0 => Invalid MOL input at line 5: Unexpected end of file
        COUNTS 1 0 | BEGIN ATOM | 1 C 0 0 0 0 | END CTAB => Invalid MOL input at line 5: Missing END ATOM
        // The ID check wins over an unknown symbol on the same record.
        COUNTS 2 0 | BEGIN ATOM | 1 C 0 0 0 0 | 1 Xx 0 0 0 0 => Invalid MOL input at line 5: Duplicate atom ID
        COUNTS 2 0 | BEGIN ATOM | 1 C 0 0 0 0 | 2 Xx 0 0 0 0 => MOL input contains unsupported chemistry: query or unknown atom symbol
        // IDs are digit-prefix bookmarks: 1a and 01 collide, as do x and -1.
        COUNTS 2 0 | BEGIN ATOM | 1a C 0 0 0 0 | 01 C 0 0 0 0 => Invalid MOL input at line 5: Duplicate atom ID
        COUNTS 2 0 | BEGIN ATOM | x C 0 0 0 0 | -1 C 0 0 0 0 => Invalid MOL input at line 5: Duplicate atom ID
        "#,
    );
}

#[test]
fn atom_fields_and_properties() {
    check_atom(
        r#"
        1 C 0 0 0 0            => ok
        1                      => Invalid MOL input at line 4: Missing V3000 field
        // Symbol, then x, y, z and the map, each before the next.
        1 Xx x 0 0 0           => MOL input contains unsupported chemistry: query or unknown atom symbol
        1 C nan y 0 0          => Invalid MOL input at line 4: Nonfinite or excessive coordinate
        1 C 0 y inf 0          => Invalid MOL input at line 4: Invalid coordinate
        1 C 0 0 1e101 x        => Invalid MOL input at line 4: Nonfinite or excessive coordinate
        1 C 0 0 0              => Invalid MOL input at line 4: Missing V3000 field
        1 C 0 0 0 x            => Invalid MOL input at line 4: Invalid integer "x"
        1 C 0 0 0 -3           => ok
        // A trailing '-' joins the next line; errors report the last line read.
        1 C 0 0 0 0 CH- | G=x  => Invalid MOL input at line 5: Invalid integer "x"
        1 C 0 0 0 0 A=B=C      => Invalid MOL input at line 4: Invalid property assignment
        1 C 0 0 0 0 CHG        => Invalid MOL input at line 4: Invalid property assignment
        1 C 0 0 0 0 FOO=1      => ok
        1 C 0 0 0 0 CHG=x      => Invalid MOL input at line 4: Invalid integer "x"
        1 C 0 0 0 0 chg=x      => Invalid MOL input at line 4: Invalid integer "x"
        1 C 0 0 0 0 CHG=128    => Invalid MOL input at line 4: Excessive charge
        1 C 0 0 0 0 CHG=-128   => ok
        1 C 0 0 0 0 RAD=4      => MOL input contains unsupported chemistry: radical code
        1 C 0 0 0 0 RAD=x      => Invalid MOL input at line 4: Invalid integer "x"
        // Properties apply in token order.
        1 C 0 0 0 0 CHG=x RAD=9 => Invalid MOL input at line 4: Invalid integer "x"
        1 C 0 0 0 0 RAD=9 CHG=x => MOL input contains unsupported chemistry: radical code
        1 C 0 0 0 0 MASS=-1    => Invalid MOL input at line 4: Invalid isotope
        1 C 0 0 0 0 MASS=65536 => Invalid MOL input at line 4: Invalid isotope
        1 C 0 0 0 0 MASS=-0.5  => Invalid MOL input at line 4: Invalid isotope
        1 C 0 0 0 0 MASS=x     => Invalid MOL input at line 4: Invalid fixed-width coordinate
        1 C 0 0 0 0 MASS=1,5   => Invalid MOL input at line 4: Invalid coordinate
        1 C 0 0 0 0 MASS=12.7  => ok
        1 C 0 0 0 0 VAL=0      => ok
        1 C 0 0 0 0 VAL=x      => Invalid MOL input at line 4: Invalid integer "x"
        1 C 0 0 0 0 CFG=x      => Invalid MOL input at line 4: Invalid integer "x"
        1 C 0 0 0 0 CFG=4      => Invalid MOL input at line 4: Invalid atom CFG
        1 C 0 0 0 0 CFG=-1     => Invalid MOL input at line 4: Invalid atom CFG
        1 C 0 0 0 0 CFG=3      => ok
        1 C 0 0 0 0 HCOUNT=1   => MOL input contains unsupported chemistry: atom query
        1 C 0 0 0 0 RBCNT=1    => MOL input contains unsupported chemistry: atom query
        1 C 0 0 0 0 SUBST=x    => MOL input contains unsupported chemistry: atom query
        1 C 0 0 0 0 HCOUNT=0 RBCNT=0 SUBST=0 => ok
        1 C 0 0 0 0 UNSAT=1    => MOL input contains unsupported chemistry: unsaturation query
        1 C 0 0 0 0 UNSAT=2    => ok
        1 C 0 0 0 0 RGROUPS=1  => Invalid MOL input at line 4: Invalid RGROUPS list
        1 C 0 0 0 0 RGROUPS=() => Invalid MOL input at line 4: Missing V3000 field
        1 C 0 0 0 0 RGROUPS=(100001) => MOL input exceeds the size or work limit
        1 C 0 0 0 0 RGROUPS=(2 1) => Invalid MOL input at line 4: Missing RGROUPS entries
        1 C 0 0 0 0 RGROUPS=(1 1) => MOL input contains unsupported chemistry: R-group query
        1 C 0 0 0 0 RGROUPS=(0) => ok
        1 C 0 0 0 0 ATTCHPT=1 ATTCHPT=0 => ok
        1 C 0 0 0 0 ATTCHPT=1 ATTCHPT=2 => Invalid MOL input at line 4: Duplicate attachment point
        1 C 0 0 0 0 ATTCHPT=x  => Invalid MOL input at line 4: Invalid integer "x"
        1 C 0 0 0 0 ATTCHORD=(2 1 Al)x => Invalid MOL input at line 4: Invalid template attachment list
        1 C 0 0 0 0 ATTCHORD=() => Invalid MOL input at line 4: Missing V3000 field
        1 C 0 0 0 0 ATTCHORD=(-2 1 Al) => Invalid MOL input at line 4: Negative count
        1 C 0 0 0 0 ATTCHORD=(0) => Invalid MOL input at line 4: Invalid template attachment count
        1 C 0 0 0 0 ATTCHORD=(1 1) => Invalid MOL input at line 4: Invalid template attachment count
        1 C 0 0 0 0 ATTCHORD=(2 1) => Invalid MOL input at line 4: Invalid template attachment count
        1 C 0 0 0 0 ATTCHORD=(2 x Al) => Invalid MOL input at line 4: Invalid integer "x"
        1 C 0 0 0 0 ATTCHORD=(4 1 Al 1 Br) => Invalid MOL input at line 4: Duplicate template attachment
        1 C 0 0 0 0 ATTCHORD=(4 1 Al 2 Al) => Invalid MOL input at line 4: Duplicate template attachment
        1 C 0 0 0 0 ATTCHORD=(4 1 Al 2 Br) => ok
        1 C 0 0 0 0 ATTCHORD=x => Invalid MOL input at line 4: Invalid integer "x"
        1 C 0 0 0 0 ATTCHORD=3 => ok
        1 C 0 0 0 0 STBOX=x    => Invalid MOL input at line 4: Invalid integer "x"
        1 C 0 0 0 0 EXACHG=x   => Invalid MOL input at line 4: Invalid integer "x"
        1 C 0 0 0 0 INVRET=x   => Invalid MOL input at line 4: Invalid integer "x"
        1 C 0 0 0 0 SEQID=x    => Invalid MOL input at line 4: Invalid integer "x"
        "#,
    );
    let joined = ctab(
        &[
            "COUNTS 1 0",
            "BEGIN ATOM",
            "1 C 1 2 -",
            "3 4",
            "END ATOM",
            "END CTAB",
        ],
        false,
    )
    .unwrap();
    assert_eq!(
        joined.positions,
        [Point3 {
            x: 1.,
            y: 2.,
            z: 3.
        }]
    );
    assert_eq!(joined.metadata.atoms[0].map_number, 4);
    assert_eq!(
        outcome(ctab(&["COUNTS 1 0", "BEGIN ATOM", "1 C 0 0 0 0 -"], false)),
        "Invalid MOL input at line 5: Unexpected end of file"
    );
}

#[test]
fn bond_block_framing_ids_and_properties() {
    assert_eq!(
        outcome(after_atoms("COUNTS 5 1", &["BOND"])),
        "Invalid MOL input at line 10: Missing BEGIN BOND"
    );
    assert_eq!(
        outcome(after_atoms(
            "COUNTS 5 1",
            &["BEGIN BOND", "1 1 2 3", "END CTAB"]
        )),
        "Invalid MOL input at line 12: Missing END BOND"
    );
    check_bond(
        r#"
        1 1 2 3                => ok
        // The ID check wins over a missing endpoint on the same record.
        1 1 2 3 | 1 1 2 9      => Invalid MOL input at line 12: Duplicate bond ID
        1 1 2 3 | 2 1 2 9      => Invalid MOL input at line 12: Missing bond atom
        1 1 2 3 | 2 1 2 3      => ok
        1                      => Invalid MOL input at line 11: Missing V3000 field
        // The first endpoint resolves before the second is read.
        1 1 9                  => Invalid MOL input at line 11: Missing bond atom
        1 1 2                  => Invalid MOL input at line 11: Missing V3000 field
        1 1 2 9                => Invalid MOL input at line 11: Missing bond atom
        1 x 2 3                => ok
        1 1 2 3 A=B=C          => Invalid MOL input at line 11: Invalid property assignment
        1 1 2 3 CFG=0 STBOX=x  => ok
        1 1 2 3 CFG=5          => Invalid MOL input at line 11: Invalid bond CFG
        1 1 2 3 CFG=x          => Invalid MOL input at line 11: Invalid integer "x"
        1 1 2 3 TOPO=0         => ok
        1 1 2 3 TOPO=1         => MOL input contains unsupported chemistry: bond topology query
        1 1 2 3 CFG=5 TOPO=1   => Invalid MOL input at line 11: Invalid bond CFG
        1 1 2 3 TOPO=1 CFG=5   => MOL input contains unsupported chemistry: bond topology query
        1 1 2 3 RXCTR=x        => Invalid MOL input at line 11: Invalid integer "x"
        "#,
    );
}

#[test]
fn bond_attachment_endpoints() {
    check_bond(
        r#"
        1 1 1 2 ENDPTS=(2 3 4) ATTACH=ALL => ok
        1 1 1 2 ENDPTS=(2 3 4) ENDPTS=(2 3 4) ATTACH=ALL => Invalid MOL input at line 11: Duplicate ENDPTS
        1 1 1 2 ENDPTS=3 ATTACH=ALL => Invalid MOL input at line 11: Invalid ENDPTS list
        1 1 1 2 ENDPTS=() ATTACH=ALL => Invalid MOL input at line 11: Invalid ENDPTS count
        1 1 1 2 ENDPTS=(1 3) ATTACH=ALL => Invalid MOL input at line 11: Invalid ENDPTS count
        1 1 1 2 ENDPTS=(301 3) ATTACH=ALL => Invalid MOL input at line 11: Invalid ENDPTS count
        // The count check fires before the extra value is parsed.
        1 1 1 2 ENDPTS=(2 3 4 x) ATTACH=ALL => Invalid MOL input at line 11: ENDPTS count mismatch
        1 1 1 2 ENDPTS=(2 3 x) ATTACH=ALL => Invalid MOL input at line 11: Invalid ENDPTS atom ID
        1 1 1 2 ENDPTS=(2 3 9) ATTACH=ALL => Invalid MOL input at line 11: Missing ENDPTS atom
        1 1 1 2 ENDPTS=(2 2 3) ATTACH=ALL => Invalid MOL input at line 11: Invalid or duplicate ENDPTS atom
        1 1 1 2 ENDPTS=(2 3 3) ATTACH=ALL => Invalid MOL input at line 11: Invalid or duplicate ENDPTS atom
        1 1 1 2 ENDPTS=(3 3 4) ATTACH=ALL => Invalid MOL input at line 11: ENDPTS count mismatch
        1 1 1 2 ENDPTS=(2 3 4) ATTACH=ALL ATTACH=ANY => Invalid MOL input at line 11: Duplicate ATTACH
        1 1 1 2 ENDPTS=(2 3 4) ATTACH=all => Invalid MOL input at line 11: Unknown ATTACH mode
        1 1 1 2 ATTACH=ALL => Invalid MOL input at line 11: ENDPTS and ATTACH must occur together
        1 1 1 2 ENDPTS=(2 3 4) => Invalid MOL input at line 11: ENDPTS and ATTACH must occur together
        1 1 2 3 ENDPTS=(2 1 4) ATTACH=ALL => Invalid MOL input at line 11: Attachment bond needs exactly one dummy endpoint
        1 1 1 5 ENDPTS=(2 2 3) ATTACH=ANY => Invalid MOL input at line 11: Attachment bond needs exactly one dummy endpoint
        "#,
    );
    // The attachment ID is the dummy endpoint's index + 1; members keep
    // their file order.
    let p = bonds(&[
        "1 1 3 5 ENDPTS=(3 4 2 1) ATTACH=ANY",
        "2 1 1 2 ATTACH=ALL ENDPTS=(2 4 3)",
    ])
    .unwrap();
    let attachments: Vec<_> = p
        .bonds
        .iter()
        .map(|b| {
            b.attachment
                .as_ref()
                .map(|a| (a.id, a.kind, a.members.clone()))
        })
        .collect();
    assert_eq!(
        attachments,
        [
            Some((5, Kind::Variable, vec![4, 2, 1])),
            Some((1, Kind::MultiCenter, vec![4, 3])),
        ]
    );
    // With nonsequential atom IDs the IDs and members are converted, not the
    // raw bookmarks 10, 20 and 30.
    let p = ctab(
        &[
            "COUNTS 4 1",
            "BEGIN ATOM",
            "40 C 0 0 0 0",
            "10 R 1 0 0 0",
            "30 C 2 0 0 0",
            "20 C 3 0 0 0",
            "END ATOM",
            "BEGIN BOND",
            "7 1 40 10 ENDPTS=(2 20 30) ATTACH=ANY",
            "END BOND",
            "END CTAB",
        ],
        false,
    )
    .unwrap();
    let attachment = p.bonds[0].attachment.as_ref().unwrap();
    assert_eq!((p.graph.bonds[0].a, p.graph.bonds[0].b), (0, 1));
    assert_eq!(
        (attachment.id, attachment.kind, attachment.members.clone()),
        (2, Kind::Variable, vec![4, 3])
    );
}

#[test]
fn bond_cfg_sets_direction_stereo_and_chirality() {
    use Direction::{EitherDouble, Hash, Unknown, Wedge};
    let p = bonds(&[
        "1 1 2 3 CFG=0",
        "2 1 3 4 CFG=2",
        "3 2 2 4 CFG=2",
        "4 3 2 3 CFG=2",
        "5 x 3 4 CFG=2",
    ])
    .unwrap();
    assert_eq!(
        p.directions,
        [
            Direction::None,
            Unknown,
            EitherDouble,
            Direction::None,
            Direction::None
        ]
    );
    let stereo: Vec<_> = p.metadata.bonds.iter().map(|b| b.stereo).collect();
    assert_eq!(stereo, [0, 0, 1, 0, 0]);
    assert!(!p.chirality);
    for (record, direction) in [("1 2 2 3 CFG=1", Wedge), ("1 2 2 3 CFG=3", Hash)] {
        let p = bonds(&[record]).unwrap();
        assert_eq!(p.directions, [direction]);
        assert!(p.chirality);
    }
}

#[test]
fn trailer_blocks_and_final_checks() {
    check_ctab(
        r#"
        // LINKNODE records are skipped only before the first block.
        COUNTS 0 0 | LINKNODE 1 2 2 1 2 1 3 | linknode x | END CTAB => ok
        COUNTS 0 0 | BEGIN COLLECTION | END COLLECTION | LINKNODE | END CTAB => Invalid MOL input at line 5: Missing END CTAB
        COUNTS 0 0 | BEGIN COLLECTION => Invalid MOL input at line 4: Unexpected end of file
        // A zero atom count skips an atom block as an unknown block.
        COUNTS 0 0 | BEGIN ATOM | 1 C 0 0 0 0 | END ATOM | END CTAB => ok
        // Unknown blocks close on a case-sensitive END on the raw line.
        COUNTS 0 0 | BEGIN TEMPLATE | TEMPLATE 1 AA/Gly/G/ | END TEMPLATE | END CTAB => ok
        COUNTS 0 0 | BEGIN TEMPLATE | end template | END CTAB => Invalid MOL input at line 6: Unexpected end of file
        COUNTS 0 0 | BEGIN TEMPLATE | end template | END CTAB | END CTAB => ok
        // SGROUP and OBJ3D blocks need a nonzero count and occur once.
        COUNTS 0 0 | BEGIN SGROUP | 1 DAT 0 | END SGROUP | END CTAB => Invalid MOL input at line 3: Unexpected or repeated substance group block
        COUNTS 0 0 1 | BEGIN SGROUP | 1 DAT 0 | END SGROUP | END CTAB => ok
        COUNTS 0 0 1 | BEGIN SGROUP | 1 DAT 0 | END SGROUP | BEGIN SGROUP | 1 DAT 0 | END SGROUP | END CTAB => Invalid MOL input at line 6: Unexpected or repeated substance group block
        COUNTS 0 0 | BEGIN OBJ3D | x | END OBJ3D | END CTAB => Invalid MOL input at line 3: Unexpected or repeated 3D constraint block
        COUNTS 0 0 0 1 | BEGIN OBJ3D | any payload | END OBJ3D | END CTAB => ok
        COUNTS 0 0 0 1 | BEGIN OBJ3D | x | END OBJ3D | BEGIN OBJ3D | x | END OBJ3D | END CTAB => Invalid MOL input at line 6: Unexpected or repeated 3D constraint block
        COUNTS 0 0 0 1 | BEGIN OBJ3D | x | END CTAB => Invalid MOL input at line 5: Missing END OBJ3D
        // Final checks: END CTAB, then the SGROUP block, then the OBJ3D block.
        COUNTS 0 0 1 1 | END => Invalid MOL input at line 3: Missing END CTAB
        COUNTS 0 0 1 1 | END CTAB => Invalid MOL input at line 3: Missing substance group block
        COUNTS 0 0 1 1 | BEGIN SGROUP | 1 DAT 0 | END SGROUP | END CTAB => Invalid MOL input at line 6: Missing 3D constraint block
        "#,
    );
    let skipped = ctab(
        &[
            "COUNTS 0 0",
            "BEGIN ATOM",
            "1 C 0 0 0 0",
            "END ATOM",
            "END CTAB",
        ],
        false,
    )
    .unwrap();
    assert!(skipped.graph.atoms.is_empty());
}

#[test]
fn collection_stereo_groups() {
    check(
        r#"
        MDLV30/STEABS ATOMS=(1 1) | MDLV30/STEREL2 ATOMS=(2 3 2) => [StereoGroup { kind: 0, atoms: [0], bonds: [], read_id: 0, write_id: 0 }, StereoGroup { kind: 1, atoms: [2, 1], bonds: [], read_id: 2, write_id: 0 }]
        // Only the first record is uppercased; later lowercase records are ignored.
        mdlv30/steabs atoms=(1 1) | mdlv30/sterel1 atoms=(1 2) => [StereoGroup { kind: 0, atoms: [0], bonds: [], read_id: 0, write_id: 0 }]
        MDLV30/STEABS ATOMS=(1 1) | MDLV30/STEABS ATOMS=(1 2) => Invalid MOL input at line 10: Multiple absolute stereo groups
        MDLV30/STEXYZ1 ATOMS=(1 1) => Invalid MOL input at line 9: Unknown enhanced stereo group type
        MDLV30/STEREL1 ATOMS=(2 1) => Invalid MOL input at line 9: Missing stereo group members
        MDLV30/STEREL1 ATOMS=(2 1 1) => Invalid MOL input at line 9: Duplicate stereo group member
        MDLV30/STEREL1 ATOMS=(1 4) => Invalid MOL input at line 9: Missing atom or bond reference
        "#,
        stereo_groups,
    );
}

#[test]
fn end_of_ctab_and_whole_file_lines() {
    let text = ctab_text(&["COUNTS 0 0", "END CTAB"]);
    assert_eq!(outcome(read_text(&text, false)), "ok");
    assert_eq!(
        outcome(read_text(&text, true)),
        "Invalid MOL input at line 4: Unexpected end of file"
    );
    assert_eq!(outcome(read_text(&format!("{text}M  END\n"), true)), "ok");
    assert_eq!(
        outcome(read_text(&format!("{text}M  V30 END\n"), true)),
        "Invalid MOL input at line 4: Missing M END"
    );
    // In a whole MOL file BEGIN CTAB is line 5, so record i is at line i + 6.
    assert_eq!(
        outcome(crate::chemistry::molfile::read(&format!(
            "{MOL_HEADER}{text}"
        ))),
        "Invalid MOL input at line 8: Unexpected end of file"
    );
    check(
        r#"
        COUNTS 1 0 | BEGIN ATOM | 1 C 0 0 0 0 | END ATOM | END CTAB => ok
        COUNTS 1 0 | BEGIN ATOM | 1 C 0 0 0 0 CHG=x => Invalid MOL input at line 8: Invalid integer "x"
        COUNTS 1 0 | BEGIN ATOM | 1 C 0 0 0 0 RAD=4 => MOL input contains unsupported chemistry: radical code
        COUNTS 100001 0 => MOL input exceeds the size or work limit
        // The reader accepts an unspecified query bond; finishing rejects it.
        COUNTS 2 1 | BEGIN ATOM | 1 C 0 0 0 0 | 2 C 1 0 0 0 | END ATOM | BEGIN BOND | 1 8 1 2 | END BOND | END CTAB => MOL input contains unsupported chemistry: query or unspecified bond order
        "#,
        mol,
    );
}

#[test]
fn accepted_ctab_fills_the_raw_parse() {
    use Direction::{EitherDouble, Hash, Wedge};
    let p = ctab(
        &[
            "COUNTS 6 8 0 0 0",
            "BEGIN ATOM",
            // A zero VAL or ATTCHPT is ignored and keeps the earlier value.
            "1 C 0 0 0 3 CHG=-1 MASS=13 VAL=4 VAL=0",
            "2 N 1.5 -0.25 0 0 RAD=2 chg=1",
            "3 O -1 1 0.5 -2 MASS=12.7 CFG=2 ATTCHPT=-1",
            "4 R 2 2 0 0 ATTCHPT=2 ATTCHPT=0 ATTCHORD=(2 1 Al)",
            "5 D 0 -1 0 7 RAD=1 VAL=0",
            "6 Pol 3 0 0 0 UNSAT=2 ATTCHPT=0",
            "END ATOM",
            "BEGIN BOND",
            "1 1 1 2 CFG=1",
            "2 2 2 3 CFG=2",
            "3 4 1 3",
            "4 9 2 5",
            "5 10 3 6 CFG=3",
            "6 1 1 4 ENDPTS=(2 6 5) ATTACH=ALL",
            "7 8 5 6",
            "8 0 4 5",
            "END BOND",
            "BEGIN COLLECTION",
            "MDLV30/STEABS ATOMS=(1 1)",
            "END COLLECTION",
            "END CTAB",
        ],
        false,
    )
    .unwrap();
    let atoms: Vec<_> = p
        .graph
        .atoms
        .iter()
        .map(|a| (a.atomic_number, a.charge, a.isotope, a.radical_electrons))
        .collect();
    assert_eq!(
        atoms,
        [
            (6, -1, 13, 0),
            (7, 1, 0, 1),
            (8, 0, 12, 0),
            (0, 0, 0, 0),
            (1, 0, 2, 2),
            (0, 0, 0, 0)
        ]
    );
    let point = |x, y, z| Point3 { x, y, z };
    assert_eq!(
        p.positions,
        [
            point(0., 0., 0.),
            point(1.5, -0.25, 0.),
            point(-1., 1., 0.5),
            point(2., 2., 0.),
            point(0., -1., 0.),
            point(3., 0., 0.),
        ]
    );
    let maps: Vec<_> = p
        .metadata
        .atoms
        .iter()
        .map(|m| (m.map_number, m.map_present))
        .collect();
    assert_eq!(
        maps,
        [
            (3, true),
            (0, false),
            (0, false),
            (0, false),
            (7, true),
            (0, false)
        ]
    );
    let props: Vec<_> = p
        .atoms
        .iter()
        .map(|a| (a.valence, a.attachment, a.dummy_label.as_deref()))
        .collect();
    assert_eq!(
        props,
        [
            (4, None, None),
            (0, None, None),
            (0, Some(-1), None),
            (0, Some(2), Some("R")),
            (0, None, None),
            (0, None, Some("Pol")),
        ]
    );
    let graph_bonds: Vec<_> = p
        .graph
        .bonds
        .iter()
        .map(|b| (b.a, b.b, b.order, b.aromatic))
        .collect();
    assert_eq!(
        graph_bonds,
        [
            (0, 1, 1, false),
            (1, 2, 2, false),
            (0, 2, 4, true),
            (1, 4, 5, false),
            (2, 5, 0, false),
            (0, 3, 1, false),
            (4, 5, 0, false),
            (3, 4, 0, false),
        ]
    );
    let none = Direction::None;
    assert_eq!(
        p.directions,
        [Wedge, EitherDouble, none, none, Hash, none, none, none]
    );
    let stereo: Vec<_> = p.metadata.bonds.iter().map(|b| b.stereo).collect();
    assert_eq!(stereo, [0, 1, 0, 0, 0, 0, 0, 0]);
    let file_bonds: Vec<_> = p
        .bonds
        .iter()
        .map(|b| {
            let attachment = b
                .attachment
                .as_ref()
                .map(|a| (a.id, a.kind, a.members.clone()));
            (attachment, b.unspecified, b.query)
        })
        .collect();
    assert_eq!(
        file_bonds,
        [
            (None, false, false),
            (None, false, false),
            (None, false, false),
            (None, false, false),
            (None, false, false),
            (Some((4, Kind::MultiCenter, vec![6, 5])), false, false),
            (None, true, true),
            (None, true, false),
        ]
    );
    assert!(p.chirality);
    assert_eq!(
        format!("{:?}", p.metadata.groups),
        "[StereoGroup { kind: 0, atoms: [0], bonds: [], read_id: 0, write_id: 0 }]"
    );
}

#[test]
fn rxn_participants_stop_at_end_ctab() {
    let participant =
        |atom: &str| ctab_text(&["COUNTS 1 0", "BEGIN ATOM", atom, "END ATOM", "END CTAB"]);
    let rxn = |product: &str| {
        format!(
            "$RXN V3000\n\n      ReShiki\n\nM  V30 COUNTS 1 1\nM  V30 BEGIN REACTANT\n{}\
             M  V30 END REACTANT\nM  V30 BEGIN PRODUCT\n{}M  V30 END PRODUCT\nM  END\n",
            participant("1 C 0 0 0 0"),
            participant(product),
        )
    };
    // A participant read with expect_end would demand M  END after END CTAB.
    let reaction = crate::chemistry::reaction::read_rxn(&rxn("1 O 0 0 0 0")).unwrap();
    let elements = |parts: &[crate::chemistry::molfile::Imported]| -> Vec<Vec<u8>> {
        parts
            .iter()
            .map(|part| {
                let atoms = &part.molecule.state.graph.atoms;
                atoms.iter().map(|a| a.atomic_number).collect()
            })
            .collect()
    };
    assert_eq!(elements(&reaction.reactants), [[6]]);
    assert_eq!(elements(&reaction.products), [[8]]);
    assert!(reaction.agents.is_empty());
    // Line 18 of the file is the product's atom record.
    assert_eq!(
        outcome(crate::chemistry::reaction::read_rxn(&rxn(
            "1 O 0 0 0 0 CHG=x"
        ))),
        "Invalid MOL input at line 18: Invalid integer \"x\""
    );
}
