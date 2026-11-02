//! The library's metadata: `index.toml` listing every group and instrument.

use std::collections::BTreeMap;
use std::path::PathBuf;

use nice_plug::prelude::*;

/// The default location of the sample library. Can be overridden with the `OGZI_LIB`
/// environment variable.
const DEFAULT_LIB_PATH: &str = "/home/shamil/devel/ogzi-music/lib";

/// The root directory of the sample library.
pub(crate) fn lib_root() -> PathBuf {
    std::env::var_os("OGZI_LIB")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_LIB_PATH))
}

/// The contents of the library's `index.toml`.
#[derive(Clone)]
pub(crate) struct Library {
    pub(crate) root: PathBuf,
    pub(crate) groups: Vec<Group>,
}

/// A group of instruments, e.g. brass or strings.
#[derive(Clone)]
pub(crate) struct Group {
    /// The group's slug, used to build paths into the library.
    pub(crate) slug: String,
    /// A human readable group name.
    pub(crate) name: String,
    pub(crate) instruments: Vec<InstrumentRef>,
}

/// An instrument listed in `index.toml`.
#[derive(Clone)]
pub(crate) struct InstrumentRef {
    /// The instrument's slug, used to build paths into the library.
    pub(crate) slug: String,
    /// The instrument's human readable name.
    pub(crate) name: String,
}

impl Library {
    /// Load `index.toml` from the library root. Returns an empty library on failure so the
    /// plugin still loads (and the GUI can show what went wrong).
    pub(crate) fn load() -> Library {
        let root = lib_root();
        let empty = Library {
            root: root.clone(),
            groups: Vec::new(),
        };

        let index_path = root.join("index.toml");
        let Ok(index_raw) = std::fs::read_to_string(&index_path) else {
            nice_error!("Ogzi: failed to read {}", index_path.display());
            return empty;
        };

        // index.toml is a table of `[group-slug]` tables of `instrument-slug = "Display Name"`.
        let Ok(index) = toml::from_str::<BTreeMap<String, BTreeMap<String, String>>>(&index_raw)
        else {
            nice_error!("Ogzi: failed to parse {}", index_path.display());
            return empty;
        };

        let groups = index
            .into_iter()
            .map(|(slug, instruments)| Group {
                name: capitalize(&slug),
                instruments: instruments
                    .into_iter()
                    .map(|(slug, name)| InstrumentRef { slug, name })
                    .collect(),
                slug,
            })
            .collect();

        Library { root, groups }
    }

    pub(crate) fn group(&self, index: usize) -> Option<&Group> {
        self.groups.get(index)
    }

    /// The greatest number of instruments in any group. The instrument parameter's range needs
    /// to be static, so this is used as its maximum.
    pub(crate) fn max_instruments(&self) -> usize {
        self.groups
            .iter()
            .map(|g| g.instruments.len())
            .max()
            .unwrap_or(0)
            .saturating_sub(1)
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_library_index() {
        let lib = Library::load();
        assert!(
            !lib.groups.is_empty(),
            "library at {} not found or empty — set OGZI_LIB",
            lib_root().display()
        );

        let brass = lib
            .groups
            .iter()
            .find(|g| g.slug == "brass")
            .expect("brass group");
        assert!(brass.instruments.iter().any(|i| i.slug == "trumpet-jazz"));
    }
}
