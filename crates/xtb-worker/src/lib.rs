//! Home of the xTB protocol, client and engine-backed worker.
//!
//! Only the `engine` feature links the LGPL axtb fork. See `docs/xtb-licensing.md`.

pub const AXTB_REV: &str = "06e29058416b26b69db9715728f637b43558adff";
pub const AXTB_SOURCE: &str = "https://github.com/Ameyanagi/axtb";
pub const AXTB_LICENSE: &str = "LGPL-3.0-or-later";
pub const INCLUDED: bool = cfg!(feature = "engine");

#[cfg(feature = "engine")]
pub const AXTB_VERSION: &str = axtb_driver::VERSION;
