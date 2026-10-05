//! Read embedded ChemDraw's root CONTENTS stream without activating its server.
use super::*;

const CDX_LIMIT: usize = 16 * 1024 * 1024;
const CDX_SIGNATURE: &[u8; 12] = b"VjCD0100\x04\x03\x02\x01";

#[cfg(test)]
mod tests;

pub(super) fn read_contents(storage: &IStorage) -> CResult<Option<Vec<u8>>> {
    // Inspect only this exact root stream. A preview, nested object or arbitrary
    // binary signature elsewhere in a container is not a chemical drawing.
    // SAFETY: storage stays on its initialized STA; the stream name is static
    // and terminated, and no optional pointers are supplied.
    let stream = match unsafe {
        storage.OpenStream(w!("CONTENTS"), None, STGM_READ | STGM_SHARE_EXCLUSIVE, 0)
    } {
        Ok(stream) => stream,
        Err(_) => return Ok(None),
    };
    let mut signature = [0; CDX_SIGNATURE.len()];
    let mut read = 0;
    // SAFETY: both output pointers remain valid for the call, and the requested
    // byte count is exactly the signature buffer's capacity.
    if unsafe {
        stream
            .Read(
                signature.as_mut_ptr().cast(),
                signature.len() as u32,
                Some(&mut read),
            )
            .is_err()
    } {
        return Ok(None);
    }
    // Unreadable, unrecognized and incomplete signatures leave image paste intact.
    if read as usize != signature.len() || &signature != CDX_SIGNATURE {
        return Ok(None);
    }
    let mut stat = STATSTG::default();
    // SAFETY: stat is a valid output buffer; NONAME avoids an allocated name.
    unsafe { stream.Stat(&mut stat, STATFLAG_NONAME)? };
    if stat.cbSize > CDX_LIMIT as u64 {
        return Err(error(
            "Embedded ChemDraw drawing exceeds the 16 MB structure limit",
        ));
    }
    if stat.cbSize < signature.len() as u64 {
        return Err(error("Truncated embedded ChemDraw drawing"));
    }
    let mut bytes = vec![0; stat.cbSize as usize];
    bytes[..signature.len()].copy_from_slice(&signature);
    let rest = &mut bytes[signature.len()..];
    if !rest.is_empty() {
        // SAFETY: the size check bounds this live output slice to 16 MB, and the
        // requested byte count fits both its capacity and u32.
        unsafe {
            stream
                .Read(rest.as_mut_ptr().cast(), rest.len() as u32, Some(&mut read))
                .ok()?;
        }
        if read as usize != rest.len() {
            return Err(error("Truncated embedded ChemDraw drawing"));
        }
    }
    // The application's bounded CDX parser validates the complete payload. Once
    // recognized, unsupported/malformed CDX must produce that import diagnostic,
    // rather than silently replacing the structure with its presentation image.
    Ok(Some(bytes))
}
