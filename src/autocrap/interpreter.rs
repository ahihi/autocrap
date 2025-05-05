use log::{warn, info, trace};
use rosc::{OscMessage, OscType};

use super::config::{Config, CtrlKind, Mapping, MidiKind, MidiSpec, OnOffMode, RelativeMode};

// Main interpreter struct holding the logic for each control
#[derive(Debug)]
pub struct Interpreter {
    ctrls: Vec<Box<dyn CtrlLogic>>, // A vector of trait objects, each handling a specific control
}

impl Interpreter {
    // Creates a new Interpreter based on the provided configuration
    pub fn new(config: &Config) -> Interpreter {
        // A list of factory functions, one for each CtrlLogic implementation
        let constructors: Vec<Box<dyn Fn(&Mapping) -> Option<Box<dyn CtrlLogic>>>> = vec![
            Box::new(OnOffLogic::from_mapping),
            Box::new(EightBitLogic::from_mapping),
            Box::new(RelativeLogic::from_mapping),
        ];
        let mut ctrls: Vec<Box<dyn CtrlLogic>> = vec![];

        // Iterate through abstract mappings defined in the config
        for abstract_mapping in config.mappings.iter() {
            // Expand each abstract mapping (single or range) into concrete mappings
            for mapping in abstract_mapping.expand_iter() {
                let mut logic_opt: Option<Box<dyn CtrlLogic>> = None;

                // Try each constructor to find one that matches the mapping's CtrlKind
                for make_logic in &constructors {
                    if let Some(logic) = make_logic(&mapping) {
                        logic_opt = Some(logic); // Found a matching logic constructor
                        break;
                    }
                }

                // If a logic handler was created, add it to the list
                if let Some(logic) = logic_opt {
                    info!("adding control mapping: {:?}", logic);
                    ctrls.push(logic);
                } else {
                    // Warn if no logic handler could be found for a mapping
                    warn!("unhandled mapping definition: {:?}", mapping);
                }
            }
        }

        Interpreter { ctrls }
    }

    // Handles incoming control data from the USB device
    pub fn handle_ctrl(&mut self, num: u8, val: u8) -> Option<Response> {
        // Iterate through the control logic handlers
        for ctrl in &mut self.ctrls {
            // If a handler processes the input, return its response
            if let Some(response) = ctrl.handle_ctrl(num, val) {
                return Some(response);
            }
        }
        // No handler processed the input
        None
    }

    // Handles incoming OSC messages
    pub fn handle_osc(&mut self, msg: &OscMessage) -> Option<Response> {
        // Iterate through the control logic handlers
        for ctrl in &mut self.ctrls {
            // If a handler processes the OSC message, return its response
            if let Some(response) = ctrl.handle_osc(msg) {
                return Some(response);
            }
        }
        // No handler processed the message
        None
    }

    // Handles incoming MIDI messages
    pub fn handle_midi(&mut self, msg: &[u8]) -> Option<Response> {
        // Iterate through the control logic handlers
        for ctrl in &mut self.ctrls {
            // If a handler processes the MIDI message, return its response
            if let Some(response) = ctrl.handle_midi(msg) {
                return Some(response);
            }
        }
        // No handler processed the message
        None
    }
}

// Trait defining the interface for control logic handlers
pub trait CtrlLogic: core::fmt::Debug + Send + Sync {
    // Factory method to create a logic handler from a mapping, if applicable
    fn from_mapping(mapping: &Mapping) -> Option<Box<dyn CtrlLogic>> where Self: Sized;
    // Method to handle incoming control data
    fn handle_ctrl(&mut self, num: u8, val: u8) -> Option<Response>;
    // Method to handle incoming OSC messages
    fn handle_osc(&mut self, msg: &OscMessage) -> Option<Response>;
    // Method to handle incoming MIDI messages
    fn handle_midi(&mut self, msg: &[u8]) -> Option<Response>;
}

