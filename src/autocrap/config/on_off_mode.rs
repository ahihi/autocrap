use serde::{Serialize, Deserialize};

/// How a button or touch sensor turns its physical press into a value.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum OnOffMode {
    /// Follows the press exactly and drives no indicator LED.
    Raw,
    /// On while held, off on release, with the indicator following.
    Momentary,
    /// Flips on each press and ignores the release entirely.
    Toggle
}
