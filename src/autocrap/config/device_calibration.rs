use std::collections::HashMap;

use serde::{Serialize, Deserialize};

use super::{Mapping, ReversalFilter};

/// How one physical control differs from the ideal the config assumes.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct ControlCalibration {
    /// Multiplier applied to each raw encoder tick before accumulating, compensating
    /// for how many detents the hardware reports per rotation.
    #[serde(default)]
    pub step: Option<u8>,
    /// Rejects the false reversals a worn encoder reports.
    #[serde(default)]
    pub reversal_filter: Option<ReversalFilter>,
}

/// Corrections for a particular physical device, applied whichever config is loaded.
///
/// How a knob physically behaves is a property of the hardware rather than of what
/// you have mapped it to, so its correction has to survive switching between configs
/// instead of being copied into each one and drifting out of step.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeviceCalibration {
    /// Keyed by `ctrl_in_num`, since that identifies the physical control whatever
    /// a given config happens to call it.
    #[serde(default)]
    pub controls: HashMap<u8, ControlCalibration>,
}

impl DeviceCalibration {
    /// Returns the mapping with any correction for its physical control applied.
    ///
    /// A calibration entry wins over one written into the mapping: the config
    /// describes intent, the calibration describes the hardware in front of you.
    pub fn applied_to(&self, mapping: Mapping) -> Mapping {
        let Some(ctrl_in_num) = mapping.ctrl_in_num else {
            return mapping;
        };

        let Some(calibration) = self.controls.get(&ctrl_in_num) else {
            return mapping;
        };

        Mapping {
            step: calibration.step.or(mapping.step),
            reversal_filter: calibration.reversal_filter.or(mapping.reversal_filter),
            ..mapping
        }
    }
}
