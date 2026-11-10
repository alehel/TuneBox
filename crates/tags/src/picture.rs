//! Turning an image the listener chose into a cover fit to put in their songs.
//!
//! JPEG and PNG are what every player shows, so they are kept byte for byte when they are a
//! sensible size. Other images TuneBox can read (GIF, BMP, WebP) are converted: to PNG when they
//! have transparency, else to JPEG. Anything larger than [`MAX_COVER_SIDE`] is scaled down,
//! because the cover is copied into every song of an album. Anything that is not an image is
//! refused before a single song is touched.

use crate::Artwork;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use std::io::Cursor;

/// The longest side a cover keeps. A 1400 px JPEG is a few hundred kilobytes and still sharp on
/// a large, dense screen; a 4000 px scan would add megabytes to each of an album's songs.
pub const MAX_COVER_SIDE: u32 = 1400;

/// JPEG quality for covers TuneBox encodes: indistinguishable from the original at cover size.
const JPEG_QUALITY: u8 = 90;

/// The largest image TuneBox agrees to decode, in pixels per side. Larger claims are far more
/// likely a damaged or hostile file than a cover, and decoding them could exhaust memory.
const MAX_DECODED_SIDE: u32 = 20_000;

/// Why an image cannot be a cover.
#[derive(Debug, thiserror::Error)]
pub enum PictureError {
    #[error("that file is not a picture TuneBox can use. Choose a JPEG, PNG, GIF, BMP or WebP image")]
    NotAnImage,
    #[error("the picture could not be read ({0}). Choose another image")]
    Unreadable(String),
}

/// Whether `data` starts like an image TuneBox can read. Cheap: it looks at the first bytes
/// only, so a frontend can refuse a wrong file at once; [`prepare_cover`] does the real work.
pub fn looks_like_image(data: &[u8]) -> bool {
    image::guess_format(data).is_ok_and(|format| format.reading_enabled())
}

/// `data`, an image file's contents, as a cover: JPEG or PNG, at most [`MAX_COVER_SIDE`] on
/// its longest side.
pub fn prepare_cover(data: &[u8]) -> Result<Artwork, PictureError> {
    let format = image::guess_format(data).ok().filter(|f| f.reading_enabled()).ok_or(PictureError::NotAnImage)?;
    let reader = || {
        let mut reader = ImageReader::with_format(Cursor::new(data), format);
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_DECODED_SIDE);
        limits.max_image_height = Some(MAX_DECODED_SIDE);
        reader.limits(limits);
        reader
    };
    let unreadable = |e: image::ImageError| PictureError::Unreadable(e.to_string());
    // Decoded even when the bytes are kept: a picture cut short still has a readable size, and
    // would otherwise go into every song as a broken cover.
    let mut image = reader().decode().map_err(unreadable)?;
    let fits = image.width().max(image.height()) <= MAX_COVER_SIDE;
    match format {
        ImageFormat::Jpeg if fits => return Ok(Artwork { mime: "image/jpeg".into(), data: data.to_vec() }),
        ImageFormat::Png if fits => return Ok(Artwork { mime: "image/png".into(), data: data.to_vec() }),
        _ => {}
    }
    if !fits {
        image = image.resize(MAX_COVER_SIDE, MAX_COVER_SIDE, FilterType::Lanczos3);
    }
    // A JPEG stays a JPEG; for the others, transparency decides, since JPEG has none.
    let as_png = format == ImageFormat::Png || (format != ImageFormat::Jpeg && image.color().has_alpha());
    encode(&image, as_png).map_err(unreadable)
}

fn encode(image: &DynamicImage, as_png: bool) -> Result<Artwork, image::ImageError> {
    let mut data = Vec::new();
    if as_png {
        image.write_to(&mut Cursor::new(&mut data), ImageFormat::Png)?;
        Ok(Artwork { mime: "image/png".into(), data })
    } else {
        JpegEncoder::new_with_quality(&mut data, JPEG_QUALITY).encode_image(&image.to_rgb8())?;
        Ok(Artwork { mime: "image/jpeg".into(), data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn image_file(width: u32, height: u32, format: ImageFormat, alpha: u8) -> Vec<u8> {
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(width, height, Rgba([200, 40, 90, alpha])));
        let image = if alpha == 255 { DynamicImage::ImageRgb8(image.to_rgb8()) } else { image };
        let mut data = Vec::new();
        image.write_to(&mut Cursor::new(&mut data), format).unwrap();
        data
    }

    fn size_of(artwork: &Artwork) -> (u32, u32) {
        let image = image::load_from_memory(&artwork.data).unwrap();
        (image.width(), image.height())
    }

    #[test]
    fn jpeg_and_png_of_a_sensible_size_are_kept_as_they_are() {
        for (format, mime) in [(ImageFormat::Jpeg, "image/jpeg"), (ImageFormat::Png, "image/png")] {
            let data = image_file(600, 600, format, 255);
            let cover = prepare_cover(&data).unwrap();
            assert_eq!(cover.mime, mime);
            assert_eq!(cover.data, data, "byte for byte");
        }
    }

    #[test]
    fn large_pictures_are_scaled_down_keeping_their_shape_and_format() {
        let cover = prepare_cover(&image_file(2800, 1400, ImageFormat::Jpeg, 255)).unwrap();
        assert_eq!((cover.mime.as_str(), size_of(&cover)), ("image/jpeg", (MAX_COVER_SIDE, 700)));
        let cover = prepare_cover(&image_file(1000, 3000, ImageFormat::Png, 128)).unwrap();
        assert_eq!(cover.mime, "image/png");
        assert_eq!(size_of(&cover).1, MAX_COVER_SIDE);
    }

    #[test]
    fn other_formats_become_jpeg_or_png_by_their_transparency() {
        let cover = prepare_cover(&image_file(300, 300, ImageFormat::Bmp, 255)).unwrap();
        assert_eq!((cover.mime.as_str(), size_of(&cover)), ("image/jpeg", (300, 300)));
        let cover = prepare_cover(&image_file(300, 200, ImageFormat::Gif, 0)).unwrap();
        assert_eq!((cover.mime.as_str(), size_of(&cover)), ("image/png", (300, 200)));
    }

    #[test]
    fn what_is_not_an_image_is_refused() {
        assert!(!looks_like_image(b"fLaC this is a song, not a picture"));
        assert!(matches!(prepare_cover(b"hello"), Err(PictureError::NotAnImage)));
        assert!(matches!(prepare_cover(&[]), Err(PictureError::NotAnImage)));
        // The right first bytes, then nothing: an error, not a crash.
        let mut cut = image_file(600, 600, ImageFormat::Png, 255);
        assert!(looks_like_image(&cut));
        cut.truncate(40);
        assert!(matches!(prepare_cover(&cut), Err(PictureError::Unreadable(_))));
    }
}
