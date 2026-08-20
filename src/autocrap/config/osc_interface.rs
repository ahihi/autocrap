use std::net::SocketAddrV4;

use serde::{Serialize, Deserialize};

/// The UDP sockets autocrap translates the device onto.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OscInterface {
    pub host_addr: SocketAddrV4,
    pub out_addr: SocketAddrV4,
    pub in_addr: SocketAddrV4
}
