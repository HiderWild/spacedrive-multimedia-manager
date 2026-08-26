use crate::{
	consts,
	error::{Error, Result},
	generic::GenericHandler,
	pdf::PdfHandler,
	svg::SvgHandler,
	ImageHandler,
};
use image::DynamicImage;
use std::{
	ffi::{OsStr, OsString},
	path::Path,
};

#[cfg(feature = "heif")]
use crate::heif::HeifHandler;

pub fn format_image(path: impl AsRef<Path>) -> Result<DynamicImage> {
	let path = path.as_ref();
	match_to_handler(path.extension())?.handle_image(path)
}

/// Load an image optimized for thumbnail generation.
///
/// When the `turbojpeg` feature is enabled, JPEG/JPG sources are decoded with
/// libjpeg-turbo and an optional scale so the intermediate bitmap stays near
/// `min_edge` (long edge). Other formats fall back to [`format_image`].
pub fn format_image_for_thumbnail(
	path: impl AsRef<Path>,
	min_edge: u32,
) -> Result<DynamicImage> {
	let path = path.as_ref();
	#[cfg(feature = "turbojpeg")]
	{
		let is_jpeg = path
			.extension()
			.and_then(|e| e.to_str())
			.map(|e| {
				let e = e.to_ascii_lowercase();
				e == "jpg" || e == "jpeg"
			})
			.unwrap_or(false);
		if is_jpeg {
			match crate::jpeg_fast::decode_jpeg(path, Some(min_edge.max(1))) {
				Ok(img) => return Ok(img),
				Err(e) => {
					tracing::debug!(
						path = %path.display(),
						error = %e,
						"turbojpeg path failed; falling back to image crate"
					);
				}
			}
		}
	}
	let _ = min_edge;
	format_image(path)
}

pub fn convert_image(path: impl AsRef<Path>, desired_ext: &OsStr) -> Result<DynamicImage> {
	let path = path.as_ref();
	match_to_handler(path.extension())?.convert_image(match_to_handler(Some(desired_ext))?, path)
}

#[allow(clippy::useless_let_if_seq)]
fn match_to_handler(ext: Option<&OsStr>) -> Result<Box<dyn ImageHandler>> {
	let ext = ext.map(OsStr::to_ascii_lowercase).unwrap_or_default();
	let mut handler: Option<Box<dyn ImageHandler>> = None;

	if consts::GENERIC_EXTENSIONS
		.iter()
		.map(OsString::from)
		.any(|x| x == ext)
	{
		handler = Some(Box::new(GenericHandler {}));
	}

	#[cfg(feature = "heif")]
	if consts::HEIF_EXTENSIONS
		.iter()
		.map(OsString::from)
		.any(|x| x == ext)
	{
		handler = Some(Box::new(HeifHandler {}));
	}

	if consts::SVG_EXTENSIONS
		.iter()
		.map(OsString::from)
		.any(|x| x == ext)
	{
		handler = Some(Box::new(SvgHandler {}));
	}

	if consts::PDF_EXTENSIONS
		.iter()
		.map(OsString::from)
		.any(|x| x == ext)
	{
		handler = Some(Box::new(PdfHandler {}));
	}

	handler.ok_or(Error::Unsupported)
}
