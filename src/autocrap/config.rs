use std::{net::{SocketAddrV4}};

use serde::{Serialize, Deserialize};

// Enum defining how an On/Off control behaves
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum OnOffMode {
    Raw,        // Sends raw 0x00 or 0x7F value
    Momentary,  // Sends On when pressed, Off when released
    Toggle      // Toggles between On and Off on press
}

// Enum defining how a relative control behaves
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum RelativeMode {
    Raw,        // Sends raw delta value
    Accumulate  // Accumulates delta and sends the current value
}

// Enum defining the kind of hardware control
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum CtrlKind {
    OnOff { mode: OnOffMode }, // An On/Off button/switch
    EightBit,                  // An 8-bit absolute value (e.g., from two 7-bit inputs)
    Relative { mode: RelativeMode }, // A relative encoder
}

// Enum defining the kind of MIDI message to send/receive
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum MidiKind {
    Cc,         // Control Change message
    NoteOnOff,  // Note On / Note Off message
    // CoarseFine, // Potential future addition for 14-bit CC
}

// Enum defining the mode (currently unused, potentially for future expansion)
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Mode {
    Raw,
    Accumulate,
}

// Struct defining the specifics of a MIDI mapping
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MidiSpec {
    pub channel: u8, // MIDI channel (0-15)
    pub kind: MidiKind, // Type of MIDI message
    pub num: u8,     // MIDI CC number or Note number
}

impl MidiSpec {
    // Helper function to create a new MidiSpec with an indexed number
    // Useful for ranges of controls.
    pub fn index(&self, i: u8) -> MidiSpec {
        MidiSpec {
            channel: self.channel,
            kind: self.kind,
            num: self.num + i // Increment the number by the index
        }
    }
}

// Struct defining a single control mapping
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mapping {
    pub name: String, // Name used for OSC address generation
    pub ctrl_in_sequence: Option<Vec<u8>>, // Input control numbers for sequences (e.g., 8-bit)
    pub ctrl_in_num: Option<u8>, // Input control number for single controls
    pub ctrl_out_num: Option<u8>, // Output control number (for LED feedback, etc.)
    pub ctrl_kind: CtrlKind, // The kind of hardware control
    pub midi: Option<MidiSpec>, // Optional MIDI mapping details
}

impl Mapping {
    // Helper function to create an indexed mapping
    // Replaces "{i}" in the name and indexes control/MIDI numbers.
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

    // Generates the OSC address string from the mapping name
    pub fn osc_addr(&self) -> String {
        format!("/{}", self.name)
    }
}

// Enum to define either a single mapping or a range of similar mappings
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AbstractMapping {
    Single(Mapping), // A single, unique mapping
    Range {         // A range of mappings based on a template
        count: u8,      // Number of mappings in the range
        mapping: Mapping // The template mapping (using "{i}" for indexing)
    }
}

impl AbstractMapping {
    // Expands an AbstractMapping into an iterator of concrete Mapping structs
    pub fn expand_iter(&self) -> impl Iterator<Item = Mapping> {
        let mut mappings = vec![];
        match self {
            AbstractMapping::Single(mapping) => mappings.push(mapping.index(0)), // Single just gets index 0
            AbstractMapping::Range { count, mapping } => {
                for i in 0..*count { // Iterate and create indexed mappings
                    mappings.push(mapping.index(i));
                }
            }
        };
        mappings.into_iter() // Return an iterator over the generated mappings
    }
}

// Struct defining OSC interface addresses
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OscInterface {
    pub host_addr: SocketAddrV4, // Address the application binds to listen
    pub out_addr: SocketAddrV4,  // Address to send OSC messages to
    pub in_addr: SocketAddrV4    // Address to receive OSC messages from (often same as host_addr)
}

// Enum defining how to specify a MIDI port
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MidiPort {
    Index(usize),     // Select port by its index number
    Name(String),     // Select port by its exact name
    Virtual(String),  // Create a virtual MIDI port with the given name (Unix only)
}

// Struct defining MIDI interface configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MidiInterface {
    pub client_name: String, // Name for the MIDI client
    pub out_port: MidiPort,  // MIDI output port specification
    pub in_port: MidiPort   // MIDI input port specification
}

// Enum defining the type of external interface (OSC or MIDI)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Interface {
    Osc(OscInterface),
    Midi(MidiInterface)
}

// The main configuration struct, loaded from JSON
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub vendor_id: u16,     // USB Vendor ID of the hardware controller
    pub product_id: u16,    // USB Product ID of the hardware controller
    pub in_endpoint: u8,    // USB Input endpoint address
    pub out_endpoint: u8,   // USB Output endpoint address
    pub interface: Interface, // Network/MIDI interface configuration
    pub mappings: Vec<AbstractMapping> // List of control mappings
}
