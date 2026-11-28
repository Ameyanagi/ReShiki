//! The xTB protocol, client and engine-backed worker.
//!
//! Only the `engine` feature links the LGPL axtb fork \[Axtb\]. See `docs/xtb-licensing.md`;
//! the methods and their full references are in docs/references/xtb.md.

#[cfg(any(feature = "engine", test))]
mod ao;
#[cfg(not(target_arch = "wasm32"))]
pub mod client;
#[cfg(feature = "engine")]
mod engine;
pub mod protocol;
mod worker;

pub use worker::serve;

pub const AXTB_REV: &str = "648ca6fe0beea6d3c429a8f6b17f9da85d98692e";
pub const AXTB_SOURCE: &str = "https://github.com/Ameyanagi/axtb";
pub const AXTB_LICENSE: &str = "LGPL-3.0-or-later";
pub const INCLUDED: bool = cfg!(feature = "engine");

#[cfg(feature = "engine")]
pub const AXTB_VERSION: &str = engine::VERSION;
