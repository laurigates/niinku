//! VoikkoLexicon — wraps libvoikko's Finnish morphological recogniser
//! and exposes it through [`niinku_pipeline::Lexicon`].
//!
//! "Accepts" maps to `SpellReturn::SpellOk`. Internal / charset errors
//! from libvoikko are treated as non-acceptance so the pipeline does
//! not drop a word it couldn't actually analyse.

use anyhow::{Context, Result};
use niinku_pipeline::Lexicon;
use voikko_rs::voikko::{SpellReturn, Voikko};

pub struct VoikkoLexicon {
    voikko: Voikko,
}

impl VoikkoLexicon {
    /// Initialise with the default "fi" language and the system
    /// dictionary search path.
    pub fn new() -> Result<Self> {
        Self::with_path(None)
    }

    /// Initialise pointing at an explicit dictionary path (e.g. when
    /// libvoikko isn't installed in a default location).
    pub fn with_path(path: Option<&str>) -> Result<Self> {
        let voikko = Voikko::new("fi", path)
            .map_err(|e| anyhow::anyhow!("voikko init failed: {e:?}"))
            .context("initialising libvoikko (is libvoikko installed?)")?;
        Ok(Self { voikko })
    }
}

impl Lexicon for VoikkoLexicon {
    fn accepts(&self, word: &str) -> bool {
        matches!(self.voikko.spell(word), SpellReturn::SpellOk)
    }
}