// Logic handler for On/Off controls (buttons, switches)
#[derive(Debug)]
pub struct OnOffLogic {
    mode: OnOffMode,          // Behavior mode (Raw, Momentary, Toggle)
    ctrl_in_num: Option<u8>,  // Input control number from the device
    ctrl_out_num: Option<u8>, // Output control number (e.g., for LED)
    midi: Option<MidiSpec>,   // MIDI mapping details
    osc_addr: String,         // OSC address for this control
    state: bool,              // Current state (true = On, false = Off)
}

impl OnOffLogic {
    // Updates the state and generates the corresponding response messages
    // `remember`: If true, updates the internal state. If false, only generates messages.
    fn update(&mut self, new_state: bool, remember: bool) -> Response {
        let mut changed = true; // Assume state changed unless proven otherwise

        if remember {
            changed = new_state != self.state; // Check if the state actually changed
            self.state = new_state; // Update the internal state

            // If the state didn't change, return an empty response
            if !changed {
                return Response::new();
            }
        }

        // Generate OSC response
        let osc_resp = Some(OscResponse {
            addr: self.osc_addr.clone(),
            args: vec![OscType::Float(if new_state { 1.0 } else { 0.0 })]
        });

        // Generate Control (USB device feedback) response
        let ctrl_resp = self.ctrl_out_num.map(|num| CtrlResponse {
            data: vec![num, if new_state { 0x7f } else { 0x00 }] // Send max value for On, 0 for Off
        });

        // Generate MIDI response based on MidiKind
        let midi_resp = self.midi.map(|midi| {
            let data = match midi.kind {
                // --- MIDI CC Handling ---
                MidiKind::Cc => {
                    vec![
                        0b10110000 | midi.channel, // CC status byte + channel
                        midi.num,                   // CC number
                        if new_state { 0x7f } else { 0x00 } // CC value (max for On, 0 for Off)
                    ]
                }
                // --- MIDI Note On/Off Handling ---
                MidiKind::NoteOnOff => {
                    if new_state {
                        // Send Note On
                        vec![
                            0b10010000 | midi.channel, // Note On status byte + channel
                            midi.num,                   // Note number
                            0x7f                        // Velocity (max)
                        ]
                    } else {
                        // Send Note Off
                        vec![
                            0b10000000 | midi.channel, // Note Off status byte + channel
                            midi.num,                   // Note number
                            0x00                        // Velocity (0)
                        ]
                    }
                }
            };
            MidiResponse { data }
        });

        // Combine responses
        Response {
            osc: osc_resp,
            ctrl: ctrl_resp,
            midi: midi_resp,
        }
    }
}

impl CtrlLogic for OnOffLogic {
    // Factory method for OnOffLogic
    fn from_mapping(mapping: &Mapping) -> Option<Box<dyn CtrlLogic>> {
        // Check if the mapping's kind is OnOff
        if let CtrlKind::OnOff { mode } = mapping.ctrl_kind {
            Some(Box::new(OnOffLogic {
                mode: mode,
                ctrl_in_num: mapping.ctrl_in_num,
                ctrl_out_num: mapping.ctrl_out_num,
                midi: mapping.midi,
                osc_addr: mapping.osc_addr(),
                state: false // Initial state is Off
            }))
        } else {
            None // Not an OnOff control
        }
    }

