use std::{net::{SocketAddrV4}};

use serde::{Serialize, Deserialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum OnOffMode {
    Raw,
    Momentary,
    Toggle
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum RelativeMode {
    Raw,
    Accumulate
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum CtrlKind {
    OnOff { mode: OnOffMode },
    EightBit,
    Relative { mode: RelativeMode },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum MidiKind {
    Cc,
    // CoarseFine,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Mode {
    Raw,
    Accumulate,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MidiSpec {
    pub channel: u8,
    pub kind: MidiKind,
    pub num: u8,
}

impl MidiSpec {
    pub fn index(&self, i: u8) -> MidiSpec {
        MidiSpec {
            channel: self.channel,
            kind: self.kind,
            num: self.num + i
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mapping {
    pub name: String,
    pub ctrl_in_sequence: Option<Vec<u8>>,
    pub ctrl_in_num: Option<u8>,
    pub ctrl_out_num: Option<u8>,
    pub ctrl_kind: CtrlKind,
    pub midi: Option<MidiSpec>,
}

impl Mapping {
    pub fn index(&self, i: u8) -> Mapping {
        Mapping {
            name: self.name.replace("{i}", &i.to_string()),
            ctrl_in_sequence: self.ctrl_in_sequence.as_ref().map(|s| s.iter().map(|n| n+i).collect()),
            ctrl_in_num: self.ctrl_in_num.map(|n| n+i),
            ctrl_out_num: self.ctrl_out_num.map(|n| n+i),
            ctrl_kind: self.ctrl_kind,
            midi: self.midi.map(|m| m.index(i)),
        }
    }

    pub fn osc_addr(&self) -> String {
        format!("/{}", self.name)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AbstractMapping {
    Single(Mapping),
    Range {
        count: u8,
        mapping: Mapping
    }
}

impl AbstractMapping {
    pub fn expand_iter(&self) -> impl Iterator<Item = Mapping> {
        let mut mappings = vec![];
        match self {
            AbstractMapping::Single(mapping) => mappings.push(mapping.index(0)),
            AbstractMapping::Range { count, mapping } => {
                for i in 0..*count {
                    mappings.push(mapping.index(i));
                }
            }
        };
        mappings.into_iter()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OscInterface {
    pub host_addr: SocketAddrV4,
    pub out_addr: SocketAddrV4,
    pub in_addr: SocketAddrV4
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MidiPort {
    Index(usize),
    Name(String),
    Virtual(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MidiInterface {
    pub client_name: String,
    pub out_port: MidiPort
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Interface {
    Osc(OscInterface),
    Midi(MidiInterface)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub vendor_id: u16,
    pub product_id: u16,
    pub in_endpoint: u8,
    pub out_endpoint: u8,
    pub interface: Interface,
    pub mappings: Vec<AbstractMapping>
}

