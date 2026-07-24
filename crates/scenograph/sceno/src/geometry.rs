//! Plain 2D geometry primitives.
//!
//! Deliberately dependency-free (no euclid, no kernel types): these are the
//! wire shapes of the scene contract, serde-friendly today and archivable
//! later. Hosts convert to their own geometry types at the boundary.

use serde::{Deserialize, Serialize};

/// A 2D point or displacement, in the units of whatever space carries it.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A 2D extent (non-negative by convention; not enforced).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Size2 {
    pub w: f32,
    pub h: f32,
}

impl Size2 {
    pub fn new(w: f32, h: f32) -> Self {
        Self { w, h }
    }
}

/// An axis-aligned rectangle: origin is the min corner.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub origin: Vec2,
    pub size: Size2,
}

impl Rect {
    pub fn new(origin: Vec2, size: Size2) -> Self {
        Self { origin, size }
    }

    /// The rect covering both `self` and `other`.
    pub fn union(self, other: Rect) -> Rect {
        let min_x = self.origin.x.min(other.origin.x);
        let min_y = self.origin.y.min(other.origin.y);
        let max_x = (self.origin.x + self.size.w).max(other.origin.x + other.size.w);
        let max_y = (self.origin.y + self.size.h).max(other.origin.y + other.size.h);
        Rect::new(
            Vec2::new(min_x, min_y),
            Size2::new(max_x - min_x, max_y - min_y),
        )
    }
}

/// A 2D similarity transform: uniform scale, then rotation (radians,
/// counter-clockwise), then translation. Uniform scale keeps footprint
/// shapes similar under transform; non-uniform scale is a representation
/// concern, not a placement one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform2 {
    pub translate: Vec2,
    pub scale: f32,
    pub rotate: f32,
}

impl Default for Transform2 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform2 {
    pub const IDENTITY: Self = Self {
        translate: Vec2::ZERO,
        scale: 1.0,
        rotate: 0.0,
    };

    pub fn translation(x: f32, y: f32) -> Self {
        Self {
            translate: Vec2::new(x, y),
            ..Self::IDENTITY
        }
    }

    /// Apply to a point: scale, rotate, translate.
    pub fn apply(&self, p: Vec2) -> Vec2 {
        let (sin, cos) = self.rotate.sin_cos();
        let sx = p.x * self.scale;
        let sy = p.y * self.scale;
        Vec2::new(
            sx * cos - sy * sin + self.translate.x,
            sx * sin + sy * cos + self.translate.y,
        )
    }

    /// The transform equivalent to applying `self` after `inner`
    /// (`self.then(inner).apply(p) == self.apply(inner.apply(p))`).
    pub fn then(&self, inner: &Transform2) -> Transform2 {
        Transform2 {
            translate: self.apply(inner.translate),
            scale: self.scale * inner.scale,
            rotate: self.rotate + inner.rotate,
        }
    }

    /// The transform undoing `self`, for carrying a point in an outer space
    /// back into a local one (what picking needs). `None` when `scale` is
    /// zero, which collapses the space and cannot be undone.
    pub fn inverse(&self) -> Option<Transform2> {
        if self.scale == 0.0 || !self.scale.is_finite() {
            return None;
        }
        let unrotate = Transform2 {
            translate: Vec2::ZERO,
            scale: 1.0 / self.scale,
            rotate: -self.rotate,
        };
        let shifted = unrotate.apply(self.translate);
        Some(Transform2 {
            translate: Vec2::new(-shifted.x, -shifted.y),
            ..unrotate
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_composes_scale_rotate_translate() {
        let t = Transform2 {
            translate: Vec2::new(10.0, 0.0),
            scale: 2.0,
            rotate: std::f32::consts::FRAC_PI_2,
        };
        // (1, 0) scaled -> (2, 0), rotated 90° ccw -> (0, 2), translated -> (10, 2).
        let p = t.apply(Vec2::new(1.0, 0.0));
        assert!((p.x - 10.0).abs() < 1e-5 && (p.y - 2.0).abs() < 1e-5);
    }

    #[test]
    fn then_matches_sequential_application() {
        let outer = Transform2 {
            translate: Vec2::new(3.0, 4.0),
            scale: 2.0,
            rotate: 0.7,
        };
        let inner = Transform2 {
            translate: Vec2::new(-1.0, 2.0),
            scale: 0.5,
            rotate: -0.2,
        };
        let p = Vec2::new(5.0, -3.0);
        let a = outer.then(&inner).apply(p);
        let b = outer.apply(inner.apply(p));
        assert!((a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4);
    }

    #[test]
    fn inverse_undoes_apply() {
        let t = Transform2 {
            translate: Vec2::new(3.0, -4.0),
            scale: 2.5,
            rotate: 0.9,
        };
        let p = Vec2::new(7.0, 11.0);
        let back = t.inverse().unwrap().apply(t.apply(p));
        assert!((back.x - p.x).abs() < 1e-3 && (back.y - p.y).abs() < 1e-3);
    }

    #[test]
    fn a_collapsed_transform_has_no_inverse() {
        let t = Transform2 {
            scale: 0.0,
            ..Transform2::IDENTITY
        };
        assert_eq!(t.inverse(), None);
    }

    #[test]
    fn rect_union_covers_both() {
        let a = Rect::new(Vec2::new(0.0, 0.0), Size2::new(2.0, 2.0));
        let b = Rect::new(Vec2::new(5.0, -1.0), Size2::new(1.0, 1.0));
        let u = a.union(b);
        assert_eq!(u.origin, Vec2::new(0.0, -1.0));
        assert_eq!(u.size, Size2::new(6.0, 3.0));
    }
}
