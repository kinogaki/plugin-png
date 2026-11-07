//! One standard Image to a PNG ([`Writer`]).
use crate::IMAGE;
use kinogaki_png::{DisplayTag, Level, Samples};
use kinogaki_studio_api::color::SRGB_DISPLAY;
use kinogaki_studio_api::format::options;
use kinogaki_studio_api::geometry::image::{ColorTag, Encoding, Image, Sample};
use kinogaki_studio_api::params::{Parameter, Schema};
use kinogaki_studio_api::{
    DataTypeId, Diagnostic, Error, FileItem, FormatOptions, FormatWriter, Value, Writing, standard,
};
use std::path::Path;

/// The option naming the bits a sample is written with: `image` (as the
/// Image holds them: 8 for 8-bit samples, 16 for any wider), `8` or `16`.
const DEPTH: &str = "depth";
/// The option naming the compression's effort (`fast`, `balanced`, `small`).
const COMPRESSION: &str = "compression";

/// The writer.
pub(crate) struct Writer;

impl FormatWriter for Writer {
    fn accepts(&self) -> &[DataTypeId] {
        IMAGE
    }
    /// Its files hold a display's code values.
    fn display_referred(&self) -> bool {
        true
    }
    fn options(&self) -> FormatOptions {
        FormatOptions::from([
            (DEPTH.to_owned(), Value::Choice("image".into())),
            (COMPRESSION.to_owned(), Value::Choice("balanced".into())),
        ])
    }
    fn schema(&self, chosen: &FormatOptions) -> Schema {
        let key = |name| options::option(chosen, name, Value::choice).unwrap_or_default();
        let depth = [
            ("image", "As the Image"),
            ("8", "8 Bits"),
            ("16", "16 Bits"),
        ];
        let effort = [
            ("fast", "Fast"),
            ("balanced", "Balanced"),
            ("small", "Small"),
        ];
        Schema::default()
            .parameter(Parameter::choice(DEPTH, "Depth", &depth, key(DEPTH)))
            .parameter(Parameter::choice(
                COMPRESSION,
                "Compression",
                &effort,
                key(COMPRESSION),
            ))
    }
    /// The first item's Image (see the crate docs); what else the items
    /// hold is left out with a warning.
    fn write(
        &self,
        path: &Path,
        items: &[FileItem],
        writing: &Writing<'_>,
        chosen: &FormatOptions,
    ) -> Result<Vec<Diagnostic>, Error> {
        writing.check()?;
        let depth = options::option(chosen, DEPTH, Value::choice)?;
        let level = match options::option(chosen, COMPRESSION, Value::choice)? {
            "fast" => Level::Fast,
            "small" => Level::Small,
            _ => Level::Balanced,
        };
        let mut images =
            (items.iter().filter_map(|i| i.data.as_ref())).filter_map(|d| standard::image().get(d));
        let image = images
            .next()
            .ok_or_else(|| Error::invalid("no image to write"))?;
        let mut warnings = Vec::new();
        let more = images.count();
        if more > 0 {
            warnings.push(Diagnostic::warning(format!(
                "a PNG holds one image: {more} more left out"
            )));
        }
        let at = (image
            .layers()
            .iter()
            .position(|l| l.format().name.is_empty()))
        .unwrap_or(0);
        if image.layers().len() > 1 {
            warnings.push(Diagnostic::warning(format!(
                "a PNG holds one layer: {} more left out",
                image.layers().len() - 1
            )));
        }
        let sixteen = match depth {
            "8" => false,
            "16" => true,
            _ => image.layers()[at].format().sample != Sample::U8,
        };
        let png = pixels(image, at, sixteen)?;
        let tag = tag(image, at, &mut warnings);
        let tag = tag.as_ref().map(|(name, profile)| match profile {
            Some(profile) => DisplayTag::Icc { name, profile },
            None => DisplayTag::Srgb,
        });
        let bytes = kinogaki_png::encode_tagged(&png, level, tag)
            .map_err(|e| Error::invalid(format!("{}: {e}", path.display())))?;
        writing.check()?;
        let all = |file: &mut std::fs::File| std::io::Write::write_all(file, &bytes);
        (kinogaki_studio_api::format::files::replace(path, all))
            .map_err(|e| Error::invalid(format!("{}: {e}", path.display())))?;
        Ok(warnings)
    }
}

/// Layer `at` of `image` as the codec's image, straight, of 8 or
/// (`sixteen`) 16-bit samples.
fn pixels(image: &Image, at: usize, sixteen: bool) -> Result<kinogaki_png::Image, Error> {
    let layer = &image.layers()[at];
    let format = layer.format();
    let plane = image.plane(at).ok_or_else(|| {
        Error::invalid("an image whose pixels the host holds isn't written; read it back first")
    })?;
    let channels = format.channels.len();
    let alpha = channels == 2 || channels == 4;
    let premultiplied = matches!(
        format.encoding,
        Encoding::Color {
            premultiplied: true,
            ..
        }
    ) && alpha;
    let samples = match (format.sample, sixteen, premultiplied) {
        (Sample::U32, ..) => return Err(Error::invalid("a PNG holds no 32-bit integers")),
        (Sample::U8, false, false) => Samples::U8(plane),
        (Sample::U16, true, false) => Samples::U16(
            (plane.as_chunks::<2>().0.iter())
                .map(|b| u16::from_ne_bytes(*b))
                .collect(),
        ),
        _ => {
            let values = straight(&plane, format.sample, channels, premultiplied);
            let top = if sixteen { 65535.0 } else { 255.0 };
            let code = |v: f32| (v.clamp(0.0, 1.0) * top).round();
            match sixteen {
                true => Samples::U16(values.map(|v| code(v) as u16).collect()),
                false => Samples::U8(values.map(|v| code(v) as u8).collect()),
            }
        }
    };
    let [width, height] = image.data().size();
    let made = kinogaki_png::Image::new(width, height, channels as u8, samples);
    made.map_err(|e| Error::invalid(e.to_string()))
}

/// The values of `plane` (pixels of `channels` channels of `sample`), 0 to 1
/// for integers, each color divided by its alpha when `premultiplied`.
fn straight(
    plane: &[u8],
    sample: Sample,
    channels: usize,
    premultiplied: bool,
) -> impl Iterator<Item = f32> + '_ {
    let size = sample.bytes();
    plane.chunks_exact(channels * size).flat_map(move |pixel| {
        let alpha = sample.read(&pixel[(channels - 1) * size..]);
        let by = match premultiplied && alpha > 0.0 {
            true => 1.0 / alpha,
            false => 1.0,
        };
        (0..channels).map(move |k| match k + 1 == channels && premultiplied {
            true => alpha,
            false => sample.read(&pixel[k * size..]) * by,
        })
    })
}

/// What the file says of layer `at`'s colors: an `sRGB` chunk (no profile),
/// or an `iCCP` chunk's name and profile; none for data or a tag a PNG
/// can't hold, which is said.
fn tag(
    image: &Image,
    at: usize,
    warnings: &mut Vec<Diagnostic>,
) -> Option<(String, Option<Vec<u8>>)> {
    let Encoding::Color { tag, .. } = &image.layers()[at].format().encoding else {
        return None;
    };
    match tag {
        ColorTag::Space(space) if &**space == SRGB_DISPLAY => Some(("sRGB".into(), None)),
        ColorTag::Icc(profile) => Some((crate::icc::name(profile), Some(profile.to_vec()))),
        ColorTag::Unknown { .. } => None,
        held => {
            warnings.push(Diagnostic::warning(format!(
                "a PNG can't say its colors are {held}: written untagged"
            )));
            None
        }
    }
}