    // Handle incoming control data for OnOffLogic
    fn handle_ctrl(&mut self, num: u8, val: u8) -> Option<Response> {
        // Check if this handler is responsible for the received control number
        let Some(ctrl_in_num) = self.ctrl_in_num else { return None; };
        if num != ctrl_in_num { return None; }

        let pressed = val != 0x00; // Determine if the button/switch is pressed/active
        let mut new_state = self.state;
        let mut send_ctrl = true; // Flag to control sending USB feedback
        let mut send_osc_midi = true; // Flag to control sending OSC/MIDI
        let mut remember = true;    // Flag to control updating internal state

        // Determine behavior based on the mode
        match self.mode {
            OnOffMode::Raw => {
                new_state = pressed;
                send_ctrl = false; // Don't send feedback for raw mode
                remember = false;  // Don't remember state for raw mode (stateless)
            },
            OnOffMode::Momentary => {
                new_state = pressed; // State follows the press directly
            },
            OnOffMode::Toggle => {
                if pressed {
                    new_state = !self.state; // Toggle state only on press
                } else {
                    // Do nothing on release for toggle mode
                    send_ctrl = false;
                    send_osc_midi = false;
                }
            }
        }

        // Generate the basic response based on the calculated new state
        let mut response = self.update(new_state, remember);

        // Modify the response based on the flags
        if !send_ctrl { response.ctrl = None; }
        if !send_osc_midi {
            response.osc = None;
            response.midi = None;
        }

        Some(response)
    }

    // Handle incoming OSC messages for OnOffLogic
    fn handle_osc(&mut self, msg: &OscMessage) -> Option<Response> {
        // Only handle OSC if this control has a physical output (e.g., LED)
        let Some(_num) = self.ctrl_out_num else { return None; };
        // Check if the OSC address matches
        if msg.addr != self.osc_addr { return None; }
        // Ensure there's at least one argument
        if msg.args.len() < 1 { return None; }
        // Expect the first argument to be a float
        let OscType::Float(val) = msg.args[0] else { return None; };

        // Update the state based on the float value (non-zero means On)
        // Only generate the control (USB feedback) part of the response
        let mut response = Response::new();
        response.ctrl = self.update(val != 0.0, true).ctrl;
        Some(response)
    }

    // Handle incoming MIDI messages for OnOffLogic
    fn handle_midi(&mut self, msg: &[u8]) -> Option<Response> {
        // Only handle MIDI if this control has a physical output and a MIDI mapping
        let Some(_num) = self.ctrl_out_num else { return None; };
        let Some(midi_spec) = self.midi else { return None; };

        // Basic validation of MIDI message structure (expecting 3 bytes)
        if msg.len() != 3 { return None; }

        let status_byte = msg[0];
        let data1 = msg[1]; // Usually CC number or Note number
        let data2 = msg[2]; // Usually CC value or Velocity

        let channel = status_byte & 0x0F; // Extract channel (lower 4 bits)
        let status = status_byte & 0xF0;  // Extract status (upper 4 bits)

        // Check if the channel matches
        if channel != midi_spec.channel { return None; }

        let mut is_on_message = false;

        // Check message type based on configured MidiKind
        match midi_spec.kind {
            MidiKind::Cc => {
                // Check for CC status byte and matching CC number
                if status != 0b10110000 || data1 != midi_spec.num { return None; }
                // Treat non-zero CC value as "On"
                is_on_message = data2 > 0;
                trace!("Received matching CC: #{} Val: {}", data1, data2);
            }
            MidiKind::NoteOnOff => {
                // Check for Note On or Note Off status byte and matching Note number
                if (status != 0b10010000 && status != 0b10000000) || data1 != midi_spec.num { return None; }
                // Note On with velocity > 0 means "On"
                // Note Off (any velocity) OR Note On with velocity 0 means "Off"
                is_on_message = status == 0b10010000 && data2 > 0;
                trace!("Received matching Note: #{} Vel: {} (Is On: {})", data1, data2, is_on_message);
            }
        }

        // Update the state based on the parsed MIDI message
        // Only generate the control (USB feedback) part of the response
        let mut response = Response::new();
        response.ctrl = self.update(is_on_message, true).ctrl;
        Some(response)
    }
}


// --- EightBitLogic --- (Handles controls sending 8-bit values via two 7-bit messages)
#[derive(Debug)]
pub struct EightBitLogic {
    ctrl_in_hi_num: u8,       // Control number for the high 7 bits
    ctrl_in_lo_num: u8,       // Control number for the low 1 bit
    midi: Option<MidiSpec>,   // MIDI mapping
    osc_addr: String,         // OSC address
    state: [u8;2]             // Internal state holding the two 7-bit parts [hi, lo]
}

