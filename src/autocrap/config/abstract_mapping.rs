use serde::{Serialize, Deserialize};

use super::Mapping;

/// A config entry, which may describe one control or a whole run of them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AbstractMapping {
    Single(Mapping),
    Range {
        count: u8,
        mapping: Mapping
    }
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
            }
        };
        mappings.into_iter()
    }
}
