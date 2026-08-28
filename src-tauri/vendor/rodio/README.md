# Vendored copy -- read this first

This is rodio 0.19.0's source, copied into this repo (not a git submodule/fork) and patched
with two small added methods: `OutputStream::pause()` and `OutputStream::play()`, passing
straight through to the private underlying `cpal::Stream` (see `src/stream.rs`).

**Why this exists:** rodio's public API has no way to pause/resume the actual OS-level audio
stream -- `Sink::pause()` only stops feeding a sink real samples, it never tells the OS anything,
so an app using rodio looks like it's continuously "producing sound" to the OS (and to per-app
volume mixers reading the OS's own session state) for as long as the process is alive, even
while paused or with nothing loaded. This is a real, reported problem upstream --
<https://github.com/RustAudio/rodio/issues/782> -- where the original reporter drafted this
exact fix, but the maintainers preferred exploring a fancier "automatic" solution first; that
attempt stalled (their own words: "nontrivial... not going to be easy") and remains unresolved
over a year later. Waiting on it isn't practical, so we're using this exact fix locally instead.

**Maintenance note:** if a future rodio version needs to be adopted, this same two-method patch
needs to be manually reapplied to the new version's `stream.rs` first (or dropped entirely, if
issue #782 has been resolved upstream by then, in which case just depend on the plain `rodio`
crate from crates.io again and delete this directory).

Everything else in this directory is rodio's own unmodified source and license files.

---

# Audio playback library

[![Crates.io Version](https://img.shields.io/crates/v/rodio.svg)](https://crates.io/crates/rodio)
[![Crates.io Downloads](https://img.shields.io/crates/d/rodio.svg)](https://crates.io/crates/rodio)
[![Build Status](https://github.com/RustAudio/rodio/workflows/CI/badge.svg)](https://github.com/RustAudio/rodio/actions)

Rust playback library.

Playback is handled by [cpal](https://github.com/RustAudio/cpal). Format decoding can be handled either by [Symphonia](https://github.com/pdeljanov/Symphonia), or by format-specific decoders:

 - MP3 by [minimp3](https://github.com/lieff/minimp3) (but defaults to [Symphonia](https://github.com/pdeljanov/Symphonia)).
 - WAV by [hound](https://github.com/ruud-v-a/hound).
 - Vorbis by [lewton](https://github.com/est31/lewton).
 - FLAC by [claxon](https://github.com/ruuda/claxon).
 - MP4 and AAC (both disabled by default) are handled only by [Symphonia](https://github.com/pdeljanov/Symphonia).

See [the docs](https://docs.rs/rodio/latest/rodio/#alternative-decoder-backends) for more details on backends.

# [Documentation](http://docs.rs/rodio)

[The documentation](http://docs.rs/rodio) contains an introduction to the library.

## License
[License]: #license

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0), or
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### License of your contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