impl CtrlLogic for EightBitLogic {
    // Factory method
    fn from_mapping(mapping: &Mapping) -> Option<Box<dyn CtrlLogic>> {
        // Check if it's an EightBit control and has a sequence defined
        let CtrlKind::EightBit = mapping.ctrl_kind else { return None; };
        let Some(ref ctrl_in_sequence) = mapping.ctrl_in_sequence else { return None; };
        // Ensure the sequence has at least two control numbers
        if ctrl_in_sequence.len() < 2 { return None; };

        Some(Box::new(EightBitLogic {
            ctrl_in_hi_num: ctrl_in_sequence[0],
            ctrl_in_lo_num: ctrl_in_sequence[1],
            midi: mapping.midi,
            osc_addr: mapping.osc_addr(),
            state: [0x00, 0x00] // Initial state
        }))
    }

    // Handle incoming control data
    fn handle_ctrl(&mut self, num: u8, val: u8) -> Option<Response> {
        // Update the high bits part of the state
        if num == self.ctrl_in_hi_num {
            self.state[0] = val;
            return Some(Response::new()); // No immediate output, wait for low bits
        }

        // Update the low bit part of the state and generate output
        if num == self.ctrl_in_lo_num {
            self.state[1] = val;
            // Combine the parts: high bits shifted left, low bit added
            let val8 = (self.state[0] << 1) | (if self.state[1] != 0x00 { 1 } else { 0 });

            // Generate OSC response (scaled to 0.0-1.0)
            let osc_resp = Some(OscResponse {
                addr: self.osc_addr.clone(),
                args: vec![OscType::Float(val8 as f32 / 255.0)]
            });

            // Generate MIDI response (only CC supported currently for 8-bit)
            let midi_resp = self.midi.map(|midi| {
                let data = match midi.kind {
                    MidiKind::Cc => {
                        vec![
                            0b10110000 | midi.channel, // CC status + channel
                            midi.num,                   // CC number
                            val8 >> 1                   // Send the upper 7 bits as CC value
                        ]
                    }
                    // NoteOnOff doesn't make sense for an 8-bit absolute value
                    MidiKind::NoteOnOff => {
                        warn!("NoteOnOff MIDI Kind is not supported for EightBit controls.");
                        vec![] // Return empty vec if NoteOnOff is incorrectly configured
                    }
                };
                if data.is_empty() { None } else { Some(MidiResponse { data }) }
            }).flatten(); // Flatten Option<Option<MidiResponse>> to Option<MidiResponse>


            return Some(Response {
                ctrl: None, // No direct control feedback for 8-bit inputs currently
                osc: osc_resp,
                midi: midi_resp,
            })
        }

        None // Control number didn't match either part
    }

    // Handle incoming OSC (Not implemented for EightBitLogic)
    fn handle_osc(&mut self, _msg: &OscMessage) -> Option<Response> {
        warn!("Receiving OSC for EightBit controls is not implemented.");
        None
    }

    // Handle incoming MIDI (Not implemented for EightBitLogic)
    fn handle_midi(&mut self, _msg: &[u8]) -> Option<Response> {
        warn!("Receiving MIDI for EightBit controls is not implemented.");
        None
    }
}

// --- RelativeLogic --- (Handles relative encoders)
#[derive(Debug)]
pub struct RelativeLogic {
    mode: RelativeMode,       // Behavior mode (Raw delta or Accumulated value)
    ctrl_in_num: Option<u8>,  // Input control number
    ctrl_out_num: Option<u8>, // Output control number (for LED ring)
    midi: Option<MidiSpec>,   // MIDI mapping
    osc_addr: String,         // OSC address
    state: u8                 // Current accumulated value (0-127) if mode is Accumulate
}

