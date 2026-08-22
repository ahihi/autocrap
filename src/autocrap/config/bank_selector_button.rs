use serde::{Serialize, Deserialize};

/// One of the two physical buttons that cycle banks.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct BankSelectorButton {
    /// Control number the device reports when this button is pressed.
    pub input_control_number: u8,
    /// Control number to write to drive this button's indicator LED.
    pub led_control_number: u8,
}
