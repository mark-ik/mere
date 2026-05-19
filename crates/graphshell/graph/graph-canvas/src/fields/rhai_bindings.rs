/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Rhai composition surface for the field algebra.
//!
//! Hosts call [`build_from_script`] with a Rhai script; the script's final
//! expression must be a [`FieldProjection`]. The script body composes
//! scalar/vector fields, attaches them to the projection's registry, and
//! declares coupling and edge-path rules.
//!
//! ## Surface
//!
//! Constructors:
//! - `new_projection()` → `FieldProjection`
//! - `scalar_const(c)`, `scalar_x()`, `scalar_y()`, `scalar_time()` → `ScalarField`
//! - `gaussian(cx, cy, sigma)`, `linear(nx, ny, offset)` → `ScalarField`
//! - `negate(scalar)`, `scale_by(scalar, k)`, `add(a, b)`, `mul(a, b)` → `ScalarField`
//! - `vector_const(x, y)`, `vector_coord()`, `gradient(scalar)` → `VectorField`
//! - `perp(vector)` → `VectorField`
//!
//! Methods on `FieldProjection`:
//! - `p.add_scalar(name, field)` → field id
//! - `p.add_vector(name, field)` → field id
//! - `p.set_z_field(id)`
//! - `p.couple_attract(kind, id, strength)`
//! - `p.couple_repel(kind, id, strength)`
//! - `p.couple_align(kind, id, strength)`
//! - `p.couple_advect(kind, id, strength)`
//! - `p.couple_wall(kind, id, strength)`
//! - `p.edge_path_field_line(edge_kind, field_id, max_steps, step_size)`
//! - `p.edge_path_straight(edge_kind)`
//!
//! ## Example
//!
//! ```rhai
//! let p = new_projection();
//! let focus = p.add_scalar("focus", gaussian(0.0, 0.0, 200.0));
//! p.set_z_field(focus);
//! p.couple_attract("paper", focus, 1.0);
//! p.edge_path_field_line("cites", focus, 32, 4.0);
//! p
//! ```

use rhai::{Engine, ImmutableString};

use crate::fields::ast::{ScalarField, VectorField};
use crate::fields::coupling::{Coupling, CouplingResponse, EdgePath, EdgePathRule, NodeSelector};
use crate::fields::projection::FieldProjection;
use crate::fields::registry::FieldId;

/// Reasons a Rhai script could not be evaluated into a `FieldProjection`.
#[derive(Debug, Clone, PartialEq)]
pub enum BuildError {
    /// The Rhai engine returned a parse or runtime error.
    EvalError(String),
}

/// Build a Rhai engine pre-loaded with the field-algebra surface.
///
/// Hosts that want to extend the surface (e.g. add app-specific selectors)
/// can call this and then register additional functions before evaluating
/// scripts.
pub fn build_engine() -> Engine {
    let mut engine = Engine::new();

    engine.register_type_with_name::<FieldProjection>("FieldProjection");
    engine.register_type_with_name::<ScalarField>("ScalarField");
    engine.register_type_with_name::<VectorField>("VectorField");

    register_constructors(&mut engine);
    register_combinators(&mut engine);
    register_projection_methods(&mut engine);
    register_coupling_methods(&mut engine);
    register_edge_path_methods(&mut engine);

    engine
}

/// Evaluate a script and return its final `FieldProjection` value.
pub fn build_from_script(script: &str) -> Result<FieldProjection, BuildError> {
    let engine = build_engine();
    engine
        .eval::<FieldProjection>(script)
        .map_err(|e| BuildError::EvalError(e.to_string()))
}

// ── Registration ────────────────────────────────────────────────────────────

fn register_constructors(engine: &mut Engine) {
    engine.register_fn("new_projection", FieldProjection::new);

    engine.register_fn("scalar_const", |c: f64| ScalarField::Const(c as f32));
    engine.register_fn("scalar_x", || ScalarField::CoordX);
    engine.register_fn("scalar_y", || ScalarField::CoordY);
    engine.register_fn("scalar_time", || ScalarField::Time);
    engine.register_fn("gaussian", |cx: f64, cy: f64, sigma: f64| {
        ScalarField::gaussian_at(cx as f32, cy as f32, sigma as f32)
    });
    engine.register_fn("linear", |nx: f64, ny: f64, offset: f64| {
        ScalarField::linear(nx as f32, ny as f32, offset as f32)
    });

    engine.register_fn("vector_const", |x: f64, y: f64| VectorField::ConstVec {
        x: x as f32,
        y: y as f32,
    });
    engine.register_fn("vector_coord", || VectorField::Coord);
    engine.register_fn("gradient", |s: ScalarField| VectorField::gradient_of(s));
    engine.register_fn("perp", |v: VectorField| VectorField::Perp(Box::new(v)));
}

