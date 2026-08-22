use log::{warn, info};
use rosc::OscMessage;

use crate::autocrap::config::{BankSelectorMapping, Config, Mapping};

use super::{CtrlLogic, CtrlResponse, EightBitLogic, OnOffLogic, RelativeLogic, Response};

/// Owns every control's logic and routes each event to whichever one claims it.
///
/// The bank counter lives here rather than in a `CtrlLogic` of its own because a
/// bank change must reach every control at once, which the one-object-per-control
/// model cannot express.
#[derive(Debug)]
pub struct Interpreter {
    ctrls: Vec<Box<dyn CtrlLogic>>,
    bank_selector: Option<BankSelectorMapping>,
    active_bank_index: u8,
}

impl Interpreter {
    /// Builds one logic object per control described by the config.
    pub fn new(config: &Config) -> Interpreter {
        let bank_selector = config.bank_selector();
        let bank_count = bank_selector.map_or(1, |selector| selector.bank_count);

        let constructors: Vec<Box<dyn Fn(&Mapping, u8) -> Option<Box<dyn CtrlLogic>>>> = vec![
            Box::new(OnOffLogic::from_mapping),
            Box::new(EightBitLogic::from_mapping),
            Box::new(RelativeLogic::from_mapping),
        ];
        let mut ctrls: Vec<Box<dyn CtrlLogic>> = vec![];
        for abstract_mapping in config.mappings.iter() {
            for mapping in abstract_mapping.expand_iter() {
                let mut logic_opt: Option<Box<dyn CtrlLogic>> = None;

                for make_logic in &constructors {
                    let Some(logic) = make_logic(&mapping, bank_count) else {
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
            ctrls,
            bank_selector,
            active_bank_index: 0,
        };

        interp
    }

    /// Builds the two indicator writes that display the active bank in binary.
    fn build_bank_indicator_writes(&self) -> Vec<Response> {
        let Some(bank_selector) = self.bank_selector else {
            return vec![];
        };
        let bank = self.active_bank_index;

        vec![
            CtrlResponse { data: vec![
                bank_selector.previous_bank_button.led_control_number,
                if bank & 0b10 != 0 { 0x7f } else { 0x00 },
            ]}.into(),
            CtrlResponse { data: vec![
                bank_selector.next_bank_button.led_control_number,
                if bank & 0b01 != 0 { 0x7f } else { 0x00 },
            ]}.into(),
        ]
    }

    fn cycle_bank(&mut self, step_forwards: bool, bank_count: u8) -> Vec<Response> {
        // bank_count is added before the modulo so stepping back from bank 0
        // cannot underflow the u8
        self.active_bank_index = if step_forwards {
            (self.active_bank_index + 1) % bank_count
        } else {
            (self.active_bank_index + bank_count - 1) % bank_count
        };

        let mut device_writes = self.build_bank_indicator_writes();
        for ctrl in &mut self.ctrls {
            device_writes.extend(ctrl.switch_to_bank(self.active_bank_index));
        }

        device_writes
    }

    /// Handles the device reporting a control number and value.
    ///
    /// `None` means no control claimed the number; `Some(vec![])` means one
    /// claimed it with nothing to emit.
    pub fn handle_ctrl(&mut self, num: u8, val: u8) -> Option<Vec<Response>> {
        if let Some(bank_selector) = self.bank_selector {
            let is_previous_button = num == bank_selector.previous_bank_button.input_control_number;
            let is_next_button = num == bank_selector.next_bank_button.input_control_number;

            if is_previous_button || is_next_button {
                if val == 0x00 {
                    // handled; banks change on press only
                    return Some(vec![]);
                }

                return Some(self.cycle_bank(is_next_button, bank_selector.bank_count));
            }
        }

        for ctrl in &mut self.ctrls {
            let Some(response) = ctrl.handle_ctrl(num, val) else {
                continue;
            };

            return Some(vec![response]);
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

#[cfg(test)]
mod tests {
    use super::*;

    const MIDI_CC_STATUS_ON_CHANNEL_0: u8 = 0b10110000;

    fn interpreter_from_mappings_json(mappings_json: &str) -> Interpreter {
        let config_json = format!(r#"{{
            "vendor_id": 4661,
            "product_id": 2,
            "in_endpoint": 129,
            "out_endpoint": 2,
            "interface": {{"Midi": {{
                "client_name": "autocrap",
                "out_port": {{"Virtual": "autocrap"}},
                "in_port": {{"Virtual": "autocrap"}}
            }}}},
            "mappings": {}
        }}"#, mappings_json);
        let config: Config = serde_json::from_str(&config_json).unwrap();
        config.validate().unwrap();
        Interpreter::new(&config)
    }

    /// A four-bank device with one banked knob, one unbanked knob, a banked
    /// momentary button and a banked toggle button.
    fn interpreter_with_a_bank_selector() -> Interpreter {
        interpreter_from_mappings_json(r#"[
            {"BankSelector": {
                "bank_count": 4,
                "previous_bank_button": {"input_control_number": 126, "led_control_number": 126},
                "next_bank_button": {"input_control_number": 127, "led_control_number": 127}
            }},
            {"Single": {
                "name": "bankedKnob",
                "ctrl_in_num": 64,
                "ctrl_out_num": 64,
                "ctrl_kind": {"Relative": {"mode": "Accumulate"}},
                "bank_midi_targets": [
                    {"channel": 0, "kind": "Cc", "num": 20},
                    {"channel": 0, "kind": "Cc", "num": 21},
                    {"channel": 0, "kind": "Cc", "num": 22},
                    {"channel": 0, "kind": "Cc", "num": 23}
                ]
            }},
            {"Single": {
                "name": "pinnedKnob",
                "ctrl_in_num": 65,
                "ctrl_out_num": 65,
                "ctrl_kind": {"Relative": {"mode": "Accumulate"}},
                "midi": {"channel": 0, "kind": "Cc", "num": 65}
            }},
            {"Single": {
                "name": "bankedMomentaryButton",
                "ctrl_in_num": 112,
                "ctrl_out_num": 112,
                "ctrl_kind": {"OnOff": {"mode": "Momentary"}},
                "bank_midi_targets": [
                    {"channel": 0, "kind": "Cc", "num": 30},
                    {"channel": 0, "kind": "Cc", "num": 31},
                    {"channel": 0, "kind": "Cc", "num": 32},
                    {"channel": 0, "kind": "Cc", "num": 33}
                ]
            }},
            {"Single": {
                "name": "bankedToggleButton",
                "ctrl_in_num": 113,
                "ctrl_out_num": 113,
                "ctrl_kind": {"OnOff": {"mode": "Toggle"}},
                "bank_midi_targets": [
                    {"channel": 0, "kind": "Cc", "num": 40},
                    {"channel": 0, "kind": "Cc", "num": 41},
                    {"channel": 0, "kind": "Cc", "num": 42},
                    {"channel": 0, "kind": "Cc", "num": 43}
                ]
            }}
        ]"#)
    }

    const PREVIOUS_BANK_BUTTON: u8 = 126;
    const NEXT_BANK_BUTTON: u8 = 127;
    const PRESS: u8 = 0x7f;
    const RELEASE: u8 = 0x00;

    fn device_writes_in(responses: &[Response]) -> Vec<Vec<u8>> {
        responses.iter()
            .filter_map(|response| response.ctrl.as_ref().map(|write| write.data.clone()))
            .collect()
    }

    fn midi_messages_in(responses: &[Response]) -> Vec<Vec<u8>> {
        responses.iter()
            .filter_map(|response| response.midi.as_ref().map(|message| message.data.clone()))
            .collect()
    }

    fn osc_addresses_in(responses: &[Response]) -> Vec<String> {
        responses.iter()
            .filter_map(|response| response.osc.as_ref().map(|message| message.addr.clone()))
            .collect()
    }

    fn press_next_bank_button(interpreter: &mut Interpreter) -> Vec<Response> {
        interpreter.handle_ctrl(NEXT_BANK_BUTTON, PRESS).unwrap()
    }

    #[test]
    fn unmatched_control_number_is_not_handled() {
        let mut interpreter = interpreter_with_a_bank_selector();

        assert!(interpreter.handle_ctrl(50, PRESS).is_none());
    }

    #[test]
    fn the_next_bank_button_cycles_forwards_and_wraps_to_zero() {
        let mut interpreter = interpreter_with_a_bank_selector();

        let cycled_bank_indices: Vec<u8> = (0..5).map(|_| {
            press_next_bank_button(&mut interpreter);
            interpreter.active_bank_index
        }).collect();

        assert_eq!(cycled_bank_indices, vec![1, 2, 3, 0, 1]);
    }

    #[test]
    fn the_previous_bank_button_wraps_from_bank_zero_to_the_last_bank() {
        let mut interpreter = interpreter_with_a_bank_selector();

        interpreter.handle_ctrl(PREVIOUS_BANK_BUTTON, PRESS).unwrap();

        assert_eq!(interpreter.active_bank_index, 3);
    }

    #[test]
    fn the_selector_button_leds_spell_the_bank_index_in_binary() {
        let mut interpreter = interpreter_with_a_bank_selector();

        let indicator_writes_per_bank: Vec<Vec<Vec<u8>>> = (0..4).map(|_| {
            let responses = press_next_bank_button(&mut interpreter);
            device_writes_in(&responses[..2]).to_vec()
        }).collect();

        assert_eq!(indicator_writes_per_bank, vec![
            vec![vec![PREVIOUS_BANK_BUTTON, 0x00], vec![NEXT_BANK_BUTTON, 0x7f]],
            vec![vec![PREVIOUS_BANK_BUTTON, 0x7f], vec![NEXT_BANK_BUTTON, 0x00]],
            vec![vec![PREVIOUS_BANK_BUTTON, 0x7f], vec![NEXT_BANK_BUTTON, 0x7f]],
            vec![vec![PREVIOUS_BANK_BUTTON, 0x00], vec![NEXT_BANK_BUTTON, 0x00]],
        ]);
    }

    #[test]
    fn releasing_a_bank_button_is_handled_but_emits_nothing() {
        let mut interpreter = interpreter_with_a_bank_selector();

        let responses = interpreter.handle_ctrl(NEXT_BANK_BUTTON, RELEASE).unwrap();

        assert!(responses.is_empty());
        assert_eq!(interpreter.active_bank_index, 0);
    }

    #[test]
    fn a_bank_switch_with_nothing_held_sends_no_midi_and_no_osc() {
        let mut interpreter = interpreter_with_a_bank_selector();

        let responses = press_next_bank_button(&mut interpreter);

        assert!(midi_messages_in(&responses).is_empty());
        assert!(osc_addresses_in(&responses).is_empty());
    }

    #[test]
    fn each_bank_remembers_a_banked_knob_independently() {
        let mut interpreter = interpreter_with_a_bank_selector();

        interpreter.handle_ctrl(64, 40).unwrap();

        let writes_entering_bank_1 = device_writes_in(&press_next_bank_button(&mut interpreter));
        assert!(writes_entering_bank_1.contains(&vec![64, 0]));

        for _ in 0..2 {
            press_next_bank_button(&mut interpreter);
        }

        let writes_returning_to_bank_0 = device_writes_in(&press_next_bank_button(&mut interpreter));
        assert!(writes_returning_to_bank_0.contains(&vec![64, 40]));
    }

    #[test]
    fn a_banked_knob_sends_the_active_banks_control_number() {
        let mut interpreter = interpreter_with_a_bank_selector();

        press_next_bank_button(&mut interpreter);
        let responses = interpreter.handle_ctrl(64, 5).unwrap();

        assert_eq!(
            midi_messages_in(&responses),
            vec![vec![MIDI_CC_STATUS_ON_CHANNEL_0, 21, 5]]
        );
    }

    #[test]
    fn an_unbanked_knob_keeps_its_single_control_number_across_a_bank_switch() {
        let mut interpreter = interpreter_with_a_bank_selector();

        let writes_entering_bank_1 = device_writes_in(&press_next_bank_button(&mut interpreter));
        assert!(!writes_entering_bank_1.iter().any(|write| write[0] == 65));

        let responses = interpreter.handle_ctrl(65, 5).unwrap();
        assert_eq!(
            midi_messages_in(&responses),
            vec![vec![MIDI_CC_STATUS_ON_CHANNEL_0, 65, 5]]
        );
    }

    #[test]
    fn a_banked_indicator_is_rewritten_even_when_both_banks_hold_the_same_value() {
        let mut interpreter = interpreter_with_a_bank_selector();

        let writes_entering_bank_1 = device_writes_in(&press_next_bank_button(&mut interpreter));

        assert!(writes_entering_bank_1.contains(&vec![64, 0]));
        assert!(writes_entering_bank_1.contains(&vec![112, 0x00]));
    }

    #[test]
    fn a_held_momentary_control_is_released_against_the_bank_being_left() {
        let mut interpreter = interpreter_with_a_bank_selector();

        interpreter.handle_ctrl(112, PRESS).unwrap();
        let responses = press_next_bank_button(&mut interpreter);

        let midi_messages = midi_messages_in(&responses);
        assert_eq!(midi_messages, vec![vec![MIDI_CC_STATUS_ON_CHANNEL_0, 30, 0x00]]);

        let writes_returning_to_bank_0 = device_writes_in(
            &interpreter.handle_ctrl(PREVIOUS_BANK_BUTTON, PRESS).unwrap());
        assert!(writes_returning_to_bank_0.contains(&vec![112, 0x00]));
    }

    #[test]
    fn the_release_following_a_forced_release_emits_nothing() {
        let mut interpreter = interpreter_with_a_bank_selector();

        interpreter.handle_ctrl(112, PRESS).unwrap();
        press_next_bank_button(&mut interpreter);

        let responses = interpreter.handle_ctrl(112, RELEASE).unwrap();

        assert_eq!(responses.len(), 1);
        assert!(device_writes_in(&responses).is_empty());
        assert!(midi_messages_in(&responses).is_empty());
        assert!(osc_addresses_in(&responses).is_empty());
    }

    #[test]
    fn a_held_toggle_control_is_not_force_released_by_a_bank_switch() {
        let mut interpreter = interpreter_with_a_bank_selector();

        interpreter.handle_ctrl(113, PRESS).unwrap();
        let responses = press_next_bank_button(&mut interpreter);

        assert!(midi_messages_in(&responses).is_empty());
    }

    #[test]
    fn inbound_midi_for_an_inactive_bank_is_stored_without_touching_the_indicator() {
        let mut interpreter = interpreter_with_a_bank_selector();

        let response = interpreter
            .handle_midi(&[MIDI_CC_STATUS_ON_CHANNEL_0, 21, 100])
            .unwrap();

        assert!(response.ctrl.is_none());
    }

    #[test]
    fn switching_to_a_bank_restores_the_value_that_arrived_inbound_for_it() {
        let mut interpreter = interpreter_with_a_bank_selector();

        interpreter.handle_midi(&[MIDI_CC_STATUS_ON_CHANNEL_0, 21, 100]).unwrap();
        let writes_entering_bank_1 = device_writes_in(&press_next_bank_button(&mut interpreter));

        assert!(writes_entering_bank_1.contains(&vec![64, 100]));
    }

    #[test]
    fn inbound_midi_for_the_active_bank_still_writes_the_indicator() {
        let mut interpreter = interpreter_with_a_bank_selector();

        let response = interpreter
            .handle_midi(&[MIDI_CC_STATUS_ON_CHANNEL_0, 20, 100])
            .unwrap();

        assert_eq!(response.ctrl.unwrap().data, vec![64, 100]);
    }
}
