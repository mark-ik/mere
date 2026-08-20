//! Read-only Rosette projection over a document interior.
//!
//! The adapter discloses line and stanza spans as scene items, then uses Mora
//! to derive visible sound relations. It never writes those derived relations
//! into document or graph truth.

use mora::Phone;
use mora::english::{SYLLABLE_RULE, WEIGHT_RULE};
use mora::meter::{Beat, Foot, Mode, beats, scan_best};
use mora::sonance::{is_perfect_rhyme, is_slant_rhyme};
use mora::syllable::syllabify;
use sceno::{
    Footprint, InstanceId, ProjectedItem, Rect, Representation, RoutedRelation, Scene, Size2,
    SourceRef, Transform2, Vec2,
};

const LINE_REPRESENTATION: &str = "knot.rosette.line";
const STANZA_REPRESENTATION: &str = "knot.rosette.stanza";
const PERFECT_RHYME: &str = "mora.perfect-rhyme";
const SLANT_RHYME: &str = "mora.slant-rhyme";

/// Supplies every known pronunciation for one normalized token.
///
/// The trait lives at the consumer boundary so Mora remains a phone-level
/// engine and writers may replace Knot's bundled English default.
pub trait PronunciationLexicon {
    fn pronunciations(&self, token: &str) -> Option<&[Vec<Phone>]>;
}

impl PronunciationLexicon for mora_cmudict::Cmudict {
    fn pronunciations(&self, token: &str) -> Option<&[Vec<Phone>]> {
        self.pronunciations(token)
    }
}

/// Knot's offline first-party English default.
#[derive(Debug, Clone, Copy, Default)]
pub struct CmudictPronunciations;

impl PronunciationLexicon for CmudictPronunciations {
    fn pronunciations(&self, token: &str) -> Option<&[Vec<Phone>]> {
        mora_cmudict::Cmudict::embedded().pronunciations(token)
    }
}

/// User-configurable Rosette geometry. Values are scene units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RosetteConfig {
    /// Radius of the line wheel.
    pub radius: f32,
    /// Radius of the inner stanza wheel.
    pub stanza_radius: f32,
    /// Disclosed footprint for a line item.
    pub line_footprint: Size2,
    /// Disclosed footprint for a stanza item.
    pub stanza_footprint: Size2,
    /// Angle of the first line item.
    pub start_angle_radians: f32,
    /// Whether terminal perfect-rhyme chords are derived.
    pub perfect_rhyme: bool,
    /// Whether terminal slant-rhyme chords are derived.
    pub slant_rhyme: bool,
    /// Whether line-level accentual scansion is returned.
    pub meter: bool,
}

impl Default for RosetteConfig {
    fn default() -> Self {
        Self {
            radius: 220.0,
            stanza_radius: 112.0,
            line_footprint: Size2::new(176.0, 48.0),
            stanza_footprint: Size2::new(72.0, 28.0),
            start_angle_radians: -std::f32::consts::FRAC_PI_2,
            perfect_rhyme: true,
            slant_rhyme: true,
            meter: true,
        }
    }
}

/// One token Mora could not analyze because the lexicon had no pronunciation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedToken {
    /// Zero-based projected line ordinal.
    pub line: u32,
    /// Inclusive byte offset in the authored source.
    pub byte_start: usize,
    /// Exclusive byte offset in the authored source.
    pub byte_end: usize,
    /// Authored token text.
    pub token: String,
}

/// Explicit coverage for the pronunciation-dependent projection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LexiconCoverage {
    /// Number of word tokens offered to the pronunciation provider.
    pub total_tokens: usize,
    /// Number of tokens with at least one pronunciation.
    pub resolved_tokens: usize,
    /// Tokens left unresolved rather than guessed.
    pub unresolved: Vec<UnresolvedToken>,
}

/// The portable scene plus the analysis coverage needed to judge it honestly.
#[derive(Debug, Clone, PartialEq)]
pub struct RosetteProjection {
    /// Portable scene containing the wheel and its sound-derived chords.
    pub scene: Scene,
    /// Explicit pronunciation coverage for this source.
    pub coverage: LexiconCoverage,
    /// Source ranges presented by the scene's items.
    pub interiors: Vec<RosetteInterior>,
    /// Derived accentual meter for lines with at least one resolved token.
    pub meter: Vec<LineMeter>,
}

