use std::{iter, net::{SocketAddrV4}};

use rosc::{OscMessage, OscType};
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Numbering {
    Single {
        ctrl_in_sequence: Option<Vec<u8>>,
        ctrl_in: Option<u8>,
        ctrl_out: Option<u8>,
        midi: Option<u8>,
    },
    Range {
        ctrl_in: Option<(u8, u8)>,
        ctrl_out: Option<(u8, u8)>,
        midi: Option<(u8, u8)>,
    }
}

impl Numbering {
    pub fn num_alternatives(&self) -> Option<u8> {
        match self {
            Numbering::Single { .. } => Some(1),
            Numbering::Range {ctrl_in, ctrl_out, midi} => {
                let ranges = iter::once(ctrl_in)
                    .chain(iter::once(ctrl_out))
                    .chain(iter::once(midi))
                    .filter_map(|r| *r);

                let mut num_opt: Option<u8> = None;
                for (lo, hi) in ranges {
                    let range_num = hi - lo + 1;
                    let Some(num) = num_opt else {
                        num_opt = Some(range_num);
                        continue;
                    };

                    if range_num != num {
                        return None;
                    }
                }
                
                num_opt
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CtrlNum {
    Single(u8),
    Range(u8, u8),
    Sequence(Vec<u8>)
}

impl CtrlNum {
    pub fn match_num(&self, num: u8) -> Option<u8> {
        match *self {
            CtrlNum::Single(n) if num == n =>
                Some(0),
            CtrlNum::Range(lo, hi) if lo <= num && num <= hi =>
                Some(num - lo),
            // TODO: Sequence
            _ =>
                None
        }
    }

    pub fn range_size(&self) -> u8 {
        match *self {
            CtrlNum::Single(_) => 1,
            CtrlNum::Range(lo, hi) => hi - lo + 1,
            _ => unimplemented!()
        }
    }

    pub fn index_to_num(&self, i: u8) -> Option<u8> {
        match *self {
            CtrlNum::Single(num) if i == 0 =>
                Some(num),
            CtrlNum::Range(lo, hi) if i <= hi-lo =>
                Some(lo + i),
            CtrlNum::Sequence(_) =>
                unimplemented!(),
            _ => None
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum CtrlKind {
    OnOff,
    EightBit,
    Relative,
}

impl CtrlKind {
    pub fn ctrl_to_osc(&self, val: u8) -> Vec<OscType> {
        match self {
            CtrlKind::OnOff =>
                vec![OscType::Float(if val == 0x7f { 1.0 } else { 0.0 })],
            CtrlKind::Relative =>
                vec![OscType::Float(if val < 0x40 { val as f32 } else { val as f32 - 128.0 })],
            _ => unimplemented!()
        }
    }

    pub fn osc_to_ctrl(&self, args: &[OscType]) -> Option<u8> {
        if args.len() < 1 {
            return None;
        }

        let OscType::Float(val) = args[0] else {
            return None;
        };

        match self {
            CtrlKind::OnOff =>
                Some(float_to_7bit(val)),
            CtrlKind::Relative =>
                Some(float_to_7bit(val)),
            _ => unimplemented!()
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum MidiKind {
    Cc,
    CoarseFine,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mapping {
    pub name: String,
    pub numbering: Numbering,
    pub ctrl_kind: CtrlKind,
    pub midi_kind: MidiKind,
}

impl Mapping {
    pub fn expand_iter(&self) -> impl Iterator<Item = Mapping> {
        let mut mappings = vec![];
        match self.numbering {
            Numbering::Single { .. } => mappings.push(self.clone()),
            Numbering::Range {ctrl_in, ctrl_out, midi} => {
                println!("{:?}", self.numbering);
                let num = self.numbering.num_alternatives().unwrap();
                for i in 0..num {
                    mappings.push(Mapping {
                        name: self.name.replace("{i}", &i.to_string()),
                        numbering: Numbering::Single {
                            ctrl_in_sequence: None,
                            ctrl_in: ctrl_in.map(|(lo, hi)| lo + i),
                            ctrl_out: ctrl_out.map(|(lo, hi)| lo + i),
                            midi: midi.map(|(lo, hi)| lo + i),
                        },
                        ctrl_kind: self.ctrl_kind,
                        midi_kind: self.midi_kind,
                    });
                }
            }
        };
        mappings.into_iter()
    }

    pub fn osc_addr(&self, i: u8) -> String {
        format!("/{}", self.name.replace("{i}", &i.to_string()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub vendor_id: u16,
    pub product_id: u16,
    pub in_endpoint: u8,
    pub out_endpoint: u8,
    pub host_addr: SocketAddrV4,
    pub osc_out_addr: SocketAddrV4,
    pub osc_in_addr: SocketAddrV4,
    pub mappings: Vec<Mapping>
}

impl Config {    
    // pub fn match_ctrl(&self, num: u8, val: u8) -> Option<CtrlMatchData> {
    //     for mapping in self.mappings.iter() {
    //         let Some(ctrl_in_num) = mapping.ctrl_in_num else {
    //             continue;
    //         };

    //         let Some(i) = ctrl_in_num.match_num(num) else {
    //             continue;
    //         };

    //         return Some(CtrlMatchData {
    //             osc_addr: mapping.osc_addr(i),
    //             osc_args: mapping.ctrl_kind.ctrl_to_osc(val)
    //         })
    //     }

    //     None
    // }

    // pub fn match_osc(&self, msg: &OscMessage) -> Option<OscMatchData> {
    //     for mapping in self.mappings.iter() {
    //         let Some(ctrl_out_num) = mapping.ctrl_out_num else {
    //             continue;
    //         };

    //         for i in 0..ctrl_out_num.range_size() {
    //             let addr = mapping.osc_addr(i);

    //             if addr != msg.addr {
    //                 continue;
    //             }

    //             let Some(num) = ctrl_out_num.index_to_num(i) else {
    //                 continue;
    //             };

    //             let Some(val) = mapping.ctrl_kind.osc_to_ctrl(&msg.args) else {
    //                 continue;
    //             };

    //             return Some(OscMatchData {
    //                 ctrl_data: vec![num, val]
    //             });
    //         }
    //     }

    //     None
    // }
}

#[derive(Clone, Debug)]
pub struct CtrlMatchData {
    pub osc_addr: String,
    pub osc_args: Vec<OscType>,
}

#[derive(Clone, Debug)]
pub struct OscMatchData {
    pub ctrl_data: Vec<u8>,
}


fn float_to_7bit(val: f32) -> u8 {
    (val.max(0.0).min(1.0) * 127.0).round() as u8
}
