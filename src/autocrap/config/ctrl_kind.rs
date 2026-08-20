use serde::{Serialize, Deserialize};

use super::{OnOffMode, RelativeMode};

/// Which family of physical control a mapping describes, and how it behaves.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum CtrlKind {
    OnOff { mode: OnOffMode },
    /// A slider reported as a high and a low control number in sequence.
    EightBit,
    Relative { mode: RelativeMode },
}