/// A line-level metrical reading derived from the selected pronunciations.
#[derive(Debug, Clone, PartialEq)]
pub struct LineMeter {
    /// Zero-based projected line ordinal.
    pub line: u32,
    /// Stress pattern in source order.
    pub beats: Vec<MetricalBeat>,
    /// Best common foot for the available pronunciation coverage.
    pub foot: MetricalFoot,
    /// Number of repetitions of `foot` in the best scan.
    pub feet: usize,
    /// Share of compared positions matching that meter.
    pub fit: f32,
    /// Difference between observed syllables and expected positions.
    pub overrun: isize,
    /// Whether every position matched with no overrun.
    pub regular: bool,
    /// Tokens represented in this scan. Unresolved tokens remain in coverage.
    pub resolved_tokens: usize,
}

/// Portable stress strength used by [`LineMeter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricalBeat {
    Weak,
    Strong,
}

/// Portable names for the common feet Mora scans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricalFoot {
    Iamb,
    Trochee,
    Dactyl,
    Anapest,
}

/// Which document interior one Rosette item presents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RosetteInteriorKind {
    /// One non-empty authored line.
    Line,
    /// One blank-line-delimited stanza.
    Stanza,
}

/// Stable source coordinates for one item in the projected scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RosetteInterior {
    /// Dense scene instance assigned to this interior.
    pub instance: InstanceId,
    /// Whether the item presents a line or a stanza.
    pub kind: RosetteInteriorKind,
    /// Zero-based ordinal within its kind.
    pub ordinal: u32,
    /// Inclusive byte offset in the authored source.
    pub byte_start: usize,
    /// Exclusive byte offset in the authored source.
    pub byte_end: usize,
}

