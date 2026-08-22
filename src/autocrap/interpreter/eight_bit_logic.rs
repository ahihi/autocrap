use rosc::{OscMessage, OscType};

use crate::autocrap::config::{CtrlKind, Mapping, MidiKind, MidiSpec};

use super::{CtrlLogic, MidiResponse, OscResponse, Response};

/// A slider whose position arrives as a high byte then a low bit.
#[derive(Debug)]
pub struct EightBitLogic {
    ctrl_in_hi_num: u8,
    ctrl_in_lo_num: u8,
    midi: Option<MidiSpec>,
    osc_addr: String,
    state: [u8;2]
}

impl CtrlLogic for EightBitLogic {
    fn from_mapping(mapping: &Mapping, _bank_count: u8) -> Option<Box<dyn CtrlLogic>> {
        let CtrlKind::EightBit = mapping.ctrl_kind else {
            return None;
        };

        let Some(ref ctrl_in_sequence) = mapping.ctrl_in_sequence else {
            return None;
        };

        Some(Box::new(EightBitLogic {
            ctrl_in_hi_num: ctrl_in_sequence[0],
            ctrl_in_lo_num: ctrl_in_sequence[1],
            midi: mapping.midi,
            osc_addr: format!("/{}", mapping.name),
            state: [0x00,0x00]
        }))
    }

    fn handle_ctrl(&mut self, num: u8, val: u8) -> Option<Response> {
        if num == self.ctrl_in_hi_num {
            self.state[0] = val;
            return Some(Response::new());
        }

        if num == self.ctrl_in_lo_num {
            self.state[1] = val;
            let val8 = self.state[0] << 1 | (if self.state[1] != 0x00 { 1 } else { 0 });
            return Some(Response {
                ctrl: None,
                osc: Some(OscResponse {
                    addr: self.osc_addr.clone(),
                    args: vec![OscType::Float(val8 as f32 / 255.0)]
                }),
                midi: self.midi.map(|midi| {
                    let data = match midi.kind {
                        MidiKind::Cc => {
                            vec![
                                0b10110000 | midi.channel,
                                midi.num,
                                val8 >> 1
                            ]
                        }
                    };
                    MidiResponse {
                        data
                    }
                })
            })
        }

        None
    }

    fn handle_osc(&mut self, _msg: &OscMessage) -> Option<Response> {
        None
    }

    fn handle_midi(&mut self, _msg: &[u8]) -> Option<Response> {
        None
    }
}
