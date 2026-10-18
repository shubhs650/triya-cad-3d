// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The vessel document: what the user describes, as JSON (`schema: "triya.vessel/0.1"`),
//! field for field as `triya/docs/vessel-schema.md` lays it out.
//!
//! Lengths are millimetres, angles degrees, masses kilograms (the document is for people; the
//! engine's radians are the emitter's business). Every numeric field is hostile until
//! [`VesselDoc::validate`] has passed it: finite, within caps, counts bounded.
//!
//! Elements and attachments are flat JSON objects whose `kind` names the variant
//! (`{"id": "E2", "kind": "cylinder", "length_mm": 3000, "thickness_mm": 12}`); in Rust the
//! kind-specific fields live in [`ElementKind`] / [`AttachmentKind`], flattened into the
//! common struct. Unknown top-level fields are kept as they were ([`VesselDoc::unknown`]), and
//! `extra` is free-form, so a newer document survives a round trip through this version.
//!
//! The JSON schema (`vessel-schema.json` next to `Cargo.toml`) is generated from these types
//! with `schemars`; the test `schema_file_is_current` keeps it in step (`TRIYA_BLESS=1`
//! rewrites it).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use triya_stdlib::{FlangeKind, HeadPreset, RatioBasis, StandardRef};

use crate::{Result, VesselError};

/// The schema tag every document carries.
pub const SCHEMA: &str = "triya.vessel/0.1";
/// Most shell elements one vessel may have.
pub const MAX_ELEMENTS: usize = 200;
/// Most attachments one vessel may have.
pub const MAX_ATTACHMENTS: usize = 500;
/// Longest length (mm) any field may hold: a kilometre-long vessel is a typo.
pub const MAX_LENGTH_MM: f64 = 1.0e6;
/// Longest id, size key or short code.
pub const MAX_ID_LEN: usize = 64;
/// Longest free text (a note, a description, a service).
pub const MAX_TEXT_LEN: usize = 4096;
/// Most notes and revisions.
pub const MAX_NOTES: usize = 200;
/// Largest count in a pattern (legs, ribs, bolts, segments).
pub const MAX_COUNT: u32 = 1000;
/// Largest serialised size of `extra` and of the unknown top-level fields together.
pub const MAX_EXTRA_BYTES: usize = 16 * 1024;

fn default_vessel_id() -> String {
    "V-1".into()
}

/// A whole vessel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VesselDoc {
    /// Always `triya.vessel/0.1`.
    pub schema: String,
    /// The vessel tag (`V-101`); generated parameter names start with it.
    #[serde(default = "default_vessel_id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub global: Global,
    /// The axisymmetric stack, bottom → top (vertical) or left → right (horizontal).
    #[serde(default)]
    pub elements: Vec<Element>,
    /// Nozzles, supports and details placed on elements.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    /// Drawing and BOM data only (no code calculations in the beta).
    #[serde(default, skip_serializing_if = "DesignData::is_empty")]
    pub design_data: DesignData,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revisions: Vec<Revision>,
    /// Free-form, preserved.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, Value>,
    /// Top-level fields this version does not know, kept as they were.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

// ---------------------------------------------------------------------------------------------
// §2 global

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    /// Axis +Z, stack bottom → top.
    #[default]
    Vertical,
    /// Axis +X, stack left → right.
    Horizontal,
}

/// The 0.0 of every `axial` reference and of the drawing's dimensions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Datum {
    /// The bottom head's tangent line.
    #[default]
    BottomTl,
    /// The bottom head-to-shell seam (the end of its straight flange).
    BottomSeam,
    /// The underside of the skirt's base ring.
    SkirtBase,
    /// Plant grade: the bottom tangent line sits at `plant_elevation_mm`.
    PlantElevation,
    /// A horizontal vessel's left tangent line.
    LeftTl,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiameterBasis {
    #[default]
    Inside,
    Outside,
}

/// What 0° means on the plan view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AngleZero {
    #[default]
    PlantNorth,
    PlusX,
    PlusY,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AngleSense {
    #[default]
    ClockwiseFromAbove,
    CounterclockwiseFromAbove,
}

