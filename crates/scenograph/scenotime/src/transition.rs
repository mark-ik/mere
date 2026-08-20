//! Declarative, host-clocked transitions between scene revisions.
//!
//! A transition is derived from a validated [`SceneDiff`](crate::SceneDiff)
//! and the snapshot it advances. Scenotime owns deterministic staging and
//! interpolation; the host owns elapsed time and whether that clock advances.
//! Consumers that do not construct a schedule continue to apply diffs as snaps.

use std::collections::{BTreeMap, BTreeSet};

use sceno::{InstanceId, ProjectedItem, Transform2, Vec2};
use serde::{Deserialize, Serialize};

use crate::{ApplyOutcome, DiffError, SceneDiff, SceneOp, SceneSnapshot};

/// The three item-change classes a scene diff can expose to a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransitionClass {
    Enter,
    Update,
    Exit,
}

/// Pure easing applied inside each scheduled item's time window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransitionEasing {
    Linear,
    EaseInOut,
}

impl TransitionEasing {
    fn apply(self, progress: f32) -> f32 {
        let progress = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => progress,
            // Smoothstep has exact endpoints, is deterministic, and avoids
            // importing a renderer-specific timing-function vocabulary.
            Self::EaseInOut => progress * progress * (3.0 - 2.0 * progress),
        }
    }
}

/// One change class's window within the whole transition.
///
/// Ratios are in `0..=1`. `stagger_ratio` reserves that fraction of the
/// window for spreading item starts in stable instance order; the remainder
/// is each item's playback duration. A zero stagger plays the class together.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TransitionStage {
    pub class: TransitionClass,
    pub start_ratio: f32,
    pub end_ratio: f32,
    pub stagger_ratio: f32,
}

impl TransitionStage {
    pub const fn together(class: TransitionClass, start_ratio: f32, end_ratio: f32) -> Self {
        Self {
            class,
            start_ratio,
            end_ratio,
            stagger_ratio: 0.0,
        }
    }

    pub const fn staggered(
        class: TransitionClass,
        start_ratio: f32,
        end_ratio: f32,
        stagger_ratio: f32,
    ) -> Self {
        Self {
            class,
            start_ratio,
            end_ratio,
            stagger_ratio,
        }
    }
}

/// Serializable instructions for staging one scene revision into the next.
///
/// Omitting a change class means it is not animated by this spec. The target
/// snapshot remains authoritative and can still be installed at completion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionSpec {
    pub duration_ms: u32,
    pub easing: TransitionEasing,
    pub stages: Vec<TransitionStage>,
}

impl Default for TransitionSpec {
    fn default() -> Self {
        Self {
            duration_ms: 520,
            easing: TransitionEasing::EaseInOut,
            stages: vec![
                TransitionStage::staggered(TransitionClass::Exit, 0.0, 0.28, 0.12),
                TransitionStage::staggered(TransitionClass::Update, 0.10, 0.90, 0.18),
                TransitionStage::staggered(TransitionClass::Enter, 0.62, 1.0, 0.18),
            ],
        }
    }
}

impl TransitionSpec {
    pub fn validate(&self) -> Result<(), TransitionError> {
        if self.duration_ms == 0 {
            return Err(TransitionError::InvalidSpec(
                "transition duration must be greater than zero".to_string(),
            ));
        }
        let mut classes = BTreeSet::new();
        for stage in &self.stages {
            let ratios = [stage.start_ratio, stage.end_ratio, stage.stagger_ratio];
            if ratios
                .iter()
                .any(|ratio| !ratio.is_finite() || !(0.0..=1.0).contains(ratio))
            {
                return Err(TransitionError::InvalidSpec(format!(
                    "{:?} stage ratios must be finite and within 0..=1",
                    stage.class
                )));
            }
            if stage.end_ratio <= stage.start_ratio {
                return Err(TransitionError::InvalidSpec(format!(
                    "{:?} stage must end after it starts",
                    stage.class
                )));
            }
            if stage.stagger_ratio >= 1.0 {
                return Err(TransitionError::InvalidSpec(format!(
                    "{:?} stage stagger must leave time for playback",
                    stage.class
                )));
            }
            if !classes.insert(stage.class) {
                return Err(TransitionError::InvalidSpec(format!(
                    "{:?} has more than one stage",
                    stage.class
                )));
            }
        }
        Ok(())
    }
}

/// A transition value independent of any renderer resource.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TransitionValue {
    pub transform: Transform2,
    pub opacity: f32,
}

impl TransitionValue {
    fn of(item: &ProjectedItem) -> Self {
        Self {
            transform: item.transform,
            opacity: f32::from(item.visible),
        }
    }

    fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    fn interpolate(self, target: Self, progress: f32) -> Self {
        Self {
            transform: Transform2 {
                translate: Vec2::new(
                    lerp(
                        self.transform.translate.x,
                        target.transform.translate.x,
                        progress,
                    ),
                    lerp(
                        self.transform.translate.y,
                        target.transform.translate.y,
                        progress,
                    ),
                ),
                scale: lerp(self.transform.scale, target.transform.scale, progress),
                rotate: lerp(self.transform.rotate, target.transform.rotate, progress),
            },
            opacity: lerp(self.opacity, target.opacity, progress),
        }
    }
}

