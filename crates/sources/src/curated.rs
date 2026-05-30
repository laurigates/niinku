//! Curated puhekieli/slang seed list.
//!
//! Unlike the corpus adapters, this source is hand-authored and committed
//! to the repo (`data/curated-fi.txt`). It guarantees that known-good
//! colloquial forms — reduced question forms (`oliks`, `saaks`), clitic
//! chains (`miksköhän`, `saakohan`), and pronoun/verb contractions (`mä`,
//! `oon`) — land in the dictionary even when no fetched corpus happens to
//! contain them often enough to clear the min-count floor, or when
//! libvoikko would accept a form and the kirjakieli filter would otherwise
//! drop it (the assemble step adds every curated token to the allowlist).
//!
//! Format: one entry per line, `word` or `word <weight>`. A bare word gets
//! [`DEFAULT_BASELINE`]; an explicit integer weight lets high-frequency
//! function words be tiered above rarer slang after log-scoring. Blank
//! lines and `#` comments are skipped.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use niinku_pipeline::{Count, FreqTable};

/// Baseline count for a curated entry that omits an explicit weight.
/// Matches the Urbaani headword baseline so bare curated words and slang
/// headwords land in the same `f` band.
pub const DEFAULT_BASELINE: Count = 1_000;

pub struct Curated {
    path: PathBuf,
    baseline: Count,
}

impl Curated {
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

impl crate::Source for Curated {
    fn name(&self) -> &str {
        "curated"
    }

    fn fetch(&self) -> Result<FreqTable> {
        let f =
            File::open(&self.path).with_context(|| format!("opening {}", self.path.display()))?;
        read_curated(BufReader::new(f), self.baseline)
    }
}

fn read_curated<R: BufRead>(reader: R, baseline: Count) -> Result<FreqTable> {
    let mut t = FreqTable::new();
    for (lineno, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("reading line {}", lineno + 1))?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let word = match parts.next() {
            Some(w) => w,
            None => continue,
        };
        let count: Count = match parts.next() {
            Some(c) => c
                .parse()
                .with_context(|| format!("line {}: weight not a u64", lineno + 1))?,
            None => baseline,
        };
        t.insert(word, count);
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn bare_words_get_baseline() {
        let data = "oliks\nsaaks\nmiksköhän\n";
        let t = read_curated(Cursor::new(data), 1000).unwrap();
        assert_eq!(t.len(), 3);
        assert_eq!(t.get("oliks"), 1000);
        assert_eq!(t.get("saaks"), 1000);
        assert_eq!(t.get("miksköhän"), 1000);
    }

    #[test]
    fn explicit_weight_overrides_baseline() {
        let data = "mä 5000\noon 4000\n";
        let t = read_curated(Cursor::new(data), 1000).unwrap();
        assert_eq!(t.get("mä"), 5000);
        assert_eq!(t.get("oon"), 4000);
    }

    #[test]
    fn mixes_weighted_and_bare_entries() {
        let data = "mä 5000\noliks\n";
        let t = read_curated(Cursor::new(data), 1000).unwrap();
        assert_eq!(t.get("mä"), 5000);
        assert_eq!(t.get("oliks"), 1000);
    }

    #[test]
    fn skips_blank_and_comment_lines() {
        let data = "# puhekieli seed\n\nmä 5000\n# pronouns done\noliks\n";
        let t = read_curated(Cursor::new(data), 1000).unwrap();
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn errors_on_malformed_weight() {
        let data = "mä notanumber\n";
        assert!(read_curated(Cursor::new(data), 1000).is_err());
    }

    #[test]
    fn duplicate_words_sum() {
        let data = "miks 1000\nmiks 500\n";
        let t = read_curated(Cursor::new(data), 1000).unwrap();
        assert_eq!(t.get("miks"), 1500);
    }

    /// Guards the committed seed list against the silent-summing footgun:
    /// because [`read_curated`] *sums* duplicate tokens (see
    /// [`duplicate_words_sum`]), a token accidentally listed twice in
    /// `data/curated-fi.txt` would inflate its weight without any error. The
    /// file is hand-edited and grows via PR, so assert every token is unique.
    /// Also enforces the format invariants the header documents: lowercase
    /// tokens and well-formed `word [<weight>]` lines.
    #[test]
    fn committed_seed_list_has_no_duplicates() {
        use std::collections::HashMap;

        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/curated-fi.txt");
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {path}: {e}"));

        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut dups: Vec<String> = Vec::new();
        let mut not_lowercase: Vec<String> = Vec::new();

        for (i, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let mut parts = trimmed.split_whitespace();
            let word = parts
                .next()
                .expect("non-empty line has a token")
                .to_string();
            // The optional second field must parse as a weight; anything else
            // (e.g. a stray token) is a malformed line.
            if let Some(w) = parts.next() {
                w.parse::<Count>()
                    .unwrap_or_else(|_| panic!("line {}: weight `{w}` is not a u64", i + 1));
            }
            assert!(
                parts.next().is_none(),
                "line {}: expected `word [<weight>]`, got extra fields: {trimmed:?}",
                i + 1
            );

            if word != word.to_lowercase() {
                not_lowercase.push(word.clone());
            }
            if let Some(first) = seen.insert(word.clone(), i + 1) {
                dups.push(format!("`{word}` (lines {first} and {})", i + 1));
            }
        }

        assert!(
            not_lowercase.is_empty(),
            "curated-fi.txt entries must be lowercase: {not_lowercase:?}"
        );
        assert!(
            dups.is_empty(),
            "duplicate tokens in curated-fi.txt (their weights would silently sum): {dups:?}"
        );
    }
}
