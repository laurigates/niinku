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
use std::io::{self, BufRead, Write};

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

/// A morphological judge — typically a thin wrapper around libvoikko, but
/// the trait is left abstract so the pipeline stays pure and tests can
/// drive a mock without requiring the Voikko C library.
pub trait Lexicon {
    /// Return `true` if `word` is a recognised standard-language form.
    fn accepts(&self, word: &str) -> bool;
}

/// Drop tokens the lexicon accepts (those are kirjakieli, already covered
/// by HeliBoard's `main_fi`) — keep tokens the lexicon rejects, since
/// "frequent in colloquial corpus AND rejected by the standard-language
/// morphology" is the puhekieli/slang signal. The allowlist overrides:
/// listed tokens are kept even if accepted.
pub fn apply_kirjakieli_filter<L: Lexicon>(
    mut table: FreqTable,
    lexicon: &L,
    allowlist: &HashSet<String>,
) -> FreqTable {
    table
        .counts
        .retain(|word, _| allowlist.contains(word) || !lexicon.accepts(word));
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

/// HeliBoard `.combined` header. All five fields are required by
/// `dicttool_aosp.jar`; any non-`main` `dict_type` makes this an
/// additional dictionary loaded alongside HeliBoard's `main_fi`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombinedHeader {
    /// Dictionary kind. `main` substitutes the locale's primary dict;
    /// any other string (e.g. `puhekieli`, `slang`, `emoji`) loads as
    /// an additional dictionary.
    pub dict_type: String,
    /// Type-suffix locale, lowercased (e.g. `fi`).
    pub dict_locale: String,
    /// BCP 47 locale tag (e.g. `fi_FI`).
    pub locale: String,
    /// Human-readable label.
    pub description: String,
    /// Unix timestamp in seconds.
    pub date: i64,
    /// Integer version. HeliBoard accepts any value; stock AOSP needs >18.
    pub version: u32,
}

/// Emit the single-line `.combined` header HeliBoard's loader expects:
/// `dictionary=<type>:<dict_locale>,locale=<locale>,description=<desc>,date=<ts>,version=<v>`
pub fn emit_combined_header(h: &CombinedHeader, w: &mut impl Write) -> io::Result<()> {
    writeln!(
        w,
        "dictionary={}:{},locale={},description={},date={},version={}",
        h.dict_type, h.dict_locale, h.locale, h.description, h.date, h.version
    )
}

/// Read one token per line from a denylist/allowlist file. Blank lines
/// and lines starting with `#` are skipped; remaining lines are trimmed.
pub fn read_token_list<R: BufRead>(reader: R) -> io::Result<HashSet<String>> {
    let mut out = HashSet::new();
    for line in reader.lines() {
        let line = line?;
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        out.insert(t.to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deny(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// Test lexicon backed by an explicit acceptance set — stand-in for
    /// libvoikko so the pipeline's tests stay hermetic.
    struct StaticLexicon {
        accepted: HashSet<String>,
    }

    impl Lexicon for StaticLexicon {
        fn accepts(&self, word: &str) -> bool {
            self.accepted.contains(word)
        }
    }

    fn lex(items: &[&str]) -> StaticLexicon {
        StaticLexicon {
            accepted: items.iter().map(|s| s.to_string()).collect(),
        }
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
    fn kirjakieli_filter_drops_accepted_keeps_rejected() {
        // kissa/koira are kirjakieli (accepted); mä/niinku are puhekieli
        // (rejected).
        let t = FreqTable::from_pairs([("kissa", 10), ("koira", 8), ("mä", 50), ("niinku", 20)]);
        let l = lex(&["kissa", "koira"]);
        let out = apply_kirjakieli_filter(t, &l, &HashSet::new());
        assert_eq!(out.len(), 2);
        assert_eq!(out.get("mä"), 50);
        assert_eq!(out.get("niinku"), 20);
        assert_eq!(out.get("kissa"), 0);
    }

    #[test]
    fn allowlist_overrides_lexicon_acceptance() {
        // läppä is technically accepted by the lexicon but we want to
        // keep it as slang via the allowlist.
        let t = FreqTable::from_pairs([("läppä", 30), ("kissa", 10)]);
        let l = lex(&["läppä", "kissa"]);
        let allow: HashSet<String> = ["läppä"].iter().map(|s| s.to_string()).collect();
        let out = apply_kirjakieli_filter(t, &l, &allow);
        assert_eq!(out.len(), 1);
        assert_eq!(out.get("läppä"), 30);
        assert_eq!(out.get("kissa"), 0);
    }

    #[test]
    fn read_token_list_skips_blanks_and_comments() {
        let data = "# header\n\nmoro\n# mid\nläppä\n  spaced  \n";
        let set = read_token_list(std::io::Cursor::new(data)).unwrap();
        assert_eq!(set.len(), 3);
        assert!(set.contains("moro"));
        assert!(set.contains("läppä"));
        assert!(set.contains("spaced"));
    }

    #[test]
    fn emit_combined_header_matches_heliboard_format() {
        let h = CombinedHeader {
            dict_type: "puhekieli".into(),
            dict_locale: "fi".into(),
            locale: "fi_FI".into(),
            description: "Finnish puhekieli".into(),
            date: 1716800000,
            version: 1,
        };
        let mut buf = Vec::new();
        emit_combined_header(&h, &mut buf).unwrap();
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "dictionary=puhekieli:fi,locale=fi_FI,description=Finnish puhekieli,date=1716800000,version=1\n"
        );
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
