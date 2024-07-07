use std::{
    sync::{Arc, RwLock}
};

use rosc::{OscType};

use super::config::{Config, CtrlKind, Mapping, OnOffMode, RelativeMode};

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

pub trait CtrlLogic: core::fmt::Debug {
    fn from_mapping(mapping: &Mapping) -> Option<Box<dyn CtrlLogic>> where Self: Sized;
    fn handle_ctrl(&self, num: u8, val: u8) -> Option<CtrlResponse>;
}

#[derive(Debug)]
pub struct OnOffLogic {
    mode: OnOffMode,
    ctrl_in_num: Option<u8>,
    ctrl_out_num: Option<u8>,
    osc_addr: String,
    state: Arc<RwLock<bool>>
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
            osc_addr: mapping.osc_addr(),
            state: Arc::new(RwLock::new(false))
        }))
    }

    fn handle_ctrl(&self, num: u8, val: u8) -> Option<CtrlResponse> {
        let Some(ctrl_in_num) = self.ctrl_in_num else {
            return None;
        };

        if num != ctrl_in_num {
            return None;
        }

        let pressed = if val != 0 { true } else { false };
        let mut state = self.state.write().unwrap();
        let mut send = true;
        match self.mode {
            OnOffMode::Raw => {
                *state = pressed;
                send = false;
            },
            OnOffMode::Momentary => {
                *state = pressed;
            },
            OnOffMode::Toggle => {
                if pressed {
                    *state = !*state;
                } else {
                    send = false;
                }
            }
        }

        let osc = if send {
            Some((self.osc_addr.clone(), vec![OscType::Float(if *state { 1.0 } else { 0.0 })]))
        } else {
            None
        };

        let ctrl_out = if send {
            self.ctrl_out_num.map(|num| vec![num, if *state { 0x7f } else { 0x00 }])
        } else {
            None
        };

        Some(CtrlResponse {
            osc: osc,
            ctrl: ctrl_out
        })
    }
}

#[derive(Debug)]
pub struct EightBitLogic {
    ctrl_in_hi_num: u8,
    ctrl_in_lo_num: u8,
    osc_addr: String,
    state: Arc<RwLock<[u8;2]>>
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
            osc_addr: format!("/{}", mapping.name),
            state: Arc::new(RwLock::new([0x00,0x00]))
        }))
    }

    fn handle_ctrl(&self, num: u8, val: u8) -> Option<CtrlResponse> {
        if num == self.ctrl_in_hi_num {
            let mut state = self.state.write().unwrap();
            state[0] = val;
            return Some(CtrlResponse {
                osc: None,
                ctrl: None
            });
        }

        if num == self.ctrl_in_lo_num {
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

#[derive(Debug)]
pub struct RelativeLogic {
    mode: RelativeMode,
    ctrl_in_num: Option<u8>,
    ctrl_out_num: Option<u8>,
    osc_addr: String,
    state: Arc<RwLock<u8>>
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
            osc_addr: mapping.osc_addr(),
            state: Arc::new(RwLock::new(0x00))
        }))
    }

    fn handle_ctrl(&self, num: u8, val: u8) -> Option<CtrlResponse> {
        let Some(ctrl_in_num) = self.ctrl_in_num else {
            return None;
        };

        if num != ctrl_in_num {
            return None;
        }

        let delta: i8 = if val < 0x40 { val as i8 } else { val as i8 + i8::MIN };
        let osc_val;
        let mut ctrl_out = None;
        match self.mode {
            RelativeMode::Raw => {
                osc_val = delta as f32;
            },
            RelativeMode::Accumulate => {
                let mut state = self.state.write().unwrap();
                *state = state.saturating_add_signed(delta).min(127);
                osc_val = *state as f32 / 127.0;
                ctrl_out = self.ctrl_out_num.map(|num| vec![num, *state]);
            }
        }

        Some(CtrlResponse {
            osc: Some((self.osc_addr.clone(), vec![OscType::Float(osc_val)])),
            ctrl: ctrl_out
        })
    }
}

pub struct CtrlResponse {
    pub osc: Option<(String, Vec<OscType>)>,
    pub ctrl: Option<Vec<u8>>
}
