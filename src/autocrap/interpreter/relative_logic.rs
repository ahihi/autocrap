use rosc::{OscMessage, OscType};

use crate::autocrap::config::{CtrlKind, Mapping, MidiKind, RelativeMode};

use super::{BankAssignment, CtrlLogic, CtrlResponse, MidiResponse, OscResponse, Response, build_bank_assignments};

/// An endless encoder, holding one accumulated 7-bit value per bank.
#[derive(Debug)]
pub struct RelativeLogic {
    mode: RelativeMode,
    ctrl_in_num: Option<u8>,
    ctrl_out_num: Option<u8>,
    step: u8,
    bank_assignments: Vec<BankAssignment<u8>>,
    active_bank_index: usize,
}

impl RelativeLogic {
    fn update(&mut self, new_value: u8) -> Response {
        let previous_value = self.bank_assignments[self.active_bank_index].remembered_value;
        let changed = new_value != previous_value;
        let ring_position_changed =
            Self::encoder_led_val(new_value) != Self::encoder_led_val(previous_value);
        self.bank_assignments[self.active_bank_index].remembered_value = new_value;

        if !changed {
            return Response::new();
        }

        let ctrl = if ring_position_changed {
            self.build_indicator_write(new_value)
        } else {
            None
        };

        let assignment = &self.bank_assignments[self.active_bank_index];

        Response {
            ctrl,
            osc: Some(OscResponse {
                addr: assignment.osc_address.clone(),
                args: vec![OscType::Float(new_value as f32 / 127.0)]
            }),
            midi: assignment.midi_target.map(|midi| {
                let data = match midi.kind {
                    MidiKind::Cc => {
                        vec![
                            0b10110000 | midi.channel,
                            midi.num,
                            new_value
                        ]
                    }
                };
                MidiResponse {
                    data
                }
            })
        }
    }

    fn build_indicator_write(&self, value: u8) -> Option<CtrlResponse> {
        self.ctrl_out_num.map(|num| CtrlResponse {
            data: vec![num, value]
        })
    }

    fn store_inbound_value(&mut self, matching_bank_index: usize, new_value: u8) -> Response {
        let previous_value = self.bank_assignments[matching_bank_index].remembered_value;
        let changed = new_value != previous_value;
        let ring_position_changed =
            Self::encoder_led_val(new_value) != Self::encoder_led_val(previous_value);
        self.bank_assignments[matching_bank_index].remembered_value = new_value;

        // a bank the user is not on has no visible ring; the value waits there
        // until they switch to it
        if !changed || !ring_position_changed || matching_bank_index != self.active_bank_index {
            return Response::new();
        }

        let mut response = Response::new();
        response.ctrl = self.build_indicator_write(new_value);
        response
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
    fn from_mapping(mapping: &Mapping, bank_count: u8) -> Option<Box<dyn CtrlLogic>> {
        let CtrlKind::Relative { mode } = mapping.ctrl_kind else {
            return None;
        };

        Some(Box::new(RelativeLogic {
            mode: mode,
            ctrl_in_num: mapping.ctrl_in_num,
            ctrl_out_num: mapping.ctrl_out_num,
            step: mapping.step.unwrap_or(1),
            bank_assignments: build_bank_assignments(mapping, bank_count, 0x00),
            active_bank_index: 0,
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
                    addr: self.bank_assignments[self.active_bank_index].osc_address.clone(),
                    args: vec![OscType::Float(delta as f32)]
                }.into()
            },
            RelativeMode::Accumulate => {
                // scale the raw per-detent tick by `step` so the control
                // reaches its full range in fewer physical turns. clamp
                // through i32 first so a large step can't wrap the i8 delta.
                let scaled_delta = (delta as i32 * self.step as i32)
                    .clamp(i8::MIN as i32, i8::MAX as i32) as i8;
                let accumulated_value = self.bank_assignments[self.active_bank_index]
                    .remembered_value
                    .saturating_add_signed(scaled_delta)
                    .min(127);
                self.update(accumulated_value)
            }
        };

        Some(response)
    }

    fn handle_osc(&mut self, msg: &OscMessage) -> Option<Response> {
        self.ctrl_out_num?;

        if msg.args.len() < 1 {
            return None;
        }

        let OscType::Float(val) = msg.args[0] else {
            return None;
        };

        let matching_bank_index = self.bank_assignments.iter()
            .position(|assignment| assignment.osc_address == msg.addr)?;

        Some(self.store_inbound_value(matching_bank_index, float_to_7bit(val)))
    }

    fn handle_midi(&mut self, msg: &[u8]) -> Option<Response> {
        self.ctrl_out_num?;

        if msg.len() != 3 {
            return None;
        }

        let status = msg[0];
        let num = msg[1];
        let val = msg[2];

        let matching_bank_index = self.bank_assignments.iter()
            .position(|assignment| match assignment.midi_target {
                Some(target) => status == (0b10110000 | target.channel) && num == target.num,
                None => false,
            })?;

        Some(self.store_inbound_value(matching_bank_index, val))
    }

    fn switch_to_bank(&mut self, new_bank_index: u8) -> Vec<Response> {
        if self.bank_assignments.len() <= 1 {
            return vec![];
        }

        self.active_bank_index = new_bank_index as usize;

        // unconditional: the ring is currently displaying a different bank's value,
        // so it must be rewritten even when the two banks happen to agree
        let restored_value = self.bank_assignments[self.active_bank_index].remembered_value;
        let Some(restore_write) = self.build_indicator_write(restored_value) else {
            return vec![];
        };

        vec![restore_write.into()]
    }
}

fn float_to_7bit(val: f32) -> u8 {
    (val.max(0.0).min(1.0) * 127.0).round() as u8
}
