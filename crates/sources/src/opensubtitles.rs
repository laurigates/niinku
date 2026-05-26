//! OpenSubtitles Finnish frequency adapter (stub).
//!
//! OpenSubtitles publishes a `fi.txt`-style frequency file derived from
//! Finnish subtitles — conversational register, openly available. Stage A
//! downloads and tokenises it; this adapter reads the resulting cached
//! frequency table.

use anyhow::Result;
use niinku_pipeline::FreqTable;

pub struct OpenSubtitles;

impl crate::Source for OpenSubtitles {
    fn name(&self) -> &str {
        "opensubtitles-fi"
    }

    fn fetch(&self) -> Result<FreqTable> {
        anyhow::bail!("opensubtitles adapter not yet implemented")
    }
}
