// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The vessel document: what the user describes, as JSON (`schema: "triya.vessel/0.1"`).
//!
//! Lengths are millimetres, angles degrees (the document is for people; the engine's radians
//! are the emitter's business). Every numeric field is hostile until [`VesselDoc::validate`]
//! has passed it: finite, within caps, counts bounded.

use serde::{Deserialize, Serialize};
use triya_stdlib::{FlangeKind, HeadPreset, StandardRef};

use crate::{Result, VesselError};

/// The schema tag every document carries.
pub const SCHEMA: &str = "triya.vessel/0.1";
/// Most shell elements one vessel may have.
pub const MAX_ELEMENTS: usize = 256;
/// Most attachments one vessel may have.
pub const MAX_ATTACHMENTS: usize = 2048;
/// Longest length (mm) any field may hold: a kilometre-long vessel is a typo.
pub const MAX_LENGTH_MM: f64 = 1.0e6;
/// Longest id or name.
pub const MAX_ID_LEN: usize = 64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VesselDoc {
    pub schema: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub global: Global,
    /// Shell elements in order along the axis (from the datum, toward +Z).
    #[serde(default)]
    pub elements: Vec<Element>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    #[default]
    Vertical,
    Horizontal,
}

/// Where axial zero is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Datum {
    /// The start of the first element (a bottom head's crown, a skirt's base plate).
    #[default]
    FirstElementStart,
    /// The end of the first element: its tangent line when it is a head.
    FirstTangentLine,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiameterBasis {
    #[default]
    Inside,
    Outside,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AngleSense {
    #[default]
    CounterClockwise,
    Clockwise,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Global {
    #[serde(default)]
    pub orientation: Orientation,
    #[serde(default)]
    pub datum: Datum,
    /// What `Element::id_or_od_mm` means.
    #[serde(default)]
    pub diameter_basis: DiameterBasis,
    /// Where 0° is, as the drawing names it (`"+X"`, `"north"`, `"nozzle N1"`); informational.
    #[serde(default = "default_angle_zero")]
    pub angle_zero_ref: String,
    #[serde(default)]
    pub angle_sense: AngleSense,
    #[serde(default)]
    pub default_material: String,
}

fn default_angle_zero() -> String {
    "+X".into()
}

impl Default for Global {
    fn default() -> Self {
        Global {
            orientation: Orientation::Vertical,
            datum: Datum::FirstElementStart,
            diameter_basis: DiameterBasis::Inside,
            angle_zero_ref: default_angle_zero(),
            angle_sense: AngleSense::CounterClockwise,
            default_material: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ElementKind {
    Cylinder,
    Head {
        preset: HeadPreset,
    },
    /// A cone from `id_or_od_mm` (large end, at the start) to `small_diameter_mm` (at the end).
    Cone {
        small_diameter_mm: f64,
    },
    FlatCover,
    /// A body flange ring; `length_mm` is its thickness.
    BodyFlange,
    Skirt,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub id: String,
    pub kind: ElementKind,
    /// Diameter on the vessel's `diameter_basis`.
    pub id_or_od_mm: f64,
    pub thickness_mm: f64,
    /// Axial length: a cylinder's, cone's or skirt's length, a flange's or cover's thickness.
    /// Unused by heads (their depth comes from the preset and the straight flange).
    #[serde(default)]
    pub length_mm: f64,
    /// A head's straight flange (0 otherwise).
    #[serde(default)]
    pub straight_flange_mm: f64,
    #[serde(default)]
    pub corrosion_allowance_mm: f64,
    /// Overrides `global.default_material`.
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

/// Where along the axis an attachment sits.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AxialRef {
    /// Absolute, from the datum.
    FromDatum { z_mm: f64 },
    /// From the host element's start.
    FromHostStart { dz_mm: f64 },
    /// From the host element's end (toward its start).
    FromHostEnd { dz_mm: f64 },
    /// A fraction 0…1 of the host element's length.
    Fraction { t: f64 },
}

/// How an attachment's axis sits relative to the vessel.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Frame {
    /// Along a radius.
    Radial,
    /// Parallel to a radius, offset sideways by `offset_mm`.
    Hillside {
        offset_mm: f64,
    },
    Tangential,
    /// Radial, tilted `deg` from the plane normal to the axis.
    Tilted {
        deg: f64,
    },
    ParallelToAxis,
    /// On a head, parallel to the axis at radius `r_mm`.
    HeadRadialAtR {
        r_mm: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pad {
    pub od_mm: f64,
    pub thickness_mm: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AttachmentKind {
    Nozzle {
        nps: String,
        class: String,
        flange: FlangeKind,
        /// From the vessel surface to the flange face.
        projection_mm: f64,
        #[serde(default)]
        pad: Option<Pad>,
    },
    Saddle {
        width_mm: f64,
        contact_angle_deg: f64,
        height_mm: f64,
    },
    Leg {
        count: u32,
        section: String,
        length_mm: f64,
    },
    Bracket {
        width_mm: f64,
        height_mm: f64,
    },
    AnchorChair {
        count: u32,
        bolt: String,
    },
    StiffeningRing {
        width_mm: f64,
        thickness_mm: f64,
    },
    LimpetCoil {
        pipe_nps: String,
        pitch_mm: f64,
        turns: f64,
    },
    LiftingLug {
        thickness_mm: f64,
        hole_mm: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    /// The element it sits on (`Element::id`).
    pub host: String,
    pub axial: AxialRef,
    pub angle_deg: f64,
    pub frame: Frame,
    pub kind: AttachmentKind,
    #[serde(default)]
    pub standard_ref: Option<StandardRef>,
    /// The table row key (`"2/150/wn"`) when a standard supplies the dimensions.
    #[serde(default)]
    pub size_key: Option<String>,
}

fn length(name: &str, x: f64, min_exclusive: f64) -> Result<()> {
    if x.is_finite() && x > min_exclusive && x <= MAX_LENGTH_MM {
        Ok(())
    } else {
        Err(VesselError::Invalid(format!("`{name}` must be a finite length in mm, > {min_exclusive} and ≤ {MAX_LENGTH_MM} (got {x})")))
    }
}

fn non_negative_length(name: &str, x: f64) -> Result<()> {
    if x.is_finite() && (0.0..=MAX_LENGTH_MM).contains(&x) {
        Ok(())
    } else {
        Err(VesselError::Invalid(format!("`{name}` must be a finite length in mm, 0…{MAX_LENGTH_MM} (got {x})")))
    }
}

fn ident(what: &str, s: &str) -> Result<()> {
    if s.is_empty() || s.len() > MAX_ID_LEN || s.chars().any(|c| c.is_control()) {
        return Err(VesselError::Invalid(format!("{what} id must be 1…{MAX_ID_LEN} characters without control characters")));
    }
    Ok(())
}

impl VesselDoc {
    /// A new, empty vessel with the default globals.
    pub fn new(name: Option<String>) -> VesselDoc {
        VesselDoc { schema: SCHEMA.into(), name, global: Global::default(), elements: Vec::new(), attachments: Vec::new() }
    }

    /// A single cylinder (inside diameter `d`, wall `t`, length `l`): the smallest useful vessel.
    pub fn cylinder(id: &str, d: f64, t: f64, l: f64) -> VesselDoc {
        let mut doc = VesselDoc::new(None);
        doc.elements.push(Element {
            id: id.into(),
            kind: ElementKind::Cylinder,
            id_or_od_mm: d,
            thickness_mm: t,
            length_mm: l,
            straight_flange_mm: 0.0,
            corrosion_allowance_mm: 0.0,
            material: None,
            note: None,
        });
        doc
    }

    pub fn from_json(text: &str) -> Result<VesselDoc> {
        if text.len() > 1 << 20 {
            return Err(VesselError::Json("vessel JSON is larger than 1 MiB".into()));
        }
        let doc: VesselDoc = serde_json::from_str(text)?;
        doc.validate()?;
        Ok(doc)
    }

    pub fn from_value(v: &serde_json::Value) -> Result<VesselDoc> {
        let doc: VesselDoc = serde_json::from_value(v.clone())?;
        doc.validate()?;
        Ok(doc)
    }

    pub fn to_value(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }

    pub fn element(&self, id: &str) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }

    /// Everything a resolver or emitter may assume: the schema, bounded counts, unique ids,
    /// finite lengths, attachments on existing hosts.
    pub fn validate(&self) -> Result<()> {
        if self.schema != SCHEMA {
            return Err(VesselError::Invalid(format!("schema must be `{SCHEMA}` (got `{}`)", self.schema.chars().take(40).collect::<String>())));
        }
        if let Some(n) = &self.name
            && (n.len() > 256 || n.chars().any(char::is_control))
        {
            return Err(VesselError::Invalid("name must be at most 256 characters without control characters".into()));
        }
        if self.elements.len() > MAX_ELEMENTS {
            return Err(VesselError::Invalid(format!("at most {MAX_ELEMENTS} elements")));
        }
        if self.attachments.len() > MAX_ATTACHMENTS {
            return Err(VesselError::Invalid(format!("at most {MAX_ATTACHMENTS} attachments")));
        }
        if self.global.angle_zero_ref.len() > MAX_ID_LEN || self.global.default_material.len() > 256 {
            return Err(VesselError::Invalid("global text fields are too long".into()));
        }
        let mut ids: Vec<&str> = Vec::with_capacity(self.elements.len() + self.attachments.len());
        for e in &self.elements {
            e.validate()?;
            if ids.contains(&e.id.as_str()) {
                return Err(VesselError::Invalid(format!("element id `{}` is used twice", e.id)));
            }
            ids.push(&e.id);
        }
        for a in &self.attachments {
            a.validate()?;
            if ids.contains(&a.id.as_str()) {
                return Err(VesselError::Invalid(format!("id `{}` is used twice", a.id)));
            }
            ids.push(&a.id);
            if self.element(&a.host).is_none() {
                return Err(VesselError::Invalid(format!("attachment `{}` sits on unknown element `{}`", a.id, a.host)));
            }
        }
        Ok(())
    }
}

impl Element {
    pub fn validate(&self) -> Result<()> {
        ident("element", &self.id)?;
        length("id_or_od_mm", self.id_or_od_mm, 0.0)?;
        length("thickness_mm", self.thickness_mm, 0.0)?;
        non_negative_length("length_mm", self.length_mm)?;
        non_negative_length("straight_flange_mm", self.straight_flange_mm)?;
        non_negative_length("corrosion_allowance_mm", self.corrosion_allowance_mm)?;
        if let Some(m) = &self.material
            && m.len() > 256
        {
            return Err(VesselError::Invalid(format!("element `{}`: material name too long", self.id)));
        }
        if let Some(n) = &self.note
            && n.len() > 4096
        {
            return Err(VesselError::Invalid(format!("element `{}`: note too long", self.id)));
        }
        match &self.kind {
            ElementKind::Cylinder | ElementKind::Skirt | ElementKind::FlatCover | ElementKind::BodyFlange => length("length_mm", self.length_mm, 0.0),
            ElementKind::Cone { small_diameter_mm } => {
                length("length_mm", self.length_mm, 0.0)?;
                non_negative_length("small_diameter_mm", *small_diameter_mm)
            }
            ElementKind::Head { preset } => match preset {
                HeadPreset::Torispherical { crown_ratio, knuckle_ratio, .. } => {
                    if !(crown_ratio.is_finite() && *crown_ratio > 0.0 && *crown_ratio <= 10.0)
                        || !(knuckle_ratio.is_finite() && *knuckle_ratio > 0.0 && *knuckle_ratio < 0.5)
                    {
                        return Err(VesselError::Invalid(format!("element `{}`: head ratios out of range", self.id)));
                    }
                    Ok(())
                }
                HeadPreset::Ellipsoidal { ratio } => {
                    if ratio.is_finite() && *ratio >= 1.0 && *ratio <= 10.0 {
                        Ok(())
                    } else {
                        Err(VesselError::Invalid(format!("element `{}`: ellipsoidal ratio must be 1…10", self.id)))
                    }
                }
                HeadPreset::Hemispherical => Ok(()),
                HeadPreset::Toriconical { half_apex_angle_deg, knuckle_ratio } => {
                    if half_apex_angle_deg.is_finite()
                        && *half_apex_angle_deg > 0.0
                        && *half_apex_angle_deg < 90.0
                        && knuckle_ratio.is_finite()
                        && *knuckle_ratio >= 0.0
                    {
                        Ok(())
                    } else {
                        Err(VesselError::Invalid(format!("element `{}`: toriconical angle must be 0…90°", self.id)))
                    }
                }
            },
        }
    }
}

impl Attachment {
    pub fn validate(&self) -> Result<()> {
        ident("attachment", &self.id)?;
        ident("host", &self.host)?;
        if !self.angle_deg.is_finite() || self.angle_deg.abs() > 3600.0 {
            return Err(VesselError::Invalid(format!("attachment `{}`: angle_deg must be finite", self.id)));
        }
        match self.axial {
            AxialRef::FromDatum { z_mm } => {
                if !z_mm.is_finite() || z_mm.abs() > MAX_LENGTH_MM {
                    return Err(VesselError::Invalid(format!("attachment `{}`: z_mm out of range", self.id)));
                }
            }
            AxialRef::FromHostStart { dz_mm } | AxialRef::FromHostEnd { dz_mm } => non_negative_length("dz_mm", dz_mm)?,
            AxialRef::Fraction { t } => {
                if !(t.is_finite() && (0.0..=1.0).contains(&t)) {
                    return Err(VesselError::Invalid(format!("attachment `{}`: fraction t must be 0…1", self.id)));
                }
            }
        }
        match self.frame {
            Frame::Hillside { offset_mm } => non_negative_length("offset_mm", offset_mm)?,
            Frame::HeadRadialAtR { r_mm } => non_negative_length("r_mm", r_mm)?,
            Frame::Tilted { deg } => {
                if !(deg.is_finite() && deg.abs() < 90.0) {
                    return Err(VesselError::Invalid(format!("attachment `{}`: tilt must be within ±90°", self.id)));
                }
            }
            Frame::Radial | Frame::Tangential | Frame::ParallelToAxis => {}
        }
        if let Some(k) = &self.size_key
            && k.len() > MAX_ID_LEN
        {
            return Err(VesselError::Invalid(format!("attachment `{}`: size_key too long", self.id)));
        }
        match &self.kind {
            AttachmentKind::Nozzle { nps, class, projection_mm, pad, .. } => {
                if nps.len() > 16 || class.len() > 16 {
                    return Err(VesselError::Invalid(format!("attachment `{}`: nps/class too long", self.id)));
                }
                non_negative_length("projection_mm", *projection_mm)?;
                if let Some(p) = pad {
                    length("pad.od_mm", p.od_mm, 0.0)?;
                    length("pad.thickness_mm", p.thickness_mm, 0.0)?;
                }
                Ok(())
            }
            AttachmentKind::Saddle { width_mm, contact_angle_deg, height_mm } => {
                length("width_mm", *width_mm, 0.0)?;
                length("height_mm", *height_mm, 0.0)?;
                if contact_angle_deg.is_finite() && *contact_angle_deg > 0.0 && *contact_angle_deg <= 180.0 {
                    Ok(())
                } else {
                    Err(VesselError::Invalid(format!("attachment `{}`: contact angle must be 0…180°", self.id)))
                }
            }
            AttachmentKind::Leg { count, section, length_mm } => {
                if *count == 0 || *count > 64 || section.len() > 64 {
                    return Err(VesselError::Invalid(format!("attachment `{}`: 1…64 legs with a short section name", self.id)));
                }
                length("length_mm", *length_mm, 0.0)
            }
            AttachmentKind::Bracket { width_mm, height_mm } => {
                length("width_mm", *width_mm, 0.0)?;
                length("height_mm", *height_mm, 0.0)
            }
            AttachmentKind::AnchorChair { count, bolt } => {
                if *count == 0 || *count > 256 || bolt.len() > 32 {
                    return Err(VesselError::Invalid(format!("attachment `{}`: 1…256 anchor chairs with a short bolt name", self.id)));
                }
                Ok(())
            }
            AttachmentKind::StiffeningRing { width_mm, thickness_mm } => {
                length("width_mm", *width_mm, 0.0)?;
                length("thickness_mm", *thickness_mm, 0.0)
            }
            AttachmentKind::LimpetCoil { pipe_nps, pitch_mm, turns } => {
                if pipe_nps.len() > 16 || !(turns.is_finite() && *turns > 0.0 && *turns <= 10_000.0) {
                    return Err(VesselError::Invalid(format!("attachment `{}`: turns must be 0…10000", self.id)));
                }
                length("pitch_mm", *pitch_mm, 0.0)
            }
            AttachmentKind::LiftingLug { thickness_mm, hole_mm } => {
                length("thickness_mm", *thickness_mm, 0.0)?;
                length("hole_mm", *hole_mm, 0.0)
            }
        }
    }
}
