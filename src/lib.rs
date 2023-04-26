#![deny(clippy::all)]

use aho_corasick::AhoCorasick;

#[macro_use]
extern crate napi_derive;

pub struct Slacc {
  ac: AhoCorasick,
}

#[napi(js_name = "Slacc")]
pub struct JsSlacc {
  slacc: Slacc,
}

#[napi]
impl JsSlacc {
  #[napi(factory)]
  pub fn with_patterns(patterns: Vec<String>) -> Self {
    let ac = AhoCorasick::new(patterns).unwrap();
    Self {
      slacc: Slacc { ac },
    }
  }

  #[napi]
  pub fn is_match(&self, input: String) -> bool {
    self.slacc.ac.is_match(&input)
  }
}
