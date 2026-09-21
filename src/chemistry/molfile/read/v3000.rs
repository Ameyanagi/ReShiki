use super::*;
use crate::chemistry::ranking::StereoGroup;
use std::collections::{HashMap, HashSet};

// Native V3000 bookmark fields use unsigned from_chars without checking its
// status. Preserve zero on invalid/overflowing input and signed map storage.
fn bookmark(text: &str) -> i32 {
    let length = text.bytes().take_while(u8::is_ascii_digit).count();
    text.get(..length)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0) as i32
}

impl Reader<'_> {
    pub(super) fn v3(&mut self) -> Result<String> {
        let mut output = String::new();
        loop {
            let line = self.next()?;
            let content = line
                .strip_prefix("M  V30 ")
                .ok_or_else(|| self.invalid("Missing V3000 line prefix"))?;
            let continuation = content.strip_suffix('-');
            output.push_str(continuation.unwrap_or(content));
            if output.len() > 1024 * 1024 {
                return Err(ReadError::Limit);
            }
            if continuation.is_none() {
                return Ok(output);
            }
        }
    }
    pub(super) fn expect(&mut self, prefix: &str) -> Result<()> {
        if self.v3()?.to_ascii_uppercase().starts_with(prefix) {
            Ok(())
        } else {
            Err(self.invalid(format!("Missing {prefix}")))
        }
    }
    fn tokens<'a>(&self, line: &'a str) -> Result<Vec<&'a str>> {
        let (mut start, mut depth, mut quoted) = (0usize, 0usize, false);
        let mut tokens = Vec::new();
        for (i, c) in line.char_indices() {
            match c {
                '"' if depth == 0 => quoted = !quoted,
                '(' if !quoted => {
                    depth += 1;
                    if depth > 64 {
                        return Err(ReadError::Limit);
                    }
                }
                ')' if !quoted => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| self.invalid("Unbalanced property list"))?;
                }
                ' ' | '\t' if !quoted && depth == 0 => {
                    if i > start {
                        tokens.push(self.field(line, start, i)?.trim_matches('"'));
                    }
                    start = i + 1;
                }
                _ => (),
            }
        }
        if quoted || depth != 0 {
            return Err(self.invalid("Unclosed quoted field or property list"));
        }
        if start < line.len() {
            tokens.push(self.field(line, start, line.len())?.trim_matches('"'));
        }
        Ok(tokens)
    }
    fn token<'a>(&self, tokens: &[&'a str], i: usize) -> Result<&'a str> {
        tokens
            .get(i)
            .copied()
            .ok_or_else(|| self.invalid("Missing V3000 field"))
    }
    fn assignment<'a>(&self, token: &'a str) -> Result<(String, &'a str)> {
        let (key, value) = token
            .split_once('=')
            .filter(|(_, v)| !v.contains('='))
            .ok_or_else(|| self.invalid("Invalid property assignment"))?;
        Ok((key.to_ascii_uppercase(), value))
    }
}

