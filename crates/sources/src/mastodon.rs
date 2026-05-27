//! Mastodon source adapter.
//!
//! Reads a token-frequency cache file produced by Stage A
//! (`niinku ingest mastodon`). Format is `word count` per line — same
//! shape as the OpenSubtitles cache.
//!
//! The HTTP-fetching code lives in the CLI's ingest subcommand so this
//! adapter — which Stage B calls — has no network or async deps and
//! stays trivially testable.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use niinku_pipeline::{Count, FreqTable};

pub struct Mastodon {
    path: PathBuf,
}

impl Mastodon {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl crate::Source for Mastodon {
    fn name(&self) -> &str {
        "mastodon"
    }

    fn fetch(&self) -> Result<FreqTable> {
        let f =
            File::open(&self.path).with_context(|| format!("opening {}", self.path.display()))?;
        read_freq_table(BufReader::new(f))
    }
}

fn read_freq_table<R: BufRead>(reader: R) -> Result<FreqTable> {
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
        let count: Count = parts
            .next()
            .with_context(|| format!("line {}: missing count", lineno + 1))?
            .parse()
            .with_context(|| format!("line {}: count not a u64", lineno + 1))?;
        t.insert(word, count);
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn parses_cached_word_count_pairs() {
        let data = "moro 42\nläppä 17\nniinku 99\n";
        let t = read_freq_table(Cursor::new(data)).unwrap();
        assert_eq!(t.get("moro"), 42);
        assert_eq!(t.get("läppä"), 17);
        assert_eq!(t.get("niinku"), 99);
        assert_eq!(t.len(), 3);
    }

    #[test]
    fn skips_blank_and_comment_lines() {
        let data = "# fetched 2026-05-27\n\nmoro 10\n# from 1000 posts\nläppä 5\n";
        let t = read_freq_table(Cursor::new(data)).unwrap();
        assert_eq!(t.len(), 2);
    }
}
