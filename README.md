# ogzi

[![Crates.io](https://img.shields.io/crates/v/ogzi.svg)](https://crates.io/crates/ogzi)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A CLAP sampler plugin for the [Ogzi sample library](https://github.com/ogzi-music/lib),
built with [nice-plug](https://codeberg.org/RustAudio/nice-plug) and an [egui](https://github.com/emilk/egui) GUI.

It plays the library's FLAC samples in response to note events, picking the sample
for the pressed key and velocity (including the library's key ranges, velocity
layers, tuning, panning, loop points, and sample rates). The plugin has exactly
two settings: the instrument group and the instrument.

## Building

```sh
cargo build --release
```

## Installing

Copy the built library to your CLAP plugin directory, renaming it to have the
`.clap` extension:

```sh
mkdir -p ~/.clap
cp target/release/libogzi.so ~/.clap/ogzi.clap
```

(On macOS the artifact is a `.dylib`, on Windows a `.dll`.)

Alternatively, use the bundler: `cargo install cargo-nice-plug` and then
`cargo nice-plug bundle ogzi --release`.

## Sample library path

The plugin reads the library from `/home/shamil/devel/ogzi-music/lib` by
default. Point the `OGZI_LIB` environment variable at another copy of the
library to use a different location.

## Structure

| Module              | Contents                                      |
| ------------------- | --------------------------------------------- |
| `src/library.rs`    | `index.toml` parsing (groups and instruments) |
| `src/instrument.rs` | instrument metadata + FLAC decoding           |
| `src/voice.rs`      | sample playback voices                        |
| `src/params.rs`     | the two plugin parameters                     |
| `src/editor.rs`     | the egui GUI                                  |
| `src/plugin.rs`     | the plugin, note events, background loading   |
