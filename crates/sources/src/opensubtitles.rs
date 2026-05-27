//! OpenSubtitles Finnish frequency adapter.
//!
//! Reads a Hermit Dave-style frequency file (one `word count` per line,
//! space-separated, sorted desc) into a [`FreqTable`]. The file is
//! expected to live in `data/cached/` after `just download`.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use niinku_pipeline::{Count, FreqTable};

pub struct OpenSubtitles {
    path: PathBuf,
}

impl OpenSubtitles {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl crate::Source for OpenSubtitles {
    fn name(&self) -> &str {
        "opensubtitles-fi"
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
    fn parses_word_count_pairs() {
        let data = "mä 1234\noon 987\ntää 500\n";
        let t = read_freq_table(Cursor::new(data)).unwrap();
        assert_eq!(t.get("mä"), 1234);
        assert_eq!(t.get("oon"), 987);
        assert_eq!(t.get("tää"), 500);
        assert_eq!(t.len(), 3);
    }

    #[test]
    fn skips_blank_and_comment_lines() {
        let data = "# header\n\nmä 10\n# mid\noon 5\n";
        let t = read_freq_table(Cursor::new(data)).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t.get("mä"), 10);
    }

    #[test]
    fn errors_on_malformed_count() {
        let data = "mä notanumber\n";
        assert!(read_freq_table(Cursor::new(data)).is_err());
    }
}
