//! Images kept with memories (5.1).
//!
//! An image arrives as bytes: from a file, a pipe, the clipboard. It is read once,
//! downscaled, and stored as WebP in the folder's own file; the original is never
//! changed, moved or removed, and no loose file is written. (Jack, 9 Nov 2026: "no
//! files. all in the database"; "the images are for agents. humans are incidental".)
//!
//! The settings were chosen by eye and by reading on 10 Nov 2026, on real
//! screenshots: at 1568 px and quality 80 a person can't tell WebP from PNG, and an
//! agent still reads every line a full step smaller. 1568 px is also the most that
//! Claude models before 4.7 look at, so no model throws away what is stored.

use sha2::{Digest, Sha256};

/// The longest side an image is stored at. A smaller image is never scaled up.
pub const LONG_EDGE: u32 = 1568;
/// WebP quality.
pub const QUALITY: f32 = 80.0;
/// What every stored image is.
pub const MEDIA_TYPE: &str = "image/webp";
/// The largest file that will be read. A full-screen capture is 1 to 6 MB.
pub const MAX_INPUT_BYTES: usize = 40 * 1024 * 1024;
/// How many images one memory can carry. Five, as Claude Desktop allows in a
/// message (Jack, 10 Nov 2026): "adding replies with addl images forces the human
/// to think twice." Each one costs a reader about 1,750 tokens to look at.
pub const MAX_PER_MEMORY: usize = 5;

/// An image ready to store.
#[derive(Debug, Clone)]
pub struct Picture {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Of `data`, lowercase hex.
    pub sha256: String,
}

/// What a Claude model spends to look at an image: one token per 28 x 28 px patch.
pub fn tokens(width: u32, height: u32) -> u32 {
    width.div_ceil(28) * height.div_ceil(28)
}

/// An image as the text a web page or a model takes: `data:image/webp;base64,…`.
pub fn data_url(media_type: &str, data: &[u8]) -> String {
    use base64::Engine;
    format!("data:{};base64,{}", media_type, base64::engine::general_purpose::STANDARD.encode(data))
}

/// The bytes inside a `data:…;base64,…` address, as a web page hands over a pasted
/// or dropped image.
pub fn from_data_url(url: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let (head, body) = url.split_once(',').ok_or("That isn't an image Geniuz can read.")?;
    if !head.starts_with("data:") || !head.ends_with(";base64") {
        return Err("That isn't an image Geniuz can read.".to_string());
    }
    base64::engine::general_purpose::STANDARD
        .decode(body.trim())
        .map_err(|_| "That isn't an image Geniuz can read.".to_string())
}

/// Lowercase hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{:02x}", b)).collect()
}

/// Turn an image's bytes into what is stored: turned upright, at most `LONG_EDGE`
/// on its longest side, as WebP. An animation keeps its first frame.
pub fn prepare(bytes: &[u8]) -> Result<Picture, String> {
    use image::{DynamicImage, ImageDecoder, ImageReader};

    if bytes.is_empty() {
        return Err("That image is empty.".to_string());
    }
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(format!(
            "That image is {} MB; the most Geniuz reads is {} MB.",
            bytes.len() / (1024 * 1024),
            MAX_INPUT_BYTES / (1024 * 1024)
        ));
    }
    const NOT_AN_IMAGE: &str = "That isn't an image Geniuz can read. It reads PNG, JPEG, WebP, GIF, BMP and TIFF.";
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| NOT_AN_IMAGE.to_string())?;
    // A small file can claim an enormous canvas. Refuse before allocating it.
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    limits.max_alloc = Some(1024 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|_| NOT_AN_IMAGE.to_string())?;
    // A phone photo is stored sideways with a note saying which way is up. WebP
    // output drops the note, so turn the pixels now.
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut im = DynamicImage::from_decoder(decoder).map_err(|e| match e {
        image::ImageError::Limits(_) => "That image is too large to read: more than 20,000 pixels on a side.".to_string(),
        _ => NOT_AN_IMAGE.to_string(),
    })?;
    im.apply_orientation(orientation);

    let (w, h) = (im.width(), im.height());
    if w == 0 || h == 0 {
        return Err(NOT_AN_IMAGE.to_string());
    }
    let scale = (LONG_EDGE as f64 / w.max(h) as f64).min(1.0);
    if scale < 1.0 {
        let nw = ((w as f64 * scale).round() as u32).max(1);
        let nh = ((h as f64 * scale).round() as u32).max(1);
        im = im.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3);
    }
    let (width, height) = (im.width(), im.height());

    // Alpha only when the image uses it: an opaque image is smaller without.
    let encoded = if im.color().has_alpha() {
        let px = im.to_rgba8();
        webp::Encoder::from_rgba(&px, width, height).encode_simple(false, QUALITY).map(|m| m.to_vec())
    } else {
        let px = im.to_rgb8();
        webp::Encoder::from_rgb(&px, width, height).encode_simple(false, QUALITY).map(|m| m.to_vec())
    };
    let data = encoded.map_err(|e| format!("Couldn't store that image as WebP: {:?}", e))?;
    if data.is_empty() {
        return Err("Couldn't store that image as WebP.".to_string());
    }
    let sha256 = sha256_hex(&data);
    Ok(Picture { data, width, height, sha256 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let im = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 251) as u8, (y % 241) as u8, 120]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(im)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[test]
    fn a_large_image_is_stored_at_1568_on_its_long_side_as_webp() {
        let p = prepare(&png(3136, 1960)).unwrap();
        assert_eq!((p.width, p.height), (1568, 980));
        assert_eq!(&p.data[0..4], b"RIFF");
        assert_eq!(&p.data[8..12], b"WEBP");
        assert_eq!(p.sha256.len(), 64);
        // A tall one is held to the same long side.
        let tall = prepare(&png(1000, 4000)).unwrap();
        assert_eq!((tall.width, tall.height), (392, 1568));
    }

    #[test]
    fn a_small_image_is_never_scaled_up() {
        let p = prepare(&png(202, 180)).unwrap();
        assert_eq!((p.width, p.height), (202, 180));
    }

    #[test]
    fn the_same_bytes_always_store_the_same_bytes() {
        let src = png(800, 600);
        assert_eq!(prepare(&src).unwrap().sha256, prepare(&src).unwrap().sha256);
    }

    #[test]
    fn what_is_not_an_image_is_refused_in_plain_words() {
        assert!(prepare(b"").unwrap_err().contains("empty"));
        let e = prepare(b"just some words, not a picture").unwrap_err();
        assert!(e.contains("isn't an image"), "{e}");
        // A truncated PNG is refused, not stored broken.
        let mut cut = png(400, 300);
        cut.truncate(60);
        assert!(prepare(&cut).is_err());
    }

    #[test]
    fn a_data_address_gives_back_its_bytes() {
        let src = png(40, 30);
        let url = data_url("image/png", &src);
        assert!(url.starts_with("data:image/png;base64,"));
        assert_eq!(from_data_url(&url).unwrap(), src);
        for bad in ["", "hello", "data:image/png,plain", "data:image/png;base64,***"] {
            assert!(from_data_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn tokens_count_28_pixel_patches() {
        assert_eq!(tokens(1568, 980), 56 * 35);
        assert_eq!(tokens(1, 1), 1);
        assert_eq!(tokens(29, 28), 2);
    }
}
