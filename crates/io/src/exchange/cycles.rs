//! A catalytic cycle's own data in a ChemDraw file. ChemDraw gets the cycle
//! as the arrows and text it draws; ReShiki's cycle rides along privately with
//! a fingerprint of the rest of the file. Reopened unchanged, the cycle comes
//! back; edited elsewhere, the file opens as it was saved there.
//!
//! Stable IDs: a cycle names its parts and what it drew by the file's own
//! object IDs (a structure by its `fragment`, a text by its `t`, an arrow by
//! its `arrow`). The import reports which drawing objects each file object
//! became, so the restored cycle points at those. The fingerprint makes sure
//! the IDs still mean what they meant when the file was written.
use crate::{document::Document, graphics::Graphic};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// The CDXML attribute on the root element.
pub(crate) const ATTRIBUTE: &str = "ReShikiCycles";
/// The CDX property on the document object, in the range the CDX format
/// leaves to other programs (0x4000–0x7FFF).
pub(crate) const CODE: u16 = 0x5243;

/// What the import says when a file's cycle was changed elsewhere.
pub const EDITED_NOTICE: &str = "This file's catalytic cycle was edited outside ReShiki, so it opens as plain arrows and text · Select its structures and choose Make catalytic cycle to rebuild it";

/// Every cycle of a file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Payload {
    pub cycles: Vec<Saved>,
}

/// One cycle: its graphic in file coordinates, each part one file object ID,
/// and the file objects drawn in its place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub graphic: Graphic,
    pub drawn: Vec<u64>,
}

/// A file's cycle data, if it has any.
#[derive(Debug, Clone, PartialEq)]
pub enum Found {
    /// The rest of the file is as ReShiki wrote it.
    Fresh(Payload),
    /// The file was changed after ReShiki wrote it.
    Edited,
}

fn fingerprint(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(crate) fn encode(payload: &Payload) -> Option<String> {
    serde_json::to_vec(payload)
        .ok()
        .map(|json| STANDARD.encode(json))
}

/// The value of the CDXML attribute or CDX property for `rest`, the file
/// without it: a fingerprint of the file and the payload, a space and the
/// encoded payload.
pub(crate) fn sealed(rest: &[u8], encoded: &str) -> String {
    format!(
        "{} {encoded}",
        fingerprint(&[rest, encoded.as_bytes()].concat())
    )
}

fn opened(rest: &[u8], value: &str) -> Found {
    let Some((print, encoded)) = value.split_once(' ') else {
        return Found::Edited;
    };
    if print != fingerprint(&[rest, encoded.as_bytes()].concat()) {
        return Found::Edited;
    }
    STANDARD
        .decode(encoded)
        .ok()
        .and_then(|json| serde_json::from_slice(&json).ok())
        .map_or(Found::Edited, Found::Fresh)
}

/// Add the cycle data to a written CDXML file.
pub(crate) fn seal_cdxml(xml: String, encoded: &str) -> String {
    let value = sealed(xml.as_bytes(), encoded);
    xml.replacen("<CDXML", &format!("<CDXML {ATTRIBUTE}=\"{value}\""), 1)
}

/// The CDXML file without its cycle data, and the data.
pub fn take_cdxml(text: &str) -> (std::borrow::Cow<'_, str>, Option<Found>) {
    let start = format!(" {ATTRIBUTE}=\"");
    // Only in the document element's own start tag.
    let Some(at) = text.find("<CDXML").and_then(|root| {
        let tag = text.get(root..)?;
        let end = tag.find('>')?;
        Some(root + tag.get(..end)?.find(&start)?)
    }) else {
        return (text.into(), None);
    };
    let from = at + start.len();
    let Some(length) = text.get(from..).and_then(|rest| rest.find('"')) else {
        return (text.into(), Some(Found::Edited));
    };
    let value = text.get(from..from + length).unwrap_or_default();
    let rest = format!(
        "{}{}",
        text.get(..at).unwrap_or_default(),
        text.get(from + length + 1..).unwrap_or_default()
    );
    let found = opened(rest.as_bytes(), value);
    (rest.into(), Some(found))
}

/// Where the document properties of the file ReShiki writes start: after
/// the file header and the document object's tag and ID.
const PROPERTIES: usize = super::HEADER.len() + 6;

/// Add the cycle data to written CDX bytes as the document's first property:
/// sealed for these bytes if `xml`, the file they were made from, was as
/// ReShiki wrote it; else as it was, so it still reads as edited.
pub(crate) fn seal_cdx(bytes: &mut Vec<u8>, xml: &str) -> Result<(), String> {
    let (_, found) = take_cdxml(xml);
    let Some((_, value)) = xml
        .split_once(&format!(" {ATTRIBUTE}=\""))
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(value, _)| value.split_once(' ').unwrap_or(("", value)))
    else {
        return Ok(());
    };
    let value = match found {
        Some(Found::Fresh(_)) => sealed(bytes, value),
        Some(Found::Edited) => format!("edited {value}"),
        None => return Ok(()),
    };
    let mut property = Vec::new();
    super::property(&mut property, CODE, value.as_bytes())?;
    if bytes.len() < PROPERTIES {
        return Err("Truncated binary drawing".into());
    }
    bytes.splice(PROPERTIES..PROPERTIES, property);
    Ok(())
}