/// One item's derived window and endpoints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduledItem {
    pub instance: InstanceId,
    pub class: TransitionClass,
    pub start_ratio: f32,
    pub end_ratio: f32,
    pub from: TransitionValue,
    pub to: TransitionValue,
}

/// A deterministic schedule derived from one validated diff and one spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionSchedule {
    pub duration_ms: u32,
    pub easing: TransitionEasing,
    pub items: Vec<ScheduledItem>,
}

/// One sampled item at a host-supplied elapsed time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransitionSample {
    pub instance: InstanceId,
    pub class: TransitionClass,
    pub value: TransitionValue,
}

/// The pure result for one host frame.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionFrame {
    pub elapsed_ms: f32,
    pub complete: bool,
    pub items: Vec<TransitionSample>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    InvalidSpec(String),
    InvalidDiff(DiffError),
}

impl TransitionSchedule {
    /// Derive a schedule without mutating `before`.
    ///
    /// Applying `diff` to a clone supplies the target values and also reuses
    /// scenotime's epoch, revision, table, and reference validation.
    pub fn from_diff(
        before: &SceneSnapshot,
        diff: &SceneDiff,
        spec: &TransitionSpec,
    ) -> Result<Self, TransitionError> {
        spec.validate()?;
        let mut after = before.clone();
        let outcome = after
            .apply_diff(diff)
            .map_err(TransitionError::InvalidDiff)?;
        if outcome == ApplyOutcome::AlreadyApplied {
            return Ok(Self {
                duration_ms: spec.duration_ms,
                easing: spec.easing,
                items: Vec::new(),
            });
        }

        let mut affected = BTreeSet::new();
        for operation in &diff.operations {
            if let Some(instance) = affected_instance(operation) {
                affected.insert(instance.0);
            }
        }

        let stages = spec
            .stages
            .iter()
            .map(|stage| (stage.class, *stage))
            .collect::<BTreeMap<_, _>>();
        let mut changes = BTreeMap::<TransitionClass, Vec<_>>::new();
        for index in affected {
            let instance = InstanceId(index);
            let old = before.active_item(instance);
            let new = after.active_item(instance);
            let Some((class, from, to)) = transition_endpoints(old, new) else {
                continue;
            };
            if stages.contains_key(&class) {
                changes.entry(class).or_default().push((instance, from, to));
            }
        }

        let mut items = Vec::new();
        for (class, class_items) in changes {
            let stage = stages[&class];
            let count = class_items.len();
            let window = stage.end_ratio - stage.start_ratio;
            let stagger_span = if count <= 1 {
                0.0
            } else {
                window * stage.stagger_ratio
            };
            let item_span = window - stagger_span;
            for (ordinal, (instance, from, to)) in class_items.into_iter().enumerate() {
                let offset = if count <= 1 {
                    0.0
                } else {
                    stagger_span * ordinal as f32 / (count - 1) as f32
                };
                let start_ratio = stage.start_ratio + offset;
                items.push(ScheduledItem {
                    instance,
                    class,
                    start_ratio,
                    end_ratio: start_ratio + item_span,
                    from,
                    to,
                });
            }
        }
        items.sort_by_key(|item| item.instance.0);
        Ok(Self {
            duration_ms: spec.duration_ms,
            easing: spec.easing,
            items,
        })
    }

    /// Evaluate the schedule at elapsed host time. Calling this repeatedly
    /// with the same time is a paused frame and returns the same result.
    pub fn sample_at(&self, elapsed_ms: f32) -> TransitionFrame {
        let elapsed_ms = elapsed_ms.max(0.0);
        let whole = (elapsed_ms / self.duration_ms as f32).clamp(0.0, 1.0);
        let items = self
            .items
            .iter()
            .map(|item| {
                let local = ((whole - item.start_ratio) / (item.end_ratio - item.start_ratio))
                    .clamp(0.0, 1.0);
                TransitionSample {
                    instance: item.instance,
                    class: item.class,
                    value: item.from.interpolate(item.to, self.easing.apply(local)),
                }
            })
            .collect();
        TransitionFrame {
            elapsed_ms,
            complete: elapsed_ms >= self.duration_ms as f32,
            items,
        }
    }
}

fn affected_instance(operation: &SceneOp) -> Option<InstanceId> {
    match operation {
        SceneOp::AddItem { index, .. }
        | SceneOp::UpdateItem { index, .. }
        | SceneOp::TombstoneItem { index }
        | SceneOp::SetItemLayer { index, .. }
        | SceneOp::SetItemOrder { index, .. } => Some(*index),
        _ => None,
    }
}

