use std::{
    sync::{Arc, RwLock}
};

use rosc::{OscMessage, OscType};

use super::config::{Config, CtrlKind, Numbering};

pub struct Interpreter {
    ctrls: Vec<Box<dyn CtrlLogic>>,
}

impl Interpreter {
    pub fn new(config: &Config) -> Interpreter {
        let mut ctrls: Vec<Box<dyn CtrlLogic>> = vec![];
        for mapping in config.mappings.iter() {
            for m in mapping.expand_iter() {
                let Numbering::Single { ctrl_in, ctrl_out, midi, ref ctrl_in_sequence } = m.numbering
                else {
                    unreachable!();
                };

                match m.ctrl_kind {
                    CtrlKind::OnOff => {
                        ctrls.push(Box::new(OnOffLogic {
                            ctrl_in_num: ctrl_in,
                            ctrl_out_num: ctrl_out,
                            osc_addr: format!("/{}", m.name),
                            state: Arc::new(RwLock::new(false))
                        }));
                    },
                    CtrlKind::EightBit => {
                        if let Some(ctrl_in_sequence) = ctrl_in_sequence {
                            ctrls.push(Box::new(EightBitLogic {
                                ctrl_in_first: ctrl_in_sequence[0],
                                ctrl_in_num: ctrl_in_sequence[1],
                                osc_addr: format!("/{}", m.name),
                                state: Arc::new(RwLock::new([0x00,0x00]))
                            }));
                        }
                    },
                    _ => {
                        println!("{:?}", m);
                    }
                }
            }
        }

        Interpreter {
            ctrls
        }
    }

    pub fn handle_ctrl(&self, num: u8, val: u8) -> Option<CtrlResponse> {
        for ctrl in &self.ctrls {
            let Some(response) = ctrl.handle_ctrl(num, val) else {
                continue;
            };

            return Some(response);
        }

        None
    }
}

pub trait CtrlLogic {
    fn handle_ctrl(&self, num: u8, val: u8) -> Option<CtrlResponse>;
}

pub struct OnOffLogic {
    ctrl_in_num: Option<u8>,
    ctrl_out_num: Option<u8>,
    osc_addr: String,
    state: Arc<RwLock<bool>>
}

impl CtrlLogic for OnOffLogic {
    fn handle_ctrl(&self, num: u8, val: u8) -> Option<CtrlResponse> {
        let Some(ctrl_in_num) = self.ctrl_in_num else {
            return None;
        };

        if num != ctrl_in_num {
            return None;
        }

        let mut state = self.state.write().unwrap();
        *state = if val != 0 { true } else { false };
        let ctrl_out = self.ctrl_out_num.map(|num| vec![num, if *state { 0x7f} else { 0x00 }]);
        Some(CtrlResponse {
            osc: Some((self.osc_addr.clone(), vec![OscType::Float(if *state { 1.0 } else { 0.0 })])),
            ctrl: ctrl_out
        })
    }
}

pub struct EightBitLogic {
    ctrl_in_first: u8,
    ctrl_in_num: u8,
    osc_addr: String,
    state: Arc<RwLock<[u8;2]>>
}

impl CtrlLogic for EightBitLogic {
    fn handle_ctrl(&self, num: u8, val: u8) -> Option<CtrlResponse> {
        if num == self.ctrl_in_first {
            let mut state = self.state.write().unwrap();
            state[0] = val;
            return Some(CtrlResponse {
                osc: None,
                ctrl: None
            });
        }

        if num == self.ctrl_in_num {
            let mut state = self.state.write().unwrap();
            state[1] = val;
            let val8 = state[0] << 1 | (if state[1] != 0x00 { 1 } else { 0 });
            return Some(CtrlResponse {
                osc: Some((self.osc_addr.clone(), vec![OscType::Float(val8 as f32 / 255.0)])),
                ctrl: None
            })
        }

        None
    }
}

pub struct CtrlResponse {
    pub osc: Option<(String, Vec<OscType>)>,
    pub ctrl: Option<Vec<u8>>
}
