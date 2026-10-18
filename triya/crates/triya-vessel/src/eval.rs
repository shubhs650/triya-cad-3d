// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Resolution: the vessel's elements as axial stations (`z_from`, `z_to` from the datum) with
//! inside radii, the inside radius at any `z`, and the warnings a drawing should carry.
//!
//! # The stack
//! Elements stack along the axis (+Z vertical, +X horizontal) in document order. A leading
//! `skirt` is not part of the pressure envelope: it hangs below the bottom head (its top
//! `top_offset_mm` below the bottom tangent line, its base `height_mm` lower, `height_mm`
//! measured from the underside of the base ring) and every other element follows. The first
//! and last envelope elements may be heads; a bottom head has its crown at `z_from` and its
//! tangent line (TL) at `z_to − straight_flange`, a top head its TL at `z_from + straight_flange`
//! and its crown at `z_to`. A head's station runs from the seam to the inside of the crown.
//!
//! # Diameters
//! `diameter_mm` is read on `global.diameter_basis` and inherited from the previous element
//! when omitted. A cone states both ends: the end nearer the inherited diameter comes first
//! (a leading cone starts with its large end), and the element after it inherits the far end,
//! so a change to a course's diameter propagates down the stack and stops at the cone. A skirt
//! without a diameter gets the bottom head's outside diameter.
//!
//! # Heads (inside geometry)
//! Torispherical presets give crown `L` and knuckle `r` (ASME F&D: `L = D`, `r = max(0.06 D, 3 t)`;
//! DIN presets on `Da = D + 2 t`); the dish depth is `L − √((L − r)² − (D/2 − r)²)`, the exact
//! geometry. DIN 28011/28013 publish linear fits (`0.1935 Da − 0.455 s`, `0.255 Da − 0.635 s`)
//! that differ from the exact geometry by 0.06 mm and 0.39 mm at Da 1000 / s 10 (188.89 vs
//! 188.95, 248.26 vs 248.65); Triya models the exact geometry and the tests hold it within
//! 0.5 mm of the fits. 2:1 ellipsoidal heads are true semi-ellipses (depth D/4), hemispheres
//! depth D/2.
//!
//! # Datums
//! `bottom_tl` / `left_tl`: the bottom (left) head's TL; `bottom_seam`: the end of its straight
//! flange; `skirt_base`: the underside of the skirt's base ring; `plant_elevation`: grade, with
//! the bottom TL at `plant_elevation_mm`. Without a bottom head, the TL datums are the start of
//! the first envelope element.

use triya_stdlib::TableSet;

use crate::model::{Datum, DiameterBasis, Element, ElementKind, HeadOrientation, HeadPresetId, Orientation, Rating, VesselDoc};
use crate::{Result, VesselError};

/// Half-apex angle above which a cone gets a warning (ASME VIII-1 UG-33/1-5 territory).
pub const CONE_WARN_DEG: f64 = 30.0;
/// Diameter mismatch (mm) at a cone's near end that gets a warning.
pub const JOINT_TOL_MM: f64 = 0.5;

/// Where a head's dish faces, along the axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadSide {
    /// The crown is at `z_from`, the tangent line toward `z_to` (a bottom head).
    CrownAtStart,
    /// The tangent line is toward `z_from`, the crown at `z_to` (a top head).
    CrownAtEnd,
}

/// A head's dished part (inside surface).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeadShape {
    /// Spherical crown of radius `crown_r` blended by a knuckle of radius `knuckle_r`.
    Torispherical { crown_r: f64, knuckle_r: f64 },
    /// Semi-ellipse with semi-axes R (radial) and the dish depth (axial); a hemisphere when equal.
    Ellipsoidal,
}

/// The shape a station's inside radius follows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Profile {
    /// Constant inside radius.
    Straight,
    /// From `r_from` to `r_to`, with an inside knuckle of the given radius at each end (0: none).
    Conical { knuckle_from: f64, knuckle_to: f64 },
    /// A formed head: straight flange `sf`, then a dish of depth `dish` (both along the axis).
    Head { side: HeadSide, sf: f64, dish: f64, shape: HeadShape },
}