fn transition_endpoints(
    before: Option<&ProjectedItem>,
    after: Option<&ProjectedItem>,
) -> Option<(TransitionClass, TransitionValue, TransitionValue)> {
    match (before, after) {
        (None, Some(after)) => {
            let target = TransitionValue::of(after);
            Some((TransitionClass::Enter, target.with_opacity(0.0), target))
        }
        (Some(before), Some(after)) => Some((
            TransitionClass::Update,
            TransitionValue::of(before),
            TransitionValue::of(after),
        )),
        (Some(before), None) => {
            let source = TransitionValue::of(before);
            Some((TransitionClass::Exit, source, source.with_opacity(0.0)))
        }
        (None, None) => None,
    }
}

fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Revision, SceneEpoch};
    use sceno::{Footprint, Representation, Scene, SourceIx};

    fn item(source: SourceIx, x: f32) -> ProjectedItem {
        ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::translation(x, 0.0),
            footprint: Footprint::Point,
            representation: Representation::Glyph,
            layer: 0,
            visible: true,
            hit: None,
            channels: Vec::new(),
        }
    }

    fn fixture() -> (SceneSnapshot, SceneDiff) {
        let mut scene = Scene::new();
        let source = scene.intern_source(sceno::SourceRef::new("fixture", "one"));
        scene.items.push(item(source, 0.0));
        scene.items.push(item(source, 10.0));
        let before = SceneSnapshot::from_dense(SceneEpoch(3), Revision(7), scene).unwrap();
        let diff = SceneDiff {
            epoch: SceneEpoch(3),
            base: Revision(7),
            revision: Revision(8),
            operations: vec![
                SceneOp::UpdateItem {
                    index: InstanceId(0),
                    value: item(source, 100.0),
                },
                SceneOp::UpdateItem {
                    index: InstanceId(1),
                    value: item(source, 210.0),
                },
            ],
        };
        (before, diff)
    }

    #[test]
    fn identical_inputs_produce_an_identical_schedule_and_wire_shape() {
        let (before, diff) = fixture();
        let spec = TransitionSpec::default();
        let first = TransitionSchedule::from_diff(&before, &diff, &spec).unwrap();
        let second = TransitionSchedule::from_diff(&before, &diff, &spec).unwrap();
        assert_eq!(first, second);
        let wire = serde_json::to_string(&spec).unwrap();
        assert_eq!(serde_json::from_str::<TransitionSpec>(&wire).unwrap(), spec);
    }

    #[test]
    fn host_time_is_pure_and_exact_at_both_ends() {
        let (before, diff) = fixture();
        let schedule = TransitionSchedule::from_diff(
            &before,
            &diff,
            &TransitionSpec {
                duration_ms: 1_000,
                easing: TransitionEasing::Linear,
                stages: vec![TransitionStage::together(TransitionClass::Update, 0.0, 1.0)],
            },
        )
        .unwrap();
        let paused = schedule.sample_at(500.0);
        assert_eq!(paused, schedule.sample_at(500.0));
        assert_eq!(paused.items[0].value.transform.translate.x, 50.0);
        assert_eq!(paused.items[1].value.transform.translate.x, 110.0);
        assert_eq!(
            schedule.sample_at(0.0).items[0].value.transform.translate.x,
            0.0
        );
        let done = schedule.sample_at(1_000.0);
        assert!(done.complete);
        assert_eq!(done.items[0].value.transform.translate.x, 100.0);
        assert_eq!(done.items[1].value.transform.translate.x, 210.0);
    }

    #[test]
    fn default_update_stage_staggers_in_stable_instance_order() {
        let (before, diff) = fixture();
        let schedule =
            TransitionSchedule::from_diff(&before, &diff, &TransitionSpec::default()).unwrap();
        assert_eq!(schedule.items.len(), 2);
        assert!(schedule.items[0].start_ratio < schedule.items[1].start_ratio);
        assert_eq!(schedule.items[0].instance, InstanceId(0));
        assert_eq!(schedule.items[1].instance, InstanceId(1));
    }

    #[test]
    fn deriving_a_schedule_does_not_apply_the_diff_for_static_consumers() {
        let (before, diff) = fixture();
        let original = before.clone();
        TransitionSchedule::from_diff(&before, &diff, &TransitionSpec::default()).unwrap();
        assert_eq!(before, original);
        assert_eq!(
            before
                .active_item(InstanceId(0))
                .unwrap()
                .transform
                .translate
                .x,
            0.0
        );
    }

    #[test]
    fn an_already_applied_diff_does_not_replay_motion() {
        let (mut current, diff) = fixture();
        current.apply_diff(&diff).unwrap();
        let schedule =
            TransitionSchedule::from_diff(&current, &diff, &TransitionSpec::default()).unwrap();
        assert!(schedule.items.is_empty());
    }

    #[test]
    fn invalid_staging_is_rejected_before_the_diff_is_read() {
        let (before, diff) = fixture();
        let invalid = TransitionSpec {
            duration_ms: 0,
            easing: TransitionEasing::Linear,
            stages: Vec::new(),
        };
        assert!(matches!(
            TransitionSchedule::from_diff(&before, &diff, &invalid),
            Err(TransitionError::InvalidSpec(_))
        ));
    }
}
