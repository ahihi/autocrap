use rosc::{OscMessage, OscType};

use crate::autocrap::config::{CtrlKind, Mapping, MidiKind, MidiSpec, OnOffMode};

use super::{CtrlLogic, CtrlResponse, MidiResponse, OscResponse, Response};

/// A button or touch sensor, holding its on/off state.
#[derive(Debug)]
pub struct OnOffLogic {
    mode: OnOffMode,
    ctrl_in_num: Option<u8>,
    ctrl_out_num: Option<u8>,
    midi: Option<MidiSpec>,
    osc_addr: String,
    state: bool
}

impl OnOffLogic {
    fn update(&mut self, new_state: bool, remember: bool) -> Response {
        if remember {
            let changed = new_state != self.state;
            self.state = new_state;

            if !changed {
                return Response::new();
            }
        }

        Response {
            osc: Some(OscResponse {
                addr: self.osc_addr.clone(),
                args: vec![OscType::Float(if new_state { 1.0 } else { 0.0 })]
            }),
            ctrl: self.ctrl_out_num.map(|num| CtrlResponse {
                data: vec![num, if new_state { 0x7f } else { 0x00 }]
            }),
            midi: self.midi.map(|midi| {
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
}

impl CtrlLogic for OnOffLogic {
    fn from_mapping(mapping: &Mapping) -> Option<Box<dyn CtrlLogic>> {
        let CtrlKind::OnOff { mode } = mapping.ctrl_kind else {
            return None;
        };

        Some(Box::new(OnOffLogic {
            mode: mode,
            ctrl_in_num: mapping.ctrl_in_num,
            ctrl_out_num: mapping.ctrl_out_num,
            midi: mapping.midi,
            osc_addr: mapping.osc_addr(),
            state: false
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
        let mut new_state = self.state;
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
                    new_state = !self.state;
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

        let mut response = Response::new();
        response.ctrl = self.update(val != 0.0, true).ctrl;
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
        response.ctrl = self.update(val != 0, true).ctrl;
        Some(response)
    }
}
