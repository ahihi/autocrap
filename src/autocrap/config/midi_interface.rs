use serde::{Serialize, Deserialize};

use super::MidiPort;

/// The MIDI ports autocrap translates the device onto.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MidiInterface {
    pub client_name: String,
    pub out_port: MidiPort,
    pub in_port: MidiPort
}
