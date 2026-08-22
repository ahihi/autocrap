use rosc::OscMessage;

use crate::autocrap::config::Mapping;

use super::{CtrlResponse, Response};

/// The behaviour of one physical control, in both directions.
pub trait CtrlLogic: core::fmt::Debug + Send + Sync {
    /// Builds this logic from a mapping, or returns `None` if the mapping is a different kind.
    fn from_mapping(mapping: &Mapping, bank_count: u8) -> Option<Box<dyn CtrlLogic>> where Self: Sized;
    /// Handles the device reporting a control number and value.
    fn handle_ctrl(&mut self, num: u8, val: u8) -> Option<Response>;
    /// Handles an inbound OSC message from the application.
    fn handle_osc(&mut self, msg: &OscMessage) -> Option<Response>;
    /// Handles an inbound MIDI message from the application.
    fn handle_midi(&mut self, msg: &[u8]) -> Option<Response>;
    /// Re-points this control at a new bank, returning any device writes that implies.
    fn switch_to_bank(&mut self, _new_bank_index: u8) -> Vec<Response> { vec![] }
    /// The write that makes this control's indicator show the value it currently holds.
    ///
    /// Used to repaint a device that has just been reconnected, since unplugging it
    /// loses every LED while the values here survive.
    fn build_current_indicator_write(&self) -> Option<CtrlResponse> { None }
}
