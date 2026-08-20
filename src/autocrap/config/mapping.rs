use serde::{Serialize, Deserialize};

use super::{CtrlKind, MidiSpec};

/// One physical control: where the device reports it, and where it sends.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mapping {
    pub name: String,
    pub ctrl_in_sequence: Option<Vec<u8>>,
    pub ctrl_in_num: Option<u8>,
    pub ctrl_out_num: Option<u8>,
    pub ctrl_kind: CtrlKind,
    pub midi: Option<MidiSpec>,
    /// multiplier applied to each raw encoder tick before accumulating,
    /// for `Relative`/`Accumulate` controls. defaults to 1 (raw device
    /// behaviour) when omitted.
    #[serde(default)]
    pub step: Option<u8>,
}

impl Mapping {
    /// Returns this mapping shifted to the given position within a `Range` expansion.
    pub fn index(&self, i: u8) -> Mapping {
        Mapping {
            name: self.name.replace("{i}", &i.to_string()),
            ctrl_in_sequence: self.ctrl_in_sequence.as_ref().map(|s| s.iter().map(|n| n+i).collect()),
            ctrl_in_num: self.ctrl_in_num.map(|n| n+i),
            ctrl_out_num: self.ctrl_out_num.map(|n| n+i),
            ctrl_kind: self.ctrl_kind,
            midi: self.midi.map(|m| m.index(i)),
            step: self.step,
        }
    }

    /// Returns the OSC address this control sends on, derived from its name.
    pub fn osc_addr(&self) -> String {
        format!("/{}", self.name)
    }
}
