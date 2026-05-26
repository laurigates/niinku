//! Suomi24 corpus adapter (stub).
//!
//! Consumes a derived frequency table from the Kielipankki Suomi24 corpus.
//! Per README licensing notes, raw text is not redistributable; the derived
//! frequency table is. This adapter expects the Stage A pipeline to have
//! produced `data/cached/suomi24.tsv` from a local Kielipankki extract.

use anyhow::Result;
use niinku_pipeline::FreqTable;

pub struct Suomi24;

impl crate::Source for Suomi24 {
    fn name(&self) -> &str {
        "suomi24"
    }

    fn fetch(&self) -> Result<FreqTable> {
        anyhow::bail!("suomi24 adapter not yet implemented")
    }
}
