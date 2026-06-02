//! Minimal JPEG segment surgery for metadata preservation.
//!
//! The `image` crate decodes JPEG pixels but discards application segments on
//! re-encode, so a decode→rotate→encode round-trip loses the ICC color profile
//! and any EXIF orientation tag. After rotating we therefore splice a fresh EXIF
//! orientation tag (always normalized to Top-Left) and re-attach the source's
//! ICC profile directly into the re-encoded byte stream.
//!
//! This is a deliberately small, allocation-light JPEG marker walker, not a
//! general metadata library: it only reads APP2 `ICC_PROFILE` chunks and writes
//! APP1 EXIF / APP2 ICC segments right after the SOI marker, which is where
//! decoders expect them.

/// EXIF orientation value for Top-Left (no rotation), the normalized target.
pub const ORIENTATION_TOP_LEFT: u16 = 1;

/// ICC payload bytes per APP2 segment.
///
/// An APPn segment length field is a `u16` covering the 2 length bytes plus the
/// payload, and our payload also carries the 12-byte `ICC_PROFILE\0` tag and a
/// 2-byte chunk header, so the profile slice per segment is capped well under
/// 65533 to stay within the marker limit.
const ICC_CHUNK_SIZE: usize = 65000;

/// Read and reassemble the embedded ICC profile from a JPEG.
///
/// Profiles larger than one segment are split across sequential APP2
/// `ICC_PROFILE` chunks; this walks every marker, collects the chunks in
/// sequence order, and concatenates them. Returns `None` when no ICC segment is
/// present.
pub fn read_icc_profile(jpeg: &[u8]) -> Option<Vec<u8>> {
	let mut chunks: Vec<(u8, Vec<u8>)> = Vec::new();
	let mut i = 2; // skip SOI

	while i + 4 <= jpeg.len() {
		if jpeg[i] != 0xFF {
			break;
		}
		let marker = jpeg[i + 1];
		if marker == 0xDA {
			break; // start of scan; no metadata beyond here
		}
		let len = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
		let seg_start = i + 4;
		let seg_end = i + 2 + len;
		if seg_end > jpeg.len() {
			break;
		}

		if marker == 0xE2 && jpeg[seg_start..].starts_with(b"ICC_PROFILE\0") {
			// Layout: "ICC_PROFILE\0" (12) + seq (1) + total (1) + data.
			let seq = jpeg.get(seg_start + 12).copied().unwrap_or(1);
			let data_start = seg_start + 14;
			if data_start <= seg_end {
				chunks.push((seq, jpeg[data_start..seg_end].to_vec()));
			}
		}

		i = seg_end;
	}

	if chunks.is_empty() {
		return None;
	}

	chunks.sort_by_key(|(seq, _)| *seq);
	let mut profile = Vec::new();
	for (_, data) in chunks {
		profile.extend_from_slice(&data);
	}
	Some(profile)
}

/// Rebuild a freshly encoded JPEG with normalized EXIF orientation and, when
/// provided, a re-attached ICC profile.
///
/// `encoded` must be a JPEG produced without metadata (as the `image` crate
/// emits). The returned stream inserts an APP1 EXIF segment carrying
/// orientation = 1 followed by the ICC APP2 chunk(s), immediately after the SOI
/// marker. If `encoded` is not a recognizable JPEG it is returned unchanged.
pub fn attach_metadata(encoded: Vec<u8>, icc: Option<&[u8]>) -> Vec<u8> {
	if encoded.len() < 2 || encoded[0] != 0xFF || encoded[1] != 0xD8 {
		return encoded;
	}

	let mut out = Vec::with_capacity(encoded.len() + icc.map_or(0, <[u8]>::len) + 64);
	out.extend_from_slice(&encoded[0..2]); // SOI

	append_exif_orientation(&mut out, ORIENTATION_TOP_LEFT);
	if let Some(profile) = icc {
		append_icc_profile(&mut out, profile);
	}

	out.extend_from_slice(&encoded[2..]);
	out
}

