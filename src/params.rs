//! The plugin's parameters: the instrument group and the instrument.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use nice_plug::prelude::*;

use crate::library::Library;

#[derive(Params)]
pub(crate) struct OgziParams {
    #[id = "group"]
    pub(crate) group: IntParam,
    #[id = "instrument"]
    pub(crate) instrument: IntParam,
}

impl OgziParams {
    /// Create the parameters for the given library. `current_group` is kept in sync with the
    /// group parameter so the host can format instrument names for the current group.
    pub(crate) fn new(lib: Arc<Library>, current_group: Arc<AtomicI32>) -> Self {
        let max_group = lib.groups.len().saturating_sub(1) as i32;
        let max_inst = lib.max_instruments() as i32;

        let group_names = Arc::new(
            lib.groups
                .iter()
                .map(|g| g.name.clone())
                .collect::<Vec<_>>(),
        );
        let group = IntParam::new(
            "Group",
            0,
            IntRange::Linear {
                min: 0,
                max: max_group,
            },
        )
        .with_value_to_string(Arc::new(move |value| {
            group_names
                .get(value.max(0) as usize)
                .cloned()
                .unwrap_or_default()
        }));

        let inst_lib = lib;
        let instrument = IntParam::new(
            "Instrument",
            0,
            IntRange::Linear {
                min: 0,
                max: max_inst,
            },
        )
        .with_value_to_string(Arc::new(move |value| {
            let group = current_group.load(Ordering::Relaxed).max(0) as usize;
            match inst_lib.group(group) {
                Some(g) => g
                    .instruments
                    .get(value.max(0) as usize)
                    .map(|i| i.name.clone())
                    .unwrap_or_else(|| "—".to_string()),
                None => "—".to_string(),
            }
        }));

        Self { group, instrument }
    }
}
