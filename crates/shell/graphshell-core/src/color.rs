/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable RGBA color primitive.
//!
//! Pre-Slice-52, this type lived inside
//! `shell::desktop::runtime::registries::theme` with a feature-flag
//! switch that aliased to `egui::Color32` under `egui-host`. That made
//! every portable consumer (knowledge registry, lens registry,
//! presentation, edge styles) reach into the shell-side runtime tree
//! to import a primitive — backwards layering. Slice 52 promotes the
//! portable form to `graphshell-core::color::Color32`. The egui-host
//! re-export at theme.rs continues to alias to `egui::Color32` (the
//! egui-host build is frozen per the iced jump-ship plan §S1, so its
//! integration with `egui::Color32` is unchanged).

use serde::{Deserialize, Serialize};

/// Portable 32-bit RGBA color (`[r, g, b, a]` bytes, alpha in
/// `0..=255`). Constructors mirror the API of `egui::Color32` so the
/// migration from the egui-flavoured original is mechanical.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color32([u8; 4]);

impl Color32 {
    pub const BLACK: Self = Self([0, 0, 0, 255]);
    pub const GRAY: Self = Self([128, 128, 128, 255]);
    pub const TRANSPARENT: Self = Self([0, 0, 0, 0]);
    pub const WHITE: Self = Self([255, 255, 255, 255]);

    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self([r, g, b, 255])
    }

    pub const fn from_rgba_unmultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self([r, g, b, a])
    }

    pub const fn from_gray(value: u8) -> Self {
        Self([value, value, value, 255])
    }

    pub const fn r(self) -> u8 {
        self.0[0]
    }

    pub const fn g(self) -> u8 {
        self.0[1]
    }

    pub const fn b(self) -> u8 {
        self.0[2]
    }

    pub const fn a(self) -> u8 {
        self.0[3]
    }

    pub const fn to_array(self) -> [u8; 4] {
        self.0
    }
}
