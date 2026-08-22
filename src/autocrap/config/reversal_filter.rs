use serde::{Serialize, Deserialize};

/// Suppresses the spurious direction reversals a worn encoder reports.
///
/// A worn contact misreads the phase sequence and emits bursts of ticks in the
/// wrong direction. Those bursts are indistinguishable from a real reversal at
/// the moment they arrive, so instead of guessing, opposing ticks are held back
/// until the new direction proves itself or the old one resumes.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ReversalFilter {
    /// Consecutive opposing ticks needed before a mid-turn reversal is believed.
    pub ticks_to_confirm_reversal: u8,
    /// Idle time after which a reversal is taken as deliberate and applied at once,
    /// since a burst of contact noise never follows a pause.
    pub deliberate_reversal_pause_ms: u64,
}
