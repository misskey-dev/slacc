#![deny(clippy::all)]

use std::sync::OnceLock;

use rayon::{ThreadPool, ThreadPoolBuilder};

#[macro_use]
extern crate napi_derive;

pub mod aho_corasick;
pub mod aws_lc_rs;
pub mod zip;

pub(crate) static THREAD_POOL: OnceLock<ThreadPool> = OnceLock::new();

#[napi]
pub fn init(num_threads: u32) -> napi::Result<()> {
  let pool = ThreadPoolBuilder::new()
    .num_threads(num_threads as usize)
    .build()
    .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?;
  THREAD_POOL.set(pool).map_err(|_| {
    napi::Error::new(
      napi::Status::GenericFailure,
      "slacc is already initialized".to_string(),
    )
  })?;
  Ok(())
}
