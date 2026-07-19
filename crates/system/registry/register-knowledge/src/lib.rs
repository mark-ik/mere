// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Knowledge registry — UDC (Universal Decimal Classification) tag
//! validation, hierarchical-distance scoring, fuzzy search, and
//! per-class color hints.
//!
//! Extracted from `registries/atomic/knowledge.rs` per Slice 52 of
//! the workspace architecture proposal. The original file lived
//! beside the canonical lens / protocol / viewer registries and
//! shared the same registry-shape: keyed namespace
//! (`udc:<code>` strings), value-type entries (`UdcEntry`), lookup
//! API (`get_label`, `parse`, `validate_tag`, `search`), and late
//! binding via `seed_defaults` (extensible to mod-provided seed
//! sets in future work).
//!
//! The seed dataset (`assets/knowledge/udc_seed.json`) travels with
//! this crate so external consumers don't need to copy it.

use std::collections::HashMap;

use kernel::color::Color32;
use nucleo::{
    Config, Matcher,
    pattern::{CaseMatching, Normalization, Pattern},
};
use parking_lot::Mutex;
use serde::Deserialize;

const UDC_SEED_JSON: &str = include_str!("../assets/knowledge/udc_seed.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KnowledgeProvider {
    Udc,
    Schema,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagValidationResult {
    Valid {
        canonical_code: String,
        display_label: String,
    },
    Unknown {
        suggestions: Vec<String>,
    },
    Malformed {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CompactCode(pub Vec<u8>);

impl CompactCode {
    pub fn distance(&self, other: &CompactCode) -> f32 {
        let len = self.0.len().min(other.0.len());
        let mut shared_prefix = 0;
        for i in 0..len {
            if self.0[i] == other.0[i] {
                shared_prefix += 1;
            } else {
                break;
            }
        }

        let max_len = self.0.len().max(other.0.len());
        if max_len == 0 {
            return 1.0;
        }

        1.0 - (shared_prefix as f32 / max_len as f32)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticClassVector {
    pub classes: Vec<CompactCode>,
    pub primary_code: Option<CompactCode>,
}

impl SemanticClassVector {
    pub fn from_codes(mut codes: Vec<CompactCode>) -> Self {
        codes.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.cmp(b)));
        codes.dedup();

        let primary_code = codes.first().cloned();
        Self {
            classes: codes,
            primary_code,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UdcEntry {
    pub code: String,
    pub label: String,
    search_text: String,
}

impl AsRef<str> for UdcEntry {
    fn as_ref(&self) -> &str {
        &self.search_text
    }
}

#[derive(Debug, Clone, Deserialize)]
struct SeedEntry {
    code: String,
    label: String,
}

pub struct KnowledgeRegistry {
    definitions: HashMap<String, String>,
    matcher: Mutex<Matcher>,
    search_items: Vec<UdcEntry>,
}

impl Default for KnowledgeRegistry {
    fn default() -> Self {
        let mut registry = Self {
            definitions: HashMap::new(),
            matcher: Mutex::new(Matcher::new(Config::DEFAULT)),
            search_items: Vec::new(),
        };
        registry.seed_defaults();
        registry
    }
}

impl KnowledgeRegistry {
    fn provider_for_tag(tag: &str) -> KnowledgeProvider {
        let trimmed = tag.trim();
        if trimmed.starts_with("udc:")
            || trimmed
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == ':' || c == '/')
        {
            return KnowledgeProvider::Udc;
        }
        if trimmed.starts_with("schema:") {
            return KnowledgeProvider::Schema;
        }
        KnowledgeProvider::Unknown
    }

    fn normalize_udc_code(raw: &str) -> Option<String> {
        let normalized = raw
            .trim()
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>();
        (!normalized.is_empty()).then_some(normalized)
    }

    fn parse_udc_code(code_str: &str) -> Option<CompactCode> {
        let bytes: Vec<u8> = code_str
            .chars()
            .filter(|c| c.is_ascii_digit())
            .map(|c| c as u8 - b'0')
            .collect();
        if bytes.is_empty() {
            return None;
        }
        Some(CompactCode(bytes))
    }

    fn seed_defaults(&mut self) {
        let defaults: Vec<SeedEntry> =
            serde_json::from_str(UDC_SEED_JSON).expect("udc seed json should parse");
        self.definitions.clear();
        self.search_items.clear();

        for SeedEntry { code, label } in defaults {
            self.definitions.insert(code.clone(), label.clone());
            self.search_items.push(UdcEntry {
                code: code.clone(),
                label: label.clone(),
                search_text: format!("{label} udc:{code} {code}"),
            });
        }
    }

    pub fn parse(&self, tag: &str) -> Option<CompactCode> {
        let trimmed = tag.trim();
        match Self::provider_for_tag(trimmed) {
            KnowledgeProvider::Udc => {
                let code = trimmed.strip_prefix("udc:").unwrap_or(trimmed);
                let normalized = Self::normalize_udc_code(code)?;
                if !self.definitions.contains_key(&normalized) {
                    return None;
                }
                Self::parse_udc_code(&normalized)
            }
            KnowledgeProvider::Schema | KnowledgeProvider::Unknown => None,
        }
    }

    pub fn canonicalize_tag(&self, tag: &str) -> Option<String> {
        let trimmed = tag.trim();
        match self.validate_tag(trimmed) {
            TagValidationResult::Valid { canonical_code, .. } => {
                Some(format!("udc:{canonical_code}"))
            }
            TagValidationResult::Unknown { .. } | TagValidationResult::Malformed { .. } => None,
        }
    }

    pub fn get_label(&self, code: &str) -> Option<&str> {
        let normalized = Self::normalize_udc_code(code.strip_prefix("udc:").unwrap_or(code))?;
        self.definitions.get(&normalized).map(|s| s.as_str())
    }

    pub fn validate(&self, tag: &str) -> bool {
        matches!(self.validate_tag(tag), TagValidationResult::Valid { .. })
    }

    pub fn validate_tag(&self, tag: &str) -> TagValidationResult {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return TagValidationResult::Malformed {
                reason: "tag is empty".to_string(),
            };
        }

        match Self::provider_for_tag(trimmed) {
            KnowledgeProvider::Udc => {
                let code = trimmed.strip_prefix("udc:").unwrap_or(trimmed);
                let Some(normalized) = Self::normalize_udc_code(code) else {
                    return TagValidationResult::Malformed {
                        reason: format!("'{trimmed}' is not a valid UDC code"),
                    };
                };
                let Some(label) = self.get_label(&normalized) else {
                    return TagValidationResult::Unknown {
                        suggestions: self.suggest_codes(&normalized),
                    };
                };
                TagValidationResult::Valid {
                    canonical_code: normalized,
                    display_label: label.to_string(),
                }
            }
            KnowledgeProvider::Schema => TagValidationResult::Unknown {
                suggestions: Vec::new(),
            },
            KnowledgeProvider::Unknown => TagValidationResult::Unknown {
                suggestions: self.suggest_codes(trimmed),
            },
        }
    }

    fn suggest_codes(&self, query: &str) -> Vec<String> {
        let mut suggestions = self
            .search(query)
            .into_iter()
            .map(|entry| format!("udc:{}", entry.code))
            .collect::<Vec<_>>();

        if suggestions.len() >= 3 {
            suggestions.truncate(3);
            return suggestions;
        }

        if let Some(target) =
            Self::normalize_udc_code(query).and_then(|code| Self::parse_udc_code(&code))
        {
            let mut distance_ranked = self
                .definitions
                .keys()
                .filter_map(|code| {
                    let compact = Self::parse_udc_code(code)?;
                    Some((target.distance(&compact), format!("udc:{code}")))
                })
                .collect::<Vec<_>>();
            distance_ranked.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            for (_, candidate) in distance_ranked {
                if suggestions.iter().any(|entry| entry == &candidate) {
                    continue;
                }
                suggestions.push(candidate);
                if suggestions.len() == 3 {
                    break;
                }
            }
        }

        suggestions.truncate(3);
        suggestions
    }

    pub fn distance(&self, a: &CompactCode, b: &CompactCode) -> f32 {
        a.distance(b)
    }

    pub fn semantic_distance(&self, a: &str, b: &str) -> Option<f32> {
        let a = self.parse(a)?;
        let b = self.parse(b)?;
        Some(self.distance(&a, &b))
    }

    pub fn get_color_hint(&self, code: &str) -> Option<Color32> {
        let normalized = Self::normalize_udc_code(code.strip_prefix("udc:").unwrap_or(code))?;
        let compact = Self::parse_udc_code(&normalized)?;
        Some(match compact.0.first().copied().unwrap_or_default() {
            0 => Color32::from_rgb(150, 150, 150),
            1 => Color32::from_rgb(180, 100, 200),
            2 => Color32::from_rgb(255, 140, 0),
            3 => Color32::from_rgb(100, 150, 250),
            5 => Color32::from_rgb(50, 200, 100),
            6 => Color32::from_rgb(0, 200, 200),
            7 => Color32::from_rgb(250, 100, 100),
            8 => Color32::from_rgb(250, 250, 100),
            9 => Color32::from_rgb(160, 100, 50),
            _ => Color32::GRAY,
        })
    }

    pub fn search(&self, query: &str) -> Vec<UdcEntry> {
        if query.trim().starts_with("schema:") {
            return Vec::new();
        }

        let normalized_query = query.trim().strip_prefix("udc:").unwrap_or(query.trim());
        let mut matcher = self.matcher.lock();
        let pattern = Pattern::parse(normalized_query, CaseMatching::Ignore, Normalization::Smart);
        let matches = pattern.match_list(&self.search_items, &mut matcher);

        matches
            .into_iter()
            .take(10)
            .map(|(item, _score)| item.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_recognizes_udc_tags() {
        let registry = KnowledgeRegistry::default();
        let code = registry.parse("udc:519.6").expect("udc tag should parse");
        assert_eq!(code.0, vec![5, 1, 9, 6]);
    }

    #[test]
    fn parse_accepts_plain_udc_codes() {
        let registry = KnowledgeRegistry::default();
        let code = registry
            .parse("519.6")
            .expect("plain UDC code should parse");
        assert_eq!(code.0, vec![5, 1, 9, 6]);
    }

    #[test]
    fn provider_routing_rejects_non_udc_prefixes_for_parse() {
        let registry = KnowledgeRegistry::default();
        assert!(registry.parse("schema:CreativeWork").is_none());
        assert!(!registry.validate("schema:CreativeWork"));
    }

    #[test]
    fn search_finds_label_first_query() {
        let registry = KnowledgeRegistry::default();
        let hits = registry.search("math");
        assert!(!hits.is_empty(), "math query should return UDC matches");
        assert!(
            hits.iter().any(|entry| entry.code == "51"),
            "mathematics code should be included in top matches"
        );
    }

    #[test]
    fn expanded_dataset_hierarchical_relationships() {
        let registry = KnowledgeRegistry::default();

        let math = registry.parse("udc:51").unwrap();
        let comp_math = registry.parse("udc:519").unwrap();
        let numerical = registry.parse("udc:519.6").unwrap();
        let probability = registry.parse("udc:519.2").unwrap();

        let dist_comp_to_numerical = registry.distance(&comp_math, &numerical);
        let dist_math_to_numerical = registry.distance(&math, &numerical);
        assert!(dist_comp_to_numerical < dist_math_to_numerical);

        let dist_comp_subfields = registry.distance(&numerical, &probability);
        let dist_to_parent = registry.distance(&numerical, &math);
        assert!(dist_comp_subfields < dist_to_parent);
    }

    #[test]
    fn search_finds_diverse_subjects() {
        let registry = KnowledgeRegistry::default();

        let cs_results = registry.search("computer");
        assert!(cs_results.iter().any(|entry| entry.code.starts_with("004")));

        let physics_results = registry.search("quantum");
        assert!(
            physics_results
                .iter()
                .any(|entry| entry.code.starts_with("539"))
        );

        let medicine_results = registry.search("surgery");
        assert!(
            medicine_results
                .iter()
                .any(|entry| entry.code.starts_with("617"))
        );

        let lit_results = registry.search("literature");
        assert!(lit_results.iter().any(|entry| entry.code.starts_with("82")));
    }

    #[test]
    fn distance_calculation_cross_domain() {
        let registry = KnowledgeRegistry::default();

        let math = registry.parse("udc:51").unwrap();
        let physics = registry.parse("udc:53").unwrap();
        let music = registry.parse("udc:78").unwrap();

        let math_to_physics = registry.distance(&math, &physics);
        let math_to_music = registry.distance(&math, &music);
        assert!(math_to_physics < math_to_music);
    }

    #[test]
    fn validate_tag_reports_canonical_label_and_suggestions() {
        let registry = KnowledgeRegistry::default();

        assert!(matches!(
            registry.validate_tag("519.6"),
            TagValidationResult::Valid { canonical_code, display_label }
                if canonical_code == "519.6" && display_label == "Computational mathematics"
        ));

        assert!(matches!(
            registry.validate_tag("519.99"),
            TagValidationResult::Unknown { suggestions } if suggestions.len() == 3
        ));
    }

    #[test]
    fn seed_floor_exposes_expected_labels_and_colors() {
        let registry = KnowledgeRegistry::default();
        assert_eq!(
            registry.get_label("5"),
            Some("Mathematics and natural sciences")
        );
        assert_eq!(
            registry.get_color_hint("7"),
            Some(Color32::from_rgb(250, 100, 100))
        );
    }

    #[test]
    fn semantic_distance_uses_canonical_tag_strings() {
        let registry = KnowledgeRegistry::default();
        let numerical_to_probability = registry
            .semantic_distance("udc:519.6", "519.2")
            .expect("semantic distance should resolve");
        let numerical_to_music = registry
            .semantic_distance("udc:519.6", "78")
            .expect("semantic distance should resolve");

        assert!(numerical_to_probability < numerical_to_music);
    }
}
