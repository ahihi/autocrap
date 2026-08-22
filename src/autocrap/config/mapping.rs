use serde::{Serialize, Deserialize};

use super::{CtrlKind, MidiSpec, ReversalFilter};

/// One physical control: where the device reports it, and where it sends.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mapping {
    pub name: String,
    pub ctrl_in_sequence: Option<Vec<u8>>,
    pub ctrl_in_num: Option<u8>,
    pub ctrl_out_num: Option<u8>,
    pub ctrl_kind: CtrlKind,
    pub midi: Option<MidiSpec>,
    /// multiplier applied to each raw encoder tick before accumulating,
    /// for `Relative`/`Accumulate` controls. defaults to 1 (raw device
    /// behaviour) when omitted.
    #[serde(default)]
    pub step: Option<u8>,
    /// One MIDI target per bank, replacing `midi`. Its presence opts the
    /// control into banking.
    #[serde(default)]
    pub bank_midi_targets: Option<Vec<MidiSpec>>,
    /// One OSC address per bank, replacing the address derived from `name`.
    /// Its presence opts the control into banking.
    #[serde(default)]
    pub bank_osc_addresses: Option<Vec<String>>,
    /// Rejects the false reversals a worn `Relative` encoder reports. Omitted
    /// means every tick is believed, which is right for a healthy control.
    #[serde(default)]
    pub reversal_filter: Option<ReversalFilter>,
}

impl Mapping {
    /// Returns this mapping shifted to the given position within a `Range` expansion.
    pub fn index(&self, i: u8) -> Mapping {
        Mapping {
            name: self.name.replace("{i}", &i.to_string()),
            ctrl_in_sequence: self.ctrl_in_sequence.as_ref().map(|s| s.iter().map(|n| n+i).collect()),
            ctrl_in_num: self.ctrl_in_num.map(|n| n+i),
            ctrl_out_num: self.ctrl_out_num.map(|n| n+i),
            ctrl_kind: self.ctrl_kind,
            midi: self.midi.map(|m| m.index(i)),
            step: self.step,
            bank_midi_targets: self.bank_midi_targets.as_ref()
                .map(|targets| targets.iter().map(|target| target.index(i)).collect()),
            // offsetting an address is meaningless, so these take the same
            // {i} substitution `name` does
            bank_osc_addresses: self.bank_osc_addresses.as_ref()
                .map(|addresses| addresses.iter()
                    .map(|address| address.replace("{i}", &i.to_string()))
                    .collect()),
            // describes the physical encoder rather than its position in the
            // range, so it is copied rather than transformed
            reversal_filter: self.reversal_filter,
        }
    }

    /// Returns the OSC address this control sends on, derived from its name.
    pub fn osc_addr(&self) -> String {
        format!("/{}", self.name)
    }

    /// Whether this control sends somewhere different depending on the active bank.
    pub fn is_banked(&self) -> bool {
        self.bank_midi_targets.is_some() || self.bank_osc_addresses.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping_from_json(json: &str) -> Mapping {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn range_expansion_offsets_every_bank_midi_target() {
        let knob_range = mapping_from_json(r#"{
            "name": "knob{i}",
            "ctrl_in_num": 64,
            "ctrl_out_num": 64,
            "ctrl_kind": {"Relative": {"mode": "Accumulate"}},
            "bank_midi_targets": [
                {"channel": 0, "kind": "Cc", "num": 20},
                {"channel": 0, "kind": "Cc", "num": 40},
                {"channel": 0, "kind": "Cc", "num": 60},
                {"channel": 0, "kind": "Cc", "num": 80}
            ]
        }"#);

        let second_knob = knob_range.index(1);
        let target_numbers: Vec<u8> = second_knob.bank_midi_targets.unwrap()
            .iter().map(|target| target.num).collect();

        assert_eq!(target_numbers, vec![21, 41, 61, 81]);
    }

    #[test]
    fn range_expansion_substitutes_the_index_into_every_bank_osc_address() {
        let knob_range = mapping_from_json(r#"{
            "name": "knob{i}",
            "ctrl_in_num": 64,
            "ctrl_out_num": 64,
            "ctrl_kind": {"Relative": {"mode": "Accumulate"}},
            "bank_osc_addresses": [
                "/bank0/knob{i}", "/bank1/knob{i}", "/bank2/knob{i}", "/bank3/knob{i}"
            ]
        }"#);

        let third_knob = knob_range.index(2);

        assert_eq!(
            third_knob.bank_osc_addresses.unwrap(),
            vec!["/bank0/knob2", "/bank1/knob2", "/bank2/knob2", "/bank3/knob2"]
        );
    }

    #[test]
    fn a_mapping_with_no_per_bank_field_is_not_banked() {
        let master_volume = mapping_from_json(r#"{
            "name": "masterVolume",
            "ctrl_in_num": 66,
            "ctrl_out_num": 66,
            "ctrl_kind": {"Relative": {"mode": "Accumulate"}},
            "midi": {"channel": 0, "kind": "Cc", "num": 66}
        }"#);

        assert!(!master_volume.is_banked());
    }
}
