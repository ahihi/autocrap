use serde::{Serialize, Deserialize};

use super::{BankSelectorMapping, Mapping};

/// A config entry, which may describe one control, a whole run of them, or the bank selector.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AbstractMapping {
    Single(Mapping),
    Range {
        count: u8,
        mapping: Mapping
    },
    BankSelector(BankSelectorMapping)
}

impl AbstractMapping {
    /// Yields the concrete controls this entry stands for.
    pub fn expand_iter(&self) -> impl Iterator<Item = Mapping> {
        let mut mappings = vec![];
        match self {
            AbstractMapping::Single(mapping) => mappings.push(mapping.index(0)),
            AbstractMapping::Range { count, mapping } => {
                for i in 0..*count {
                    mappings.push(mapping.index(i));
                }
            },
            // the bank selector is not a control; the Interpreter consumes it directly
            AbstractMapping::BankSelector(_) => {}
        };
        mappings.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BANK_SELECTOR_ENTRY_JSON: &str = r#"{"BankSelector": {
        "bank_count": 4,
        "previous_bank_button": {"input_control_number": 126, "led_control_number": 126},
        "next_bank_button": {"input_control_number": 127, "led_control_number": 127}
    }}"#;

    #[test]
    fn bank_selector_entry_deserialises_with_its_configured_buttons() {
        let entry: AbstractMapping = serde_json::from_str(BANK_SELECTOR_ENTRY_JSON).unwrap();

        let AbstractMapping::BankSelector(bank_selector) = entry else {
            panic!("expected a BankSelector entry, got {:?}", entry);
        };

        assert_eq!(bank_selector.bank_count, 4);
        assert_eq!(bank_selector.previous_bank_button.input_control_number, 126);
        assert_eq!(bank_selector.previous_bank_button.led_control_number, 126);
        assert_eq!(bank_selector.next_bank_button.input_control_number, 127);
        assert_eq!(bank_selector.next_bank_button.led_control_number, 127);
    }

    #[test]
    fn bank_selector_entry_expands_to_no_controls() {
        let entry: AbstractMapping = serde_json::from_str(BANK_SELECTOR_ENTRY_JSON).unwrap();

        assert_eq!(entry.expand_iter().count(), 0);
    }
}
