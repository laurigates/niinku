//! Urbaani Sanakirja headword adapter.
//!
//! Reads one headword per line from a cached file (no definitions, ever).
//! Each headword gets the same baseline count — high enough to survive
//! the min-count floor when merged with corpus tables.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use niinku_pipeline::{Count, FreqTable};

/// Default baseline count for each Urbaani headword. Tuned to land in
/// the slang-content-word `f` band after log-scoring against a corpus
/// whose top words are in the millions.
pub const DEFAULT_BASELINE: Count = 1_000;

pub struct UrbaaniSanakirja {
    path: PathBuf,
    baseline: Count,
}

impl UrbaaniSanakirja {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            baseline: DEFAULT_BASELINE,
        }
    }

    pub fn with_baseline(mut self, baseline: Count) -> Self {
        self.baseline = baseline;
        self
    }
}

impl crate::Source for UrbaaniSanakirja {
    fn name(&self) -> &str {
        "urbaani"
    }

    fn fetch(&self) -> Result<FreqTable> {
        let f =
            File::open(&self.path).with_context(|| format!("opening {}", self.path.display()))?;
        read_headwords(BufReader::new(f), self.baseline)
    }
}

fn read_headwords<R: BufRead>(reader: R, baseline: Count) -> Result<FreqTable> {
    let mut t = FreqTable::new();
    for (lineno, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("reading line {}", lineno + 1))?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        t.insert(trimmed, baseline);
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn assigns_baseline_count_per_headword() {
        let data = "niinku\nmoro\nläppä\n";
        let t = read_headwords(Cursor::new(data), 500).unwrap();
        assert_eq!(t.len(), 3);
        assert_eq!(t.get("niinku"), 500);
        assert_eq!(t.get("moro"), 500);
        assert_eq!(t.get("läppä"), 500);
    }

    #[test]
    fn skips_blank_and_comment_lines() {
        let data = "# slang list\n\nmoro\n# note\nläppä\n";
        let t = read_headwords(Cursor::new(data), 1).unwrap();
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn duplicate_headwords_sum() {
        let data = "moro\nmoro\n";
        let t = read_headwords(Cursor::new(data), 100).unwrap();
        assert_eq!(t.get("moro"), 200);
    }
}