impl RelativeLogic {
    // Updates the accumulated state and generates corresponding responses
    fn update(&mut self, new_state: u8) -> Response {
        let changed = new_state != self.state;
        // Calculate the value needed for the encoder's LED ring (often steps)
        let new_encoder_led_val = Self::encoder_led_val(new_state);
        let old_encoder_led_val = Self::encoder_led_val(self.state);
        let encoder_led_val_changed = new_encoder_led_val != old_encoder_led_val;

        self.state = new_state; // Update internal state

        // If the value didn't change, return empty response
        if !changed {
            return Response::new();
        }

        // Generate Control (USB feedback) response only if the LED value needs updating
        let ctrl_resp = if encoder_led_val_changed {
            self.ctrl_out_num.map(|num| CtrlResponse {
                data: vec![num, new_encoder_led_val] // Send the calculated LED value
            })
        } else {
            None
        };

        // Generate OSC response (scaled 0.0-1.0)
        let osc_resp = Some(OscResponse {
            addr: self.osc_addr.clone(),
            args: vec![OscType::Float(self.state as f32 / 127.0)]
        });

        // Generate MIDI response (only CC supported for relative/accumulated)
        let midi_resp = self.midi.map(|midi| {
            let data = match midi.kind {
                MidiKind::Cc => {
                    vec![
                        0b10110000 | midi.channel, // CC status + channel
                        midi.num,                   // CC number
                        self.state                  // Send the current 7-bit state
                    ]
                }
                 // NoteOnOff doesn't make sense for a relative/accumulated value
                MidiKind::NoteOnOff => {
                    warn!("NoteOnOff MIDI Kind is not supported for Relative controls.");
                    vec![]
                }
            };
             if data.is_empty() { None } else { Some(MidiResponse { data }) }
        }).flatten();

        Response {
            ctrl: ctrl_resp,
            osc: osc_resp,
            midi: midi_resp,
        }
    }

    // Helper function to calculate the LED value for an encoder based on the 7-bit state
    // This specific logic might be device-dependent (e.g., Behringer X-Touch Mini)
    fn encoder_led_val(val: u8) -> u8 {
        // Example logic: LEDs might light up in segments
        if val < 7 {
            0 // First segment off
        } else {
            // Calculate segment based on value, snap to segment start
            (val.saturating_sub(7) / 11) * 11 + 7
        }
    }
}

impl CtrlLogic for RelativeLogic {
    // Factory method
    fn from_mapping(mapping: &Mapping) -> Option<Box<dyn CtrlLogic>> {
        // Check if it's a Relative control
        if let CtrlKind::Relative { mode } = mapping.ctrl_kind {
            Some(Box::new(RelativeLogic {
                mode: mode,
                ctrl_in_num: mapping.ctrl_in_num,
                ctrl_out_num: mapping.ctrl_out_num,
                midi: mapping.midi,
                osc_addr: mapping.osc_addr(),
                state: 0x00 // Initial state is 0
            }))
        } else {
            None
        }
    }

    // Handle incoming control data
    fn handle_ctrl(&mut self, num: u8, val: u8) -> Option<Response> {
        // Check if this handler is responsible for the control number
        let Some(ctrl_in_num) = self.ctrl_in_num else { return None; };
        if num != ctrl_in_num { return None; }

        // Interpret the 7-bit value as a signed delta
        // Values >= 0x40 represent negative changes
        let delta: i8 = if val < 0x40 { val as i8 } else { (val as i8).wrapping_add(i8::MIN) }; // Or val as i8 - 128

        // Generate response based on the mode
        let response = match self.mode {
            // Raw mode: Send the delta directly via OSC (MIDI/Ctrl not typical for raw delta)
            RelativeMode::Raw => {
                let osc_resp = OscResponse {
                    addr: self.osc_addr.clone(),
                    args: vec![OscType::Float(delta as f32)] // Send raw delta
                };
                // No MIDI or Ctrl response for raw delta usually
                Response { osc: Some(osc_resp), ctrl: None, midi: None }
            },
            // Accumulate mode: Update internal state and send the new absolute value
            RelativeMode::Accumulate => {
                // Calculate new state, clamping between 0 and 127
                let new_state = self.state.saturating_add_signed(delta).min(127);
                self.update(new_state) // Use the update helper function
            }
        };

        Some(response)
    }