/// Project a document interior as a read-only Rosette.
///
/// Non-empty lines are placed around the wheel in source order. Stanzas occupy
/// an inner ring. Perfect rhymes between terminal words become individual
/// routed chords. Every item remains addressable as a byte span in the source
/// document, and unresolved tokens are returned rather than guessed.
pub fn project_rosette(
    document: SourceRef,
    text: &str,
    lexicon: &impl PronunciationLexicon,
    config: RosetteConfig,
) -> RosetteProjection {
    let (lines, stanzas) = parse_document(text);
    let mut scene = Scene::new();
    scene.generation = generation(&document, text);
    let mut coverage = LexiconCoverage::default();
    let mut interiors = Vec::new();
    let mut meter = Vec::new();

    if lines.is_empty() {
        return RosetteProjection {
            scene,
            coverage,
            interiors,
            meter,
        };
    }

    let positions: Vec<Vec2> = (0..lines.len())
        .map(|index| {
            let angle = config.start_angle_radians
                + std::f32::consts::TAU * index as f32 / lines.len() as f32;
            Vec2::new(config.radius * angle.cos(), config.radius * angle.sin())
        })
        .collect();

    for (index, line) in lines.iter().enumerate() {
        let source = scene.intern_source(interior_source(&document, "line", line.start, line.end));
        let instance = InstanceId(scene.items.len() as u32);
        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::translation(positions[index].x, positions[index].y),
            footprint: Footprint::Rect {
                size: config.line_footprint,
            },
            representation: Representation::Open {
                kind: LINE_REPRESENTATION.into(),
            },
            layer: 1,
            visible: true,
            hit: None,
            channels: Vec::new(),
        });
        interiors.push(RosetteInterior {
            instance,
            kind: RosetteInteriorKind::Line,
            ordinal: index as u32,
            byte_start: line.start,
            byte_end: line.end,
        });

        for token in &line.tokens {
            coverage.total_tokens += 1;
            if lexicon
                .pronunciations(&token.normalized)
                .is_some_and(|p| !p.is_empty())
            {
                coverage.resolved_tokens += 1;
            } else {
                coverage.unresolved.push(UnresolvedToken {
                    line: index as u32,
                    byte_start: token.start,
                    byte_end: token.end,
                    token: token.text.clone(),
                });
            }
        }
    }

    for (index, stanza) in stanzas.iter().enumerate() {
        let position = stanza_position(stanza, &positions, config.stanza_radius);
        let source = scene.intern_source(interior_source(
            &document,
            "stanza",
            stanza.start,
            stanza.end,
        ));
        let instance = InstanceId(scene.items.len() as u32);
        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::translation(position.x, position.y),
            footprint: Footprint::Rect {
                size: config.stanza_footprint,
            },
            representation: Representation::Open {
                kind: STANZA_REPRESENTATION.into(),
            },
            layer: 0,
            visible: true,
            hit: None,
            channels: Vec::new(),
        });
        interiors.push(RosetteInterior {
            instance,
            kind: RosetteInteriorKind::Stanza,
            ordinal: index as u32,
            byte_start: stanza.start,
            byte_end: stanza.end,
        });
    }

    if config.meter {
        for (index, line) in lines.iter().enumerate() {
            if let Some(scansion) = scan_line(index as u32, line, lexicon) {
                meter.push(scansion);
            }
        }
    }

    for left in 0..lines.len() {
        let Some(left_word) = lines[left].tokens.last() else {
            continue;
        };
        let Some(left_pronunciations) = lexicon.pronunciations(&left_word.normalized) else {
            continue;
        };

        for right in (left + 1)..lines.len() {
            let Some(right_word) = lines[right].tokens.last() else {
                continue;
            };
            let Some(right_pronunciations) = lexicon.pronunciations(&right_word.normalized) else {
                continue;
            };
            let relation = pronunciations_rhyme(left_pronunciations, right_pronunciations);
            let kind = match relation {
                Some(RhymeKind::Perfect) if config.perfect_rhyme => Some((PERFECT_RHYME, 1.0)),
                Some(RhymeKind::Slant) if config.slant_rhyme => Some((SLANT_RHYME, 0.6)),
                _ => None,
            };
            if let Some((kind, weight)) = kind {
                scene.relations.push(RoutedRelation {
                    from: InstanceId(left as u32),
                    to: InstanceId(right as u32),
                    space: Scene::WORLD,
                    points: vec![positions[left], positions[right]],
                    kind: Some(kind.into()),
                    weight: Some(weight),
                });
            }
        }
    }

    let half_w = config.line_footprint.w.max(config.stanza_footprint.w) * 0.5;
    let half_h = config.line_footprint.h.max(config.stanza_footprint.h) * 0.5;
    let outer_x = config.radius.abs().max(config.stanza_radius.abs()) + half_w;
    let outer_y = config.radius.abs().max(config.stanza_radius.abs()) + half_h;
    scene.bounds = Rect::new(
        Vec2::new(-outer_x, -outer_y),
        Size2::new(outer_x * 2.0, outer_y * 2.0),
    );

    RosetteProjection {
        scene,
        coverage,
        interiors,
        meter,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RhymeKind {
    Perfect,
    Slant,
}

fn pronunciations_rhyme(left: &[Vec<Phone>], right: &[Vec<Phone>]) -> Option<RhymeKind> {
    let mut slant = false;
    for left in left {
        let left_syllables = syllabify(left, SYLLABLE_RULE);
        for right in right {
            let right_syllables = syllabify(right, SYLLABLE_RULE);
            if is_perfect_rhyme((left, &left_syllables), (right, &right_syllables)) {
                return Some(RhymeKind::Perfect);
            }
            slant |= is_slant_rhyme((left, &left_syllables), (right, &right_syllables));
        }
    }
    slant.then_some(RhymeKind::Slant)
}

fn scan_line(
    line_index: u32,
    line: &Line,
    lexicon: &impl PronunciationLexicon,
) -> Option<LineMeter> {
    let mut line_beats = Vec::new();
    let mut resolved_tokens = 0;
    for token in &line.tokens {
        let Some(pronunciation) = lexicon
            .pronunciations(&token.normalized)
            .and_then(|pronunciations| pronunciations.first())
        else {
            continue;
        };
        let syllables = syllabify(pronunciation, SYLLABLE_RULE);
        line_beats.extend(beats(
            pronunciation,
            &syllables,
            Mode::Accentual,
            WEIGHT_RULE,
        ));
        resolved_tokens += 1;
    }
    let scansion = scan_best(&line_beats, &Foot::COMMON)?;
    Some(LineMeter {
        line: line_index,
        beats: line_beats
            .into_iter()
            .map(|beat| match beat {
                Beat::Weak => MetricalBeat::Weak,
                Beat::Strong => MetricalBeat::Strong,
            })
            .collect(),
        foot: match scansion.meter.foot {
            Foot::Iamb => MetricalFoot::Iamb,
            Foot::Trochee => MetricalFoot::Trochee,
            Foot::Dactyl => MetricalFoot::Dactyl,
            Foot::Anapest => MetricalFoot::Anapest,
            _ => unreachable!("Mora's common-foot scan returned a non-common foot"),
        },
        feet: scansion.meter.feet,
        fit: scansion.fit(),
        overrun: scansion.overrun,
        regular: scansion.is_regular(),
        resolved_tokens,
    })
}

fn generation(document: &SourceRef, text: &str) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(document.adapter.as_bytes());
    hasher.update(&[0]);
    hasher.update(document.id.as_bytes());
    hasher.update(&[0]);
    hasher.update(text.as_bytes());
    let mut generation = [0; 8];
    generation.copy_from_slice(&hasher.finalize().as_bytes()[..8]);
    u64::from_le_bytes(generation)
}

