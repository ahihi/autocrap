use serde::{Serialize, Deserialize};

/// How to locate the MIDI port to connect to.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MidiPort {
    Index(usize),
    Name(String),
    /// Creates a new port owned by autocrap rather than connecting to one.
    Virtual(String),
}
