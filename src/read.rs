//! A PNG to one standard Image ([`read`]).
use kinogaki_png::Samples;
use kinogaki_studio_api::color::SRGB_DISPLAY;
use kinogaki_studio_api::geometry::image::{ColorTag, Encoding, Format, Image, Rect, Sample};
use kinogaki_studio_api::{Error, FileItem, Imported, Input, Reading, standard};
use std::path::Path;
use std::sync::Arc;

/// The most pixels a file may have (2²⁸).
const MAX_PIXELS: u64 = 1 << 28;

/// `input` as one item holding its Image, named after the file.
pub(crate) fn read(input: &Input<'_>, reading: &Reading<'_>) -> Result<Imported, Error> {
    reading.check()?;
    let bytes = input.bytes()?;
    let at = |e: kinogaki_png::Error| Error::invalid(format!("{input}: {e}"));
    let decoded = kinogaki_png::decode(&bytes, MAX_PIXELS).map_err(at)?;
    let tag = match kinogaki_png::display_tag(&bytes).map_err(at)? {
        Some((_, Some(profile))) => ColorTag::Icc(profile.into()),
        Some((_, None)) => ColorTag::Space(SRGB_DISPLAY.into()),
        None => ColorTag::Unknown { linear: false },
    };
    reading.check()?;
    let image = image(decoded, tag).map_err(|e| Error::invalid(format!("{input}: {e}")))?;
    let name = (Path::new(input.name()).file_stem())
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    Ok(Imported {
        items: vec![FileItem::top(name, standard::image().wrap(image))],
        warnings: Vec::new(),
    })
}

/// `decoded` as an Image of one layer, `""`, its colors tagged `tag` and
/// straight.
fn image(decoded: kinogaki_png::Image, tag: ColorTag) -> Result<Image, Error> {
    let names: &[&str] = match decoded.channels {
        1 => &["Y"],
        2 => &["Y", "A"],
        3 => &["R", "G", "B"],
        _ => &["R", "G", "B", "A"],
    };
    let (sample, bytes) = match decoded.samples {
        Samples::U8(v) => (Sample::U8, v),
        Samples::U16(v) => (
            Sample::U16,
            v.iter().flat_map(|s| s.to_ne_bytes()).collect(),
        ),
        _ => return Err(Error::invalid("a PNG holds 8 or 16-bit samples")),
    };
    let format = Format {
        name: "".into(),
        channels: names.iter().map(|&n| Arc::from(n)).collect(),
        sample,
        encoding: Encoding::Color {
            tag,
            premultiplied: false,
        },
    };
    let window = Rect::new([0, 0], [decoded.width as i32, decoded.height as i32])?;
    Image::builder(window).plane(format, &bytes).build()
}
