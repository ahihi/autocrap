use serde::{Serialize, Deserialize};

/// Which kind of MIDI message a mapping sends and listens for.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum MidiKind {
    Cc,
    // CoarseFine,
}
