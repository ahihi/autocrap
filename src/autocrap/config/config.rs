use serde::{Serialize, Deserialize};

use super::{AbstractMapping, Interface};

/// Everything autocrap needs to talk to one device: the whole config file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub vendor_id: u16,
    pub product_id: u16,
    pub in_endpoint: u8,
    pub out_endpoint: u8,
    pub interface: Interface,
    pub mappings: Vec<AbstractMapping>
}
