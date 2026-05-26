//! Mastodon public-timeline adapter (stub).
//!
//! Pulls Finnish-language posts via the public timeline API (no auth),
//! tokenises them, and discards the raw text — only counts survive.

use anyhow::Result;
use niinku_pipeline::FreqTable;

pub struct Mastodon {
    pub instance: String,
}

impl crate::Source for Mastodon {
    fn name(&self) -> &str {
        "mastodon"
    }

    fn fetch(&self) -> Result<FreqTable> {
        anyhow::bail!("mastodon adapter not yet implemented")
    }
}
