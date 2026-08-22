use serde::{Serialize, Deserialize};

use super::BankSelectorButton;

/// Claims two physical buttons as the device's bank cycle controls.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct BankSelectorMapping {
    pub bank_count: u8,
    pub previous_bank_button: BankSelectorButton,
    pub next_bank_button: BankSelectorButton,
}
