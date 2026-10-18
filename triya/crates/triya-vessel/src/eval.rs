// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Resolution: the vessel's elements as axial stations (`z_from`, `z_to` from the datum) with
//! inside radii, and the inside radius at any `z`.
//!
//! Elements stack along +Z in document order. A head's axial extent is its straight flange plus
//! its dished depth; a bottom head (first element) has its tangent line at its end, a top head
//! (last element) at its start. The `diameter_basis` is applied here, so everything downstream
//! works with inside radii and wall thickness.

use triya_stdlib::HeadPreset;

use crate::model::{Datum, DiameterBasis, Element, ElementKind, VesselDoc};
use crate::{Result, VesselError};

/// Where a head's dish faces, along the axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadSide {
    /// The crown is at `z_from`, the tangent line at `z_to` (a bottom head).
    CrownAtStart,
    /// The tangent line is at `z_from`, the crown at `z_to` (a top head).
    CrownAtEnd,
}

/// The crown and knuckle (inside) of a torispherical head, and its dished depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Torus {
    pub crown_r: f64,
    pub knuckle_r: f64,
}

/// The shape a station's radius follows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Profile {
    /// Constant inside radius.
    Straight,
    /// Linear from `r_from` to `r_to`.
    Conical,
    /// A formed head: straight flange `sf`, then a dish of depth `dish` (both along the axis).
    Head { side: HeadSide, sf: f64, dish: f64, torus: Option<Torus>, ellipsoidal: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Station {
    pub index: usize,
    pub id: String,
    pub kind: &'static str,
    pub z_from: f64,
    pub z_to: f64,
    /// Inside radius at `z_from` and `z_to`.
    pub r_from: f64,
    pub r_to: f64,
    pub thickness: f64,
    pub profile: Profile,
}

impl Station {
    pub fn length(&self) -> f64 {
        self.z_to - self.z_from
    }

    /// Inside radius at `z` (within this station).
    pub fn radius_at(&self, z: f64) -> Option<f64> {
        if !(z.is_finite() && z >= self.z_from - 1e-9 && z <= self.z_to + 1e-9) {
            return None;
        }
        let len = self.length();
        match self.profile {
            Profile::Straight => Some(self.r_from),
            Profile::Conical => {
                let t = if len > 0.0 { ((z - self.z_from) / len).clamp(0.0, 1.0) } else { 0.0 };
                Some(self.r_from + t * (self.r_to - self.r_from))
            }
            Profile::Head { side, sf, dish, torus, ellipsoidal } => {
                // `s`: distance from the tangent line into the dish (negative within the straight flange).
                let (r_shell, s) = match side {
                    HeadSide::CrownAtStart => (self.r_to, (self.z_to - sf) - z),
                    HeadSide::CrownAtEnd => (self.r_from, z - (self.z_from + sf)),
                };
                if s <= 0.0 {
                    return Some(r_shell);
                }
                let s = s.min(dish);
                let x = match torus {
                    Some(Torus { crown_r: l, knuckle_r: r }) => {
                        // Knuckle centre at (R − r, 0), crown centre at (0, −c), c = sqrt((L−r)² − (R−r)²).
                        let a = l - r;
                        let b = r_shell - r;
                        let c = (a * a - b * b).max(0.0).sqrt();
                        let s_t = if a > 0.0 { r * c / a } else { 0.0 };
                        if s <= s_t { b + (r * r - s * s).max(0.0).sqrt() } else { (l * l - (s + c) * (s + c)).max(0.0).sqrt() }
                    }
                    None if ellipsoidal => {
                        // Semi-ellipse with semi-axes R (radial) and `dish` (axial); a hemisphere is R = dish.
                        if dish > 0.0 { r_shell * (1.0 - (s / dish) * (s / dish)).max(0.0).sqrt() } else { r_shell }
                    }
                    None => r_shell,
                };
                Some(x)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub stations: Vec<Station>,
    /// Axial extent of the whole vessel.
    pub z_min: f64,
    pub z_max: f64,
}

impl Resolved {
    pub fn station(&self, id: &str) -> Option<&Station> {
        self.stations.iter().find(|s| s.id == id)
    }

    /// Inside radius at `z`, from the station that covers it (the later one at a joint).
    pub fn radius_at(&self, z: f64) -> Option<f64> {
        self.stations.iter().rev().find_map(|s| s.radius_at(z))
    }
}

fn inside_diameter(doc: &VesselDoc, d: f64, t: f64) -> f64 {
    match doc.global.diameter_basis {
        DiameterBasis::Inside => d,
        DiameterBasis::Outside => d - 2.0 * t,
    }
}

/// An element's axial length and radii (inside), from the document's basis.
fn extent(doc: &VesselDoc, index: usize, count: usize, e: &Element) -> Result<(f64, f64, f64, Profile, &'static str)> {
    let t = e.thickness_mm;
    let di = inside_diameter(doc, e.id_or_od_mm, t);
    if !(di > 0.0) {
        return Err(VesselError::Invalid(format!("element `{}`: the wall {t} leaves no inside diameter in {}", e.id, e.id_or_od_mm)));
    }
    let r = di / 2.0;
    match &e.kind {
        ElementKind::Cylinder => Ok((e.length_mm, r, r, Profile::Straight, "cylinder")),
        ElementKind::Skirt => Ok((e.length_mm, r, r, Profile::Straight, "skirt")),
        ElementKind::FlatCover => Ok((e.length_mm, r, r, Profile::Straight, "flat_cover")),
        ElementKind::BodyFlange => Ok((e.length_mm, r, r, Profile::Straight, "body_flange")),
        ElementKind::Cone { small_diameter_mm } => {
            let small = inside_diameter(doc, *small_diameter_mm, t);
            if !(small >= 0.0) {
                return Err(VesselError::Invalid(format!("element `{}`: the wall leaves no small-end diameter", e.id)));
            }
            Ok((e.length_mm, r, small / 2.0, Profile::Conical, "cone"))
        }
        ElementKind::Head { preset } => {
            let side = if index == 0 {
                HeadSide::CrownAtStart
            } else if index + 1 == count {
                HeadSide::CrownAtEnd
            } else {
                return Err(VesselError::Invalid(format!("element `{}`: a head must be the first or the last element", e.id)));
            };
            let dish = preset.dish_depth(di, t)?;
            let sf = e.straight_flange_mm;
            let torus = match preset {
                HeadPreset::Torispherical { crown_ratio, knuckle_ratio, basis } => {
                    let d_ref = match basis {
                        triya_stdlib::RatioBasis::Inside => di,
                        triya_stdlib::RatioBasis::Outside => di + 2.0 * t,
                    };
                    Some(Torus { crown_r: crown_ratio * d_ref, knuckle_r: knuckle_ratio * d_ref })
                }
                _ => None,
            };
            let ellipsoidal = matches!(preset, HeadPreset::Ellipsoidal { .. } | HeadPreset::Hemispherical);
            let profile = Profile::Head { side, sf, dish, torus, ellipsoidal };
            Ok((sf + dish, r, r, profile, "head"))
        }
    }
}

/// Stack the elements from the datum and work out each one's station.
pub fn resolve(doc: &VesselDoc) -> Result<Resolved> {
    doc.validate()?;
    if doc.elements.is_empty() {
        return Err(VesselError::Invalid("the vessel has no elements".into()));
    }
    let count = doc.elements.len();
    let mut stations = Vec::with_capacity(count);
    let mut z = 0.0;
    for (index, e) in doc.elements.iter().enumerate() {
        let (len, r_from, r_to, profile, kind) = extent(doc, index, count, e)?;
        if !(len.is_finite() && len > 0.0) {
            return Err(VesselError::Invalid(format!("element `{}` has no axial length", e.id)));
        }
        stations.push(Station { index, id: e.id.clone(), kind, z_from: z, z_to: z + len, r_from, r_to, thickness: e.thickness_mm, profile });
        z += len;
        if !z.is_finite() || z > crate::model::MAX_LENGTH_MM * 2.0 {
            return Err(VesselError::Invalid("the vessel is longer than the model allows".into()));
        }
    }
    // The datum moves everything so that zero is where the user says.
    let shift = match doc.global.datum {
        Datum::FirstElementStart => 0.0,
        Datum::FirstTangentLine => stations.first().map_or(0.0, |s| s.z_to),
    };
    if shift != 0.0 {
        for s in &mut stations {
            s.z_from -= shift;
            s.z_to -= shift;
        }
    }
    let z_min = stations.first().map_or(0.0, |s| s.z_from);
    let z_max = stations.last().map_or(0.0, |s| s.z_to);
    Ok(Resolved { stations, z_min, z_max })
}