pub(super) fn read(r: &mut Reader<'_>, p: &mut Parsed, expect_end: bool) -> Result<()> {
    r.expect("BEGIN CTAB")?;
    let line = r.v3()?;
    let tokens = r.tokens(&line)?;
    if !r.token(&tokens, 0)?.eq_ignore_ascii_case("COUNTS") {
        return Err(r.invalid("Missing V3000 counts"));
    }
    let n = r.count(r.integer(r.token(&tokens, 1)?)?, 100_000)?;
    let e = r.count(r.integer(r.token(&tokens, 2)?)?, 300_000)?;
    let groups = tokens
        .get(3)
        .map(|s| r.integer(s))
        .transpose()?
        .unwrap_or(0);
    let objects = tokens
        .get(4)
        .map(|s| r.integer(s))
        .transpose()?
        .unwrap_or(0);
    let groups = r.count(groups, 100_000)?;
    let objects = r.count(objects, 100_000)?;
    let mut indices = HashMap::new();
    if n != 0 {
        r.expect("BEGIN ATOM")?;
        for _ in 0..n {
            let line = r.v3()?;
            let tokens = r.tokens(&line)?;
            let id = bookmark(r.token(&tokens, 0)?);
            if indices.insert(id, p.graph.atoms.len()).is_some() {
                return Err(r.invalid("Duplicate atom ID"));
            }
            let mut atom = r.symbol(r.token(&tokens, 1)?, true)?;
            let position = Point3 {
                x: r.coordinate(r.token(&tokens, 2)?, false)?,
                y: r.coordinate(r.token(&tokens, 3)?, false)?,
                z: r.coordinate(r.token(&tokens, 4)?, false)?,
            };
            let map = r.integer(r.token(&tokens, 5)?)?.max(0);
            let mut props = FileAtom {
                dummy_label: dummy_label(r.token(&tokens, 1)?),
                ..FileAtom::default()
            };
            for &token in tokens.iter().skip(6) {
                let (key, value) = r.assignment(token)?;
                match key.as_str() {
                    "CHG" => {
                        atom.charge = i8::try_from(r.integer(value)?)
                            .map_err(|_| r.invalid("Excessive charge"))?
                    }
                    "RAD" => {
                        let code = r.integer(value)?;
                        if code != 0 {
                            atom.radical_electrons = radical(code)?;
                        }
                    }
                    "MASS" => {
                        let mass = r
                            .integer(value)
                            .or_else(|_| r.coordinate(value, true).map(|v| v.floor() as i32))?;
                        atom.isotope =
                            u16::try_from(mass).map_err(|_| r.invalid("Invalid isotope"))?;
                    }
                    "VAL" if value != "0" => props.valence = r.integer(value)?,
                    "CFG" if !(0..=3).contains(&r.integer(value)?) => {
                        return Err(r.invalid("Invalid atom CFG"));
                    }
                    "HCOUNT" | "RBCNT" | "SUBST" if value != "0" => {
                        return Err(ReadError::Unsupported("atom query"));
                    }
                    "UNSAT" if value == "1" => {
                        return Err(ReadError::Unsupported("unsaturation query"));
                    }
                    "RGROUPS" => {
                        let content = value
                            .strip_prefix('(')
                            .and_then(|s| s.strip_suffix(')'))
                            .ok_or_else(|| r.invalid("Invalid RGROUPS list"))?;
                        let entries = content.split_whitespace().collect::<Vec<_>>();
                        let count = r.count(r.integer(r.token(&entries, 0)?)?, 100_000)?;
                        if entries.len() < count + 1 {
                            return Err(r.invalid("Missing RGROUPS entries"));
                        }
                        if count > 0 {
                            return Err(ReadError::Unsupported("R-group query"));
                        }
                    }
                    "ATTCHPT" if value != "0" => {
                        let attachment = r.integer(value)?;
                        if props.attachment.is_some() {
                            return Err(r.invalid("Duplicate attachment point"));
                        }
                        props.attachment = Some(attachment);
                    }
                    "ATTCHORD" if value.starts_with('(') => {
                        template_order(r, value)?;
                    }
                    "ATTCHORD" => {
                        r.integer(value)?;
                    }
                    "STBOX" | "EXACHG" | "INVRET" | "SEQID" => {
                        r.integer(value)?;
                    }
                    _ => (),
                }
            }
            p.atom(
                atom,
                position,
                AtomMetadata {
                    map_number: map,
                    map_present: map > 0,
                    ..AtomMetadata::default()
                },
                props,
            );
        }
        r.expect("END ATOM")?;
    }
    let mut bond_ids = HashMap::new();
    if e != 0 {
        r.expect("BEGIN BOND")?;
        for _ in 0..e {
            let line = r.v3()?;
            let tokens = r.tokens(&line)?;
            if bond_ids
                .insert(bookmark(r.token(&tokens, 0)?), p.graph.bonds.len())
                .is_some()
            {
                return Err(r.invalid("Duplicate bond ID"));
            }
            let (kind, props) = order(bookmark(r.token(&tokens, 1)?), true);
            let endpoint = |i| -> Result<usize> {
                indices
                    .get(&bookmark(r.token(&tokens, i)?))
                    .copied()
                    .ok_or_else(|| r.invalid("Missing bond atom"))
            };
            let (a, b) = (endpoint(2)?, endpoint(3)?);
            let mut dir = Direction::None;
            for &token in tokens.iter().skip(4) {
                let (key, value) = r.assignment(token)?;
                match key.as_str() {
                    "CFG" => match r.integer(value)? {
                        0 => (),
                        1 => {
                            dir = Direction::Wedge;
                            p.chirality = true;
                        }
                        3 => {
                            dir = Direction::Hash;
                            p.chirality = true;
                        }
                        2 => {
                            if kind == 1 {
                                dir = Direction::Unknown;
                            } else if kind == 2 {
                                dir = Direction::EitherDouble;
                            }
                        }
                        _ => return Err(r.invalid("Invalid bond CFG")),
                    },
                    "TOPO" if value != "0" => {
                        return Err(ReadError::Unsupported("bond topology query"));
                    }
                    "RXCTR" => {
                        r.integer(value)?;
                    }
                    // These properties do not modify connectivity in the native MOL reader.
                    "STBOX" | "ENDPTS" | "ATTACH" => (),
                    _ => (),
                }
            }
            p.bond(a, b, kind, dir, props);
        }
        r.expect("END BOND")?;
    }
    let mut line = r.v3()?.to_ascii_uppercase();
    let mut objects_found = false;
    let mut groups_found = false;
    while line.starts_with("LINKNODE") {
        line = r.v3()?.to_ascii_uppercase();
    }
    while line.starts_with("BEGIN") {
        if line.starts_with("BEGIN COLLECTION") {
            collection(r, p)?;
            line = r.v3()?.to_ascii_uppercase();
            continue;
        }
        if line.starts_with("BEGIN SGROUP") {
            if groups == 0 || groups_found {
                return Err(r.invalid("Unexpected or repeated substance group block"));
            }
            groups::read_v3000(r, p, groups, &indices, &bond_ids)?;
            groups_found = true;
            line = r.v3()?.to_ascii_uppercase();
            continue;
        }
        if line.starts_with("BEGIN OBJ3D") {
            if objects == 0 || objects_found {
                return Err(r.invalid("Unexpected or repeated 3D constraint block"));
            }
            objects_found = true;
            // As in the native reader, constraints do not alter atom positions.
            // Validate their framing and count without interpreting the payload.
            for _ in 0..objects {
                r.v3()?;
            }
            r.expect("END OBJ3D")?;
            line = r.v3()?.to_ascii_uppercase();
            continue;
        }
        // Bound traversal by the input limit and require a real terminating record.
        while !line.starts_with("END") {
            line = r.v3()?;
        }
        line = r.v3()?.to_ascii_uppercase();
    }
    if !line.starts_with("END CTAB") {
        return Err(r.invalid("Missing END CTAB"));
    }
    if groups != 0 && !groups_found {
        return Err(r.invalid("Missing substance group block"));
    }
    if objects != 0 && !objects_found {
        return Err(r.invalid("Missing 3D constraint block"));
    }
    if expect_end && !r.next()?.starts_with("M  END") {
        return Err(r.invalid("Missing M END"));
    }
    Ok(())
}

