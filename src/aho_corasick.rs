use aho_corasick;

pub struct AhoCorasick {
  ac: aho_corasick::AhoCorasick,
}

#[napi(js_name = "AhoCorasick")]
pub struct JsAhoCorasick {
  aho_corasick: AhoCorasick,
}

#[napi]
impl JsAhoCorasick {
  #[napi(factory)]
  pub fn with_patterns(patterns: Vec<String>) -> Self {
    let ac = aho_corasick::AhoCorasick::new(patterns).unwrap();
    Self {
      aho_corasick: AhoCorasick { ac },
    }
  }

  #[napi]
  pub fn is_match(&self, input: String) -> bool {
    self.aho_corasick.ac.is_match(&input)
  }
}
