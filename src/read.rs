//! A PNG to one standard Image ([`read`]).
use kinogaki_png::Samples;
use kinogaki_studio_api::color::SRGB_DISPLAY;
use kinogaki_studio_api::geometry::image::{ColorTag, Encoding, Format, Image, Rect, Sample};
use kinogaki_studio_api::{Diagnostic, Error, FileItem, Imported, Input, Reading, standard};
use std::path::Path;
use std::sync::Arc;

/// The most pixels a file may have (2²⁸).
const MAX_PIXELS: u64 = 1 << 28;

/// `input` as one item holding its Image, named after the file. What
/// decoding takes is checked against the read's budget before anything is
/// inflated: the compressed data's expansion, the inflated rows, the decoded
/// samples and the Image. A color profile that can't be read leaves the
/// colors unknown, said.
pub(crate) fn read(input: &Input<'_>, reading: &Reading<'_>) -> Result<Imported, Error> {
    reading.check()?;
    let bytes = input.bytes()?;
    let at = |e: &dyn std::fmt::Display| Error::invalid(format!("{input}: {e}"));
    let png = kinogaki_png::Png::read(&bytes).map_err(|e| at(&e))?;
    let (width, height) = (png.width(), png.height());
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_PIXELS {
        return Err(at(&format_args!("{width} × {height} pixels, over 2²⁸")));
    }
    let window = Rect::new([0, 0], [width as i32, height as i32]).map_err(|e| at(&e))?;
    let samples = pixels * u64::from(png.channels());
    let held = samples * if png.wide() { 2 } else { 1 };
    let budget = &reading.budget;
    let charged = budget
        .expand(png.compressed() as u64, png.inflated() as u64)
        .and_then(|()| budget.allocate(png.inflated() as u64))
        .and_then(|()| budget.allocate(samples * 2))
        .and_then(|()| budget.allocate(held));
    charged.map_err(|e| match e.is_cancelled() {
        true => e,
        false => at(&e.message()),
    })?;
    let stop = budget.cancel().shared_flag();
    let decoded = png.decode(&stop).map_err(|e| match e {
        kinogaki_png::Error::Stopped => Error::cancelled(),
        e => at(&e),
    })?;
    let mut warnings = Vec::new();
    let tag = match kinogaki_png::display_tag(&bytes) {
        Ok(Some((_, Some(profile)))) => ColorTag::Icc(profile.into()),
        Ok(Some((_, None))) => ColorTag::Space(SRGB_DISPLAY.into()),
        Ok(None) => ColorTag::Unknown { linear: false },
        Err(e) => {
            warnings.push(Diagnostic::warning(format!(
                "{input}: its color profile can't be read ({e}), so its colors are unknown"
            )));
            ColorTag::Unknown { linear: false }
        }
    };
    reading.check()?;
    let image = image(decoded, tag, window).map_err(|e| at(&e.message()))?;
    let name = (Path::new(input.name()).file_stem())
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    Ok(Imported {
        items: vec![FileItem::top(name, standard::image().wrap(image))],
        warnings,
    })
}

/// `decoded` as an Image of one layer, `""`, its colors tagged `tag` and
/// straight.
fn image(decoded: kinogaki_png::Image, tag: ColorTag, window: Rect) -> Result<Image, Error> {
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
    Image::builder(window).plane(format, &bytes).build()
}
