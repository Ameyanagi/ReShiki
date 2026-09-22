use super::{Error, invalid};

/// Native four-carrier parity, including its early-branch behavior when a
/// caller supplies repeated values. 0 is a mismatch, 1 odd and 2 even.
pub fn parity4<T: PartialEq>(target: &[T], reference: &[T]) -> Result<u8, Error> {
    let t: &[T; 4] = target
        .try_into()
        .map_err(|_| invalid("Parity vectors must have size 4"))?;
    let r: &[T; 4] = reference
        .try_into()
        .map_err(|_| invalid("Parity vectors must have size 4"))?;
    // Match the native decision tree rather than count inversions: duplicate
    // entries can select an early branch whose remaining carriers mismatch.
    let choices = if r[0] == t[0] {
        if r[1] == t[1] {
            Some(((2, 3, 2), (3, 2, 1)))
        } else if r[1] == t[2] {
            Some(((1, 3, 1), (3, 1, 2)))
        } else if r[1] == t[3] {
            Some(((2, 1, 1), (1, 2, 2)))
        } else {
            None
        }
    } else if r[0] == t[1] {
        if r[1] == t[0] {
            Some(((2, 3, 1), (3, 2, 2)))
        } else if r[1] == t[2] {
            Some(((0, 3, 2), (3, 0, 1)))
        } else if r[1] == t[3] {
            Some(((2, 0, 2), (0, 2, 1)))
        } else {
            None
        }
    } else if r[0] == t[2] {
        if r[1] == t[1] {
            Some(((0, 3, 1), (3, 0, 2)))
        } else if r[1] == t[0] {
            Some(((1, 3, 2), (3, 1, 1)))
        } else if r[1] == t[3] {
            Some(((0, 1, 2), (1, 0, 1)))
        } else {
            None
        }
    } else if r[0] == t[3] {
        if r[1] == t[1] {
            Some(((2, 0, 1), (0, 2, 2)))
        } else if r[1] == t[2] {
            Some(((1, 0, 2), (0, 1, 1)))
        } else if r[1] == t[0] {
            Some(((2, 1, 2), (1, 2, 1)))
        } else {
            None
        }
    } else {
        None
    };
    if let Some((first, second)) = choices {
        for (a, b, parity) in [first, second] {
            if t.get(a) == Some(&r[2]) && t.get(b) == Some(&r[3]) {
                return Ok(parity);
            }
        }
    }
    Ok(0)
}