fn register_combinators(engine: &mut Engine) {
    engine.register_fn("negate", |s: ScalarField| ScalarField::Negate(Box::new(s)));
    engine.register_fn("scale_by", |s: ScalarField, k: f64| {
        ScalarField::Scale(Box::new(s), k as f32)
    });
    engine.register_fn("add", |a: ScalarField, b: ScalarField| {
        ScalarField::Add(Box::new(a), Box::new(b))
    });
    engine.register_fn("mul", |a: ScalarField, b: ScalarField| {
        ScalarField::Mul(Box::new(a), Box::new(b))
    });
}

fn register_projection_methods(engine: &mut Engine) {
    engine.register_fn(
        "add_scalar",
        |p: &mut FieldProjection, name: ImmutableString, field: ScalarField| -> i64 {
            p.add_scalar(name.to_string(), field).0 as i64
        },
    );
    engine.register_fn(
        "add_vector",
        |p: &mut FieldProjection, name: ImmutableString, field: VectorField| -> i64 {
            p.add_vector(name.to_string(), field).0 as i64
        },
    );
    engine.register_fn("set_z_field", |p: &mut FieldProjection, id: i64| {
        p.z_field = Some(FieldId(id as u64));
    });
    engine.register_fn("clear_z_field", |p: &mut FieldProjection| {
        p.z_field = None;
    });
}

fn register_coupling_methods(engine: &mut Engine) {
    fn append(
        p: &mut FieldProjection,
        kind: ImmutableString,
        field_id: i64,
        strength: f64,
        response: CouplingResponse,
    ) {
        p.add_coupling(Coupling {
            selector: NodeSelector::Kind(kind.to_string()),
            field: FieldId(field_id as u64),
            response,
            strength: strength as f32,
        });
    }

    engine.register_fn(
        "couple_attract",
        |p: &mut FieldProjection, kind: ImmutableString, field_id: i64, strength: f64| {
            append(p, kind, field_id, strength, CouplingResponse::AttractToMin);
        },
    );
    engine.register_fn(
        "couple_repel",
        |p: &mut FieldProjection, kind: ImmutableString, field_id: i64, strength: f64| {
            append(p, kind, field_id, strength, CouplingResponse::RepelFromMax);
        },
    );
    engine.register_fn(
        "couple_align",
        |p: &mut FieldProjection, kind: ImmutableString, field_id: i64, strength: f64| {
            append(p, kind, field_id, strength, CouplingResponse::AlignVelocity);
        },
    );
    engine.register_fn(
        "couple_advect",
        |p: &mut FieldProjection, kind: ImmutableString, field_id: i64, strength: f64| {
            append(p, kind, field_id, strength, CouplingResponse::FlowAdvect);
        },
    );
    engine.register_fn(
        "couple_wall",
        |p: &mut FieldProjection, kind: ImmutableString, field_id: i64, strength: f64| {
            append(
                p,
                kind,
                field_id,
                strength,
                CouplingResponse::ContainmentWall,
            );
        },
    );
    engine.register_fn(
        "couple_dampen_inside",
        |p: &mut FieldProjection,
         kind: ImmutableString,
         field_id: i64,
         strength: f64,
         factor: f64| {
            append(
                p,
                kind,
                field_id,
                strength,
                CouplingResponse::DampenInside {
                    factor: factor as f32,
                },
            );
        },
    );
}