/// The CDX file without its cycle data, and the data.
pub fn take_cdx(bytes: &[u8]) -> (std::borrow::Cow<'_, [u8]>, Option<Found>) {
    // Some writers leave six more bytes before the document object.
    let mut at = if bytes.get(PROPERTIES - 6..PROPERTIES - 4) == Some(&[0, 0x80]) {
        PROPERTIES
    } else {
        PROPERTIES + 6
    };
    let read = |at: usize, n: usize| bytes.get(at..at + n);
    // Only the document object's own properties, up to its first child.
    while let Some(&[a, b]) = read(at, 2) {
        let tag = u16::from_le_bytes([a, b]);
        if tag == 0 || tag >= 0x8000 {
            break;
        }
        let Some(&[a, b]) = read(at + 2, 2) else {
            break;
        };
        let (size, head) = match u16::from_le_bytes([a, b]) {
            u16::MAX => match read(at + 4, 4) {
                Some(&[a, b, c, d]) => (u32::from_le_bytes([a, b, c, d]), 8),
                _ => break,
            },
            size => (u32::from(size), 4),
        };
        let Some(end) = usize::try_from(size)
            .ok()
            .and_then(|size| (at + head).checked_add(size))
            .filter(|end| *end <= bytes.len())
        else {
            break;
        };
        if tag == CODE {
            let Some(value) = bytes
                .get(at + head..end)
                .and_then(|v| std::str::from_utf8(v).ok())
            else {
                break;
            };
            let mut rest = bytes.get(..at).unwrap_or_default().to_vec();
            rest.extend_from_slice(bytes.get(end..).unwrap_or_default());
            let found = opened(&rest, value);
            return (rest.into(), Some(found));
        }
        at = end;
    }
    (bytes.into(), None)
}

/// Put each saved cycle back into a drawing imported from the file it came
/// with. `objects` maps the file's object IDs to the drawing objects they
/// became; `scale` turns file coordinates into the drawing's. The objects
/// the cycle drew go and the cycle takes their place in their groups.
/// Returns how many cycles came back.
pub fn restore(
    doc: &mut Document,
    payload: &Payload,
    objects: &HashMap<u64, Vec<u64>>,
    scale: f32,
) -> usize {
    let before = doc.clone();
    let mut restored = 0;
    for saved in &payload.cycles {
        let mut g = saved.graphic.clone();
        let Some(cycle) = g.cycle.as_deref_mut() else {
            continue;
        };
        // Each part is a file object of its own that the import kept.
        let mut seen = HashSet::new();
        let mut found = cycle.validate().is_ok();
        for part in cycle.parts_mut() {
            match part.first().and_then(|id| objects.get(id)) {
                Some(ids) if !ids.is_empty() && ids.iter().all(|id| seen.insert(*id)) => {
                    *part = ids.clone();
                }
                _ => found = false,
            }
        }
        if !found {
            continue;
        }
        let parts: HashSet<u64> = seen;
        let drawn: HashSet<u64> = saved
            .drawn
            .iter()
            .filter_map(|id| objects.get(id))
            .flatten()
            .copied()
            .collect();
        if doc.graphics.iter().any(|graphic| graphic.table.is_some()) {
            let Ok(ownership) = crate::graphics::table::Membership::new(doc) else {
                continue;
            };
            let basis = if drawn.is_empty() { &parts } else { &drawn };
            let mut owners = basis.iter().map(|id| ownership.cell_of(*id));
            // None is an owner too: an output explicitly released onto the
            // canvas must not be pulled back into a cell by cycle metadata.
            if let Some(owner) = owners.next()
                && owners.any(|other| other != owner)
            {
                continue;
            }
        }
        g.id = doc.next_id();
        g.origin = crate::document::Point::new(g.origin.x * scale, g.origin.y * scale);
        g.axis_x = crate::document::Point::new(g.axis_x.x * scale, g.axis_x.y * scale);
        g.axis_y = crate::document::Point::new(g.axis_y.x * scale, g.axis_y.y * scale);
        doc.arrows.retain(|a| !drawn.contains(&a.id));
        doc.annotations.retain(|a| !drawn.contains(&a.id));
        doc.graphics.retain(|x| !drawn.contains(&x.id));
        // The cycle takes the place of what it drew in its groups, or, when
        // it drew nothing, joins the groups that hold all its parts.
        for group in &mut doc.groups {
            let joins = if drawn.is_empty() {
                !parts.is_empty() && parts.iter().all(|id| group.members.contains(id))
            } else {
                group.members.iter().any(|id| drawn.contains(id))
            };
            if joins {
                group.members.retain(|id| !drawn.contains(id));
                group.members.push(g.id);
            }
        }
        let cycle_id = g.id;
        doc.graphics.push(g);
        if drawn.is_empty() {
            // Hidden, unnamed steps can draw nothing. Their checked parts
            // still identify the cell, while remaining ordinary members.
            if let Some(&part) = parts.iter().next() {
                crate::graphics::table::replace_member(doc, part, &[part, cycle_id]);
            }
        } else {
            for id in &drawn {
                crate::graphics::table::replace_member(doc, *id, &[cycle_id]);
            }
        }
        restored += 1;
    }
    if restored > 0 {
        doc.prune_groups();
        crate::cycles::reconcile(doc);
        if doc.validate().is_err() {
            *doc = before;
            return 0;
        }
    }
    restored
}
