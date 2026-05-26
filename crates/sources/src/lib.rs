//! niinku-sources: adapters that produce per-source frequency tables.
//!
//! Each adapter implements [`Source`] and is responsible for fetching its
//! corpus (or a cached snapshot), tokenising it, and discarding raw text —
//! only aggregate counts leave this crate (see README "Privacy & licensing").

use anyhow::Result;
use niinku_pipeline::FreqTable;

pub trait Source {
    /// Stable identifier, used for cache filenames (e.g. "opensubtitles-fi").
    fn name(&self) -> &str;

    /// Produce a frequency table. Adapters may read from `data/cached/` or
    /// fetch live; the trait does not distinguish.
    fn fetch(&self) -> Result<FreqTable>;
}

pub mod mastodon;
pub mod opensubtitles;
pub mod suomi24;
pub mod urbaani;