    // Handle incoming OSC messages
    fn handle_osc(&mut self, msg: &OscMessage) -> Option<Response> {
        // Only handle if there's a physical output (LED ring)
        let Some(_num) = self.ctrl_out_num else { return None; };
        // Check address and argument count/type
        if msg.addr != self.osc_addr { return None; }
        if msg.args.len() < 1 { return None; }
        let OscType::Float(val) = msg.args[0] else { return None; };

        // Convert the incoming float (0.0-1.0) to a 7-bit value
        let new_state = float_to_7bit(val);

        // Update the state and generate only the control (USB feedback) response
        let mut response = Response::new();
        response.ctrl = self.update(new_state).ctrl;
        Some(response)
    }

    // Handle incoming MIDI messages
    fn handle_midi(&mut self, msg: &[u8]) -> Option<Response> {
        // Only handle if there's a physical output and MIDI mapping
        let Some(_num) = self.ctrl_out_num else { return None; };
        let Some(midi_spec) = self.midi else { return None; };

        // Basic validation
        if msg.len() != 3 { return None; }
        let status_byte = msg[0];
        let data1 = msg[1]; // CC number
        let data2 = msg[2]; // CC value

        // Check channel, status (must be CC), and CC number
        if (status_byte & 0x0F) != midi_spec.channel { return None; } // Channel mismatch
        if (status_byte & 0xF0) != 0b10110000 { return None; } // Not a CC message
        if data1 != midi_spec.num { return None; } // CC number mismatch

        // Update the state with the received 7-bit CC value
        // Only generate the control (USB feedback) response
        let mut response = Response::new();
        response.ctrl = self.update(data2).ctrl; // data2 is the 7-bit CC value
        Some(response)
    }
}


// --- Response Structures ---

// Represents a message to be sent back to the USB controller
#[derive(Debug)]
pub struct CtrlResponse {
    pub data: Vec<u8> // Raw bytes to send
}

// Represents an OSC message to be sent
#[derive(Debug)]
pub struct OscResponse {
    pub addr: String,       // Target OSC address path
    pub args: Vec<OscType>, // OSC arguments
}

// Represents a MIDI message to be sent
#[derive(Debug)]
pub struct MidiResponse {
    pub data: Vec<u8> // Raw MIDI bytes to send
}

// Combined response structure, holding optional parts for each output type
#[derive(Debug)]
pub struct Response {
    pub ctrl: Option<CtrlResponse>,
    pub osc: Option<OscResponse>,
    pub midi: Option<MidiResponse>
}

impl Response {
    // Creates a new, empty response
    pub fn new() -> Response {
        Response { ctrl: None, osc: None, midi: None }
    }
}

// --- Conversion Helpers (Into<Response>) ---
// Allow easily creating a Response containing only one type of message

impl From<CtrlResponse> for Response {
    fn from(ctrl: CtrlResponse) -> Self {
        Response { ctrl: Some(ctrl), osc: None, midi: None }
    }
}

impl From<OscResponse> for Response {
    fn from(osc: OscResponse) -> Self {
        Response { ctrl: None, osc: Some(osc), midi: None }
    }
}

impl From<MidiResponse> for Response {
    fn from(midi: MidiResponse) -> Self {
        Response { ctrl: None, osc: None, midi: Some(midi) }
    }
}

// --- Utility Functions ---

// Converts a float (expected 0.0 to 1.0) to a 7-bit integer (0-127)
fn float_to_7bit(val: f32) -> u8 {
    (val.max(0.0).min(1.0) * 127.0).round() as u8
}
