use serde::{Serialize, Deserialize};

use super::MidiKind;

/// The MIDI message a control sends, and that it matches on inbound.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MidiSpec {
    pub channel: u8,
    pub kind: MidiKind,
    pub num: u8,
}

impl MidiSpec {
    /// Returns this spec offset to the given position within a `Range` expansion.
    pub fn index(&self, i: u8) -> MidiSpec {
        MidiSpec {
            channel: self.channel,
            kind: self.kind,
            num: self.num + i
        }
    }
}
