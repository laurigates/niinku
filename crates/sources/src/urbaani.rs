//! Urbaani Sanakirja headword adapter (stub).
//!
//! Headwords only — never the definitions. Headwords get a high baseline
//! count so they survive the min-count floor.

use anyhow::Result;
use niinku_pipeline::FreqTable;

pub struct UrbaaniSanakirja;

impl crate::Source for UrbaaniSanakirja {
    fn name(&self) -> &str {
        "urbaani"
    }

    fn fetch(&self) -> Result<FreqTable> {
        anyhow::bail!("urbaani adapter not yet implemented")
    }
}
