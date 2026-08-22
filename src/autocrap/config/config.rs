use serde::{Serialize, Deserialize};

use super::{AbstractMapping, BankSelectorMapping, Interface};

/// Everything autocrap needs to talk to one device: the whole config file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Human-readable label for this config, shown by front ends that let the
    /// user pick between several. Falls back to the filename when absent.
    #[serde(default)]
    pub name: Option<String>,
    pub vendor_id: u16,
    pub product_id: u16,
    pub in_endpoint: u8,
    pub out_endpoint: u8,
    pub interface: Interface,
    pub mappings: Vec<AbstractMapping>
}

impl Config {
    /// Every control number that drives an indicator, in ascending order.
    ///
    /// Includes the bank selector's own lights, which are not ordinary mappings but
    /// are still lights on the device.
    pub fn indicator_control_numbers(&self) -> Vec<u8> {
        let mut indicator_control_numbers: Vec<u8> = self.mappings.iter()
            .flat_map(|abstract_mapping| abstract_mapping.expand_iter())
            .filter_map(|mapping| mapping.ctrl_out_num)
            .collect();

        if let Some(bank_selector) = self.bank_selector() {
            indicator_control_numbers.push(bank_selector.previous_bank_button.led_control_number);
            indicator_control_numbers.push(bank_selector.next_bank_button.led_control_number);
        }

        indicator_control_numbers.sort();
        indicator_control_numbers.dedup();
        indicator_control_numbers
    }

    /// Returns the config's bank selector, if banking is configured at all.
    pub fn bank_selector(&self) -> Option<BankSelectorMapping> {
        self.mappings.iter().find_map(|mapping| match mapping {
            AbstractMapping::BankSelector(bank_selector) => Some(*bank_selector),
            _ => None,
        })
    }
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

    const BANK_SELECTOR_ENTRY_JSON: &str = r#"{"BankSelector": {
        "bank_count": 4,
        "previous_bank_button": {"input_control_number": 126, "led_control_number": 126},
        "next_bank_button": {"input_control_number": 127, "led_control_number": 127}
    }}"#;

    const KNOB_ENTRY_JSON: &str = r#"{"Single": {
        "name": "knob0",
        "ctrl_in_num": 64,
        "ctrl_out_num": 64,
        "ctrl_kind": {"Relative": {"mode": "Accumulate"}},
        "midi": {"channel": 0, "kind": "Cc", "num": 64}
    }}"#;

    #[test]
    fn the_bank_selector_is_found_wherever_it_appears_in_the_mappings() {
        let selector_first = config_with_mappings(
            &format!("[{}, {}]", BANK_SELECTOR_ENTRY_JSON, KNOB_ENTRY_JSON));
        let selector_last = config_with_mappings(
            &format!("[{}, {}]", KNOB_ENTRY_JSON, BANK_SELECTOR_ENTRY_JSON));

        assert_eq!(selector_first.bank_selector().unwrap().bank_count, 4);
        assert_eq!(selector_last.bank_selector().unwrap().bank_count, 4);
    }

    #[test]
    fn a_config_with_no_bank_selector_reports_none() {
        let unbanked_config = config_with_mappings(&format!("[{}]", KNOB_ENTRY_JSON));

        assert!(unbanked_config.bank_selector().is_none());
    }
}
