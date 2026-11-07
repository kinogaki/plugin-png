//! PNG, a format plugin: a PNG reads as one standard
//! [`Image`](kinogaki_studio_api::geometry::image::Image) as the file holds
//! it, and an Image writes as one PNG. The codec is lib-image's
//! `kinogaki-png`; this crate converts between its image and the SDK's.
//!
//! - **Reading.** One layer, `""`: the file's channels (`Y`, `Y A`,
//!   `R G B` or `R G B A`; an indexed file as RGB, with alpha when it has
//!   transparency) of 8 or 16-bit samples, straight alpha, tagged as the
//!   file says: an `sRGB` chunk is the sRGB display
//!   ([`SRGB_DISPLAY`](kinogaki_studio_api::color::SRGB_DISPLAY)), an
//!   `iCCP` chunk its profile, else nothing is said.
//! - **Writing.** The main layer (`""`, else the first), at the bit depth the
//!   `depth` option names, its color divided by its alpha when
//!   premultiplied, tagged from its [`ColorTag`]: the sRGB display as an
//!   `sRGB` chunk, a profile as an `iCCP` chunk named by the profile's own
//!   description. Other layers, and a tag PNG can't hold, are left out with
//!   a warning.
//!
//! [`ColorTag`]: kinogaki_studio_api::geometry::image::ColorTag
mod icc;
mod read;
mod write;

use kinogaki_studio_api::{
    DataTypeId, Error, FileFormat, FormatId, FormatOptions, FormatWriter, Imported, Input, Package,
    PackageId, Reading, standard,
};
use std::sync::LazyLock;

/// The package's id.
pub fn id() -> PackageId {
    PackageId::declared("kinogaki.png")
}

/// The format's id.
pub fn format() -> FormatId {
    FormatId::declared("kinogaki.format.png")
}

/// The package: the format alone.
pub fn package() -> Package {
    Package::new(id(), "PNG").with_format(Png)
}

/// What it reads and writes: the standard Image.
static IMAGE: LazyLock<DataTypeId> = LazyLock::new(|| standard::image().id());

/// The format.
pub struct Png;

impl FileFormat for Png {
    fn id(&self) -> FormatId {
        format()
    }
    fn label(&self) -> &str {
        "PNG"
    }
    fn extensions(&self) -> &[&str] {
        &["png"]
    }
    fn detects(&self, head: &[u8]) -> bool {
        head.starts_with(&kinogaki_png::SIGNATURE)
    }
    fn produces(&self) -> &[DataTypeId] {
        std::slice::from_ref(&*IMAGE)
    }
    fn read(
        &self,
        input: &Input<'_>,
        _: &FormatOptions,
        reading: &Reading<'_>,
    ) -> Result<Imported, Error> {
        read::read(input, reading)
    }
    fn writer(&self) -> Option<&dyn FormatWriter> {
        Some(&write::Writer)
    }
}
