//! The format through the SDK's contract: PngSuite against Pillow's decode
//! (`tools/pngsuite_oracle.py`), the tags it reads and writes, what its
//! writer converts, and damaged files.
use kinogaki_png::{DisplayTag, Level};
use kinogaki_png_format::Png;
use kinogaki_studio_api::color::SRGB_DISPLAY;
use kinogaki_studio_api::geometry::image::{ColorTag, Encoding, Format, Image, Rect, Sample};
use kinogaki_studio_api::{
    FileFormat, FileItem, Imported, Input, Purpose, Reading, Units, Value, Writing, standard,
};
use kinogaki_studio_api_testing::Unconverted;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn suite() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pngsuite")
}

fn read(input: &Input<'_>) -> Result<Imported, kinogaki_studio_api::Error> {
    let colors = Unconverted;
    let reading = Reading::new(Purpose::Value, Units::default(), &colors);
    Png.read(input, &Png.options(), &reading)
}

fn image(input: &Input<'_>) -> Image {
    let read = read(input).unwrap_or_else(|e| panic!("{input}: {e}"));
    assert_eq!(read.items.len(), 1);
    let data = read.items[0].data.as_ref().unwrap();
    standard::image().get(data).unwrap().clone()
}

/// A scratch file of its own, gone when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("kinogaki-png-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir.join(name))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// `image` written with `set` over the defaults: the file's bytes and the
/// warnings.
fn written(image: &Image, set: &[(&str, &str)]) -> (Vec<u8>, Vec<String>) {
    let writer = Png.writer().unwrap();
    let mut options = writer.options();
    for (name, key) in set {
        options.insert((*name).into(), Value::Choice((*key).into()));
    }
    let file = Scratch::new(&format!(
        "{}.png",
        std::thread::current()
            .name()
            .unwrap_or("t")
            .replace("::", "-")
    ));
    let colors = Unconverted;
    let item = FileItem::top("picture", standard::image().wrap(image.clone()));
    let warnings = writer
        .write(
            &file.0,
            &[item],
            &Writing::new(Units::default(), &colors),
            &options,
        )
        .unwrap();
    let said = warnings.iter().map(|w| w.message.clone()).collect();
    (std::fs::read(&file.0).unwrap(), said)
}

/// An Image of one layer.
fn picture(
    channels: &[&str],
    sample: Sample,
    tag: Option<ColorTag>,
    premultiplied: bool,
    bytes: &[u8],
) -> Image {
    let encoding = match tag {
        Some(tag) => Encoding::Color { tag, premultiplied },
        None => Encoding::Raw,
    };
    let format = Format {
        name: "".into(),
        channels: channels.iter().map(|&c| Arc::from(c)).collect(),
        sample,
        encoding,
    };
    let pixels = bytes.len() / format.pixel_bytes();
    let window = Rect::new([0, 0], [pixels as i32, 1]).unwrap();
    Image::builder(window).plane(format, bytes).build().unwrap()
}

