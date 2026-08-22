use super::{AbstractMapping, BankSelectorMapping, Config, CtrlKind, Mapping};

/// The smallest and largest `bank_count` a config may ask for.
///
/// The ceiling exists because the bank is displayed as a two-bit number on
/// the two selector buttons' LEDs, so beyond four banks the user is flying
/// blind. It moves only if a different feedback mechanism is designed.
const SMALLEST_ALLOWED_BANK_COUNT: u8 = 2;
const LARGEST_ALLOWED_BANK_COUNT: u8 = 4;

impl Config {
    /// Rejects a config whose banking setup cannot work, before any hardware is touched.
    ///
    /// Every rule fails loudly: silently ignoring a malformed banking config
    /// produces a device that looks broken for reasons the user cannot see.
    pub fn validate(&self) -> Result<(), String> {
        let bank_selector = self.validated_bank_selector()?;

        let Some(bank_selector) = bank_selector else {
            return self.reject_banked_control_without_a_selector();
        };

        self.reject_out_of_range_bank_count(&bank_selector)?;
        self.reject_selector_buttons_sharing_a_number(&bank_selector)?;
        self.reject_selector_buttons_claimed_by_a_control(&bank_selector)?;

        for abstract_mapping in self.mappings.iter() {
            for mapping in abstract_mapping.expand_iter() {
                reject_bank_list_of_the_wrong_length(&mapping, bank_selector.bank_count)?;
                reject_banked_eight_bit_control(&mapping)?;
                reject_control_with_both_a_single_and_per_bank_midi_target(&mapping)?;
            }
        }

        Ok(())
    }

    fn reject_banked_control_without_a_selector(&self) -> Result<(), String> {
        for abstract_mapping in self.mappings.iter() {
            for mapping in abstract_mapping.expand_iter() {
                if mapping.is_banked() {
                    return Err(format!(
                        "mapping {:?} sets a per-bank field, but the config declares no \
                         BankSelector mapping to cycle banks with",
                        mapping.name
                    ));
                }
            }
        }

        Ok(())
    }

    fn validated_bank_selector(&self) -> Result<Option<BankSelectorMapping>, String> {
        let declared_bank_selectors: Vec<BankSelectorMapping> = self.mappings.iter()
            .filter_map(|mapping| match mapping {
                AbstractMapping::BankSelector(bank_selector) => Some(*bank_selector),
                _ => None,
            })
            .collect();

        if declared_bank_selectors.len() > 1 {
            return Err(format!(
                "a config may declare at most one BankSelector mapping, but {} were found",
                declared_bank_selectors.len()
            ));
        }

        Ok(declared_bank_selectors.into_iter().next())
    }

    fn reject_out_of_range_bank_count(
        &self,
        bank_selector: &BankSelectorMapping
    ) -> Result<(), String> {
        let requested_bank_count = bank_selector.bank_count;

        if requested_bank_count < SMALLEST_ALLOWED_BANK_COUNT
            || requested_bank_count > LARGEST_ALLOWED_BANK_COUNT
        {
            return Err(format!(
                "bank_count must be between {} and {} inclusive, but is {}",
                SMALLEST_ALLOWED_BANK_COUNT, LARGEST_ALLOWED_BANK_COUNT, requested_bank_count
            ));
        }

        Ok(())
    }

    fn reject_selector_buttons_sharing_a_number(
        &self,
        bank_selector: &BankSelectorMapping
    ) -> Result<(), String> {
        let previous_button_number = bank_selector.previous_bank_button.input_control_number;
        let next_button_number = bank_selector.next_bank_button.input_control_number;

        if previous_button_number == next_button_number {
            return Err(format!(
                "previous_bank_button and next_bank_button must be different physical buttons, \
                 but both use input_control_number {}",
                previous_button_number
            ));
        }

        Ok(())
    }