/// A weld-neck body flange, resolved (all mm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlangeGeom {
    pub od: f64,
    pub bolt_circle: f64,
    pub bolt_count: u32,
    pub bolt_hole: f64,
    /// Ring thickness.
    pub thickness: f64,
    pub hub_od: f64,
    /// Hub length below the ring (to the weld end).
    pub hub_len: f64,
    /// Axial gap left at the face for the gasket.
    pub gap: f64,
    /// The hub (weld end) is at `z_from` and the face toward `z_to`; false when the previous
    /// element is a body flange too (the mating flange of a pair faces back).
    pub hub_at_start: bool,
}

/// Kind-specific geometry a station carries beyond its profile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Detail {
    None,
    Flange(FlangeGeom),
    /// A skirt's base ring: inside and outside diameter, thickness.
    BaseRing {
        id: f64,
        od: f64,
        thickness: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Station {
    pub index: usize,
    pub id: String,
    pub kind: &'static str,
    pub z_from: f64,
    pub z_to: f64,
    /// Inside radius at `z_from` and `z_to` (a head's: at its seam on both ends).
    pub r_from: f64,
    pub r_to: f64,
    pub thickness: f64,
    pub profile: Profile,
    /// The diameter (on the document's basis) the next element inherits.
    pub diameter_mm: f64,
    /// Part of the pressure envelope (a skirt is not).
    pub envelope: bool,
    /// A head's tangent line.
    pub tl: Option<f64>,
    /// A cone's half-apex angle (degrees) along its straight part.
    pub half_apex_deg: Option<f64>,
    pub detail: Detail,
}

/// A cone's straight part, knuckles included: everything `radius_at` needs.
struct ConeFit {
    /// Large-end radius, small-end radius, large and small knuckle radii, axial length.
    r_l: f64,
    r_s: f64,
    k_l: f64,
    k_s: f64,
    len: f64,
    /// Half-apex angle (radians).
    alpha: f64,
}

impl ConeFit {
    /// The straight line tangent to both knuckles (large: centre inside, small: centre outside):
    /// `(R_L − R_s − k_L − k_s) cos α − L sin α + k_L + k_s = 0`.
    fn new(r_l: f64, r_s: f64, k_l: f64, k_s: f64, len: f64) -> Option<ConeFit> {
        let a = r_l - r_s - k_l - k_s;
        let k = k_l + k_s;
        let rho = a.hypot(len);
        if !(rho > 0.0) || k > rho {
            return None;
        }
        let alpha = (-k / rho).acos() - len.atan2(a);
        let fit = ConeFit { r_l, r_s, k_l, k_s, len, alpha };
        let (u1, u2) = (k_l * alpha.sin(), len - k_s * alpha.sin());
        (alpha.is_finite() && (0.0..std::f64::consts::FRAC_PI_2 - 1e-6).contains(&alpha) && u1 <= u2 + 1e-9).then_some(fit)
    }

    /// Inside radius at `u` from the large end.
    fn radius(&self, u: f64) -> f64 {
        let (s, c) = self.alpha.sin_cos();
        if u <= self.k_l * s {
            (self.r_l - self.k_l) + (self.k_l * self.k_l - u * u).max(0.0).sqrt()
        } else if u >= self.len - self.k_s * s {
            let w = self.len - u;
            (self.r_s + self.k_s) - (self.k_s * self.k_s - w * w).max(0.0).sqrt()
        } else {
            let line_c = (self.r_l - self.k_l) * c + self.k_l;
            (line_c - u * s) / c
        }
    }
}

impl Station {
    pub fn length(&self) -> f64 {
        self.z_to - self.z_from
    }

    fn cone(&self) -> Option<(ConeFit, bool)> {
        let Profile::Conical { knuckle_from, knuckle_to } = self.profile else { return None };
        let large_first = self.r_from >= self.r_to;
        let fit = if large_first {
            ConeFit::new(self.r_from, self.r_to, knuckle_from, knuckle_to, self.length())
        } else {
            ConeFit::new(self.r_to, self.r_from, knuckle_to, knuckle_from, self.length())
        };
        fit.map(|f| (f, large_first))
    }

    /// Inside radius at `z` (within this station).
    pub fn radius_at(&self, z: f64) -> Option<f64> {
        if !(z.is_finite() && z >= self.z_from - 1e-9 && z <= self.z_to + 1e-9) {
            return None;
        }
        match self.profile {
            Profile::Straight => Some(self.r_from),
            Profile::Conical { .. } => {
                let u = (z - self.z_from).clamp(0.0, self.length());
                match self.cone() {
                    Some((fit, true)) => Some(fit.radius(u)),
                    Some((fit, false)) => Some(fit.radius(self.length() - u)),
                    None => None,
                }
            }
            Profile::Head { side, sf, dish, shape } => {
                // `s`: distance from the tangent line into the dish (negative within the straight flange).
                let (r_shell, s) = match side {
                    HeadSide::CrownAtStart => (self.r_to, (self.z_to - sf) - z),
                    HeadSide::CrownAtEnd => (self.r_from, z - (self.z_from + sf)),
                };
                if s <= 0.0 {
                    return Some(r_shell);
                }
                let s = s.min(dish);
                Some(match shape {
                    HeadShape::Torispherical { crown_r: l, knuckle_r: r } => {
                        // Knuckle centre at (R − r, 0), crown centre at (0, −c), c = √((L−r)² − (R−r)²).
                        let a = l - r;
                        let b = r_shell - r;
                        let c = (a * a - b * b).max(0.0).sqrt();
                        let s_t = if a > 0.0 { r * c / a } else { 0.0 };
                        if s <= s_t { b + (r * r - s * s).max(0.0).sqrt() } else { (l * l - (s + c) * (s + c)).max(0.0).sqrt() }
                    }
                    HeadShape::Ellipsoidal => {
                        if dish > 0.0 {
                            r_shell * (1.0 - (s / dish) * (s / dish)).max(0.0).sqrt()
                        } else {
                            r_shell
                        }
                    }
                })
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    /// In document order (a leading skirt first).
    pub stations: Vec<Station>,
    /// Axial extent of everything, skirt included.
    pub z_min: f64,
    pub z_max: f64,
    /// The bottom (left) and top (right) heads' tangent lines.
    pub tl_bottom: Option<f64>,
    pub tl_top: Option<f64>,
    pub orientation: Orientation,
    /// Things a drawing should flag: steep cones, mismatched joints.
    pub warnings: Vec<String>,
}

impl Resolved {
    pub fn station(&self, id: &str) -> Option<&Station> {
        self.stations.iter().find(|s| s.id == id)
    }

    /// Inside radius of the pressure envelope at `z` (the later station at a joint).
    pub fn radius_at(&self, z: f64) -> Option<f64> {
        self.stations.iter().rev().filter(|s| s.envelope).find_map(|s| s.radius_at(z))
    }

    /// TL to TL, when both heads exist.
    pub fn tl_to_tl(&self) -> Option<f64> {
        Some(self.tl_top? - self.tl_bottom?)
    }
}

fn invalid(e: &Element, msg: impl std::fmt::Display) -> VesselError {
    VesselError::Invalid(format!("element `{}`: {msg}", e.id))
}

fn inside(doc: &VesselDoc, d: f64, t: f64) -> f64 {
    match doc.global.diameter_basis {
        DiameterBasis::Inside => d,
        DiameterBasis::Outside => d - 2.0 * t,
    }
}

/// The dish of a head on inside diameter `di` with wall `t`: shape and depth.
pub fn head_dish(preset: HeadPresetId, di: f64, t: f64, crown: Option<f64>, knuckle: Option<f64>) -> Result<(HeadShape, f64)> {
    let da = di + 2.0 * t;
    let (l, r) = match preset {
        HeadPresetId::Ellipsoidal21 => return Ok((HeadShape::Ellipsoidal, di / 4.0)),
        HeadPresetId::Hemispherical => return Ok((HeadShape::Ellipsoidal, di / 2.0)),
        HeadPresetId::AsmeFd => (di, (0.06 * di).max(3.0 * t)),
        HeadPresetId::Ellipsoidal21Ug32Approx => (0.9 * di, 0.17 * di),
        HeadPresetId::KloepperDin28011 => (da, 0.1 * da),
        HeadPresetId::KorbbogenDin28013 => (0.8 * da, 0.154 * da),
        HeadPresetId::Torispherical8010 => (0.8 * di, 0.1 * di),
        HeadPresetId::CustomTorispherical => (crown.unwrap_or(0.0), knuckle.unwrap_or(0.0)),
    };
    let (l, r) = (crown.unwrap_or(l), knuckle.unwrap_or(r));
    let dish = triya_stdlib::head::torispherical_depth(di, l, r)?;
    Ok((HeadShape::Torispherical { crown_r: l, knuckle_r: r }, dish))
}

/// A body flange's dimensions: from the table row its `flange_ref` names, or `explicit`.
fn body_flange(tables: &TableSet, e: &Element, r_in: f64, warnings: &mut Vec<String>) -> Result<FlangeGeom> {
    let ElementKind::BodyFlange { flange_ref, explicit, gasket_gap_mm } = &e.kind else {
        return Err(invalid(e, "not a body flange"));
    };
    let t = e.thickness_mm;
    let g = match (flange_ref, explicit) {
        (_, Some(f)) => FlangeGeom {
            od: f.od_mm,
            bolt_circle: f.bolt_circle_mm,
            bolt_count: f.bolt_count,
            bolt_hole: f.bolt_hole_mm,
            thickness: f.thickness_mm,
            hub_od: f.hub_od_mm,
            hub_len: f.hub_len_mm,
            gap: *gasket_gap_mm,
            hub_at_start: true,
        },
        (Some(r), None) => {
            let class = match &r.class {
                Rating::Class(c) => c.to_string(),
                Rating::Named(s) => s.clone(),
            };
            let row = tables
                .flange(&r.nps_or_dn, &class, r.kind)
                .ok_or_else(|| invalid(e, format!("no flange row {}/{class}/{} in the loaded tables", r.nps_or_dn, r.kind.code())))?;
            if row.hub_od <= 0.0 {
                return Err(invalid(e, "the table row has no hub (a blind flange is a flat cover)"));
            }
            let bolt_hole = row.bolt_hole_mm().ok_or_else(|| invalid(e, format!("cannot read the bolt size `{}`", row.bolt_size)))?;
            if tables.flanges.is_illustrative() {
                warnings.push(format!("element `{}`: flange row {} comes from an illustrative table", e.id, row.size_key()));
            }
            warnings.push(format!("element `{}`: bolt holes {bolt_hole} mm derived from bolt {} (clearance rule)", e.id, row.bolt_size));
            FlangeGeom {
                od: row.od,
                bolt_circle: row.bolt_circle,
                bolt_count: row.bolt_count,
                bolt_hole,
                thickness: row.thickness,
                hub_od: row.hub_od,
                hub_len: (row.length_through_hub - row.thickness).max(0.0),
                gap: *gasket_gap_mm,
                hub_at_start: true,
            }
        }
        (None, None) => return Err(invalid(e, "a body flange needs `flange_ref` or `explicit`")),
    };
    let r_out = r_in + t;
    if !(g.hub_od / 2.0 >= r_out - 1e-9
        && g.bolt_circle / 2.0 - g.bolt_hole / 2.0 > g.hub_od / 2.0
        && g.bolt_circle / 2.0 + g.bolt_hole / 2.0 < g.od / 2.0)
    {
        return Err(invalid(
            e,
            format!(
                "flange (OD {}, bolt circle {}, holes {}, hub {}) does not fit a shell of OD {}: the hub must be at least the shell OD and the holes between hub and rim",
                g.od,
                g.bolt_circle,
                g.bolt_hole,
                g.hub_od,
                2.0 * r_out
            ),
        ));
    }
    Ok(g)
}

/// Stack the elements from the datum and work out each one's station, with the embedded tables.
pub fn resolve(doc: &VesselDoc) -> Result<Resolved> {
    let tables = TableSet::embedded()?;
    resolve_with(doc, &tables)
}

/// Stack the elements from the datum and work out each one's station; body flanges look their
/// rows up in `tables`.
pub fn resolve_with(doc: &VesselDoc, tables: &TableSet) -> Result<Resolved> {
    doc.validate()?;
    let mut warnings = Vec::new();
    let skirt = doc.elements.first().filter(|e| matches!(e.kind, ElementKind::Skirt { .. }));
    if doc.elements.iter().skip(1).any(|e| matches!(e.kind, ElementKind::Skirt { .. })) {
        return Err(VesselError::Invalid("a skirt must be the first element".into()));
    }
    if skirt.is_some() && doc.global.orientation == Orientation::Horizontal {
        return Err(VesselError::Invalid("a horizontal vessel has no skirt (saddles carry it)".into()));
    }
    let first = usize::from(skirt.is_some());
    let stack = doc.elements.get(first..).unwrap_or_default();
    if stack.is_empty() {
        return Err(VesselError::Invalid("the vessel has no shell elements".into()));
    }
    let last = stack.len() - 1;
    let mut stations: Vec<Station> = Vec::with_capacity(doc.elements.len());
    let mut z = 0.0;
    let mut inherited: Option<f64> = None;
    for (k, e) in stack.iter().enumerate() {
        let t = e.thickness_mm;
        let index = first + k;
        let mut st = Station {
            index,
            id: e.id.clone(),
            kind: e.kind.name(),
            z_from: z,
            z_to: z,
            r_from: 0.0,
            r_to: 0.0,
            thickness: t,
            profile: Profile::Straight,
            diameter_mm: 0.0,
            envelope: true,
            tl: None,
            half_apex_deg: None,
            detail: Detail::None,
        };
        let len = match &e.kind {
            ElementKind::Cone { length_mm, diameter_small_mm: ds, diameter_large_mm: dl, knuckle_large_mm, knuckle_small_mm } => {
                let small_first = inherited.is_some_and(|d| (d - ds).abs() < (d - dl).abs());
                let (d0, d1, k0, k1) = if small_first {
                    (*ds, *dl, knuckle_small_mm.unwrap_or(0.0), knuckle_large_mm.unwrap_or(0.0))
                } else {
                    (*dl, *ds, knuckle_large_mm.unwrap_or(0.0), knuckle_small_mm.unwrap_or(0.0))
                };
                if let Some(d) = inherited
                    && (d - d0).abs() > JOINT_TOL_MM
                {
                    warnings.push(format!("element `{}`: the cone's end ({d0} mm) does not meet the previous element ({d} mm)", e.id));
                }
                st.r_from = inside(doc, d0, t) / 2.0;
                st.r_to = inside(doc, d1, t) / 2.0;
                if !(st.r_from > 0.0 && st.r_to > 0.0) {
                    return Err(invalid(e, "the wall leaves no inside diameter"));
                }
                let k_small = if small_first { k0 } else { k1 };
                if k_small > 0.0 && k_small <= t {
                    return Err(invalid(e, "a small-end knuckle must be larger than the wall thickness (it is an inside radius)"));
                }
                st.profile = Profile::Conical { knuckle_from: k0, knuckle_to: k1 };
                st.diameter_mm = d1;
                st.z_to = z + length_mm;
                let (fit, _) = st.cone().ok_or_else(|| invalid(e, "the knuckles do not fit the cone's length and diameters"))?;
                let deg = fit.alpha.to_degrees();
                st.half_apex_deg = Some(deg);
                if deg > CONE_WARN_DEG {
                    warnings.push(format!("element `{}`: half-apex angle {deg:.1}° exceeds {CONE_WARN_DEG}°", e.id));
                }
                *length_mm
            }
            _ => {
                let d = e.diameter_mm.or(inherited).ok_or_else(|| invalid(e, "needs `diameter_mm` (nothing to inherit)"))?;
                // A flat cover's own thickness is a plate, not a wall: its radius comes from its
                // neighbours below.
                let di = inside(doc, d, if matches!(e.kind, ElementKind::FlatCover { .. }) { 0.0 } else { t });
                if !(di > 0.0) {
                    return Err(invalid(e, format!("the wall {t} leaves no inside diameter in {d}")));
                }
                st.r_from = di / 2.0;
                st.r_to = di / 2.0;
                st.diameter_mm = d;
                match &e.kind {
                    ElementKind::Cylinder { length_mm } => *length_mm,
                    ElementKind::FlatCover { .. } => t,
                    ElementKind::BodyFlange { .. } => {
                        let g = body_flange(tables, e, st.r_from, &mut warnings)?;
                        st.detail = Detail::Flange(g);
                        g.hub_len + g.thickness + g.gap
                    }
                    ElementKind::Head { preset, straight_flange_mm, crown_radius_mm, knuckle_radius_mm, orientation } => {
                        let (side, expected, word) = if k == 0 && last > 0 {
                            (HeadSide::CrownAtStart, HeadOrientation::Down, "down")
                        } else if k == last && last > 0 {
                            (HeadSide::CrownAtEnd, HeadOrientation::Up, "up")
                        } else {
                            return Err(invalid(e, "a head must be the first or the last shell element"));
                        };
                        if orientation.is_some_and(|o| o != expected) {
                            return Err(invalid(e, format!("this head's position makes its orientation `{word}`")));
                        }
                        let (shape, dish) = head_dish(*preset, di, t, *crown_radius_mm, *knuckle_radius_mm)?;
                        let sf = straight_flange_mm.unwrap_or_else(|| preset.default_straight_flange(t));
                        st.profile = Profile::Head { side, sf, dish, shape };
                        st.tl = Some(match side {
                            HeadSide::CrownAtStart => z + dish,
                            HeadSide::CrownAtEnd => z + sf,
                        });
                        sf + dish
                    }
                    ElementKind::Skirt { .. } | ElementKind::Cone { .. } => return Err(invalid(e, "out of place")),
                }
            }
        };
        if !(len.is_finite() && len > 0.0) {
            return Err(invalid(e, "has no axial length"));
        }
        st.z_to = z + len;
        z = st.z_to;
        if !z.is_finite() || z > crate::model::MAX_LENGTH_MM * 2.0 {
            return Err(VesselError::Invalid("the vessel is longer than the model allows".into()));
        }
        inherited = Some(st.diameter_mm);
        stations.push(st);
    }

    // The mating flange of a pair faces back toward the first.
    for i in 1..stations.len() {
        let prev_is_flange = stations.get(i - 1).is_some_and(|s| matches!(s.detail, Detail::Flange(_)));
        if let Some(Station { detail: Detail::Flange(g), .. }) = stations.get_mut(i) {
            g.hub_at_start = !prev_is_flange;
        }
    }

    // A flat cover is a disc out to the adjacent shell's outside radius (the previous element's,
    // or the next one's when the cover comes first).
    for i in 0..stations.len() {
        if stations.get(i).is_none_or(|s| s.kind != "flat_cover") {
            continue;
        }
        let prev = i.checked_sub(1).and_then(|j| stations.get(j)).filter(|s| s.kind != "flat_cover").map(|s| s.r_to + s.thickness);
        let next = stations.get(i + 1).filter(|s| s.kind != "flat_cover").map(|s| s.r_from + s.thickness);
        if let (Some(r), Some(st)) = (prev.or(next), stations.get_mut(i)) {
            st.r_from = r;
            st.r_to = r;
        }
    }

    let bottom = stations.first().filter(|s| s.kind == "head");
    let tl_bottom = bottom.and_then(|s| s.tl);
    let seam_bottom = bottom.map(|s| s.z_to);
    let tl_top = stations.last().filter(|s| s.kind == "head" && stations.len() > 1).and_then(|s| s.tl);
    let stack_start = stations.first().map_or(0.0, |s| s.z_from);

    // The skirt hangs from the bottom tangent line (or the stack's start).
    if let Some(e) = skirt
        && let ElementKind::Skirt { height_mm, top_offset_mm, .. } = &e.kind
    {
        let t = e.thickness_mm;
        let ri = match e.diameter_mm {
            Some(d) => inside(doc, d, t) / 2.0,
            None => stations.first().map_or(0.0, |s| s.r_to + s.thickness) - t,
        };
        if !(ri > 0.0) {
            return Err(invalid(e, "the skirt has no inside diameter"));
        }
        if let ElementKind::Skirt { base_ring, .. } = &e.kind
            && !(base_ring.id_mm / 2.0 <= ri && base_ring.od_mm / 2.0 >= ri + t && base_ring.thickness_mm < *height_mm)
        {
            return Err(invalid(
                e,
                format!(
                    "the base ring ({} … {}) must span the skirt wall ({} … {}) and be thinner than the skirt is high",
                    base_ring.id_mm,
                    base_ring.od_mm,
                    2.0 * ri,
                    2.0 * (ri + t)
                ),
            ));
        }
        let top = tl_bottom.unwrap_or(stack_start) - top_offset_mm.unwrap_or(0.0);
        let d = match doc.global.diameter_basis {
            DiameterBasis::Inside => 2.0 * ri,
            DiameterBasis::Outside => 2.0 * (ri + t),
        };
        stations.insert(
            0,
            Station {
                index: 0,
                id: e.id.clone(),
                kind: "skirt",
                z_from: top - height_mm,
                z_to: top,
                r_from: ri,
                r_to: ri,
                thickness: t,
                profile: Profile::Straight,
                diameter_mm: d,
                envelope: false,
                tl: None,
                half_apex_deg: None,
                detail: match &e.kind {
                    ElementKind::Skirt { base_ring, .. } => {
                        Detail::BaseRing { id: base_ring.id_mm, od: base_ring.od_mm, thickness: base_ring.thickness_mm }
                    }
                    _ => Detail::None,
                },
            },
        );
    }

    let tl0 = tl_bottom.unwrap_or(stack_start);
    let zero = match doc.global.datum {
        Datum::BottomTl | Datum::LeftTl => tl0,
        Datum::BottomSeam => seam_bottom.unwrap_or(stack_start),
        Datum::SkirtBase => match stations.first().filter(|s| s.kind == "skirt") {
            Some(s) => s.z_from,
            None => return Err(VesselError::Invalid("datum `skirt_base` needs a skirt".into())),
        },
        Datum::PlantElevation => tl0 - doc.global.plant_elevation_mm,
    };
    for s in &mut stations {
        s.z_from -= zero;
        s.z_to -= zero;
        s.tl = s.tl.map(|tl| tl - zero);
    }
    let z_min = stations.iter().map(|s| s.z_from).fold(f64::INFINITY, f64::min);
    let z_max = stations.iter().map(|s| s.z_to).fold(f64::NEG_INFINITY, f64::max);
    Ok(Resolved {
        stations,
        z_min,
        z_max,
        tl_bottom: tl_bottom.map(|z| z - zero),
        tl_top: tl_top.map(|z| z - zero),
        orientation: doc.global.orientation,
        warnings,
    })
}