fn template_order(r: &Reader<'_>, value: &str) -> Result<()> {
    let list = value
        .strip_prefix('(')
        .and_then(|v| v.strip_suffix(')'))
        .ok_or_else(|| r.invalid("Invalid template attachment list"))?;
    let fields = list.split_whitespace().collect::<Vec<_>>();
    let count = r.count(r.integer(r.token(&fields, 0)?)?, 100_000)?;
    if count == 0 || count % 2 != 0 || fields.len() != count + 1 {
        return Err(r.invalid("Invalid template attachment count"));
    }
    let mut indices = HashSet::new();
    let mut labels = HashSet::new();
    for entry in fields
        .get(1..)
        .ok_or_else(|| r.invalid("Missing template attachments"))?
        .chunks_exact(2)
    {
        let index = r.integer(r.token(entry, 0)?)?;
        let label = r.token(entry, 1)?;
        if !indices.insert(index) || !labels.insert(label) {
            return Err(r.invalid("Duplicate template attachment"));
        }
    }
    Ok(())
}

// Group membership uses the one-based atom insertion order, not V3000
// bookmarks. Unknown collection types are ignored by the reference reader.
fn collection(r: &mut Reader<'_>, p: &mut Parsed) -> Result<()> {
    let mut line = r.v3()?.to_ascii_uppercase();
    let mut groups = Vec::new();
    let mut absolute = false;
    while !line.starts_with("END") {
        if let Some(group) = collection_group(r, &line, p.graph.atoms.len())? {
            if group.kind == 0 {
                if absolute {
                    return Err(r.invalid("Multiple absolute stereo groups"));
                }
                absolute = true;
            }
            groups.push(group);
        }
        line = r.v3()?;
    }
    if !groups.is_empty() {
        p.metadata.groups = groups;
    }
    Ok(())
}

fn collection_group(r: &Reader<'_>, text: &str, atom_count: usize) -> Result<Option<StereoGroup>> {
    let Some(content) = text.trim_end_matches(' ').strip_prefix("MDLV30/STE") else {
        return Ok(None);
    };
    let Some((label, list)) = content.split_once("ATOMS=(") else {
        return Ok(None);
    };
    if !label.ends_with(' ') {
        return Ok(None);
    }
    let label = label.trim_end_matches(' ');
    let Some(kind) = label.get(..3) else {
        return Ok(None);
    };
    let Some(identity) = label
        .get(3..)
        .filter(|s| s.bytes().all(|c| c.is_ascii_digit()))
    else {
        return Ok(None);
    };
    let Some(list) = list.strip_suffix(')') else {
        return Ok(None);
    };
    let Some((count, members)) = list.split_once(' ') else {
        return Ok(None);
    };
    if count.is_empty() || !count.bytes().all(|c| c.is_ascii_digit()) {
        return Ok(None);
    }
    let kind = match kind {
        "ABS" => 0,
        "REL" => 1,
        "RAC" => 2,
        _ => return Err(r.invalid("Unknown enhanced stereo group type")),
    };
    let read_id = if kind == 0 || identity.is_empty() {
        0
    } else {
        identity
            .parse::<u32>()
            .map_err(|_| r.invalid("Invalid stereo group ID"))?
    };
    let count = r.count(r.integer(count)?, 100_000)?;
    let mut atoms = Vec::new();
    let mut seen = HashSet::new();
    let mut members = members.split_whitespace();
    for _ in 0..count {
        let token = members
            .next()
            .ok_or_else(|| r.invalid("Missing stereo group members"))?;
        let i = r.index(r.integer(token)?, atom_count)?;
        if !seen.insert(i) {
            return Err(r.invalid("Duplicate stereo group member"));
        }
        atoms.push(i);
    }
    Ok(Some(StereoGroup {
        kind,
        atoms,
        bonds: Vec::new(),
        read_id,
        write_id: 0,
    }))
}
