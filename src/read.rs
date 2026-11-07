//! A PNG to one standard Image ([`read`]).
use kinogaki_png::Samples;
use kinogaki_studio_api::color::SRGB_DISPLAY;
use kinogaki_studio_api::geometry::image::{
    ColorTag, Encoding, Format, Image, Rect, Sample, TILE, Tile, tiles_of,
};
use kinogaki_studio_api::{Diagnostic, Error, FileItem, Imported, Input, Reading, standard};
use std::path::Path;
use std::sync::Arc;

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
    (reading.budget.pixels(u64::from(width), u64::from(height))).map_err(|e| {
        match e.is_cancelled() {
            true => e,
            false => at(&e.message()),
        }
    })?;
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
    let format = |sample| Format {
        name: "".into(),
        channels: names.iter().map(|&n| Arc::from(n)).collect(),
        sample,
        encoding: Encoding::Color {
            tag: tag.clone(),
            premultiplied: false,
        },
    };
    let built = Image::builder(window);
    match decoded.samples {
        Samples::U8(v) => built.plane(format(Sample::U8), &v).build(),
        Samples::U16(v) => {
            let format = format(Sample::U16);
            let tiles = tiles(&v, window.size(), usize::from(decoded.channels));
            built.tiles(format, tiles).build()
        }
        _ => Err(Error::invalid("a PNG holds 8 or 16-bit samples")),
    }
}

/// 16-bit `samples` (`channels` a pixel over `size`, rows from the top) as
/// the Image's tiles of native-order bytes, written a tile at a time: no
/// plane of bytes the size of the image besides them.
fn tiles(samples: &[u16], size: [u32; 2], channels: usize) -> Vec<Option<Tile>> {
    let [width, height] = size.map(|n| n as usize);
    let side = TILE as usize;
    let [across, down] = tiles_of(size).map(|n| n as usize);
    let mut tiles = Vec::with_capacity(across * down);
    for ty in 0..down {
        for tx in 0..across {
            let mut bytes = vec![0u8; side * side * channels * 2];
            let run = side.min(width - tx * side) * channels;
            for y in 0..side.min(height - ty * side) {
                let from = ((ty * side + y) * width + tx * side) * channels;
                let to = &mut bytes[y * side * channels * 2..][..run * 2];
                for (pair, sample) in to
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip(&samples[from..from + run])
                {
                    *pair = sample.to_ne_bytes();
                }
            }
            tiles.push(Some(Tile::new(bytes)));
        }
    }
    tiles
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 16-bit samples tile as a plane of their native bytes would.
    #[test]
    fn wide_samples_tile_as_their_plane_would() {
        let size = [70u32, 65];
        let samples: Vec<u16> = (0..70 * 65 * 3)
            .map(|k| (k * 7919 % 65_536) as u16)
            .collect();
        let format = Format {
            channels: ["R", "G", "B"].map(Arc::from).into(),
            ..Format::rgba(Sample::U16, Encoding::Raw)
        };
        let window = Rect::new([0, 0], [70, 65]).unwrap();
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_ne_bytes()).collect();
        let plane = Image::builder(window)
            .plane(format.clone(), &bytes)
            .build()
            .unwrap();
        let tiled = (Image::builder(window).tiles(format, tiles(&samples, size, 3)))
            .build()
            .unwrap();
        assert_eq!(tiled, plane);
    }
}
