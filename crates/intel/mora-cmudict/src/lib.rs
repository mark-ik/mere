//! A first-party offline CMUdict provider for [`mora`].
//!
//! Mora deliberately accepts phones rather than owning spelling or a lexicon.
//! This companion keeps the separately licensed English data outside that core
//! while giving consumers a useful local default.

use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

use mora::Phone;
use mora::english::{cmudict_entry, pronounce};

const EMBEDDED_DICTIONARY: &str = include_str!("../data/cmudict.dict");

/// A parsed pronunciation dictionary. Each word retains every listed variant.
#[derive(Debug)]
pub struct Cmudict {
    entries: HashMap<String, Vec<Vec<Phone>>>,
    pronunciation_count: usize,
}

/// A malformed dictionary line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmudictError {
    pub line: usize,
    pub entry: String,
}

impl fmt::Display for CmudictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid CMUdict entry on line {}: {}",
            self.line, self.entry
        )
    }
}

impl std::error::Error for CmudictError {}

impl Cmudict {
    /// Parse CMUdict-shaped text, preserving alternate pronunciations.
    pub fn parse(source: &str) -> Result<Self, CmudictError> {
        let mut entries: HashMap<String, Vec<Vec<Phone>>> = HashMap::new();
        let mut pronunciation_count = 0;

        for (offset, line) in source.lines().enumerate() {
            let line_number = offset + 1;
            let entry = line
                .split_once('#')
                .map_or(line, |(entry, _)| entry)
                .trim_end();
            let trimmed = entry.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(";;;") {
                continue;
            }

            let (word, pronunciation) = cmudict_entry(entry).ok_or_else(|| CmudictError {
                line: line_number,
                entry: line.to_owned(),
            })?;
            let phones = pronounce(pronunciation).ok_or_else(|| CmudictError {
                line: line_number,
                entry: line.to_owned(),
            })?;
            let variants = entries.entry(normalize(word)).or_default();
            if !variants.contains(&phones) {
                variants.push(phones);
                pronunciation_count += 1;
            }
        }

        Ok(Self {
            entries,
            pronunciation_count,
        })
    }

    /// The bundled CMUSphinx dictionary, parsed once at first use.
    pub fn embedded() -> &'static Self {
        static DICTIONARY: OnceLock<Cmudict> = OnceLock::new();
        DICTIONARY.get_or_init(|| {
            Cmudict::parse(EMBEDDED_DICTIONARY)
                .expect("the bundled CMUdict is validated by mora-cmudict tests")
        })
    }

    /// Every pronunciation recorded for `word`, or `None` when it is unknown.
    pub fn pronunciations(&self, word: &str) -> Option<&[Vec<Phone>]> {
        self.entries.get(&normalize(word)).map(Vec::as_slice)
    }

    /// Number of distinct normalized words.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Number of distinct pronunciations, including alternate readings.
    pub fn pronunciation_count(&self) -> usize {
        self.pronunciation_count
    }
}

fn normalize(word: &str) -> String {
    word.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mora::english::SYLLABLE_RULE;
    use mora::sonance::is_perfect_rhyme;
    use mora::syllable::syllabify;

    #[test]
    fn a_small_dictionary_preserves_variants_and_reports_unknown_words() {
        let dictionary = Cmudict::parse("READ R EH1 D\nREAD(2) R IY1 D\nCAT K AE1 T\n").unwrap();
        assert_eq!(dictionary.entry_count(), 2);
        assert_eq!(dictionary.pronunciation_count(), 3);
        assert_eq!(dictionary.pronunciations("read").unwrap().len(), 2);
        assert!(dictionary.pronunciations("missing").is_none());
    }

    #[test]
    fn embedded_dictionary_supplies_real_mora_pronunciations() {
        let dictionary = Cmudict::embedded();
        let cat = &dictionary.pronunciations("cat").unwrap()[0];
        let hat = &dictionary.pronunciations("HAT").unwrap()[0];
        let cat_syllables = syllabify(cat, SYLLABLE_RULE);
        let hat_syllables = syllabify(hat, SYLLABLE_RULE);
        assert!(is_perfect_rhyme(
            (cat, &cat_syllables),
            (hat, &hat_syllables)
        ));
        assert!(dictionary.entry_count() > 100_000);
        assert!(dictionary.pronunciation_count() > dictionary.entry_count());
    }
}
