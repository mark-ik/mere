// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`LayoutStrategy`] adapters over the `scenograph` solvers.
//!
//! Each adapter does three things: pick an [`Arrangement`] and its config,
//! disclose whatever the arrangement needs to read, and hand the score to
//! `scenomise`. Nothing here places anything — the placement lives in
//! `scenomise`, where it is portable and testable without a graph.
//!
//! These arrived from `crates/canvas/arrangements`, which the
//! [scenograph absorption plan](../../../../design_docs/mere_docs/implementation_strategy/2026-08-22_scenograph_absorption_plan.md)
//! retired. They live here because they are the graph-bound half: they read
//! `kernel::graph::Graph`, and cartography already owns [`LayoutStrategy`].
//!
//! Live force physics is `seiche`'s domain and has no adapter here.

use std::collections::HashMap;

use kernel::graph::NodeKey;
use sceno::Arrangement;

use crate::projection::Projection;
use crate::request::ProjectionRequest;
use crate::strategy::LayoutStrategy;

#[cfg(test)]
mod parity;
pub mod producers;
pub mod score;

pub use producers::{degree_weights, radial_rings, spectral_coords};
pub use score::{Disclosures, empty_projection, project_score, score_from_request};

/// Declare an adapter whose whole job is a config, an id, and the disclosures
/// it reads from the request.
macro_rules! analytic_adapter {
    (
        $(#[$meta:meta])*
        $name:ident, $id:literal, $config:ty, $variant:path
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, PartialEq)]
        pub struct $name {
            pub config: $config,
        }

        impl $name {
            pub const PROJECTION_ID: &'static str = $id;
        }

        impl LayoutStrategy for $name {
            fn projection_id(&self) -> &'static str {
                Self::PROJECTION_ID
            }

            fn project(&self, request: &ProjectionRequest<'_>) -> Projection {
                let (score, keys) = score_from_request(
                    request,
                    $variant(self.config.clone()),
                    &Disclosures::from_intent(request),
                );
                if keys.is_empty() {
                    return empty_projection(Self::PROJECTION_ID);
                }
                project_score(Self::PROJECTION_ID, request, &score, &keys)
            }
        }
    };
}

/// How a grid picks its column count.
///
/// This stays adapter-side rather than moving into [`sceno::Grid`] because two
/// of the three modes need the item count, and a persisted score is supposed to
/// mean the same thing however many items it happens to carry. The adapter
/// resolves the count per request and the score records the number it chose.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum GridColumns {
    /// `ceil(sqrt(n))`, keeping the grid roughly square.
    #[default]
    Auto,
    /// A fixed count. Zero falls back to [`GridColumns::Auto`].
    Explicit(u32),
    /// The count best approximating a width/height ratio. `2.0` prefers wide
    /// grids, `0.5` tall ones.
    AspectRatio(f32),
}

impl GridColumns {
    fn resolve(self, count: usize) -> u32 {
        let auto = || (count as f32).sqrt().ceil().max(1.0) as u32;
        match self {
            Self::Auto | Self::Explicit(0) => auto(),
            Self::Explicit(columns) => columns.max(1),
            // columns × rows ≈ n and columns / rows = ratio, so
            // columns ≈ sqrt(n × ratio).
            Self::AspectRatio(ratio) => {
                let raw = (count as f32 * ratio.max(0.01)).sqrt().ceil() as u32;
                raw.max(1).min(count.max(1) as u32)
            }
        }
    }
}

/// Regular cell grid; items flow left-to-right from their ordinal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridAdapter {
    pub origin: sceno::Vec2,
    /// Centre-to-centre spacing. The score carries this as `gap` with a
    /// zero-size cell, since the adapter spaces by pitch rather than by
    /// measured extent.
    pub pitch: f32,
    pub columns: GridColumns,
}

impl Default for GridAdapter {
    fn default() -> Self {
        Self {
            origin: sceno::Vec2::ZERO,
            pitch: 120.0,
            columns: GridColumns::Auto,
        }
    }
}

impl GridAdapter {
    pub const PROJECTION_ID: &'static str = "grid.default";
}

impl LayoutStrategy for GridAdapter {
    fn projection_id(&self) -> &'static str {
        Self::PROJECTION_ID
    }

    fn project(&self, request: &ProjectionRequest<'_>) -> Projection {
        let count = request.graph.nodes().count();
        let (score, keys) = score_from_request(
            request,
            Arrangement::Grid(sceno::Grid {
                origin: self.origin,
                cell: sceno::Vec2::ZERO,
                columns: self.columns.resolve(count),
                gap: self.pitch,
            }),
            &Disclosures::from_intent(request),
        );
        if keys.is_empty() {
            return empty_projection(Self::PROJECTION_ID);
        }
        project_score(Self::PROJECTION_ID, request, &score, &keys)
    }
}

analytic_adapter!(
    /// Golden-angle spiral. The product-free score path in
    /// [`crate::spiral_score`] is the one the canvas uses; this is the plain
    /// strategy-shaped form.
    PhyllotaxisAdapter,
    "phyllotaxis.default",
    sceno::Spiral,
    Arrangement::Spiral
);

analytic_adapter!(
    /// Penrose aperiodic tiling; items take vertices in ordinal order.
    PenroseAdapter,
    "penrose.default",
    sceno::Penrose,
    Arrangement::Penrose
);

analytic_adapter!(
    /// L-system fractal path; items take positions along a turtle walk.
    LSystemAdapter,
    "lsystem.default",
    sceno::LSystem,
    Arrangement::LSystem
);

