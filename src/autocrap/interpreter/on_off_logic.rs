use rosc::{OscMessage, OscType};

use crate::autocrap::config::{CtrlKind, Mapping, MidiKind, OnOffMode};

use super::{BankAssignment, CtrlLogic, CtrlResponse, MidiResponse, OscResponse, Response, build_bank_assignments};

/// A button or touch sensor, holding one on/off state per bank.
#[derive(Debug)]
pub struct OnOffLogic {
    mode: OnOffMode,
    ctrl_in_num: Option<u8>,
    ctrl_out_num: Option<u8>,
    bank_assignments: Vec<BankAssignment<bool>>,
    active_bank_index: usize,
    is_physically_held: bool,
    /// Set when a bank switch already emitted this control's "off",
    /// so the physical release that follows must not emit a second one.
    release_consumed_by_bank_switch: bool,
}

impl OnOffLogic {
    fn build_response_for_bank(
        &self,
        bank_index: usize,
        new_state: bool,
        include_indicator_write: bool
    ) -> Response {
        let assignment = &self.bank_assignments[bank_index];

        Response {
            osc: Some(OscResponse {
                addr: assignment.osc_address.clone(),
                args: vec![OscType::Float(if new_state { 1.0 } else { 0.0 })]
            }),
            ctrl: if include_indicator_write {
                self.build_indicator_write(new_state)
            } else {
                None
            },
            midi: assignment.midi_target.map(|midi| {
                let data = match midi.kind {
                    MidiKind::Cc => {
                        vec![
                            0b10110000 | midi.channel,
                            midi.num,
                            if new_state { 0x7f } else { 0x00 }
                        ]
                    }
                };
                MidiResponse {
                    data
                }
            })
        }
    }

    fn build_indicator_write(&self, new_state: bool) -> Option<CtrlResponse> {
        self.ctrl_out_num.map(|num| CtrlResponse {
            data: vec![num, if new_state { 0x7f } else { 0x00 }]
        })
    }

    fn update(&mut self, new_state: bool, remember: bool) -> Response {
        if remember {
            let changed = new_state != self.bank_assignments[self.active_bank_index].remembered_value;
            self.bank_assignments[self.active_bank_index].remembered_value = new_state;

            if !changed {
                return Response::new();
            }
        }

        self.build_response_for_bank(self.active_bank_index, new_state, true)
    }

    fn store_inbound_state(&mut self, matching_bank_index: usize, new_state: bool) -> Response {
        let changed = new_state != self.bank_assignments[matching_bank_index].remembered_value;
        self.bank_assignments[matching_bank_index].remembered_value = new_state;

        // a bank the user is not on has no visible indicator; the value waits
        // there until they switch to it
        if !changed || matching_bank_index != self.active_bank_index {
            return Response::new();
        }

        let mut response = Response::new();
        response.ctrl = self.build_indicator_write(new_state);
        response
    }
}

impl CtrlLogic for OnOffLogic {
    fn from_mapping(mapping: &Mapping, bank_count: u8) -> Option<Box<dyn CtrlLogic>> {
        let CtrlKind::OnOff { mode } = mapping.ctrl_kind else {
            return None;
        };

        Some(Box::new(OnOffLogic {
            mode: mode,
            ctrl_in_num: mapping.ctrl_in_num,
            ctrl_out_num: mapping.ctrl_out_num,
            bank_assignments: build_bank_assignments(mapping, bank_count, false),
            active_bank_index: 0,
            is_physically_held: false,
            release_consumed_by_bank_switch: false,
        }))
    }

    fn handle_ctrl(&mut self, num: u8, val: u8) -> Option<Response> {
        let Some(ctrl_in_num) = self.ctrl_in_num else {
            return None;
        };

        if num != ctrl_in_num {
            return None;
        }

        let pressed = val != 0x00;
        self.is_physically_held = pressed;

        if !pressed && self.release_consumed_by_bank_switch {
            self.release_consumed_by_bank_switch = false;
            // handled, but the bank switch already sent this control's off.
            // returning None here would log the release as unhandled data.
            return Some(Response::new());
        }

        let mut new_state = self.bank_assignments[self.active_bank_index].remembered_value;
        let mut send_ctrl = true;
        let mut send_osc = true;
        let mut remember = true;
        match self.mode {
            OnOffMode::Raw => {
                new_state = pressed;
                send_ctrl = false;
                remember = false;
            },
            OnOffMode::Momentary => {
                new_state = pressed;
            },
            OnOffMode::Toggle => {
                if pressed {
                    new_state = !new_state;
                } else {
                    send_ctrl = false;
                    send_osc = false;
                }
            }
        }

        let mut response = self.update(new_state, remember);

        if !send_ctrl {
            response.ctrl = None;
        }

        if !send_osc {
            response.osc = None;
        }

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

        Some(self.store_inbound_state(matching_bank_index, val != 0.0))
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

        Some(self.store_inbound_state(matching_bank_index, val != 0))
    }

    fn switch_to_bank(&mut self, new_bank_index: u8) -> Vec<Response> {
        if self.bank_assignments.len() <= 1 {
            return vec![];
        }

        let mut device_writes = vec![];

        // a held momentary control already sent an "on" for the bank we are leaving.
        // without an explicit "off" the receiving application keeps that parameter
        // stuck on with no way to clear it.
        let held_control_needs_release =
            matches!(self.mode, OnOffMode::Momentary | OnOffMode::Raw)
            && self.is_physically_held;

        if held_control_needs_release {
            let outgoing_bank_index = self.active_bank_index;
            let mode_drives_indicator = matches!(self.mode, OnOffMode::Momentary);

            device_writes.push(self.build_response_for_bank(
                outgoing_bank_index,
                false,
                mode_drives_indicator,
            ));
            self.bank_assignments[outgoing_bank_index].remembered_value = false;
            self.release_consumed_by_bank_switch = true;
        }

        self.active_bank_index = new_bank_index as usize;

        // restore the incoming bank's indicator. deliberately never emits midi or osc:
        // switching banks must not turn a parameter on.
        if self.ctrl_out_num.is_some() {
            let restored_state = self.bank_assignments[self.active_bank_index].remembered_value;
            let mut restore_write = Response::new();
            restore_write.ctrl = self.build_indicator_write(restored_state);
            device_writes.push(restore_write);
        }

        device_writes
    }
}
