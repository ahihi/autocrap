use rosc::{OscMessage, OscType};

use crate::autocrap::config::{CtrlKind, Mapping, MidiKind, MidiSpec, RelativeMode};

use super::{CtrlLogic, CtrlResponse, MidiResponse, OscResponse, Response};

/// An endless encoder, holding its accumulated 7-bit value.
#[derive(Debug)]
pub struct RelativeLogic {
    mode: RelativeMode,
    ctrl_in_num: Option<u8>,
    ctrl_out_num: Option<u8>,
    midi: Option<MidiSpec>,
    osc_addr: String,
    step: u8,
    state: u8
}

impl RelativeLogic {
    fn update(&mut self, new_state: u8) -> Response {
        let changed = new_state != self.state;
        let new_encoder_led_val = Self::encoder_led_val(new_state);
        let encoder_led_val_changed = new_encoder_led_val != Self::encoder_led_val(self.state);
        self.state = new_state;

        if !changed {
            return Response::new();
        }

        let ctrl = if encoder_led_val_changed {
            self.ctrl_out_num.map(|num| CtrlResponse {
                data: vec![num, self.state]
            })
        } else {
            None
        };

        Response {
            ctrl,
            osc: Some(OscResponse {
                addr: self.osc_addr.clone(),
                args: vec![OscType::Float(self.state as f32 / 127.0)]
            }),
            midi: self.midi.map(|midi| {
                let data = match midi.kind {
                    MidiKind::Cc => {
                        vec![
                            0b10110000 | midi.channel,
                            midi.num,
                            self.state
                        ]
                    }
                };
                MidiResponse {
                    data
                }
            })
        }
    }

    fn encoder_led_val(val: u8) -> u8 {
        if val < 7 {
            0
        } else {
            (val - 7) / 11 * 11 + 7
        }
    }
}

impl CtrlLogic for RelativeLogic {
    fn from_mapping(mapping: &Mapping) -> Option<Box<dyn CtrlLogic>> {
        let CtrlKind::Relative { mode } = mapping.ctrl_kind else {
            return None;
        };

        Some(Box::new(RelativeLogic {
            mode: mode,
            ctrl_in_num: mapping.ctrl_in_num,
            ctrl_out_num: mapping.ctrl_out_num,
            midi: mapping.midi,
            osc_addr: mapping.osc_addr(),
            step: mapping.step.unwrap_or(1),
            state: 0x00
        }))
    }

    fn handle_ctrl(&mut self, num: u8, val: u8) -> Option<Response> {
        let Some(ctrl_in_num) = self.ctrl_in_num else {
            return None;
        };

        if num != ctrl_in_num {
            return None;
        }

        let delta: i8 = if val < 0x40 { val as i8 } else { val as i8 + i8::MIN };
        let response = match self.mode {
            RelativeMode::Raw => {
                OscResponse {
                    addr: self.osc_addr.clone(),
                    args: vec![OscType::Float(delta as f32)]
                }.into()
            },
            RelativeMode::Accumulate => {
                // scale the raw per-detent tick by `step` so the control
                // reaches its full range in fewer physical turns. clamp
                // through i32 first so a large step can't wrap the i8 delta.
                let scaled_delta = (delta as i32 * self.step as i32)
                    .clamp(i8::MIN as i32, i8::MAX as i32) as i8;
                self.update(self.state.saturating_add_signed(scaled_delta).min(127))
            }
        };

        Some(response)
    }

    fn handle_osc(&mut self, msg: &OscMessage) -> Option<Response> {
        let Some(_num) = self.ctrl_out_num else {
            return None;
        };

        if msg.addr != self.osc_addr {
            return None;
        }

        if msg.args.len() < 1 {
            return None;
        }

        let OscType::Float(val) = msg.args[0] else {
            return None;
        };

        let new_state = float_to_7bit(val);

        let mut response = Response::new();
        response.ctrl = self.update(new_state).ctrl;
        Some(response)
    }

    fn handle_midi(&mut self, msg: &[u8]) -> Option<Response> {
        let Some(_num) = self.ctrl_out_num else {
            return None;
        };

        let Some(midi_spec) = self.midi else {
            return None;
        };

        if msg.len() != 3 {
            return None;
        }

        let status = msg[0];
        let num = msg[1];
        let val = msg[2];

        if status != 0b10110000 | midi_spec.channel {
            return None;
        }

        if num != midi_spec.num {
            return None;
        }

        let mut response = Response::new();
        response.ctrl = self.update(val).ctrl;
        Some(response)
    }
}

fn float_to_7bit(val: f32) -> u8 {
    (val.max(0.0).min(1.0) * 127.0).round() as u8
}