fn register_edge_path_methods(engine: &mut Engine) {
    engine.register_fn(
        "edge_path_field_line",
        |p: &mut FieldProjection,
         edge_kind: ImmutableString,
         field_id: i64,
         max_steps: i64,
         step_size: f64| {
            p.add_edge_path_rule(EdgePathRule {
                edge_kind: edge_kind.to_string(),
                path: EdgePath::FieldLine {
                    field: FieldId(field_id as u64),
                    max_steps: max_steps as u32,
                    step_size: step_size as f32,
                },
            });
        },
    );
    engine.register_fn(
        "edge_path_straight",
        |p: &mut FieldProjection, edge_kind: ImmutableString| {
            p.add_edge_path_rule(EdgePathRule {
                edge_kind: edge_kind.to_string(),
                path: EdgePath::Straight,
            });
        },
    );
    engine.register_fn(
        "edge_path_spline",
        |p: &mut FieldProjection, edge_kind: ImmutableString, tension: f64| {
            p.add_edge_path_rule(EdgePathRule {
                edge_kind: edge_kind.to_string(),
                path: EdgePath::Spline {
                    tension: tension as f32,
                },
            });
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::registry::FieldDef;

    #[test]
    fn empty_script_returns_empty_projection() {
        let p = build_from_script("new_projection()").unwrap();
        assert!(p.registry.is_empty());
        assert!(p.couplings.is_empty());
        assert!(p.edge_path_rules.is_empty());
        assert!(p.z_field.is_none());
    }

    #[test]
    fn add_scalar_gaussian() {
        let p = build_from_script(
            r#"
            let p = new_projection();
            p.add_scalar("focus", gaussian(0.0, 0.0, 10.0));
            p
        "#,
        )
        .unwrap();
        assert_eq!(p.registry.len(), 1);
        let id = p.registry.lookup("focus").unwrap();
        match p.registry.get(id) {
            Some(FieldDef::Scalar(ScalarField::Gaussian { sigma, .. })) => {
                assert_eq!(*sigma, 10.0);
            }
            _ => panic!("expected Gaussian"),
        }
    }

    #[test]
    fn returns_id_for_chaining() {
        let p = build_from_script(
            r#"
            let p = new_projection();
            let id = p.add_scalar("focus", gaussian(0.0, 0.0, 10.0));
            p.set_z_field(id);
            p
        "#,
        )
        .unwrap();
        assert!(p.z_field.is_some());
        assert_eq!(p.registry.name_of(p.z_field.unwrap()), Some("focus"));
    }

    #[test]
    fn couple_attract_appends() {
        let p = build_from_script(
            r#"
            let p = new_projection();
            let id = p.add_scalar("focus", gaussian(0.0, 0.0, 10.0));
            p.couple_attract("paper", id, 1.5);
            p
        "#,
        )
        .unwrap();
        assert_eq!(p.couplings.len(), 1);
        let c = &p.couplings[0];
        assert_eq!(c.selector, NodeSelector::Kind("paper".into()));
        assert_eq!(c.response, CouplingResponse::AttractToMin);
        assert_eq!(c.strength, 1.5);
    }

    #[test]
    fn multiple_couplings() {
        let p = build_from_script(
            r#"
            let p = new_projection();
            let f = p.add_scalar("f", scalar_const(1.0));
            p.couple_attract("paper", f, 1.0);
            p.couple_repel("noise", f, 0.5);
            p.couple_advect("particle", f, 0.3);
            p
        "#,
        )
        .unwrap();
        assert_eq!(p.couplings.len(), 3);
    }

    #[test]
    fn dampen_inside_takes_factor() {
        let p = build_from_script(
            r#"
            let p = new_projection();
            let f = p.add_scalar("dampener", scalar_const(1.0));
            p.couple_dampen_inside("any", f, 1.0, 0.4);
            p
        "#,
        )
        .unwrap();
        match p.couplings[0].response {
            CouplingResponse::DampenInside { factor } => assert_eq!(factor, 0.4),
            _ => panic!("expected DampenInside"),
        }
    }

    #[test]
    fn edge_path_field_line_appends() {
        let p = build_from_script(
            r#"
            let p = new_projection();
            let v = p.add_vector("flow", gradient(gaussian(0.0, 0.0, 10.0)));
            p.edge_path_field_line("cites", v, 32, 4.0);
            p
        "#,
        )
        .unwrap();
        assert_eq!(p.edge_path_rules.len(), 1);
        match &p.edge_path_rules[0].path {
            EdgePath::FieldLine {
                max_steps,
                step_size,
                ..
            } => {
                assert_eq!(*max_steps, 32);
                assert_eq!(*step_size, 4.0);
            }
            _ => panic!("expected FieldLine"),
        }
    }

    #[test]
    fn full_canvas_script() {
        let p = build_from_script(
            r#"
            let p = new_projection();
            let focus = p.add_scalar("focus", gaussian(0.0, 0.0, 200.0));
            let flow = p.add_vector("flow", gradient(gaussian(0.0, 0.0, 200.0)));
            p.set_z_field(focus);
            p.couple_attract("paper", focus, 1.0);
            p.couple_repel("paper", focus, 0.3);
            p.edge_path_field_line("cites", flow, 32, 4.0);
            p
        "#,
        )
        .unwrap();
        assert_eq!(p.registry.len(), 2);
        assert_eq!(p.couplings.len(), 2);
        assert_eq!(p.edge_path_rules.len(), 1);
        assert!(p.z_field.is_some());
    }

    #[test]
    fn parse_error_returns_eval_error() {
        let result = build_from_script("this is not valid rhai @@@");
        assert!(matches!(result, Err(BuildError::EvalError(_))));
    }

    #[test]
    fn combinators_compose_scalars() {
        let p = build_from_script(
            r#"
            let p = new_projection();
            let f = add(scalar_x(), scalar_y());
            p.add_scalar("ramp", f);
            p
        "#,
        )
        .unwrap();
        let id = p.registry.lookup("ramp").unwrap();
        match p.registry.get(id) {
            Some(FieldDef::Scalar(ScalarField::Add(_, _))) => {}
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn negate_and_scale() {
        let p = build_from_script(
            r#"
            let p = new_projection();
            p.add_scalar("flipped", negate(scalar_x()));
            p.add_scalar("scaled", scale_by(scalar_y(), 2.5));
            p
        "#,
        )
        .unwrap();
        assert_eq!(p.registry.len(), 2);
    }
}