fn interior_source(document: &SourceRef, kind: &str, start: usize, end: usize) -> SourceRef {
    SourceRef::new(
        "knot.document-interior",
        format!(
            "{}:{}#{}:bytes={start}..{end}",
            document.adapter, document.id, kind
        ),
    )
}

fn stanza_position(stanza: &Stanza, positions: &[Vec2], radius: f32) -> Vec2 {
    let (x, y) = stanza.lines.iter().fold((0.0, 0.0), |(x, y), line| {
        let position = positions[*line];
        let length = (position.x * position.x + position.y * position.y).sqrt();
        if length > f32::EPSILON {
            (x + position.x / length, y + position.y / length)
        } else {
            (x, y)
        }
    });
    let length = (x * x + y * y).sqrt();
    if length > f32::EPSILON {
        Vec2::new(radius * x / length, radius * y / length)
    } else {
        let first = positions[stanza.lines[0]];
        let first_length = (first.x * first.x + first.y * first.y).sqrt();
        if first_length > f32::EPSILON {
            Vec2::new(
                radius * first.x / first_length,
                radius * first.y / first_length,
            )
        } else {
            Vec2::ZERO
        }
    }
}

#[derive(Debug)]
struct Line {
    start: usize,
    end: usize,
    tokens: Vec<Token>,
}

#[derive(Debug)]
struct Stanza {
    start: usize,
    end: usize,
    lines: Vec<usize>,
}

#[derive(Debug)]
struct Token {
    start: usize,
    end: usize,
    text: String,
    normalized: String,
}

fn parse_document(text: &str) -> (Vec<Line>, Vec<Stanza>) {
    let mut lines = Vec::new();
    let mut stanza_lines = Vec::new();
    let mut stanzas = Vec::new();
    let mut cursor = 0;

    for segment in text.split_inclusive('\n') {
        let without_lf = segment.strip_suffix('\n').unwrap_or(segment);
        let raw = without_lf.strip_suffix('\r').unwrap_or(without_lf);
        let leading = raw.len() - raw.trim_start().len();
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            finish_stanza(&lines, &mut stanza_lines, &mut stanzas);
            cursor += segment.len();
            continue;
        }

        let start = cursor + leading;
        let end = start + trimmed.len();
        let index = lines.len();
        lines.push(Line {
            start,
            end,
            tokens: tokens(trimmed, start),
        });
        stanza_lines.push(index);
        cursor += segment.len();
    }

    finish_stanza(&lines, &mut stanza_lines, &mut stanzas);
    (lines, stanzas)
}

fn finish_stanza(lines: &[Line], pending: &mut Vec<usize>, stanzas: &mut Vec<Stanza>) {
    let Some(first) = pending.first().copied() else {
        return;
    };
    let last = *pending.last().unwrap();
    stanzas.push(Stanza {
        start: lines[first].start,
        end: lines[last].end,
        lines: std::mem::take(pending),
    });
}