analytic_adapter!(
    /// Numeric axis. Reads `ViewIntent::axis_values`; the caller decides what
    /// the axis means.
    TimelineAdapter,
    "timeline.default",
    sceno::Timeline,
    Arrangement::Timeline
);

analytic_adapter!(
    /// Categorical columns. Reads `ViewIntent::axis_values`; the caller decides
    /// what the columns are.
    KanbanAdapter,
    "kanban.default",
    sceno::Kanban,
    Arrangement::Kanban
);

analytic_adapter!(
    /// Layered stack. Reads `ViewIntent::axis_values` for the layer index.
    StackAdapter,
    "stack.default",
    sceno::Stack,
    Arrangement::Stack
);

/// Placement at coordinates a dimensionality reduction produced.
///
/// Reads `IntelligenceSignals::embeddings` — a host-run UMAP, t-SNE, or PCA.
/// The arrangement it emits is the same one [`SpectralAdapter`] emits; only the
/// producer differs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SemanticEmbeddingAdapter {
    pub config: sceno::Embedded,
}

impl SemanticEmbeddingAdapter {
    pub const PROJECTION_ID: &'static str = "semantic.embedding";
}

impl LayoutStrategy for SemanticEmbeddingAdapter {
    fn projection_id(&self) -> &'static str {
        Self::PROJECTION_ID
    }

    fn project(&self, request: &ProjectionRequest<'_>) -> Projection {
        let embedding: HashMap<NodeKey, sceno::Vec2> = request
            .signals
            .embeddings
            .as_ref()
            .map(|embeddings| {
                embeddings
                    .coords
                    .iter()
                    .map(|(key, (x, y))| (*key, sceno::Vec2::new(*x, *y)))
                    .collect()
            })
            .unwrap_or_default();

        let (score, keys) = score_from_request(
            request,
            Arrangement::Embedded(self.config.clone()),
            &Disclosures::default().with_embedding(embedding),
        );
        if keys.is_empty() {
            return empty_projection(Self::PROJECTION_ID);
        }
        project_score(Self::PROJECTION_ID, request, &score, &keys)
    }
}

/// Placement at coordinates the graph Laplacian produced, so the layout
/// reflects connectivity: clusters separate spatially and a path unrolls into a
/// line.
///
/// The expensive analytic strategy the arrangement cache exists for —
/// recomputed on a structural change, not per frame.
#[derive(Debug, Clone, PartialEq)]
pub struct SpectralAdapter {
    pub config: sceno::Embedded,
    /// Power-iteration count. A producer parameter, not a placement one, which
    /// is why it sits here rather than in the arrangement.
    pub iterations: usize,
}

impl Default for SpectralAdapter {
    fn default() -> Self {
        Self {
            config: sceno::Embedded {
                // 320, not `Embedded`'s 400: the spectral strategy has always
                // fit its coordinates into this reach, and a score stored
                // against the old strategy must still land where it landed.
                scale: 320.0,
                ..sceno::Embedded::default()
            },
            iterations: 200,
        }
    }
}

impl SpectralAdapter {
    pub const PROJECTION_ID: &'static str = "spectral.default";
}

impl LayoutStrategy for SpectralAdapter {
    fn projection_id(&self) -> &'static str {
        Self::PROJECTION_ID
    }

    fn project(&self, request: &ProjectionRequest<'_>) -> Projection {
        let (score, keys) = score_from_request(
            request,
            Arrangement::Embedded(sceno::Embedded {
                // An edgeless or perfectly symmetric graph discloses no
                // coordinates at all. Ringing them out beats stacking every
                // node on the origin, which is what a collapse would do.
                fallback: sceno::EmbeddingFallback::RingOutside,
                ..self.config.clone()
            }),
            &Disclosures::default().with_embedding(spectral_coords(request.graph, self.iterations)),
        );
        if keys.is_empty() {
            return empty_projection(Self::PROJECTION_ID);
        }
        project_score(Self::PROJECTION_ID, request, &score, &keys)
    }
}

/// Concentric rings around `ViewIntent::focus`.
///
/// The breadth-first walk runs here, where the graph is; what reaches the score
/// is one ring index per node. Without a focus there is nothing to ring around,
/// and the projection is empty.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RadialAdapter {
    pub config: sceno::Radial,
}

impl RadialAdapter {
    pub const PROJECTION_ID: &'static str = "radial.default";
}

impl LayoutStrategy for RadialAdapter {
    fn projection_id(&self) -> &'static str {
        Self::PROJECTION_ID
    }

    fn project(&self, request: &ProjectionRequest<'_>) -> Projection {
        let Some(focus) = request.intent.focus else {
            return empty_projection(Self::PROJECTION_ID);
        };

        let rings = radial_rings(request.graph, focus);
        let axis: HashMap<NodeKey, crate::request::AxisValue> = rings
            .into_iter()
            .map(|(key, ring)| (key, crate::request::AxisValue::Numeric(ring as f64)))
            .collect();

        let mut disclosures = Disclosures::default().with_axis(axis);
        // Only the weighted policy reads it, and the degree walk is not free.
        if matches!(
            self.config.angular_policy,
            sceno::RadialAngularPolicy::Weighted
        ) {
            disclosures = disclosures.with_weight(degree_weights(request.graph));
        }

        let (score, keys) =
            score_from_request(request, Arrangement::Radial(self.config), &disclosures);
        if keys.is_empty() {
            return empty_projection(Self::PROJECTION_ID);
        }
        project_score(Self::PROJECTION_ID, request, &score, &keys)
    }
}
