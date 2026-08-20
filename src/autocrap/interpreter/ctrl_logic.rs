use rosc::OscMessage;

use crate::autocrap::config::Mapping;

use super::Response;

/// The behaviour of one physical control, in both directions.
pub trait CtrlLogic: core::fmt::Debug + Send + Sync {
    /// Builds this logic from a mapping, or returns `None` if the mapping is a different kind.
    fn from_mapping(mapping: &Mapping) -> Option<Box<dyn CtrlLogic>> where Self: Sized;
    /// Handles the device reporting a control number and value.
    fn handle_ctrl(&mut self, num: u8, val: u8) -> Option<Response>;
    /// Handles an inbound OSC message from the application.
    fn handle_osc(&mut self, msg: &OscMessage) -> Option<Response>;
    /// Handles an inbound MIDI message from the application.
    fn handle_midi(&mut self, msg: &[u8]) -> Option<Response>;
}
