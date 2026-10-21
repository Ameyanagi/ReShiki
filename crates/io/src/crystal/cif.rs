//! CIF: the cell, fractional atom sites expanded by the listed symmetry
//! operators, the first alternative of disordered sites and displacement
//! parameters. Only those items are kept while reading; every other item,
//! loop and data block is read past and dropped.
use super::build::{Lattice, same_site, wrap};
use super::{Atom, Error, MAX_ATOMS, Structure, V3, elements, number};
use crate::models3d::Adp;
use std::collections::HashMap;

#[derive(Debug)]
struct Token {
    text: String,
    line: usize,
    /// Quoted or a text field: never a tag or keyword.
    quoted: bool,
}

/// The text after `skip` bytes of `text`, or nothing.
fn after(text: &str, skip: usize) -> &str {
    text.get(skip..).unwrap_or_default()
}

fn tokens(text: &str) -> Result<Vec<Token>, Error> {
    let mut out = vec![];
    let mut lines = text.lines().enumerate();
    while let Some((index, line)) = lines.next() {
        let mut line_number = index + 1;
        let mut rest = line;
        if let Some(first) = line.strip_prefix(';') {
            let mut value = first.to_owned();
            let mut closed = None;
            for (index, next) in lines.by_ref() {
                if let Some(after_field) = next.strip_prefix(';') {
                    closed = Some((index + 1, after_field));
                    break;
                }
                value.push('\n');
                value.push_str(next);
            }
            let Some((end_line, after_field)) = closed else {
                return Err(Error::at(
                    line_number,
                    "A text field opened with ; is not closed",
                ));
            };
            out.push(Token {
                text: value,
                line: line_number,
                quoted: true,
            });
            // Tokens may follow the closing semicolon on its line.
            (line_number, rest) = (end_line, after_field);
        }
        loop {
            rest = rest.trim_start();
            let Some(first) = rest.chars().next() else {
                break;
            };
            if first == '#' {
                break;
            }
            if first == '\'' || first == '"' {
                let body = after(rest, 1);
                // A quote closes only where whitespace or the line end follows.
                let end = body.char_indices().find(|(i, c)| {
                    *c == first
                        && after(body, i + c.len_utf8())
                            .chars()
                            .next()
                            .is_none_or(char::is_whitespace)
                });
                let Some((end, _)) = end else {
                    return Err(Error::at(line_number, "A quoted value is not closed"));
                };
                out.push(Token {
                    text: body.get(..end).unwrap_or_default().to_owned(),
                    line: line_number,
                    quoted: true,
                });
                rest = after(body, end + 1);
            } else {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                out.push(Token {
                    text: rest.get(..end).unwrap_or_default().to_owned(),
                    line: line_number,
                    quoted: false,
                });
                rest = after(rest, end);
            }
        }
    }
    Ok(out)
}

/// A loop's tags and rows, with each row's line.
#[derive(Debug, Default)]
struct Loop {
    tags: Vec<String>,
    rows: Vec<(usize, Vec<String>)>,
}

impl Loop {
    fn column(&self, tag: &str) -> Option<usize> {
        self.tags.iter().position(|t| t == tag)
    }
}

/// The items and loops of one data block that the structure needs.
#[derive(Debug, Default)]
struct Block {
    items: HashMap<String, (usize, String)>,
    loops: Vec<Loop>,
}

/// Tags whose items or loops the reader keeps.
fn wanted(tag: &str) -> bool {
    ["_cell_", "_atom_site", "_space_group", "_symmetry_"]
        .iter()
        .any(|prefix| tag.starts_with(prefix))
}

/// A tag or reserved word: `data_` and `save_` start one, while `loop_`,
/// `global_` and `stop_` are whole words.
fn keyword(token: &Token) -> bool {
    let lower = token.text.to_ascii_lowercase();
    !token.quoted
        && (token.text.starts_with('_')
            || lower.starts_with("data_")
            || lower.starts_with("save_")
            || matches!(lower.as_str(), "loop_" | "global_" | "stop_"))
}

