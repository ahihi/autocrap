use rosc::{OscMessage, OscType};

use super::config::{Config, CtrlKind, Mapping, MidiKind, MidiSpec, OnOffMode, RelativeMode};

#[derive(Debug)]
pub struct Interpreter {
    ctrls: Vec<Box<dyn CtrlLogic>>,
}

impl Interpreter {
    pub fn new(config: &Config) -> Interpreter {
        let constructors: Vec<Box<dyn Fn(&Mapping) -> Option<Box<dyn CtrlLogic>>>> = vec![
            Box::new(OnOffLogic::from_mapping),
            Box::new(EightBitLogic::from_mapping),
            Box::new(RelativeLogic::from_mapping),
        ];
        let mut ctrls: Vec<Box<dyn CtrlLogic>> = vec![];
        for abstract_mapping in config.mappings.iter() {
            for mapping in abstract_mapping.expand_iter() {
                let mut logic_opt: Option<Box<dyn CtrlLogic>> = None;

                for make_logic in &constructors {
                    let Some(logic) = make_logic(&mapping) else {
                        continue;
                    };

                    logic_opt = Some(logic);
                    break;
                }

                let Some(logic) = logic_opt else {
                    println!("warning: unhandled mapping {:?}", mapping);
                    continue;
                };

                println!("info: adding {:?}", logic);
                ctrls.push(logic);
            }
        }

        let interp = Interpreter {
            ctrls
        };

        interp
    }

    pub fn handle_ctrl(&mut self, num: u8, val: u8) -> Option<Response> {
        for ctrl in &mut self.ctrls {
            let Some(response) = ctrl.handle_ctrl(num, val) else {
                continue;
            };

            return Some(response);
        }

        None
    }

    pub fn handle_osc(&mut self, msg: &OscMessage) -> Option<Response> {
        for ctrl in &mut self.ctrls {
            let Some(response) = ctrl.handle_osc(msg) else {
                continue;
            };

            return Some(response);
        }

        None
    }

    pub fn handle_midi(&mut self, msg: &[u8]) -> Option<Response> {
        for ctrl in &mut self.ctrls {
            let Some(response) = ctrl.handle_midi(msg) else {
                continue;
            };

            return Some(response);
        }

        None
    }
}

pub trait CtrlLogic: core::fmt::Debug + Send + Sync {
    fn from_mapping(mapping: &Mapping) -> Option<Box<dyn CtrlLogic>> where Self: Sized;
    fn handle_ctrl(&mut self, num: u8, val: u8) -> Option<Response>;
    fn handle_osc(&mut self, msg: &OscMessage) -> Option<Response>;
    fn handle_midi(&mut self, msg: &[u8]) -> Option<Response>;
}

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

#[derive(Debug)]
pub struct EightBitLogic {
    ctrl_in_hi_num: u8,
    ctrl_in_lo_num: u8,
    midi: Option<MidiSpec>,
    osc_addr: String,
    state: [u8;2]
}

impl CtrlLogic for EightBitLogic {
    fn from_mapping(mapping: &Mapping) -> Option<Box<dyn CtrlLogic>> {
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

#[derive(Debug)]
pub struct RelativeLogic {
    mode: RelativeMode,
    ctrl_in_num: Option<u8>,
    ctrl_out_num: Option<u8>,
    midi: Option<MidiSpec>,
    osc_addr: String,
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
                self.update(self.state.saturating_add_signed(delta).min(127))
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

#[derive(Debug)]
pub struct CtrlResponse {
    pub data: Vec<u8>
}

#[derive(Debug)]
pub struct OscResponse {
    pub addr: String,
    pub args: Vec<OscType>,
}

#[derive(Debug)]
pub struct MidiResponse {
    pub data: Vec<u8>
}

#[derive(Debug)]
pub struct Response {
    pub ctrl: Option<CtrlResponse>,
    pub osc: Option<OscResponse>,
    pub midi: Option<MidiResponse>
}

impl Response {
    pub fn new() -> Response {
        Response {
            ctrl: None,
            osc: None,
            midi: None
        }
    }
}

impl Into<Response> for CtrlResponse {
    fn into(self) -> Response {
        Response {
            ctrl: Some(self),
            osc: None,
            midi: None
        }
    }
}

impl Into<Response> for OscResponse {
    fn into(self) -> Response {
        Response {
            ctrl: None,
            osc: Some(self),
            midi: None
        }
    }
}

impl Into<Response> for MidiResponse {
    fn into(self) -> Response {
        Response {
            ctrl: None,
            osc: None,
            midi: Some(self)
        }
    }
}

fn float_to_7bit(val: f32) -> u8 {
    (val.max(0.0).min(1.0) * 127.0).round() as u8
}
