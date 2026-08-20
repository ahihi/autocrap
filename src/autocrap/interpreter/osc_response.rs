use rosc::OscType;

/// An OSC message to send to the receiving application.
#[derive(Debug)]
pub struct OscResponse {
    pub addr: String,
    pub args: Vec<OscType>,
}
