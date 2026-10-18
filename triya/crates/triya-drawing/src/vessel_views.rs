// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Drawing views of a vessel (elevation with the shell outline and nozzle marks, nozzle
//! orientation plan, nozzle schedule table, weld map): the fabrication drawing that follows the
//! vessel document. Not built yet: every view returns `NotSupportedYet`, after resolving the
//! vessel so that an invalid document is reported first.

use triya_vessel::model::VesselDoc;

use crate::model::{Sheet, View};
use crate::{DrawingError, Result};

/// The side elevation: shell outline from the resolved stations, tangent lines, nozzle marks.
pub fn elevation(doc: &VesselDoc, _scale: f64) -> Result<View> {
    triya_vessel::eval::resolve(doc)?;
    Err(DrawingError::NotSupportedYet("vessel elevation view".into()))
}

/// The nozzle orientation plan (angles about the axis as seen from above).
pub fn orientation_plan(doc: &VesselDoc, _scale: f64) -> Result<View> {
    triya_vessel::eval::resolve(doc)?;
    Err(DrawingError::NotSupportedYet("nozzle orientation plan".into()))
}

/// A complete general-arrangement sheet of the vessel.
pub fn general_arrangement(doc: &VesselDoc) -> Result<Sheet> {
    triya_vessel::eval::resolve(doc)?;
    Err(DrawingError::NotSupportedYet("vessel general arrangement sheet".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_are_not_supported_yet_and_invalid_vessels_are_reported_first() {
        let doc = VesselDoc::cylinder("S1", 1000.0, 10.0, 2000.0);
        assert!(matches!(elevation(&doc, 0.1), Err(DrawingError::NotSupportedYet(_))));
        assert!(matches!(orientation_plan(&doc, 0.1), Err(DrawingError::NotSupportedYet(_))));
        assert!(matches!(general_arrangement(&doc), Err(DrawingError::NotSupportedYet(_))));
        let empty = VesselDoc::new(None);
        assert!(matches!(elevation(&empty, 0.1), Err(DrawingError::Vessel(_))));
    }
}
