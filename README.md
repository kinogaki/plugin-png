# PNG

PNG as a format plugin for [Kinogaki Studio](https://github.com/kinogaki): a PNG reads as one standard Image of the public [studio-sdk](https://github.com/kinogaki/studio-sdk), and an Image writes as one PNG. The codec is lib-image's `kinogaki-png`; this crate converts between its image and the SDK's, and converts no color: the host resolves what a file says of its colors.

The library crate is `kinogaki-png-format`; `plugin/` (`kinogaki-plugin-png`) is the dylib Studio loads, `formats/png.dylib`. Package `kinogaki.png`, format `kinogaki.format.png`.

## Reading

One layer, `""`: the file's channels (`Y`, `Y A`, `R G B` or `R G B A`; an indexed file as RGB, with alpha when its palette has transparency; `tRNS` adds alpha) of 8 or 16-bit samples, every bit depth and Adam7 interlacing, straight alpha. Its colors are tagged as the file says: an `sRGB` chunk as the sRGB display (`color::SRGB_DISPLAY`), an `iCCP` chunk as its profile, else nothing is said. A file in memory (an embedded one) reads as one on disk.

## Writing

The main layer (`""`, else the first) at the bit depth the `depth` option names (8 or 16; 16-bit samples rounded to 8 or 8-bit widened, floats clamped to 0–1), with the compression effort `compression` names (Fast, Balanced, Small). Colors premultiplied by alpha are divided by it. The tag the host gives is copied: the sRGB display as an `sRGB` chunk, a profile as an `iCCP` chunk named by the profile's own description. Other layers, a tag a PNG can't hold (a named space other than sRGB, primaries) and further images are left out, each said once; 32-bit integers are refused. The writer is display-referred: a host gives it a display's code values.

## Code

| File | Role |
|---|---|
| `src/lib.rs` | The package and the format |
| `src/read.rs` | A PNG to one Image |
| `src/write.rs` | One Image to a PNG, and the writer's options |
| `src/icc.rs` | An ICC profile's description, what an `iCCP` chunk is named by |

## Tests and data

`tests/format.rs` runs the format through the SDK's contract: every file of PngSuite reads as Pillow decodes it (`tools/pngsuite_oracle.py` writes `tests/fixtures/pngsuite/expected.txt`, grey transparency as the PNG specification says, since Pillow compares a low-bit grey file's transparent value unscaled), the corrupt ones fail naming the file; tags read and written; an sRGB image writes the bytes the codec writes and reads back; floats premultiplied are written straight at either depth; damaged and truncated files fail without panicking. The oracle runs locally (Pillow) and is never a dependency.

PngSuite (`tests/fixtures/pngsuite`) is Willem van Schaik's test suite (2017-07-19 release), freely distributable: "Permission to use, copy, modify and distribute these images for any purpose and without fee is hereby granted" ([PngSuite.LICENSE](tests/fixtures/pngsuite/PngSuite.LICENSE)).

## Building

The format builds from this repository, the public studio-sdk and lib-image, checked out side by side as Studio's tree lays them out (`sdk/`, `libs/lib-image/` and `plugins/png/`), with the compiler the SDK pins (`sdk/rust-toolchain.toml`):

```sh
cd plugins/png
cargo test
```

`plugin/` links Studio's shared library (`kinogaki_shared`), so it is built in Studio's tree, or against a Studio release's plugin kit as the SDK's guides describe. Studio's tree also runs the image formats' workflows (`workflows/`).

## License

Dual-licensed under the MIT license ([LICENSE-MIT](LICENSE-MIT)) and the Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE)), at your option.
