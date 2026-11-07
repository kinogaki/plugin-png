//! PNG as a Studio plugin library (`formats/png.dylib`).
use kinogaki_studio_api::export_package;

kinogaki_shared::link!();
export_package!(kinogaki_png_format::package);