fn blocks(tokens: Vec<Token>) -> Result<Vec<Block>, Error> {
    let mut blocks: Vec<Block> = vec![];
    let mut tokens = tokens.into_iter().peekable();
    let mut current = Block::default();
    let mut started = false;
    while let Some(token) = tokens.next() {
        let lower = token.text.to_ascii_lowercase();
        if token.quoted {
            return Err(Error::at(token.line, "A value has no tag"));
        }
        if lower.starts_with("data_") {
            if started {
                blocks.push(std::mem::take(&mut current));
            }
            started = true;
        } else if lower == "loop_" {
            let mut lp = Loop::default();
            while let Some(tag) = tokens.next_if(|t| !t.quoted && t.text.starts_with('_')) {
                lp.tags.push(tag.text.to_ascii_lowercase());
            }
            if lp.tags.is_empty() {
                return Err(Error::at(token.line, "A loop has no tags"));
            }
            let mut values = vec![];
            while let Some(value) = tokens.next_if(|t| !keyword(t)) {
                values.push(value);
            }
            if values.len() % lp.tags.len() != 0 {
                return Err(Error::at(
                    token.line,
                    &format!(
                        "A loop of {} tags has {} values, not a whole number of rows",
                        lp.tags.len(),
                        values.len()
                    ),
                ));
            }
            if lp.tags.first().is_some_and(|t| wanted(t)) {
                let width = lp.tags.len();
                let mut values = values.into_iter().peekable();
                while let Some(first) = values.peek() {
                    let line = first.line;
                    let row: Vec<String> = values.by_ref().take(width).map(|t| t.text).collect();
                    lp.rows.push((line, row));
                }
                current.loops.push(lp);
            }
        } else if lower.starts_with('_') {
            let Some(value) = tokens.next_if(|t| !keyword(t)) else {
                return Err(Error::at(
                    token.line,
                    &format!("{} has no value", token.text),
                ));
            };
            if wanted(&lower) {
                current.items.insert(lower, (value.line, value.text));
            }
        } else if lower.starts_with("save_") || matches!(lower.as_str(), "global_" | "stop_") {
            // Save frames and global blocks hold no structure.
        } else {
            return Err(Error::at(
                token.line,
                &format!("Unexpected '{}'", token.text),
            ));
        }
    }
    blocks.push(current);
    Ok(blocks)
}

/// A missing value: `?` or `.`.
fn missing(text: &str) -> bool {
    matches!(text, "?" | ".")
}

/// A symmetry operator such as `-x+1/2, y, -z` as its rotation rows and
/// translation in fractional coordinates.
fn operator(text: &str) -> Option<([V3; 3], V3)> {
    let mut parts = text.split(',');
    let mut rows = [[0.; 3]; 3];
    let mut shift = [0.; 3];
    for (row, t) in rows.iter_mut().zip(&mut shift) {
        (*row, *t) = component(parts.next()?)?;
    }
    if parts.next().is_some() || rows.iter().flatten().any(|v| v.abs() > 1. + 1e-9) {
        return None;
    }
    let det = super::dot(rows[0], super::cross(rows[1], rows[2]));
    ((det.abs() - 1.).abs() < 1e-6).then_some((rows, shift))
}

/// One coordinate of an operator: a sum of terms such as `-x`, `+1/2`, `2y`
/// or `0.25`.
fn component(text: &str) -> Option<(V3, f64)> {
    let chars: Vec<char> = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if chars.is_empty() {
        return None;
    }
    let (mut row, mut shift) = ([0.; 3], 0.);
    let mut i = 0;
    while i < chars.len() {
        let mut sign = 1.;
        while let Some(c @ ('+' | '-')) = chars.get(i) {
            if *c == '-' {
                sign = -sign;
            }
            i += 1;
        }
        let start = i;
        while chars
            .get(i)
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, '.' | '/'))
        {
            i += 1;
        }
        let digits: String = chars.get(start..i)?.iter().collect();
        let value = if digits.is_empty() {
            None
        } else {
            Some(fraction(&digits)?)
        };
        if chars.get(i) == Some(&'*') {
            i += 1;
        }
        match chars.get(i).and_then(|c| "xyz".find(*c)) {
            Some(k) => {
                *row.get_mut(k)? += sign * value.unwrap_or(1.);
                i += 1;
            }
            None => shift += sign * value?,
        }
    }
    Some((row, shift))
}

fn fraction(text: &str) -> Option<f64> {
    match text.split_once('/') {
        Some((n, d)) => {
            let (n, d) = (n.parse::<f64>().ok()?, d.parse::<f64>().ok()?);
            (d != 0.).then(|| n / d)
        }
        None => text.parse().ok(),
    }
}

