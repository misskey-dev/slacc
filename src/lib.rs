#![deny(clippy::all)]

#[macro_use]
extern crate napi_derive;

pub use slacc_system_metrics::*;
pub mod aho_corasick;
pub mod zip;