    fn reject_selector_buttons_claimed_by_a_control(
        &self,
        bank_selector: &BankSelectorMapping
    ) -> Result<(), String> {
        let selector_button_numbers = [
            bank_selector.previous_bank_button.input_control_number,
            bank_selector.next_bank_button.input_control_number,
        ];

        // ranges are expanded first, since a Range of 16 starting at 112
        // silently occupies every number through 127
        for abstract_mapping in self.mappings.iter() {
            for mapping in abstract_mapping.expand_iter() {
                let numbers_this_control_reads_on: Vec<u8> = mapping.ctrl_in_num.into_iter()
                    .chain(mapping.ctrl_in_sequence.iter().flatten().copied())
                    .collect();

                for claimed_number in numbers_this_control_reads_on {
                    if selector_button_numbers.contains(&claimed_number) {
                        return Err(format!(
                            "control number {} drives a bank selector button, so mapping {:?} \
                             cannot also read on it",
                            claimed_number, mapping.name
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}

fn reject_bank_list_of_the_wrong_length(
    mapping: &Mapping,
    bank_count: u8
) -> Result<(), String> {
    if let Some(ref midi_targets) = mapping.bank_midi_targets {
        if midi_targets.len() != bank_count as usize {
            return Err(format!(
                "mapping {:?} lists {} bank_midi_targets, but bank_count is {}",
                mapping.name, midi_targets.len(), bank_count
            ));
        }
    }

    if let Some(ref osc_addresses) = mapping.bank_osc_addresses {
        if osc_addresses.len() != bank_count as usize {
            return Err(format!(
                "mapping {:?} lists {} bank_osc_addresses, but bank_count is {}",
                mapping.name, osc_addresses.len(), bank_count
            ));
        }
    }

    Ok(())
}

/// An `EightBit` control reports the live physical position of a slider, so it
/// holds no stored value a bank switch could restore. Repointing one while the
/// slider has not moved would leave the new target reading the old bank's value.
fn reject_banked_eight_bit_control(mapping: &Mapping) -> Result<(), String> {
    if matches!(mapping.ctrl_kind, CtrlKind::EightBit) && mapping.is_banked() {
        return Err(format!(
            "mapping {:?} is EightBit, which cannot be banked: a slider has no stored \
             value to restore on a bank switch",
            mapping.name
        ));
    }

    Ok(())
}

/// Per-bank targets replace `midi` rather than overriding it, so setting both is
/// an error rather than a precedence rule someone has to memorise.
fn reject_control_with_both_a_single_and_per_bank_midi_target(
    mapping: &Mapping
) -> Result<(), String> {
    if mapping.midi.is_some() && mapping.bank_midi_targets.is_some() {
        return Err(format!(
            "mapping {:?} sets both midi and bank_midi_targets; bank_midi_targets \
             replaces midi, so set only one",
            mapping.name
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_mappings(mappings_json: &str) -> Config {
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
        serde_json::from_str(&config_json).unwrap()
    }

    fn bank_selector_entry_json(
        bank_count: u8,
        previous_button_number: u8,
        next_button_number: u8
    ) -> String {
        format!(r#"{{"BankSelector": {{
            "bank_count": {},
            "previous_bank_button": {{"input_control_number": {}, "led_control_number": {}}},
            "next_bank_button": {{"input_control_number": {}, "led_control_number": {}}}
        }}}}"#,
        bank_count,
        previous_button_number, previous_button_number,
        next_button_number, next_button_number)
    }

    fn four_bank_selector_entry_json() -> String {
        bank_selector_entry_json(4, 126, 127)
    }

    #[test]
    fn two_bank_selectors_are_rejected() {
        let config = config_with_mappings(&format!(
            "[{}, {}]", four_bank_selector_entry_json(), four_bank_selector_entry_json()));

        assert!(config.validate().unwrap_err().contains("2 were found"));
    }

    #[test]
    fn a_bank_midi_target_list_shorter_than_bank_count_is_rejected() {
        let config = config_with_mappings(&format!(r#"[{}, {{"Single": {{
            "name": "knob0",
            "ctrl_in_num": 64,
            "ctrl_out_num": 64,
            "ctrl_kind": {{"Relative": {{"mode": "Accumulate"}}}},
            "bank_midi_targets": [
                {{"channel": 0, "kind": "Cc", "num": 20}},
                {{"channel": 0, "kind": "Cc", "num": 21}}
            ]
        }}}}]"#, four_bank_selector_entry_json()));

        let complaint = config.validate().unwrap_err();
        assert!(complaint.contains("knob0"), "{}", complaint);
        assert!(complaint.contains("2 bank_midi_targets"), "{}", complaint);
    }

    #[test]
    fn a_bank_osc_address_list_longer_than_bank_count_is_rejected() {
        let config = config_with_mappings(&format!(r#"[{}, {{"Single": {{
            "name": "knob0",
            "ctrl_in_num": 64,
            "ctrl_out_num": 64,
            "ctrl_kind": {{"Relative": {{"mode": "Accumulate"}}}},
            "bank_osc_addresses": ["/a", "/b", "/c", "/d", "/e"]
        }}}}]"#, four_bank_selector_entry_json()));

        let complaint = config.validate().unwrap_err();
        assert!(complaint.contains("knob0"), "{}", complaint);
        assert!(complaint.contains("5 bank_osc_addresses"), "{}", complaint);
    }

