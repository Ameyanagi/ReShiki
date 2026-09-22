use super::{Error, MAX_REQUEST_BYTES, Output, Resource, Status};
use crate::chemistry::inchi::{INCHI_VERSION, input::Input};
use std::collections::BTreeSet;

const MAGIC: &[u8; 8] = b"RSHINCHI";
const PROTOCOL: u16 = 2;
const MAX_STRING: usize = 2 * 1024 * 1024;

pub(super) fn encode(input: &Input, kernel_heap_bytes: usize) -> Result<Vec<u8>, Error> {
    let count = input.atoms.len();
    if count > i16::MAX as usize || input.stereo.len() > i16::MAX as usize {
        return Err(Error::Input(
            "Atom or stereo count exceeds the native index range",
        ));
    }
    let mut body = Vec::new();
    body.try_reserve(8 + count * 119 + input.stereo.len() * 12)
        .map_err(|_| Error::Limit("request"))?;
    body.extend_from_slice(&(kernel_heap_bytes as u32).to_le_bytes());
    body.extend_from_slice(&(count as u16).to_le_bytes());
    body.extend_from_slice(&(input.stereo.len() as u16).to_le_bytes());
    for (id, atom) in input.atoms.iter().enumerate() {
        if atom.position.iter().any(|x| !x.is_finite())
            || atom.element.is_empty()
            || atom.element.len() > 5
            || !atom
                .element
                .bytes()
                .all(|b| b.is_ascii_alphabetic() || b == b'*')
            || atom.bonds.len() > 20
            || atom.radical != 0
        {
            return Err(Error::Input(
                "Invalid atom coordinates, element, radical or neighbor count",
            ));
        }
        for position in atom.position {
            body.extend_from_slice(&position.to_le_bytes());
        }
        body.extend_from_slice(atom.element.as_bytes());
        body.extend(std::iter::repeat_n(0, 6 - atom.element.len()));
        body.extend_from_slice(&atom.isotopic_mass.to_le_bytes());
        body.push(atom.charge as u8);
        body.extend(atom.hydrogens.iter().map(|&h| h as u8));
        body.push(atom.radical as u8);
        body.push(atom.bonds.len() as u8);
        let mut seen = BTreeSet::new();
        for bond in &atom.bonds {
            if bond.neighbor < 0
                || bond.neighbor as usize <= id
                || bond.neighbor as usize >= count
                || !seen.insert(bond.neighbor)
                || !(0..=3).contains(&bond.kind)
                || !matches!(bond.stereo, -6 | -4 | -1 | 0 | 1 | 3 | 4 | 6)
            {
                return Err(Error::Input("Invalid stored bond"));
            }
            body.extend_from_slice(&bond.neighbor.to_le_bytes());
            body.extend_from_slice(&[bond.kind as u8, bond.stereo as u8]);
        }
    }
    for stereo in &input.stereo {
        if !matches!(stereo.kind, 1 | 2)
            || !matches!(stereo.parity, 1..=3)
            || (stereo.kind == 1) != stereo.central_atom.is_none()
            || stereo
                .central_atom
                .is_some_and(|i| i < 0 || i as usize >= count)
            || stereo
                .neighbors
                .iter()
                .any(|&i| i < 0 || i as usize >= count)
        {
            return Err(Error::Input("Invalid stereo record"));
        }
        body.extend_from_slice(&stereo.central_atom.unwrap_or(-1).to_le_bytes());
        for id in stereo.neighbors {
            body.extend_from_slice(&id.to_le_bytes());
        }
        body.extend_from_slice(&[stereo.kind as u8, stereo.parity as u8]);
    }
    if body.len() + 16 > MAX_REQUEST_BYTES {
        return Err(Error::Limit("request"));
    }
    let mut result = Vec::new();
    result
        .try_reserve(body.len() + 16)
        .map_err(|_| Error::Limit("request"))?;
    result.extend_from_slice(MAGIC);
    result.extend_from_slice(&PROTOCOL.to_le_bytes());
    result.extend_from_slice(&[1, u8::from(input.has_coordinates)]);
    result.extend_from_slice(&(body.len() as u32).to_le_bytes());
    result.extend(body);
    Ok(result)
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(Error::Protocol("Response length overflow"))?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or(Error::Protocol("Truncated response"))?;
        self.position = end;
        Ok(bytes)
    }
    fn number<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        self.take(N)?
            .try_into()
            .map_err(|_| Error::Protocol("Invalid integer size"))
    }
    fn string(&mut self) -> Result<String, Error> {
        let length = u32::from_le_bytes(self.number()?) as usize;
        if length > MAX_STRING {
            return Err(Error::Limit("response string"));
        }
        std::str::from_utf8(self.take(length)?)
            .map(str::to_owned)
            .map_err(|_| Error::Protocol("Response is not UTF-8"))
    }
}

pub(super) fn decode(bytes: &[u8]) -> Result<Output, Error> {
    let mut reader = Reader { bytes, position: 0 };
    if reader.take(8)? != MAGIC || u16::from_le_bytes(reader.number()?) != PROTOCOL {
        return Err(Error::Protocol("Incompatible response protocol"));
    }
    let version = reader.string()?;
    if version != INCHI_VERSION {
        return Err(Error::Version(version));
    }
    let result_kind = u16::from_le_bytes(reader.number()?);
    if result_kind == 1 {
        let message = reader.string()?;
        if reader.position != bytes.len() {
            return Err(Error::Protocol("Trailing rejection data"));
        }
        return Err(Error::Rejected(message));
    }
    if result_kind == 2 {
        let resource = match u16::from_le_bytes(reader.number()?) {
            1 => Resource::KernelHeap,
            _ => return Err(Error::Protocol("Unknown resource scope")),
        };
        let reason = u16::from_le_bytes(reader.number()?);
        let budget = u64::from_le_bytes(reader.number()?);
        let used = u64::from_le_bytes(reader.number()?);
        let requested = u64::from_le_bytes(reader.number()?);
        if reader.position != bytes.len()
            || budget == 0
            || budget > super::MAX_KERNEL_HEAP_BYTES as u64
            || used > budget
        {
            return Err(Error::Protocol("Invalid resource result"));
        }
        return Err(match reason {
            1 => Error::ResourceLimit {
                resource,
                budget,
                used,
                requested,
            },
            2 => Error::ResourceUnavailable { resource, budget },
            3 => Error::Protocol("Native allocator invariant failed"),
            _ => Error::Protocol("Unknown resource failure"),
        });
    }
    if result_kind != 0 {
        return Err(Error::Protocol("Unknown response kind"));
    }
    let status = Status::from_code(i16::from_le_bytes(reader.number()?))?;
    let output = Output {
        status,
        inchi: reader.string()?,
        message: reader.string()?,
        log: reader.string()?,
        auxiliary: reader.string()?,
    };
    if reader.position != bytes.len() {
        return Err(Error::Protocol("Trailing response data"));
    }
    if !output.inchi.is_empty() && !output.inchi.starts_with("InChI=1S/") {
        return Err(Error::Protocol("Helper returned a nonstandard identifier"));
    }
    if !status.is_success() && !output.inchi.is_empty() {
        return Err(Error::Protocol(
            "Helper returned an identifier with a failure status",
        ));
    }
    Ok(output)
}