/// An anisotropic U as CIF writes it (U11 U22 U33 U12 U13 U23) after a
/// symmetry operation with rotation `r`, through U* = N U N, where N holds
/// the reciprocal lengths: U*' = R U* Rᵀ.
fn rotate_adp(u: [f32; 6], r: &[V3; 3], reciprocal: V3) -> [f32; 6] {
    let [u11, u22, u33, u12, u13, u23] = u.map(f64::from);
    let m = [[u11, u12, u13], [u12, u22, u23], [u13, u23, u33]];
    let n = reciprocal;
    let star: [V3; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            m.get(i).and_then(|row| row.get(j)).copied().unwrap_or(0.)
                * n.get(i).copied().unwrap_or(0.)
                * n.get(j).copied().unwrap_or(0.)
        })
    });
    let at = |m: &[V3; 3], i: usize, j: usize| m.get(i).and_then(|row| row.get(j)).copied();
    let turned = |i: usize, j: usize| -> f64 {
        let mut sum = 0.;
        for k in 0..3 {
            for l in 0..3 {
                sum += at(r, i, k).unwrap_or(0.)
                    * at(&star, k, l).unwrap_or(0.)
                    * at(r, j, l).unwrap_or(0.);
            }
        }
        let scale = n.get(i).copied().unwrap_or(1.) * n.get(j).copied().unwrap_or(1.);
        if scale > 0. { sum / scale } else { 0. }
    };
    [
        turned(0, 0),
        turned(1, 1),
        turned(2, 2),
        turned(0, 1),
        turned(0, 2),
        turned(1, 2),
    ]
    .map(|v| v as f32)
}

/// The most symmetry operators of a space group (Fm-3m and the like).
const MAX_OPERATORS: usize = 192;
/// The most sites times operators expanded, so a file cannot ask for
/// endless work below the atom limit.
const MAX_EXPANSION: usize = 40_000;

const EIGHT_PI_SQUARED: f64 = 8. * std::f64::consts::PI * std::f64::consts::PI;

/// Anisotropic U by atom label, from U or B values.
fn anisotropic(block: &Block) -> HashMap<String, [f32; 6]> {
    let mut out = HashMap::new();
    let names = ["11", "22", "33", "12", "13", "23"];
    for lp in &block.loops {
        let Some(label) = lp.column("_atom_site_aniso_label") else {
            continue;
        };
        for (prefix, factor) in [
            ("_atom_site_aniso_u_", 1.),
            ("_atom_site_aniso_b_", 1. / EIGHT_PI_SQUARED),
        ] {
            let columns: Option<Vec<usize>> = names
                .iter()
                .map(|n| lp.column(&format!("{prefix}{n}")))
                .collect();
            let Some(columns) = columns else { continue };
            for (_, row) in &lp.rows {
                let values: Option<Vec<f64>> = columns
                    .iter()
                    .map(|c| row.get(*c).and_then(|v| number(v)))
                    .collect();
                if let (Some(name), Some(values)) = (row.get(label), values)
                    && let Ok(u) = <[f64; 6]>::try_from(values)
                {
                    out.entry(name.clone())
                        .or_insert(u.map(|v| (v * factor) as f32));
                }
            }
        }
    }
    out
}