/// FNV-1a, 64 bits: what the oracle hashes pixels with.
fn fnv(bytes: &[u8]) -> String {
    let hash = (bytes.iter()).fold(0xcbf2_9ce4_8422_2325_u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// Layer 0 of `image` as 8-bit RGBA (16-bit samples by their high byte,
/// grey spread), as the oracle hashes it.
fn rgba(image: &Image) -> Vec<u8> {
    let format = image.layers()[0].format();
    let plane = image.plane(0).unwrap();
    let size = format.sample.bytes();
    let byte = |b: &[u8]| match size {
        1 => b[0],
        _ => (u16::from_ne_bytes([b[0], b[1]]) >> 8) as u8,
    };
    let channels = format.channels.len();
    (plane.chunks_exact(channels * size))
        .flat_map(|px| {
            let c = |k: usize| byte(&px[k * size..]);
            match channels {
                1 => [c(0), c(0), c(0), 255],
                2 => [c(0), c(0), c(0), c(1)],
                3 => [c(0), c(1), c(2), 255],
                _ => [c(0), c(1), c(2), c(3)],
            }
        })
        .collect()
}

#[test]
fn every_pngsuite_file_reads_as_pillow_decodes_it_and_the_corrupt_ones_fail() {
    let expected = std::fs::read_to_string(suite().join("expected.txt")).unwrap();
    let mut differ = Vec::new();
    for line in expected.lines() {
        let fields: Vec<&str> = line.split(' ').collect();
        let path = suite().join(fields[0]);
        let input = Input::file(&path);
        if fields[1] == "error" {
            let why = read(&input).unwrap_err();
            assert!(
                why.message().starts_with(&path.display().to_string()),
                "{why}"
            );
            continue;
        }
        let image = image(&input);
        let size = [
            fields[1].parse::<u32>().unwrap(),
            fields[2].parse().unwrap(),
        ];
        assert_eq!(image.data().size(), size, "{}", fields[0]);
        if fnv(&rgba(&image)) != fields[3] {
            differ.push(fields[0]);
        }
        let encoding = &image.layers()[0].format().encoding;
        assert!(
            matches!(
                encoding,
                Encoding::Color {
                    premultiplied: false,
                    ..
                }
            ),
            "{}",
            fields[0]
        );
    }
    assert!(differ.is_empty(), "{differ:?}");
}

#[test]
fn a_file_says_its_display_and_keeps_its_channels_and_depth() {
    let grey16 = kinogaki_png::Image::new(
        2,
        1,
        2,
        kinogaki_png::Samples::U16(vec![1000, 65535, 2000, 0]),
    )
    .unwrap();
    let plain = kinogaki_png::encode(&grey16, Level::Fast).unwrap();
    let held = image(&Input::memory("grey.png", &plain));
    let format = held.layers()[0].format();
    assert_eq!(
        (&*format.channels[0], &*format.channels[1], format.sample),
        ("Y", "A", Sample::U16)
    );
    assert_eq!(held.read_pixel(0, 1, 0)[0], 2000.0 / 65535.0);
    assert_eq!(
        format.encoding,
        Encoding::Color {
            tag: ColorTag::Unknown { linear: false },
            premultiplied: false
        }
    );
    let rgba = kinogaki_png::Image::rgba8(1, 1, vec![1, 2, 3, 4]);
    let srgb = kinogaki_png::encode_tagged(&rgba, Level::Fast, Some(DisplayTag::Srgb)).unwrap();
    let tagged = image(&Input::memory("s.png", &srgb));
    assert_eq!(
        tagged.layers()[0].format().encoding.space(),
        Some(SRGB_DISPLAY)
    );
    let icc = DisplayTag::Icc {
        name: "Wide",
        profile: &[7; 64],
    };
    let profiled = kinogaki_png::encode_tagged(&rgba, Level::Fast, Some(icc)).unwrap();
    let held = image(&Input::memory("p.png", &profiled));
    let Encoding::Color {
        tag: ColorTag::Icc(profile),
        ..
    } = &held.layers()[0].format().encoding
    else {
        panic!("a profile");
    };
    assert_eq!(&**profile, &[7; 64]);
}

#[test]
fn an_srgb_image_writes_the_bytes_the_codec_writes_and_reads_back() {
    let bytes: Vec<u8> = (0..64u8).collect();
    let srgb = picture(
        &["R", "G", "B", "A"],
        Sample::U8,
        Some(ColorTag::Space(SRGB_DISPLAY.into())),
        false,
        &bytes,
    );
    let (file, warnings) = written(&srgb, &[]);
    assert!(warnings.is_empty(), "{warnings:?}");
    let codec = kinogaki_png::Image::rgba8(16, 1, bytes.clone());
    let expected =
        kinogaki_png::encode_tagged(&codec, Level::Balanced, Some(DisplayTag::Srgb)).unwrap();
    assert_eq!(file, expected);
    assert_eq!(image(&Input::memory("back.png", &file)), srgb);
}

#[test]
fn a_profile_is_named_by_its_description_and_a_named_space_is_said() {
    // A profile of one `desc` tag, version 4's localized text.
    let units: Vec<u8> = "Display P3"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    let mut profile = vec![0u8; 128];
    profile.extend(1u32.to_be_bytes());
    profile.extend(b"desc");
    profile.extend(144u32.to_be_bytes());
    profile.extend((28 + units.len() as u32).to_be_bytes());
    profile.extend(b"mluc\0\0\0\0");
    profile.extend(1u32.to_be_bytes());
    profile.extend(12u32.to_be_bytes());
    profile.extend(b"enUS");
    profile.extend((units.len() as u32).to_be_bytes());
    profile.extend(28u32.to_be_bytes());
    profile.extend(units);
    let tag = Some(ColorTag::Icc(profile.clone().into()));
    let (file, _) = written(
        &picture(&["R", "G", "B"], Sample::U8, tag, false, &[9; 6]),
        &[],
    );
    assert_eq!(
        kinogaki_png::display_tag(&file).unwrap(),
        Some(("Display P3".into(), Some(profile)))
    );
    let tag = Some(ColorTag::Space("ACEScg".into()));
    let (file, warnings) = written(
        &picture(&["R", "G", "B"], Sample::U8, tag, false, &[9; 6]),
        &[],
    );
    assert_eq!(kinogaki_png::display_tag(&file).unwrap(), None);
    assert_eq!(
        warnings,
        ["a PNG can't say its colors are ACEScg: written untagged"]
    );
}

#[test]
fn floats_premultiplied_are_written_straight_at_the_depth_asked() {
    let values: Vec<u8> = [0.25f32, 0.5, 0.0, 0.5, 2.0, -1.0, 0.0, 1.0]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    let premultiplied = picture(
        &["R", "G", "B", "A"],
        Sample::F32,
        Some(ColorTag::Unknown { linear: false }),
        true,
        &values,
    );
    let (eight, _) = written(&premultiplied, &[("depth", "8")]);
    let back = image(&Input::memory("8.png", &eight));
    assert_eq!(back.plane(0).unwrap(), [128, 255, 0, 128, 255, 0, 0, 255]);
    let (sixteen, _) = written(&premultiplied, &[("depth", "16")]);
    let back = image(&Input::memory("16.png", &sixteen));
    assert_eq!(back.layers()[0].format().sample, Sample::U16);
    let half = 32768.0 / 65535.0;
    assert_eq!(back.read_pixel(0, 0, 0), [half, 1.0, 0.0, half]);
}

#[test]
fn a_file_writes_back_at_its_own_depth() {
    for name in [
        "basn0g16", "basn2c16", "basn4a16", "basn6a16", "basn2c08", "basn0g04",
    ] {
        let read = image(&Input::file(&suite().join(format!("{name}.png"))));
        let (file, warnings) = written(&read, &[]);
        assert!(warnings.is_empty(), "{name}: {warnings:?}");
        let back = image(&Input::memory("back.png", &file));
        assert_eq!(back, read, "{name}");
    }
}

#[test]
fn what_a_png_cant_hold_is_said_and_ids_are_refused() {
    let format = |name: &str| Format {
        name: name.into(),
        channels: [Arc::from("Y")].into(),
        sample: Sample::U8,
        encoding: Encoding::Raw,
    };
    let window = Rect::new([0, 0], [1, 1]).unwrap();
    let two = Image::builder(window)
        .plane(format("depth"), &[1])
        .plane(format(""), &[2])
        .build()
        .unwrap();
    let (file, warnings) = written(&two, &[]);
    assert_eq!(warnings, ["a PNG holds one layer: 1 more left out"]);
    assert_eq!(
        image(&Input::memory("main.png", &file)).plane(0).unwrap(),
        [2]
    );
    let ids = picture(&["id"], Sample::U32, None, false, &7u32.to_ne_bytes());
    let file = Scratch::new("ids.png");
    let colors = Unconverted;
    let item = FileItem::top("ids", standard::image().wrap(ids));
    let writer = Png.writer().unwrap();
    let refused = writer.write(
        &file.0,
        &[item],
        &Writing::new(Units::default(), &colors),
        &writer.options(),
    );
    assert!(refused.unwrap_err().message().contains("32-bit"));
}

#[test]
fn a_damaged_or_truncated_file_is_an_error_naming_it() {
    let rgba = kinogaki_png::Image::rgba8(4, 4, vec![200; 64]);
    let good = kinogaki_png::encode(&rgba, Level::Fast).unwrap();
    for cut in [0, 8, 20, good.len() / 2, good.len() - 1] {
        let why = read(&Input::memory("cut.png", &good[..cut])).unwrap_err();
        assert!(why.message().starts_with("cut.png: "), "{why}");
    }
    let mut flipped = good.clone();
    flipped[40] ^= 0x55;
    assert!(read(&Input::memory("flipped.png", &flipped)).is_err());
    assert!(Png.detects(&good[..8]) && !Png.detects(b"\xFF\xD8\xFF"));
}
