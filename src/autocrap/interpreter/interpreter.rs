use log::{warn, info};
use rosc::OscMessage;

use crate::autocrap::config::{Config, Mapping};

use super::{CtrlLogic, EightBitLogic, OnOffLogic, RelativeLogic, Response};

/// Owns every control's logic and routes each event to whichever one claims it.
#[derive(Debug)]
pub struct Interpreter {
    ctrls: Vec<Box<dyn CtrlLogic>>,
}

impl Interpreter {
    /// Builds one logic object per control described by the config.
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
                    warn!("unhandled mapping {:?}", mapping);
                    continue;
                };

                info!("adding {:?}", logic);
                ctrls.push(logic);
            }
        }

        let interp = Interpreter {
            ctrls
        };

        interp
    }

    /// Handles the device reporting a control number and value.
    pub fn handle_ctrl(&mut self, num: u8, val: u8) -> Option<Response> {
        for ctrl in &mut self.ctrls {
            let Some(response) = ctrl.handle_ctrl(num, val) else {
                continue;
            };

            return Some(response);
        }

        None
    }

    /// Handles an inbound OSC message from the application.
    pub fn handle_osc(&mut self, msg: &OscMessage) -> Option<Response> {
        for ctrl in &mut self.ctrls {
            let Some(response) = ctrl.handle_osc(msg) else {
                continue;
            };

            return Some(response);
        }

        None
    }

    /// Handles an inbound MIDI message from the application.
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
