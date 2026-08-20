use serde::{Serialize, Deserialize};

use super::{MidiInterface, OscInterface};

/// Which protocol autocrap speaks to the rest of the world.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Interface {
    Osc(OscInterface),
    Midi(MidiInterface)
}
