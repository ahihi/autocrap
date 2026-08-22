use crate::autocrap::config::{Mapping, MidiSpec};

/// Where one control sends on a given bank, and the value it last held there.
#[derive(Debug)]
pub struct BankAssignment<TValue> {
    pub midi_target: Option<MidiSpec>,
    pub osc_address: String,
    pub remembered_value: TValue,
}

/// Builds one assignment per bank, repeating any interface the control did not bank.
///
/// An unbanked control gets a single assignment, so every handler can index the
/// active bank unconditionally rather than branching on whether banking applies.
pub fn build_bank_assignments<TValue: Clone>(
    mapping: &Mapping,
    bank_count: u8,
    initial_value: TValue,
) -> Vec<BankAssignment<TValue>> {
    let assignment_count = if mapping.is_banked() { bank_count as usize } else { 1 };

    (0..assignment_count).map(|bank_index| BankAssignment {
        // indexing is safe: validation already checked both lists against bank_count
        midi_target: mapping.bank_midi_targets.as_ref()
            .map(|targets| targets[bank_index])
            .or(mapping.midi),
        osc_address: mapping.bank_osc_addresses.as_ref()
            .map(|addresses| addresses[bank_index].clone())
            .unwrap_or_else(|| mapping.osc_addr()),
        remembered_value: initial_value.clone(),
    }).collect()
}