pub(super) fn read(text: &str) -> Result<Structure, Error> {
    let blocks = blocks(tokens(text)?)?;
    let mut with_cell = blocks
        .into_iter()
        .filter(|b| b.items.contains_key("_cell_length_a"));
    let block = with_cell
        .next()
        .ok_or_else(|| Error::new("The CIF has no unit cell (_cell_length_a)"))?;
    let mut warnings = vec![];
    let others = with_cell.count();
    if others > 0 {
        warnings.push(format!(
            "Read the first of {} structures in the file",
            others + 1
        ));
    }
    let item = |tag: &str| -> Result<f64, Error> {
        let (line, value) = block
            .items
            .get(tag)
            .ok_or_else(|| Error(format!("The CIF has no {tag}")))?;
        number(value).ok_or_else(|| Error::at(*line, &format!("{tag} is not a number")))
    };
    let lengths = [
        item("_cell_length_a")?,
        item("_cell_length_b")?,
        item("_cell_length_c")?,
    ];
    let angles = [
        item("_cell_angle_alpha")?,
        item("_cell_angle_beta")?,
        item("_cell_angle_gamma")?,
    ];
    let lattice = Lattice::from_parameters(lengths, angles)?;

    let sites = block
        .loops
        .iter()
        .find(|l| l.column("_atom_site_fract_x").is_some())
        .ok_or_else(|| {
            Error::new("The CIF has no atom sites with fractional coordinates (_atom_site_fract_x)")
        })?;
    let column = |tag: &str| sites.column(tag);
    let (Some(x), Some(y), Some(z)) = (
        column("_atom_site_fract_x"),
        column("_atom_site_fract_y"),
        column("_atom_site_fract_z"),
    ) else {
        return Err(Error::new(
            "The CIF's atom sites need _atom_site_fract_x, _y and _z",
        ));
    };
    let label = column("_atom_site_label");
    let symbol = column("_atom_site_type_symbol");
    let group = column("_atom_site_disorder_group");
    let assembly = column("_atom_site_disorder_assembly");
    let occupancy = column("_atom_site_occupancy");
    let flag = column("_atom_site_calc_flag");
    let u_iso = column("_atom_site_u_iso_or_equiv");
    let b_iso = column("_atom_site_b_iso_or_equiv");
    let aniso = anisotropic(&block);

    // The first alternative of disordered sites: no group, group 0, or the
    // lowest group number of each disorder assembly, which is ordered
    // independently of the others (a negative group sits on a special
    // position).
    let group_of = |row: &Vec<String>| -> Option<(String, u64)> {
        let number = group
            .and_then(|g| row.get(g))
            .filter(|v| !missing(v))
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|g| *g != 0)?
            .unsigned_abs();
        let set = assembly
            .and_then(|a| row.get(a))
            .filter(|v| !missing(v))
            .cloned()
            .unwrap_or_default();
        Some((set, number))
    };
    let mut first_group: HashMap<String, u64> = HashMap::new();
    for (set, number) in sites.rows.iter().filter_map(|(_, row)| group_of(row)) {
        let first = first_group.entry(set).or_insert(number);
        *first = (*first).min(number);
    }
    let mut asymmetric = vec![];
    let mut alternatives = 0;
    for (line, row) in &sites.rows {
        let line = *line;
        if flag
            .and_then(|f| row.get(f))
            .is_some_and(|v| v.eq_ignore_ascii_case("dum"))
        {
            continue;
        }
        if let Some((set, number)) = group_of(row)
            && first_group.get(&set) != Some(&number)
        {
            alternatives += 1;
            continue;
        }
        // An occupancy of 0 means the site holds none of this atom.
        if occupancy
            .and_then(|o| row.get(o))
            .and_then(|v| number(v))
            .is_some_and(|o| o <= 0.)
        {
            continue;
        }
        let name = label
            .and_then(|l| row.get(l))
            .filter(|v| !missing(v))
            .cloned()
            .unwrap_or_default();
        let from_symbol = symbol
            .and_then(|s| row.get(s))
            .filter(|v| !missing(v))
            .and_then(|s| elements::from_symbol(s));
        let (element, charge) = match from_symbol {
            Some(found) => found,
            None => (
                elements::from_label(&name).ok_or_else(|| {
                    Error::at(line, &format!("Cannot tell the element of atom '{name}'"))
                })?,
                0,
            ),
        };
        let coordinate = |c: usize| {
            row.get(c).and_then(|v| number(v)).ok_or_else(|| {
                Error::at(
                    line,
                    &format!("Atom '{name}' has no fractional coordinates"),
                )
            })
        };
        let f = [coordinate(x)?, coordinate(y)?, coordinate(z)?];
        let iso = |c: Option<usize>, factor: f64| {
            c.and_then(|c| row.get(c))
                .and_then(|v| number(v))
                .map(|u| Adp::Iso((u * factor) as f32))
        };
        let adp = aniso
            .get(&name)
            .map(|u| Adp::Aniso(*u))
            .or_else(|| iso(u_iso, 1.))
            .or_else(|| iso(b_iso, 1. / EIGHT_PI_SQUARED));
        asymmetric.push((f, element, charge, name, adp));
    }
    if alternatives > 0 {
        warnings.push(format!(
            "Disordered sites: kept the first alternative, left out {alternatives} atom{}",
            if alternatives == 1 { "" } else { "s" }
        ));
    }

    let operators = symmetry(&block)?;
    if operators.is_empty() && !p1(&block) {
        warnings
            .push("The CIF lists no symmetry operators; only its listed atoms are shown".into());
    }
    let identity = ([[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]], [0.; 3]);
    let mut operators = if operators.is_empty() {
        vec![identity]
    } else {
        operators
    };
    // Each operator once, translations taken modulo whole cells.
    let mut seen: Vec<([V3; 3], V3)> = vec![];
    operators.retain(|(r, t)| {
        let t = t.map(wrap);
        let same =
            |(s, u): &([V3; 3], V3)| s == r && u.iter().zip(&t).all(|(a, b)| (a - b).abs() < 1e-6);
        let new = !seen.iter().any(same);
        if new {
            seen.push((*r, t));
        }
        new
    });
    if operators.len() > MAX_OPERATORS {
        return Err(Error(format!(
            "The CIF lists {} symmetry operators; a space group has at most {MAX_OPERATORS}",
            operators.len()
        )));
    }
    if asymmetric.len().saturating_mul(operators.len()) > MAX_EXPANSION {
        return Err(Error(format!(
            "{} sites under {} symmetry operators are too many to expand; at most {MAX_ATOMS} atoms can be shown",
            asymmetric.len(),
            operators.len()
        )));
    }
    let reciprocal = lattice.reciprocal_lengths();
    let mut atoms: Vec<Atom> = vec![];
    for (f, element, charge, name, adp) in asymmetric {
        for (r, t) in &operators {
            let moved: V3 = std::array::from_fn(|k| {
                wrap(
                    super::dot(r.get(k).copied().unwrap_or_default(), f)
                        + t.get(k).copied().unwrap_or(0.),
                )
            });
            let xyz = lattice.cartesian(moved);
            if atoms.iter().any(|a| same_site(&lattice, a.xyz, xyz)) {
                continue;
            }
            if atoms.len() >= MAX_ATOMS {
                return Err(Error(format!(
                    "The unit cell has more than {MAX_ATOMS} atoms; at most {MAX_ATOMS} can be shown"
                )));
            }
            atoms.push(Atom {
                element: element.clone(),
                charge,
                label: name.clone(),
                xyz,
                adp: adp.map(|a| match a {
                    Adp::Aniso(u) => Adp::Aniso(rotate_adp(u, r, reciprocal)),
                    iso => iso,
                }),
            });
        }
    }
    Ok(Structure {
        lattice: Some(lattice),
        atoms,
        warnings,
    })
}

