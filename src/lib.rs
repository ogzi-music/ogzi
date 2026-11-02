//! Ogzi — a minimal CLAP sampler plugin for the Ogzi sample library.
//!
//! The plugin reads the library's `index.toml` and plays the FLAC samples of the selected
//! instrument in response to note events. It has exactly two settings, exposed both as
//! parameters and in an egui GUI: the instrument group and the instrument.
//!
//! - [`library`] parses the library's `index.toml`.
//! - [`instrument`] decodes an instrument's metadata and FLAC samples.
//! - [`voice`] renders playing samples.
//! - [`params`] defines the plugin's parameters.
//! - [`editor`] contains the egui GUI.
//! - [`plugin`] ties everything together into the [`Ogzi`] plugin.

mod editor;
mod instrument;
mod library;
mod params;
mod plugin;
mod voice;

pub use plugin::Ogzi;

use nice_plug::nice_export_clap;

nice_export_clap!(Ogzi);