fn default_material() -> String {
    "SA-516 Gr.70".into()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Global {
    #[serde(default)]
    pub orientation: Orientation,
    #[serde(default)]
    pub datum: Datum,
    /// Elevation of the datum above plant grade (drawing only).
    #[serde(default)]
    pub plant_elevation_mm: f64,
    /// How `diameter_mm` on elements is read.
    #[serde(default)]
    pub diameter_basis: DiameterBasis,
    #[serde(default)]
    pub angle_zero: AngleZero,
    #[serde(default)]
    pub angle_sense: AngleSense,
    /// BOM default.
    #[serde(default = "default_material")]
    pub default_material: String,
    /// Drawing note only in the beta.
    #[serde(default)]
    pub corrosion_allowance_mm: f64,
    /// Nozzle projection reference when `projection.from = insulation`.
    #[serde(default)]
    pub insulation_thickness_mm: f64,
}

impl Default for Global {
    fn default() -> Self {
        Global {
            orientation: Orientation::default(),
            datum: Datum::default(),
            plant_elevation_mm: 0.0,
            diameter_basis: DiameterBasis::default(),
            angle_zero: AngleZero::default(),
            angle_sense: AngleSense::default(),
            default_material: default_material(),
            corrosion_allowance_mm: 0.0,
            insulation_thickness_mm: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// §3 elements

/// One element of the axisymmetric stack.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Element {
    /// `E1`, `E2`, … (unique across elements and attachments).
    pub id: String,
    #[serde(flatten)]
    pub kind: ElementKind,
    /// Wall (or plate) thickness.
    pub thickness_mm: f64,
    /// On the vessel's `diameter_basis`; inherited from the previous element when omitted
    /// (inheritance stops at a cone, which states both ends).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diameter_mm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ElementKind {
    /// Cylinder plus base annulus; its top meets the bottom head `top_offset_mm` below the TL.
    Skirt {
        height_mm: f64,
        base_ring: BaseRing,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        top_offset_mm: Option<f64>,
    },
    Cylinder {
        length_mm: f64,
    },
    /// A formed head: straight flange, knuckle, crown.
    Head {
        preset: HeadPresetId,
        /// Default per preset (≥ 3 t).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        straight_flange_mm: Option<f64>,
        /// Inside crown radius override (required by `custom_torispherical`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crown_radius_mm: Option<f64>,
        /// Inside knuckle radius override (required by `custom_torispherical`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        knuckle_radius_mm: Option<f64>,
        /// Auto from the head's position in the stack when omitted.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        orientation: Option<HeadOrientation>,
    },
    /// A concentric cone; toriconical when a knuckle is > 0.
    Cone {
        /// Axial length.
        length_mm: f64,
        diameter_small_mm: f64,
        diameter_large_mm: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        knuckle_large_mm: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        knuckle_small_mm: Option<f64>,
    },
    /// A welded or bolted flat plate (its thickness is the element's `thickness_mm`).
    FlatCover {
        #[serde(default)]
        bolted: bool,
    },
    /// A weld-neck body flange, from a table row or explicit dimensions.
    BodyFlange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        flange_ref: Option<FlangeRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        explicit: Option<FlangeDims>,
        #[serde(default)]
        gasket_gap_mm: f64,
    },
}

impl ElementKind {
    /// The `kind` string as the JSON writes it.
    pub fn name(&self) -> &'static str {
        match self {
            ElementKind::Skirt { .. } => "skirt",
            ElementKind::Cylinder { .. } => "cylinder",
            ElementKind::Head { .. } => "head",
            ElementKind::Cone { .. } => "cone",
            ElementKind::FlatCover { .. } => "flat_cover",
            ElementKind::BodyFlange { .. } => "body_flange",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BaseRing {
    pub od_mm: f64,
    pub id_mm: f64,
    pub thickness_mm: f64,
}

/// Head presets (`vessel-schema.md` §3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HeadPresetId {
    /// Torispherical, L = D, r = 0.06 D (min 3 t).
    AsmeFd,
    /// True semi-ellipse, a = D/2, b = D/4.
    #[serde(rename = "ellipsoidal_2_1")]
    Ellipsoidal21,
    /// Torispherical L = 0.9 D, r = 0.17 D (ASME UG-32 approximation of the 2:1 head).
    #[serde(rename = "ellipsoidal_2_1_ug32_approx")]
    Ellipsoidal21Ug32Approx,
    /// DIN 28011: L = Da, r = 0.1 Da.
    KloepperDin28011,
    /// DIN 28013: L = 0.8 Da, r = 0.154 Da.
    KorbbogenDin28013,
    /// L = 0.8 D, r = 0.1 D.
    #[serde(rename = "torispherical_80_10")]
    Torispherical8010,
    /// Sphere R = D/2.
    Hemispherical,
    /// Explicit `crown_radius_mm` and `knuckle_radius_mm`.
    CustomTorispherical,
}

impl HeadPresetId {
    pub const ALL: [HeadPresetId; 8] = [
        HeadPresetId::AsmeFd,
        HeadPresetId::Ellipsoidal21,
        HeadPresetId::Ellipsoidal21Ug32Approx,
        HeadPresetId::KloepperDin28011,
        HeadPresetId::KorbbogenDin28013,
        HeadPresetId::Torispherical8010,
        HeadPresetId::Hemispherical,
        HeadPresetId::CustomTorispherical,
    ];

    /// The preset's geometry before overrides; `custom_torispherical` has none of its own.
    pub fn geometry(self) -> Option<HeadPreset> {
        let tori = |crown_ratio, knuckle_ratio, basis| Some(HeadPreset::Torispherical { crown_ratio, knuckle_ratio, basis });
        match self {
            HeadPresetId::AsmeFd => Some(HeadPreset::ASME_FD),
            HeadPresetId::Ellipsoidal21 => Some(HeadPreset::ELLIPSOIDAL_2_1),
            HeadPresetId::Ellipsoidal21Ug32Approx => tori(0.9, 0.17, RatioBasis::Inside),
            HeadPresetId::KloepperDin28011 => Some(HeadPreset::KLOPPER),
            HeadPresetId::KorbbogenDin28013 => Some(HeadPreset::KORBBOGEN),
            HeadPresetId::Torispherical8010 => tori(0.8, 0.1, RatioBasis::Inside),
            HeadPresetId::Hemispherical => Some(HeadPreset::Hemispherical),
            HeadPresetId::CustomTorispherical => None,
        }
    }

    /// The straight flange when the document gives none: DIN 28011 asks h1 ≥ 3.5 s, DIN 28013
    /// and common ASME practice h1 ≥ 3 t.
    pub fn default_straight_flange(self, t: f64) -> f64 {
        match self {
            HeadPresetId::KloepperDin28011 => 3.5 * t,
            _ => 3.0 * t,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HeadOrientation {
    /// The crown points toward +axis (a top or right head).
    Up,
    /// The crown points toward −axis (a bottom or left head).
    Down,
}

/// A pressure rating as tables write it: `150` or `"PN16"`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Rating {
    Class(u32),
    Named(String),
}

/// A flange looked up in a table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FlangeRef {
    /// `B16.5`, `B16.47A`, `EN 1092-1`, …
    pub standard: String,
    /// `"24"` or `"DN600"`.
    pub nps_or_dn: String,
    pub class: Rating,
    #[serde(rename = "type")]
    pub kind: FlangeKind,
}

/// A flange given by its dimensions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FlangeDims {
    pub od_mm: f64,
    pub bolt_circle_mm: f64,
    pub bolt_count: u32,
    pub bolt_hole_mm: f64,
    pub thickness_mm: f64,
    pub hub_od_mm: f64,
    pub hub_len_mm: f64,
}

// ---------------------------------------------------------------------------------------------
// §4 attachments

/// One nozzle, support or detail.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Attachment {
    /// `N1`, `M1`, `L1`, … (unique across elements and attachments).
    pub id: String,
    #[serde(flatten)]
    pub kind: AttachmentKind,
    /// The element it sits on (`Element::id`).
    pub host: String,
    pub axial: Axial,
    /// Azimuth per `global.angle_zero` / `angle_sense`.
    #[serde(default)]
    pub angle_deg: f64,
    #[serde(default)]
    pub frame: Frame,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standard_ref: Option<StandardRef>,
    /// Keys into a `TableSet` row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Set by the generator.
    #[serde(default, skip_serializing_if = "Flags::is_clear")]
    pub flags: Flags,
}

/// Where along the axis an attachment sits.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Axial {
    #[serde(rename = "ref")]
    pub reference: AxialRef,
    pub mm: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AxialRef {
    /// From the datum: the elevation stays put when elements are re-stacked.
    Datum,
    /// From the host element's start node: it moves with the host.
    HostFrom,
}

/// How an attachment's axis sits relative to the vessel (`vessel-schema.md` §4.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Frame {
    /// Through the vessel axis, perpendicular to it.
    #[default]
    Radial,
    /// Parallel to a radial, offset sideways.
    Hillside { offset_mm: f64 },
    /// Offset so the neck OD is flush with the shell (default offset R − neck r).
    Tangential {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        offset_mm: Option<f64>,
    },
    /// A radial rotated about the circumferential tangent.
    Tilted { tilt_deg: f64 },
    /// On a head: parallel to the vessel axis at `radius_mm`.
    ParallelToAxis { radius_mm: f64 },
    /// On a head: normal to the head surface at `radius_mm`.
    HeadRadialAtR { radius_mm: f64 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Flags {
    /// The nozzle's cut/join failed in the kernel; it is kept as a separate body.
    #[serde(default)]
    pub boolean_failed: bool,
    /// A dimension came from a table row not marked verified.
    #[serde(default)]
    pub unverified_table: bool,
}

impl Flags {
    pub fn is_clear(&self) -> bool {
        *self == Flags::default()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttachmentKind {
    Nozzle {
        /// `"4"`, `"1/2"`, `"24"`.
        nps: String,
        /// `"STD"`, `"XS"`, `"80"`; or give `neck_thk_mm`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schedule: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        neck_thk_mm: Option<f64>,
        #[serde(default)]
        neck_type: NeckType,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        flange: Option<NozzleFlange>,
        projection: Projection,
        #[serde(default)]
        inside_projection_mm: f64,
        #[serde(default)]
        attach: NozzleAttach,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pad: Option<Pad>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        manway: Option<Manway>,
        /// Text for the nozzle schedule.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        service: Option<String>,
    },
    /// A horizontal vessel's saddle; the host is a cylinder.
    Saddle {
        width_mm: f64,
        /// Base to the vessel's bottom.
        height_mm: f64,
        #[serde(default = "default_contact_angle")]
        contact_angle_deg: f64,
        web_thk_mm: f64,
        rib_count: u32,
        rib_thk_mm: f64,
        base_plate: Plate,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wear_plate: Option<WearPlate>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anchor_bolts: Option<AnchorBolts>,
    },
    /// A pattern of legs.
    Leg {
        count: u32,
        section: LegSection,
        length_mm: f64,
        #[serde(default)]
        attach: LegAttach,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_plate: Option<Plate>,
        #[serde(default)]
        start_angle_deg: f64,
    },
    /// A pattern of bracket (lug) supports.
    Bracket {
        count: u32,
        gusset: Gusset,
        top_plate: Plate,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bottom_plate: Option<Plate>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bolt_holes: Option<BoltHoles>,
    },
    /// Anchor-bolt chairs around a skirt.
    AnchorChair {
        count: u32,
        height_mm: f64,
        width_mm: f64,
        gusset_thk_mm: f64,
        top_plate_thk_mm: f64,
        bolt_dia_mm: f64,
    },
    StiffeningRing {
        section: RingSection,
        #[serde(default)]
        inside: bool,
    },
    /// A half-pipe coil wound on the shell; give `turns` or `end_axial_mm`.
    LimpetCoil {
        pipe_nps: String,
        schedule: String,
        pitch_mm: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turns: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end_axial_mm: Option<f64>,
        #[serde(default)]
        start_angle_deg: f64,
        #[serde(default)]
        hand: Hand,
        #[serde(default = "yes")]
        half_pipe: bool,
    },
    LiftingLug {
        style: LugStyle,
        plate: LugPlate,
        hole_dia_mm: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pad: Option<Pad>,
    },
    InsulationRing {
        width_mm: f64,
        thk_mm: f64,
        segments: u32,
    },
    Clip {
        plate: Plate,
        count: u32,
        purpose: ClipPurpose,
    },
}

impl AttachmentKind {
    /// The `kind` string as the JSON writes it.
    pub fn name(&self) -> &'static str {
        match self {
            AttachmentKind::Nozzle { .. } => "nozzle",
            AttachmentKind::Saddle { .. } => "saddle",
            AttachmentKind::Leg { .. } => "leg",
            AttachmentKind::Bracket { .. } => "bracket",
            AttachmentKind::AnchorChair { .. } => "anchor_chair",
            AttachmentKind::StiffeningRing { .. } => "stiffening_ring",
            AttachmentKind::LimpetCoil { .. } => "limpet_coil",
            AttachmentKind::LiftingLug { .. } => "lifting_lug",
            AttachmentKind::InsulationRing { .. } => "insulation_ring",
            AttachmentKind::Clip { .. } => "clip",
        }
    }
}

fn default_contact_angle() -> f64 {
    120.0
}

fn yes() -> bool {
    true
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NeckType {
    #[default]
    Pipe,
    Lwn,
    Forged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum FlangeStandard {
    #[serde(rename = "B16.5")]
    B16_5,
    #[serde(rename = "B16.47A")]
    B16_47A,
    #[serde(rename = "B16.47B")]
    B16_47B,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Facing {
    #[default]
    Rf,
    Ff,
    Rtj,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NozzleFlange {
    pub standard: FlangeStandard,
    /// 150, 300, 600, 900, 1500, 2500.
    pub class: u32,
    #[serde(rename = "type")]
    pub kind: FlangeKind,
    #[serde(default)]
    pub facing: Facing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionFrom {
    /// The vessel centreline.
    Cl,
    /// The shell's outside surface.
    Od,
    /// The insulation's outside surface (`global.insulation_thickness_mm`).
    Insulation,
}

/// Where the flange face is: `mm` from `from`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Projection {
    pub from: ProjectionFrom,
    pub mm: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NozzleAttach {
    #[default]
    SetIn,
    SetOn,
}

/// A reinforcing pad around a nozzle or under a lug.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Pad {
    pub width_mm: f64,
    pub thickness_mm: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Manway {
    #[serde(default)]
    pub davit: bool,
}

/// A rectangular plate: length, width, thickness.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Plate {
    pub len_mm: f64,
    pub wid_mm: f64,
    pub thk_mm: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WearPlate {
    pub thk_mm: f64,
    pub width_mm: f64,
    pub angle_deg: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AnchorBolts {
    pub count: u32,
    pub dia_mm: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LegSection {
    Pipe {
        nps: String,
        schedule: String,
    },
    /// Angle designation as written (`"ISA 75x75x8"`).
    Angle {
        size: String,
    },
    Channel {
        size: String,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LegAttach {
    Head,
    #[default]
    Shell,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Gusset {
    pub height_mm: f64,
    pub width_mm: f64,
    pub thk_mm: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BoltHoles {
    pub count: u32,
    pub dia_mm: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RingSection {
    Bar { height_mm: f64, thk_mm: f64 },
    Angle { size: String },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Hand {
    #[default]
    Rh,
    Lh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LugStyle {
    Longitudinal,
    Circumferential,
    Trunnion,
    Tailing,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LugPlate {
    pub len_mm: f64,
    pub height_mm: f64,
    pub thk_mm: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClipPurpose {
    Ladder,
    Platform,
    PipeSupport,
    Earthing,
    Nameplate,
}

// ---------------------------------------------------------------------------------------------
// §5 design data, notes, revisions

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Pressure {
    pub kpa: f64,
    pub at_c: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Nameplate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
}

/// Drawing and BOM data only: Triya does no code calculations in the beta.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DesignData {
    /// `"ASME VIII-1 2025"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mawp_int: Option<Pressure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mawp_ext: Option<Pressure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design_temp_c: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mdmt_c: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hydrotest_kpa: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pwht: Option<bool>,
    /// `"Spot"`, `"Full"`, `"None"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joint_eff: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seismic_code: Option<String>,
    /// Computed values fill the weights when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_weight_kg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operating_weight_kg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_weight_kg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity_m3: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nameplate: Option<Nameplate>,
}

impl DesignData {
    pub fn is_empty(&self) -> bool {
        *self == DesignData::default()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Revision {
    #[serde(default)]
    pub rev: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub desc: String,
}

// ---------------------------------------------------------------------------------------------
// Validation

fn invalid(at: &str, msg: impl std::fmt::Display) -> VesselError {
    VesselError::Invalid(format!("{at}: {msg}"))
}

/// A length: finite, > 0, ≤ [`MAX_LENGTH_MM`].
fn positive(at: &str, name: &str, x: f64) -> Result<()> {
    if x.is_finite() && x > 0.0 && x <= MAX_LENGTH_MM {
        Ok(())
    } else {
        Err(invalid(at, format!("`{name}` must be a finite length in mm, > 0 and ≤ {MAX_LENGTH_MM} (got {x})")))
    }
}

/// A length that may be zero: finite, 0 … [`MAX_LENGTH_MM`].
fn non_negative(at: &str, name: &str, x: f64) -> Result<()> {
    if x.is_finite() && (0.0..=MAX_LENGTH_MM).contains(&x) {
        Ok(())
    } else {
        Err(invalid(at, format!("`{name}` must be a finite length in mm, 0 … {MAX_LENGTH_MM} (got {x})")))
    }
}

fn opt_positive(at: &str, name: &str, x: Option<f64>) -> Result<()> {
    x.map_or(Ok(()), |x| positive(at, name, x))
}

fn opt_non_negative(at: &str, name: &str, x: Option<f64>) -> Result<()> {
    x.map_or(Ok(()), |x| non_negative(at, name, x))
}

/// A finite number within `lo ..= hi` (angles, temperatures, pressures).
fn within(at: &str, name: &str, x: f64, lo: f64, hi: f64) -> Result<()> {
    if x.is_finite() && x >= lo && x <= hi { Ok(()) } else { Err(invalid(at, format!("`{name}` must be {lo} … {hi} (got {x})"))) }
}

fn count(at: &str, name: &str, n: u32) -> Result<()> {
    if (1..=MAX_COUNT).contains(&n) { Ok(()) } else { Err(invalid(at, format!("`{name}` must be 1 … {MAX_COUNT} (got {n})"))) }
}

fn ident(at: &str, name: &str, s: &str) -> Result<()> {
    if s.is_empty() || s.len() > MAX_ID_LEN || s.chars().any(char::is_control) {
        return Err(invalid(at, format!("`{name}` must be 1 … {MAX_ID_LEN} characters without control characters")));
    }
    Ok(())
}

/// Free text: at most `max` bytes. Newlines and tabs are allowed, other control characters not.
fn text(at: &str, name: &str, s: &str, max: usize) -> Result<()> {
    if s.len() > max || s.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(invalid(at, format!("`{name}` must be at most {max} bytes without control characters")));
    }
    Ok(())
}

fn opt_text(at: &str, name: &str, s: &Option<String>, max: usize) -> Result<()> {
    s.as_deref().map_or(Ok(()), |s| text(at, name, s, max))
}

fn plate(at: &str, name: &str, p: &Plate) -> Result<()> {
    positive(at, &format!("{name}.len_mm"), p.len_mm)?;
    positive(at, &format!("{name}.wid_mm"), p.wid_mm)?;
    positive(at, &format!("{name}.thk_mm"), p.thk_mm)
}

fn pad(at: &str, p: &Pad) -> Result<()> {
    positive(at, "pad.width_mm", p.width_mm)?;
    positive(at, "pad.thickness_mm", p.thickness_mm)
}

fn pressure(at: &str, name: &str, p: &Option<Pressure>) -> Result<()> {
    if let Some(p) = p {
        within(at, &format!("{name}.kpa"), p.kpa, -1.0e3, 1.0e6)?;
        within(at, &format!("{name}.at_c"), p.at_c, -273.15, 2000.0)?;
    }
    Ok(())
}

impl VesselDoc {
    /// A new, empty vessel with the default globals.
    pub fn new(title: Option<String>) -> VesselDoc {
        VesselDoc {
            schema: SCHEMA.into(),
            id: default_vessel_id(),
            title,
            global: Global::default(),
            elements: Vec::new(),
            attachments: Vec::new(),
            design_data: DesignData::default(),
            notes: Vec::new(),
            revisions: Vec::new(),
            extra: serde_json::Map::new(),
            unknown: BTreeMap::new(),
        }
    }

    /// A single cylinder (inside diameter `d`, wall `t`, length `l`): the smallest useful vessel.
    pub fn cylinder(id: &str, d: f64, t: f64, l: f64) -> VesselDoc {
        let mut doc = VesselDoc::new(None);
        doc.elements.push(Element {
            id: id.into(),
            kind: ElementKind::Cylinder { length_mm: l },
            thickness_mm: t,
            diameter_mm: Some(d),
            material: None,
            description: None,
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

    pub fn attachment(&self, id: &str) -> Option<&Attachment> {
        self.attachments.iter().find(|a| a.id == id)
    }

    /// The JSON schema of the document (what `vessel-schema.json` holds).
    pub fn json_schema() -> Value {
        serde_json::to_value(schemars::schema_for!(VesselDoc)).unwrap_or(Value::Null)
    }

    /// Everything a resolver or emitter may assume: the schema, bounded counts and texts, unique
    /// ids, finite lengths, attachments on existing hosts.
    pub fn validate(&self) -> Result<()> {
        if self.schema != SCHEMA {
            return Err(VesselError::Invalid(format!("schema must be `{SCHEMA}` (got `{}`)", self.schema.chars().take(40).collect::<String>())));
        }
        ident("vessel", "id", &self.id)?;
        opt_text("vessel", "title", &self.title, 256)?;
        if self.elements.len() > MAX_ELEMENTS {
            return Err(VesselError::Invalid(format!("at most {MAX_ELEMENTS} elements (got {})", self.elements.len())));
        }
        if self.attachments.len() > MAX_ATTACHMENTS {
            return Err(VesselError::Invalid(format!("at most {MAX_ATTACHMENTS} attachments (got {})", self.attachments.len())));
        }
        if self.notes.len() > MAX_NOTES || self.revisions.len() > MAX_NOTES {
            return Err(VesselError::Invalid(format!("at most {MAX_NOTES} notes and {MAX_NOTES} revisions")));
        }
        for n in &self.notes {
            text("notes", "note", n, MAX_TEXT_LEN)?;
        }
        for r in &self.revisions {
            for (name, s) in [("rev", &r.rev), ("date", &r.date), ("by", &r.by)] {
                text("revisions", name, s, MAX_ID_LEN)?;
            }
            text("revisions", "desc", &r.desc, MAX_TEXT_LEN)?;
        }
        let extra_bytes =
            serde_json::to_string(&self.extra).map_or(usize::MAX, |s| s.len()) + serde_json::to_string(&self.unknown).map_or(usize::MAX, |s| s.len());
        if extra_bytes > MAX_EXTRA_BYTES {
            return Err(VesselError::Invalid(format!("`extra` and unknown fields together must be at most {MAX_EXTRA_BYTES} bytes")));
        }
        self.global.validate()?;
        self.design_data.validate()?;
        let mut ids: Vec<&str> = Vec::with_capacity(self.elements.len() + self.attachments.len());
        for e in &self.elements {
            e.validate()?;
            if ids.contains(&e.id.as_str()) {
                return Err(VesselError::Invalid(format!("id `{}` is used twice", e.id)));
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

impl Global {
    pub fn validate(&self) -> Result<()> {
        let at = "global";
        within(at, "plant_elevation_mm", self.plant_elevation_mm, -MAX_LENGTH_MM, MAX_LENGTH_MM)?;
        non_negative(at, "corrosion_allowance_mm", self.corrosion_allowance_mm)?;
        non_negative(at, "insulation_thickness_mm", self.insulation_thickness_mm)?;
        text(at, "default_material", &self.default_material, 256)?;
        let horizontal_datum = self.datum == Datum::LeftTl;
        if horizontal_datum != (self.orientation == Orientation::Horizontal) {
            return Err(invalid(at, "datum `left_tl` is for horizontal vessels, and a horizontal vessel's datum is `left_tl`"));
        }
        Ok(())
    }
}

impl DesignData {
    pub fn validate(&self) -> Result<()> {
        let at = "design_data";
        for (name, s) in [("code", &self.code), ("rt", &self.rt), ("wind_code", &self.wind_code), ("seismic_code", &self.seismic_code)] {
            opt_text(at, name, s, 256)?;
        }
        pressure(at, "mawp_int", &self.mawp_int)?;
        pressure(at, "mawp_ext", &self.mawp_ext)?;
        for (name, x) in [("design_temp_c", self.design_temp_c), ("mdmt_c", self.mdmt_c)] {
            if let Some(x) = x {
                within(at, name, x, -273.15, 2000.0)?;
            }
        }
        if let Some(x) = self.hydrotest_kpa {
            within(at, "hydrotest_kpa", x, 0.0, 1.0e6)?;
        }
        if let Some(x) = self.joint_eff {
            within(at, "joint_eff", x, 0.0, 1.0)?;
        }
        for (name, x) in [
            ("empty_weight_kg", self.empty_weight_kg),
            ("operating_weight_kg", self.operating_weight_kg),
            ("test_weight_kg", self.test_weight_kg),
            ("capacity_m3", self.capacity_m3),
        ] {
            if let Some(x) = x {
                within(at, name, x, 0.0, 1.0e9)?;
            }
        }
        if let Some(n) = &self.nameplate {
            opt_text(at, "nameplate.serial", &n.serial, MAX_ID_LEN)?;
            if let Some(y) = n.year
                && !(1900..=2200).contains(&y)
            {
                return Err(invalid(at, format!("`nameplate.year` must be 1900 … 2200 (got {y})")));
            }
        }
        Ok(())
    }
}

impl Element {
    pub fn validate(&self) -> Result<()> {
        ident("element", "id", &self.id)?;
        let at = format!("element `{}`", self.id);
        let at = at.as_str();
        positive(at, "thickness_mm", self.thickness_mm)?;
        opt_positive(at, "diameter_mm", self.diameter_mm)?;
        opt_text(at, "material", &self.material, 256)?;
        opt_text(at, "description", &self.description, MAX_TEXT_LEN)?;
        match &self.kind {
            ElementKind::Skirt { height_mm, base_ring, top_offset_mm } => {
                positive(at, "height_mm", *height_mm)?;
                positive(at, "base_ring.od_mm", base_ring.od_mm)?;
                non_negative(at, "base_ring.id_mm", base_ring.id_mm)?;
                positive(at, "base_ring.thickness_mm", base_ring.thickness_mm)?;
                if base_ring.id_mm >= base_ring.od_mm {
                    return Err(invalid(at, "the base ring's `id_mm` must be smaller than its `od_mm`"));
                }
                opt_non_negative(at, "top_offset_mm", *top_offset_mm)
            }
            ElementKind::Cylinder { length_mm } => positive(at, "length_mm", *length_mm),
            ElementKind::Head { preset, straight_flange_mm, crown_radius_mm, knuckle_radius_mm, .. } => {
                opt_non_negative(at, "straight_flange_mm", *straight_flange_mm)?;
                opt_positive(at, "crown_radius_mm", *crown_radius_mm)?;
                opt_positive(at, "knuckle_radius_mm", *knuckle_radius_mm)?;
                let torispherical = !matches!(preset, HeadPresetId::Ellipsoidal21 | HeadPresetId::Hemispherical);
                if !torispherical && (crown_radius_mm.is_some() || knuckle_radius_mm.is_some()) {
                    return Err(invalid(at, "crown and knuckle radii apply to torispherical heads only"));
                }
                if *preset == HeadPresetId::CustomTorispherical && (crown_radius_mm.is_none() || knuckle_radius_mm.is_none()) {
                    return Err(invalid(at, "`custom_torispherical` needs `crown_radius_mm` and `knuckle_radius_mm`"));
                }
                Ok(())
            }
            ElementKind::Cone { length_mm, diameter_small_mm, diameter_large_mm, knuckle_large_mm, knuckle_small_mm } => {
                positive(at, "length_mm", *length_mm)?;
                positive(at, "diameter_small_mm", *diameter_small_mm)?;
                positive(at, "diameter_large_mm", *diameter_large_mm)?;
                if diameter_small_mm > diameter_large_mm {
                    return Err(invalid(at, "`diameter_small_mm` must not exceed `diameter_large_mm`"));
                }
                if self.diameter_mm.is_some() {
                    return Err(invalid(at, "a cone states both ends (`diameter_small_mm`, `diameter_large_mm`), not `diameter_mm`"));
                }
                opt_non_negative(at, "knuckle_large_mm", *knuckle_large_mm)?;
                opt_non_negative(at, "knuckle_small_mm", *knuckle_small_mm)
            }
            ElementKind::FlatCover { .. } => Ok(()),
            ElementKind::BodyFlange { flange_ref, explicit, gasket_gap_mm } => {
                non_negative(at, "gasket_gap_mm", *gasket_gap_mm)?;
                match (flange_ref, explicit) {
                    (Some(_), Some(_)) | (None, None) => Err(invalid(at, "a body flange takes `flange_ref` or `explicit`, exactly one")),
                    (Some(r), None) => {
                        text(at, "flange_ref.standard", &r.standard, MAX_ID_LEN)?;
                        ident(at, "flange_ref.nps_or_dn", &r.nps_or_dn)?;
                        match &r.class {
                            Rating::Class(c) => count(at, "flange_ref.class", *c),
                            Rating::Named(s) => ident(at, "flange_ref.class", s),
                        }
                    }
                    (None, Some(f)) => {
                        positive(at, "explicit.od_mm", f.od_mm)?;
                        positive(at, "explicit.bolt_circle_mm", f.bolt_circle_mm)?;
                        count(at, "explicit.bolt_count", f.bolt_count)?;
                        positive(at, "explicit.bolt_hole_mm", f.bolt_hole_mm)?;
                        positive(at, "explicit.thickness_mm", f.thickness_mm)?;
                        positive(at, "explicit.hub_od_mm", f.hub_od_mm)?;
                        non_negative(at, "explicit.hub_len_mm", f.hub_len_mm)?;
                        if !(f.hub_od_mm < f.bolt_circle_mm - f.bolt_hole_mm && f.bolt_circle_mm + f.bolt_hole_mm < f.od_mm) {
                            return Err(invalid(at, "the bolt holes must lie between the hub and the flange's outside"));
                        }
                        Ok(())
                    }
                }
            }
        }
    }
}

impl Attachment {
    pub fn validate(&self) -> Result<()> {
        ident("attachment", "id", &self.id)?;
        let at = format!("attachment `{}`", self.id);
        let at = at.as_str();
        ident(at, "host", &self.host)?;
        within(at, "angle_deg", self.angle_deg, -3600.0, 3600.0)?;
        within(at, "axial.mm", self.axial.mm, -MAX_LENGTH_MM, MAX_LENGTH_MM)?;
        if let Some(k) = &self.size_key {
            ident(at, "size_key", k)?;
        }
        if let Some(r) = &self.standard_ref {
            for (name, s) in [("standard_ref.org", &r.org), ("standard_ref.number", &r.number), ("standard_ref.rev", &r.rev)] {
                text(at, name, s, MAX_ID_LEN)?;
            }
        }
        opt_text(at, "material", &self.material, 256)?;
        opt_text(at, "description", &self.description, MAX_TEXT_LEN)?;
        match self.frame {
            Frame::Radial => {}
            Frame::Hillside { offset_mm } => non_negative(at, "frame.offset_mm", offset_mm)?,
            Frame::Tangential { offset_mm } => opt_non_negative(at, "frame.offset_mm", offset_mm)?,
            Frame::Tilted { tilt_deg } => within(at, "frame.tilt_deg", tilt_deg, -89.0, 89.0)?,
            Frame::ParallelToAxis { radius_mm } | Frame::HeadRadialAtR { radius_mm } => non_negative(at, "frame.radius_mm", radius_mm)?,
        }
        match &self.kind {
            AttachmentKind::Nozzle { nps, schedule, neck_thk_mm, flange, projection, inside_projection_mm, pad: p, service, .. } => {
                ident(at, "nps", nps)?;
                if let Some(s) = schedule {
                    ident(at, "schedule", s)?;
                }
                opt_positive(at, "neck_thk_mm", *neck_thk_mm)?;
                if let Some(f) = flange {
                    count(at, "flange.class", f.class)?;
                }
                within(at, "projection.mm", projection.mm, -MAX_LENGTH_MM, MAX_LENGTH_MM)?;
                non_negative(at, "inside_projection_mm", *inside_projection_mm)?;
                if let Some(p) = p {
                    pad(at, p)?;
                }
                opt_text(at, "service", service, 256)
            }
            AttachmentKind::Saddle {
                width_mm,
                height_mm,
                contact_angle_deg,
                web_thk_mm,
                rib_count,
                rib_thk_mm,
                base_plate,
                wear_plate,
                anchor_bolts,
            } => {
                positive(at, "width_mm", *width_mm)?;
                positive(at, "height_mm", *height_mm)?;
                within(at, "contact_angle_deg", *contact_angle_deg, 1.0, 180.0)?;
                positive(at, "web_thk_mm", *web_thk_mm)?;
                if *rib_count > MAX_COUNT {
                    return Err(invalid(at, format!("`rib_count` must be 0 … {MAX_COUNT}")));
                }
                positive(at, "rib_thk_mm", *rib_thk_mm)?;
                plate(at, "base_plate", base_plate)?;
                if let Some(w) = wear_plate {
                    positive(at, "wear_plate.thk_mm", w.thk_mm)?;
                    positive(at, "wear_plate.width_mm", w.width_mm)?;
                    within(at, "wear_plate.angle_deg", w.angle_deg, 1.0, 360.0)?;
                }
                if let Some(b) = anchor_bolts {
                    count(at, "anchor_bolts.count", b.count)?;
                    positive(at, "anchor_bolts.dia_mm", b.dia_mm)?;
                }
                Ok(())
            }
            AttachmentKind::Leg { count: n, section, length_mm, base_plate, start_angle_deg, .. } => {
                count(at, "count", *n)?;
                match section {
                    LegSection::Pipe { nps, schedule } => {
                        ident(at, "section.pipe.nps", nps)?;
                        ident(at, "section.pipe.schedule", schedule)?;
                    }
                    LegSection::Angle { size } | LegSection::Channel { size } => ident(at, "section.size", size)?,
                }
                positive(at, "length_mm", *length_mm)?;
                if let Some(b) = base_plate {
                    plate(at, "base_plate", b)?;
                }
                within(at, "start_angle_deg", *start_angle_deg, -3600.0, 3600.0)
            }
            AttachmentKind::Bracket { count: n, gusset, top_plate, bottom_plate, bolt_holes } => {
                count(at, "count", *n)?;
                positive(at, "gusset.height_mm", gusset.height_mm)?;
                positive(at, "gusset.width_mm", gusset.width_mm)?;
                positive(at, "gusset.thk_mm", gusset.thk_mm)?;
                plate(at, "top_plate", top_plate)?;
                if let Some(b) = bottom_plate {
                    plate(at, "bottom_plate", b)?;
                }
                if let Some(h) = bolt_holes {
                    count(at, "bolt_holes.count", h.count)?;
                    positive(at, "bolt_holes.dia_mm", h.dia_mm)?;
                }
                Ok(())
            }
            AttachmentKind::AnchorChair { count: n, height_mm, width_mm, gusset_thk_mm, top_plate_thk_mm, bolt_dia_mm } => {
                count(at, "count", *n)?;
                for (name, x) in [
                    ("height_mm", *height_mm),
                    ("width_mm", *width_mm),
                    ("gusset_thk_mm", *gusset_thk_mm),
                    ("top_plate_thk_mm", *top_plate_thk_mm),
                    ("bolt_dia_mm", *bolt_dia_mm),
                ] {
                    positive(at, name, x)?;
                }
                Ok(())
            }
            AttachmentKind::StiffeningRing { section, .. } => match section {
                RingSection::Bar { height_mm, thk_mm } => {
                    positive(at, "section.bar.height_mm", *height_mm)?;
                    positive(at, "section.bar.thk_mm", *thk_mm)
                }
                RingSection::Angle { size } => ident(at, "section.angle.size", size),
            },
            AttachmentKind::LimpetCoil { pipe_nps, schedule, pitch_mm, turns, end_axial_mm, start_angle_deg, .. } => {
                ident(at, "pipe_nps", pipe_nps)?;
                ident(at, "schedule", schedule)?;
                positive(at, "pitch_mm", *pitch_mm)?;
                match (turns, end_axial_mm) {
                    (Some(t), None) => within(at, "turns", *t, 0.01, 10_000.0)?,
                    (None, Some(z)) => within(at, "end_axial_mm", *z, -MAX_LENGTH_MM, MAX_LENGTH_MM)?,
                    _ => return Err(invalid(at, "a limpet coil takes `turns` or `end_axial_mm`, exactly one")),
                }
                within(at, "start_angle_deg", *start_angle_deg, -3600.0, 3600.0)
            }
            AttachmentKind::LiftingLug { plate: lp, hole_dia_mm, pad: p, .. } => {
                positive(at, "plate.len_mm", lp.len_mm)?;
                positive(at, "plate.height_mm", lp.height_mm)?;
                positive(at, "plate.thk_mm", lp.thk_mm)?;
                positive(at, "hole_dia_mm", *hole_dia_mm)?;
                if let Some(p) = p {
                    pad(at, p)?;
                }
                Ok(())
            }
            AttachmentKind::InsulationRing { width_mm, thk_mm, segments } => {
                positive(at, "width_mm", *width_mm)?;
                positive(at, "thk_mm", *thk_mm)?;
                count(at, "segments", *segments)
            }
            AttachmentKind::Clip { plate: p, count: n, .. } => {
                plate(at, "plate", p)?;
                count(at, "count", *n)
            }
        }
    }
}
