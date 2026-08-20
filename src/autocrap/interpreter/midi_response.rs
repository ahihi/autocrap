/// A MIDI message to send to the receiving application.
#[derive(Debug)]
pub struct MidiResponse {
    pub data: Vec<u8>
}
