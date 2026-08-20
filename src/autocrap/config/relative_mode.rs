use serde::{Serialize, Deserialize};

/// How an endless encoder turns its per-detent ticks into a value.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum RelativeMode {
    /// Emits each tick as a signed delta, holding no value of its own.
    Raw,
    /// Sums ticks into a 7-bit value clamped to 0 through 127.
    Accumulate
}
