/// Bytes to write back to the device, typically to drive an indicator LED.
#[derive(Debug)]
pub struct CtrlResponse {
    pub data: Vec<u8>
}
