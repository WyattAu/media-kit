//! Magic-byte format sniffing — zero dependencies, no decoding.
//!
//! Sniffing looks only at the first few dozen bytes of the buffer, so it is
//! cheap and safe to run on untrusted input before doing anything else.

/// Media formats recognized by [`sniff`].
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// JPEG (JFIF/EXIF), starts `FF D8 FF`.
    Jpeg,
    /// PNG, starts the 8-byte signature `89 50 4E 47 0D 0A 1A 0A`.
    Png,
    /// GIF87a / GIF89a.
    Gif,
    /// WebP (`RIFF....WEBP`).
    WebP,
    /// AVIF (ISOBMFF with `ftyp` brand `avif`).
    Avif,
    /// SVG (XML text starting `<?xml` or `<svg`).
    Svg,
    /// PDF (used to reject non-image uploads early).
    Pdf,
}

impl Format {
    /// Lowercase canonical extension for the format, e.g. `"jpg"`.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Format::Jpeg => "jpg",
            Format::Png => "png",
            Format::Gif => "gif",
            Format::WebP => "webp",
            Format::Avif => "avif",
            Format::Svg => "svg",
            Format::Pdf => "pdf",
        }
    }
}

/// Identify the media format from magic bytes. Returns [`None`] for unknown
/// or too-short input.
#[must_use]
pub fn sniff(bytes: &[u8]) -> Option<Format> {
    if bytes.len() >= 3 && bytes[..3] == [0xFF, 0xD8, 0xFF] {
        return Some(Format::Jpeg);
    }
    if bytes.len() >= 8 && bytes[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some(Format::Png);
    }
    if bytes.len() >= 6 && (&bytes[..6] == b"GIF87a" || &bytes[..6] == b"GIF89a") {
        return Some(Format::Gif);
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some(Format::WebP);
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && &bytes[8..12] == b"avif" {
        return Some(Format::Avif);
    }
    if starts_with(bytes, b"<?xml") || starts_with_ci(bytes, b"<svg") {
        return Some(Format::Svg);
    }
    if starts_with(bytes, b"%PDF") {
        return Some(Format::Pdf);
    }
    None
}

/// `true` when the bytes look like one of the raster/vector image formats
/// (everything except [`Format::Pdf`]).
#[must_use]
pub fn is_image(bytes: &[u8]) -> bool {
    matches!(sniff(bytes), Some(f) if f != Format::Pdf)
}

fn starts_with(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && &haystack[..needle.len()] == needle
}

fn starts_with_ci(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && haystack[..needle.len()].eq_ignore_ascii_case(needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_magic() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(Format::Jpeg));
    }

    #[test]
    fn jpeg_needs_three_bytes() {
        assert_eq!(sniff(&[0xFF, 0xD8]), None);
        assert_eq!(sniff(&[0xFF]), None);
    }

    #[test]
    fn png_magic() {
        let sig = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(sniff(&sig), Some(Format::Png));
        assert_eq!(sniff(&[]), None);
    }

    #[test]
    fn gif_variants() {
        assert_eq!(sniff(b"GIF87a rest"), Some(Format::Gif));
        assert_eq!(sniff(b"GIF89a rest"), Some(Format::Gif));
        assert_eq!(sniff(b"GIF88a"), None);
    }

    #[test]
    fn webp_magic() {
        let mut b = b"RIFF\x00\x00\x00\x00WEBPVP8 ".to_vec();
        assert_eq!(sniff(&b), Some(Format::WebP));
        b[8..12].copy_from_slice(b"MP3 ");
        assert_eq!(sniff(&b), None);
    }

    #[test]
    fn avif_magic() {
        let b = b"\x00\x00\x00\x20ftypavif\x00\x00\x00\x00avifmif1";
        assert_eq!(sniff(b), Some(Format::Avif));
        // Non-avif brand (heic) must not match.
        let h = b"\x00\x00\x00\x20ftypheic\x00\x00\x00\x00heicmif1";
        assert_eq!(sniff(h), None);
    }

    #[test]
    fn svg_magic() {
        assert_eq!(
            sniff(b"<?xml version=\"1.0\"?><svg xmlns=\"...\">"),
            Some(Format::Svg)
        );
        assert_eq!(sniff(b"<svg xmlns=\"...\">"), Some(Format::Svg));
        // Case-insensitive on the tag itself.
        assert_eq!(sniff(b"<SVG xmlns=\"...\">"), Some(Format::Svg));
    }

    #[test]
    fn pdf_magic() {
        assert_eq!(sniff(b"%PDF-1.7 ..."), Some(Format::Pdf));
        assert!(!is_image(b"%PDF-1.7 ..."));
    }

    #[test]
    fn garbage_is_none() {
        assert_eq!(sniff(b"not a media file at all"), None);
        assert_eq!(sniff(&[]), None);
        assert!(!is_image(b"garbage"));
    }

    #[test]
    fn is_image_true_for_rasters() {
        assert!(is_image(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(is_image(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]));
        assert!(is_image(b"GIF89a......"));
        assert!(is_image(b"RIFF\x12\x00\x00\x00WEBP"));
    }

    #[test]
    fn extensions() {
        assert_eq!(Format::Jpeg.extension(), "jpg");
        assert_eq!(Format::WebP.extension(), "webp");
    }
}