fn tokens(line: &str, absolute_start: usize) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut start = None;

    for (offset, character) in line.char_indices() {
        let in_word = character.is_alphabetic() || matches!(character, '\'' | '’');
        match (start, in_word) {
            (None, true) => start = Some(offset),
            (Some(word_start), false) => {
                push_token(&mut tokens, line, absolute_start, word_start, offset);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(word_start) = start {
        push_token(&mut tokens, line, absolute_start, word_start, line.len());
    }
    tokens
}

fn push_token(
    tokens: &mut Vec<Token>,
    line: &str,
    absolute_start: usize,
    start: usize,
    end: usize,
) {
    let text = &line[start..end];
    if !text.chars().any(char::is_alphabetic) {
        return;
    }
    tokens.push(Token {
        start: absolute_start + start,
        end: absolute_start + end,
        text: text.to_owned(),
        normalized: text.replace('’', "'").to_ascii_lowercase(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use scenotime::{Revision, SceneEpoch, SceneSnapshot};

    const POEM: &str = "Morning gathers light\nBranches answer night\n\nFootsteps cross the hill\nEvening settles still\n";
    const LYRIC: &str = "Raise your open hand\nWe will take a stand\n\nCarry home the song\nLet the road run long\n";

    #[test]
    fn poem_and_lyric_are_two_deterministic_rosette_receipts() {
        let lexicon = CmudictPronunciations;
        for (id, text) in [("poem", POEM), ("lyric", LYRIC)] {
            let source = SourceRef::new("knot.fixture", id);
            let projection = project_rosette(source.clone(), text, &lexicon, Default::default());
            let repeated = project_rosette(source, text, &lexicon, Default::default());

            assert_eq!(projection, repeated);
            assert_eq!(
                projection.scene.items.len(),
                6,
                "four lines plus two stanzas"
            );
            assert!(projection.scene.relations.len() >= 2);
            assert!(
                projection
                    .scene
                    .relations
                    .iter()
                    .all(|relation| relation.kind.as_deref() == Some(PERFECT_RHYME))
            );
            assert!(projection.coverage.total_tokens > 0);
            assert!(projection.coverage.resolved_tokens > 0);
            assert!(projection.coverage.unresolved.len() < projection.coverage.total_tokens);
            assert_eq!(projection.meter.len(), 4);
            assert!(projection.meter.iter().all(|line| !line.beats.is_empty()));

            let first = serde_json::to_vec(&projection.scene).unwrap();
            let second = serde_json::to_vec(&repeated.scene).unwrap();
            assert_eq!(first, second);
            SceneSnapshot::from_dense(SceneEpoch(1), Revision(1), projection.scene).unwrap();
        }
    }

    #[test]
    fn unknown_words_are_reported_with_source_spans() {
        let projection = project_rosette(
            SourceRef::new("knot.fixture", "unknown"),
            "Known flibbertigibbet\n",
            &CmudictPronunciations,
            Default::default(),
        );
        let unresolved = projection
            .coverage
            .unresolved
            .iter()
            .find(|token| token.token == "flibbertigibbet")
            .unwrap();
        assert_eq!(unresolved.byte_start, 6);
        assert_eq!(unresolved.byte_end, 21);
        assert!(projection.scene.sources[0].id.contains("line:bytes=0..21"));
    }

    #[test]
    fn slant_rhyme_and_meter_are_derived_without_touching_source_truth() {
        let projection = project_rosette(
            SourceRef::new("knot.fixture", "slant"),
            "A cat\nA cut\n",
            &CmudictPronunciations,
            Default::default(),
        );
        assert!(projection.scene.relations.iter().any(|relation| {
            relation.kind.as_deref() == Some(SLANT_RHYME) && relation.weight == Some(0.6)
        }));
        assert_eq!(projection.meter.len(), 2);
        assert!(
            projection
                .meter
                .iter()
                .all(|line| line.resolved_tokens == 2)
        );
        assert_eq!(projection.interiors[0].byte_start, 0);
        assert_eq!(projection.interiors[0].byte_end, 5);
    }
}