    #[test]
    fn a_banked_eight_bit_control_is_rejected() {
        let config = config_with_mappings(&format!(r#"[{}, {{"Single": {{
            "name": "xfader",
            "ctrl_in_sequence": [72, 73],
            "ctrl_kind": "EightBit",
            "bank_osc_addresses": ["/a", "/b", "/c", "/d"]
        }}}}]"#, four_bank_selector_entry_json()));

        let complaint = config.validate().unwrap_err();
        assert!(complaint.contains("xfader"), "{}", complaint);
        assert!(complaint.contains("EightBit"), "{}", complaint);
    }

    #[test]
    fn a_banked_control_without_a_bank_selector_is_rejected() {
        let config = config_with_mappings(r#"[{"Single": {
            "name": "knob0",
            "ctrl_in_num": 64,
            "ctrl_out_num": 64,
            "ctrl_kind": {"Relative": {"mode": "Accumulate"}},
            "bank_osc_addresses": ["/a", "/b", "/c", "/d"]
        }}]"#);

        let complaint = config.validate().unwrap_err();
        assert!(complaint.contains("knob0"), "{}", complaint);
        assert!(complaint.contains("BankSelector"), "{}", complaint);
    }

    #[test]
    fn a_control_setting_both_midi_and_bank_midi_targets_is_rejected() {
        let config = config_with_mappings(&format!(r#"[{}, {{"Single": {{
            "name": "knob0",
            "ctrl_in_num": 64,
            "ctrl_out_num": 64,
            "ctrl_kind": {{"Relative": {{"mode": "Accumulate"}}}},
            "midi": {{"channel": 0, "kind": "Cc", "num": 64}},
            "bank_midi_targets": [
                {{"channel": 0, "kind": "Cc", "num": 20}},
                {{"channel": 0, "kind": "Cc", "num": 21}},
                {{"channel": 0, "kind": "Cc", "num": 22}},
                {{"channel": 0, "kind": "Cc", "num": 23}}
            ]
        }}}}]"#, four_bank_selector_entry_json()));

        let complaint = config.validate().unwrap_err();
        assert!(complaint.contains("knob0"), "{}", complaint);
        assert!(complaint.contains("bank_midi_targets"), "{}", complaint);
    }

    #[test]
    fn a_bank_button_colliding_with_an_expanded_range_is_rejected() {
        let config = config_with_mappings(&format!(r#"[{}, {{"Range": {{
            "count": 16,
            "mapping": {{
                "name": "button{{i}}",
                "ctrl_in_num": 112,
                "ctrl_out_num": 112,
                "ctrl_kind": {{"OnOff": {{"mode": "Momentary"}}}},
                "midi": {{"channel": 0, "kind": "Cc", "num": 112}}
            }}
        }}}}]"#, bank_selector_entry_json(4, 120, 121)));

        let complaint = config.validate().unwrap_err();
        assert!(complaint.contains("120"), "{}", complaint);
        assert!(complaint.contains("button8"), "{}", complaint);
    }

    #[test]
    fn a_bank_button_colliding_with_a_control_input_sequence_is_rejected() {
        let config = config_with_mappings(&format!(r#"[{}, {{"Single": {{
            "name": "xfader",
            "ctrl_in_sequence": [72, 73],
            "ctrl_kind": "EightBit",
            "midi": {{"channel": 0, "kind": "Cc", "num": 72}}
        }}}}]"#, bank_selector_entry_json(4, 73, 127)));

        let complaint = config.validate().unwrap_err();
        assert!(complaint.contains("73"), "{}", complaint);
        assert!(complaint.contains("xfader"), "{}", complaint);
    }

    #[test]
    fn two_bank_buttons_sharing_one_control_number_are_rejected() {
        let config = config_with_mappings(&format!("[{}]", bank_selector_entry_json(4, 127, 127)));

        assert!(config.validate().unwrap_err().contains("127"));
    }

    #[test]
    fn a_bank_count_below_two_or_above_four_is_rejected() {
        let one_bank = config_with_mappings(&format!("[{}]", bank_selector_entry_json(1, 126, 127)));
        let five_banks = config_with_mappings(&format!("[{}]", bank_selector_entry_json(5, 126, 127)));

        assert!(one_bank.validate().unwrap_err().contains("is 1"));
        assert!(five_banks.validate().unwrap_err().contains("is 5"));
    }

    #[test]
    fn the_stock_midi_config_still_parses_and_validates() {
        let stock_config_json = include_str!("../../../config/nocturn-midi.json");
        let stock_config: Config = serde_json::from_str(stock_config_json).unwrap();

        assert_eq!(stock_config.validate(), Ok(()));
    }
}
