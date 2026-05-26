//! niinku-pipeline: pure functions for building a Finnish puhekieli/slang
//! dictionary for HeliBoard.
//!
//! Stage B (assemble) consumes per-source frequency tables, merges them,
//! applies allow/deny lists and a min-count floor, scores each token onto
//! HeliBoard's 0..=255 `f` scale, and emits a `.combined` wordlist body.
//!
//! Voikko-based morphological filtering and the dictionary header are
//! intentionally not implemented yet — see Open questions in README.

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

pub type Count = u64;

/// Token-to-count mapping for one source or a merged set.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FreqTable {
    counts: HashMap<String, Count>,
}

impl FreqTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_pairs<I, S>(it: I) -> Self
    where
        I: IntoIterator<Item = (S, Count)>,
        S: Into<String>,
    {
        let mut t = Self::new();
        for (k, v) in it {
            t.insert(k.into(), v);
        }
        t
    }

    /// Add `count` to the running total for `token`.
    pub fn insert(&mut self, token: impl Into<String>, count: Count) {
        *self.counts.entry(token.into()).or_insert(0) += count;
    }

    pub fn get(&self, token: &str) -> Count {
        self.counts.get(token).copied().unwrap_or(0)
    }

    pub fn len(&self) -> usize {
        self.counts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Count)> {
        self.counts.iter()
    }
}

/// Merge frequency tables by summing counts per token.
pub fn merge<I>(tables: I) -> FreqTable
where
    I: IntoIterator<Item = FreqTable>,
{
    let mut out = FreqTable::new();
    for t in tables {
        for (k, v) in t.counts {
            *out.counts.entry(k).or_insert(0) += v;
        }
    }
    out
}

/// Drop every token listed in `denylist`.
pub fn apply_denylist(mut table: FreqTable, denylist: &HashSet<String>) -> FreqTable {
    table.counts.retain(|k, _| !denylist.contains(k));
    table
}

/// Drop tokens with count below `min_count`.
pub fn apply_min_count(mut table: FreqTable, min_count: Count) -> FreqTable {
    table.counts.retain(|_, c| *c >= min_count);
    table
}

/// A scored entry ready to emit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub word: String,
    pub freq: u8,
}

/// Map a count onto `freq_min..=freq_max` via a log curve so the head
/// doesn't drown out the tail. `count == 0` always maps to 0.
pub fn score_log(count: Count, max_count: Count, freq_min: u8, freq_max: u8) -> u8 {
    if count == 0 || max_count == 0 {
        return 0;
    }
    debug_assert!(freq_min <= freq_max);
    let lc = (count as f64).ln_1p();
    let lm = (max_count as f64).ln_1p();
    let ratio = (lc / lm).clamp(0.0, 1.0);
    let span = (freq_max - freq_min) as f64;
    (freq_min as f64 + ratio * span).round() as u8
}

/// Score every token in `table` and return entries sorted by freq desc,
/// word asc as the tiebreaker (stable output for diffs in PRs).
pub fn score_table(table: &FreqTable, freq_min: u8, freq_max: u8) -> Vec<Entry> {
    let max_count = table.counts.values().copied().max().unwrap_or(0);
    let mut entries: Vec<Entry> = table
        .iter()
        .map(|(w, c)| Entry {
            word: w.clone(),
            freq: score_log(*c, max_count, freq_min, freq_max),
        })
        .collect();
    entries.sort_by(|a, b| b.freq.cmp(&a.freq).then(a.word.cmp(&b.word)));
    entries
}

/// Emit entries as `.combined` body lines (no header).
pub fn emit_combined_body(entries: &[Entry], w: &mut impl Write) -> io::Result<()> {
    for e in entries {
        writeln!(w, " word={},f={}", e.word, e.freq)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deny(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn insert_sums_duplicates() {
        let mut t = FreqTable::new();
        t.insert("mä", 3);
        t.insert("mä", 2);
        assert_eq!(t.get("mä"), 5);
    }

    #[test]
    fn merge_sums_overlapping_tokens() {
        let a = FreqTable::from_pairs([("mä", 10), ("oon", 5)]);
        let b = FreqTable::from_pairs([("mä", 3), ("tää", 1)]);
        let merged = merge([a, b]);
        assert_eq!(merged.get("mä"), 13);
        assert_eq!(merged.get("oon"), 5);
        assert_eq!(merged.get("tää"), 1);
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn denylist_drops_listed_tokens() {
        let t = FreqTable::from_pairs([("mä", 10), ("oon", 5), ("typo", 2)]);
        let out = apply_denylist(t, &deny(&["typo"]));
        assert_eq!(out.get("typo"), 0);
        assert_eq!(out.get("mä"), 10);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn min_count_drops_low_freq() {
        let t = FreqTable::from_pairs([("a", 10), ("b", 3), ("c", 1)]);
        let out = apply_min_count(t, 3);
        assert_eq!(out.len(), 2);
        assert_eq!(out.get("c"), 0);
    }

    #[test]
    fn score_log_endpoints() {
        assert_eq!(score_log(0, 100, 50, 200), 0);
        assert_eq!(score_log(100, 100, 50, 200), 200);
    }

    #[test]
    fn score_log_is_monotonic() {
        let lo = score_log(10, 1000, 50, 200);
        let mid = score_log(100, 1000, 50, 200);
        let hi = score_log(500, 1000, 50, 200);
        assert!(lo <= mid && mid <= hi);
    }

    #[test]
    fn score_table_sorts_freq_desc_then_word_asc() {
        let t = FreqTable::from_pairs([
            ("rare", 1u64),
            ("common", 100),
            ("medium", 20),
            ("common-twin", 100),
        ]);
        let entries = score_table(&t, 50, 200);
        assert_eq!(entries[0].word, "common");
        assert_eq!(entries[1].word, "common-twin");
        assert!(entries[0].freq >= entries[2].freq);
        assert_eq!(entries[3].word, "rare");
    }

    #[test]
    fn emit_combined_body_format() {
        let entries = vec![
            Entry {
                word: "mä".into(),
                freq: 220,
            },
            Entry {
                word: "oon".into(),
                freq: 210,
            },
        ];
        let mut buf = Vec::new();
        emit_combined_body(&entries, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s, " word=mä,f=220\n word=oon,f=210\n");
    }
}