/// Append an APP1 EXIF segment with a single Orientation entry.
fn append_exif_orientation(out: &mut Vec<u8>, orientation: u16) {
	// Big-endian (MM) TIFF block with one IFD0 entry.
	let mut tiff = Vec::with_capacity(26);
	tiff.extend_from_slice(b"MM");
	tiff.extend_from_slice(&[0x00, 0x2A]); // TIFF magic
	tiff.extend_from_slice(&[0x00, 0x00, 0x00, 0x08]); // offset to IFD0
	tiff.extend_from_slice(&[0x00, 0x01]); // one entry
	tiff.extend_from_slice(&[0x01, 0x12]); // tag: Orientation
	tiff.extend_from_slice(&[0x00, 0x03]); // type: SHORT
	tiff.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]); // count: 1
	tiff.extend_from_slice(&orientation.to_be_bytes()); // value in high 2 bytes
	tiff.extend_from_slice(&[0x00, 0x00]); // value padding
	tiff.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // next IFD offset

	let mut payload = Vec::with_capacity(6 + tiff.len());
	payload.extend_from_slice(b"Exif\0\0");
	payload.extend_from_slice(&tiff);

	append_app_segment(out, 0xE1, &payload);
}

/// Append the ICC profile as one or more sequential APP2 `ICC_PROFILE` chunks.
fn append_icc_profile(out: &mut Vec<u8>, profile: &[u8]) {
	let chunks: Vec<&[u8]> = if profile.is_empty() {
		vec![&[][..]]
	} else {
		profile.chunks(ICC_CHUNK_SIZE).collect()
	};
	let total = chunks.len().min(255) as u8;

	for (idx, chunk) in chunks.iter().enumerate() {
		let seq = (idx + 1).min(255) as u8;
		let mut payload = Vec::with_capacity(14 + chunk.len());
		payload.extend_from_slice(b"ICC_PROFILE\0");
		payload.push(seq);
		payload.push(total);
		payload.extend_from_slice(chunk);
		append_app_segment(out, 0xE2, &payload);
	}
}

/// Append a single APPn segment (`marker` is the low byte, e.g. 0xE1 for APP1).
fn append_app_segment(out: &mut Vec<u8>, marker: u8, payload: &[u8]) {
	let seg_len = (payload.len() + 2) as u16;
	out.push(0xFF);
	out.push(marker);
	out.extend_from_slice(&seg_len.to_be_bytes());
	out.extend_from_slice(payload);
}

/// Read the EXIF orientation tag from a JPEG, if present. Used in tests.
#[cfg(test)]
pub fn read_orientation(jpeg: &[u8]) -> Option<u16> {
	let mut i = 2;
	while i + 4 <= jpeg.len() {
		if jpeg[i] != 0xFF {
			return None;
		}
		let marker = jpeg[i + 1];
		if marker == 0xDA {
			return None;
		}
		let len = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
		let seg_start = i + 4;
		let seg_end = i + 2 + len;
		if seg_end > jpeg.len() {
			return None;
		}
		if marker == 0xE1 && jpeg[seg_start..].starts_with(b"Exif\0\0") {
			return parse_orientation(&jpeg[seg_start + 6..seg_end]);
		}
		i = seg_end;
	}
	None
}

#[cfg(test)]
fn parse_orientation(tiff: &[u8]) -> Option<u16> {
	if tiff.len() < 8 || &tiff[0..2] != b"MM" {
		return None;
	}
	let ifd_offset = u32::from_be_bytes([tiff[4], tiff[5], tiff[6], tiff[7]]) as usize;
	if ifd_offset + 2 > tiff.len() {
		return None;
	}
	let entry_count = u16::from_be_bytes([tiff[ifd_offset], tiff[ifd_offset + 1]]) as usize;
	let mut entry = ifd_offset + 2;
	for _ in 0..entry_count {
		if entry + 12 > tiff.len() {
			return None;
		}
		if u16::from_be_bytes([tiff[entry], tiff[entry + 1]]) == 0x0112 {
			return Some(u16::from_be_bytes([tiff[entry + 8], tiff[entry + 9]]));
		}
		entry += 12;
	}
	None
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn round_trips_orientation_and_icc() {
		// A minimal valid JPEG: SOI + EOI is enough for marker walking.
		let encoded = vec![0xFF, 0xD8, 0xFF, 0xD9];
		let icc = vec![1u8, 2, 3, 4, 5];

		let out = attach_metadata(encoded, Some(&icc));

		assert_eq!(read_orientation(&out), Some(ORIENTATION_TOP_LEFT));
		assert_eq!(read_icc_profile(&out).as_deref(), Some(icc.as_slice()));
	}

	#[test]
	fn no_icc_when_absent() {
		let encoded = vec![0xFF, 0xD8, 0xFF, 0xD9];
		let out = attach_metadata(encoded, None);
		assert_eq!(read_orientation(&out), Some(ORIENTATION_TOP_LEFT));
		assert_eq!(read_icc_profile(&out), None);
	}
}
