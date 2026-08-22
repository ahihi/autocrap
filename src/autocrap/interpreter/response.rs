use super::{CtrlResponse, MidiResponse, OscResponse};

/// Everything one handled event should emit, across all three directions.
#[derive(Debug)]
pub struct Response {
    pub ctrl: Option<CtrlResponse>,
    pub osc: Option<OscResponse>,
    pub midi: Option<MidiResponse>
}

impl Response {
    /// Returns a response that emits nothing, meaning "handled, with nothing to send".
    pub fn new() -> Response {
        Response {
            ctrl: None,
            osc: None,
            midi: None
        }
    }
}

impl Into<Response> for CtrlResponse {
    fn into(self) -> Response {
        Response {
            ctrl: Some(self),
            osc: None,
            midi: None
        }
    }
}

impl Into<Response> for OscResponse {
    fn into(self) -> Response {
        Response {
            ctrl: None,
            osc: Some(self),
            midi: None
        }
    }
}

impl Into<Response> for MidiResponse {
    fn into(self) -> Response {
        Response {
            ctrl: None,
            osc: None,
            midi: Some(self)
        }
    }
}