/// Whether the space group is P1, which needs no operators.
fn p1(block: &Block) -> bool {
    [
        "_space_group_name_h-m_alt",
        "_symmetry_space_group_name_h-m",
    ]
    .iter()
    .filter_map(|tag| block.items.get(*tag))
    .any(|(_, name)| {
        name.split_whitespace()
            .collect::<String>()
            .eq_ignore_ascii_case("P1")
    })
}

/// The listed symmetry operators, in a loop or as a single item.
fn symmetry(block: &Block) -> Result<Vec<([V3; 3], V3)>, Error> {
    let tags = [
        "_space_group_symop_operation_xyz",
        "_symmetry_equiv_pos_as_xyz",
    ];
    let mut listed: Vec<(usize, &str)> = vec![];
    for tag in tags {
        if let Some(lp) = block.loops.iter().find(|l| l.column(tag).is_some()) {
            let column = lp.column(tag).unwrap_or_default();
            listed = lp
                .rows
                .iter()
                .filter_map(|(line, row)| Some((*line, row.get(column)?.as_str())))
                .collect();
            break;
        }
        if let Some((line, value)) = block.items.get(tag) {
            listed = vec![(*line, value.as_str())];
            break;
        }
    }
    listed
        .into_iter()
        .map(|(line, text)| {
            operator(text).ok_or_else(|| {
                Error::at(line, &format!("Cannot read the symmetry operator '{text}'"))
            })
        })
        .collect()
}

#[cfg(test)]
pub(super) fn parse_operator(text: &str) -> Option<([V3; 3], V3)> {
    operator(text)
}
