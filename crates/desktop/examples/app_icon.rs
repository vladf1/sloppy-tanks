//! The macOS app icon from the web app's (`public/icons/icon-512.png`): Apple's shape,
//! a rounded square inside the 1024-pixel canvas with its margin and soft shadow, so
//! the Dock and Finder show it beside other apps rather than as a full-bleed square.
//! Writes an iconset folder for `iconutil` (`scripts/build-desktop.mjs` runs it):
//!
//! ```text
//! cargo run -p sloppy-desktop --example app_icon -- <AppIcon.iconset>
//! ```

use image::imageops::{self, FilterType};
use image::{Rgba, RgbaImage};

/// Apple's macOS icon grid at 1024 pixels: the body's size and corner radius, and its
/// shadow's drop, blur and strength.
const CANVAS: u32 = 1024;
const BODY: u32 = 824;
const RADIUS: f32 = 185.4;
const SHADOW_DROP: i64 = 10;
const SHADOW_BLUR: f32 = 12.0;
const SHADOW_ALPHA: f32 = 0.35;
/// The iconset's sizes, each at 1x and 2x.
const SIZES: [u32; 5] = [16, 32, 128, 256, 512];

/// Coverage of the pixel centred at `(x, y)` by a rounded square of `size` and `radius`
/// at the origin, antialiased over one pixel.
fn coverage(x: f32, y: f32, size: f32, radius: f32) -> f32 {
    let half = size / 2.0;
    let (dx, dy) = (
        (x - half).abs() - (half - radius),
        (y - half).abs() - (half - radius),
    );
    let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() + dx.max(dy).min(0.0);
    (radius - outside + 0.5).clamp(0.0, 1.0)
}

fn icon() -> RgbaImage {
    let source = image::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../public/icons/icon-512.png"
    ))
    .expect("the web app's icon decodes")
    .into_rgba8();
    let mut body = imageops::resize(&source, BODY, BODY, FilterType::Lanczos3);
    for (x, y, pixel) in body.enumerate_pixels_mut() {
        let cover = coverage(x as f32 + 0.5, y as f32 + 0.5, BODY as f32, RADIUS);
        pixel[3] = (f32::from(pixel[3]) * cover).round() as u8;
    }
    let inset = i64::from((CANVAS - BODY) / 2);
    let mut shadow = RgbaImage::new(CANVAS, CANVAS);
    for (x, y, pixel) in body.enumerate_pixels() {
        let alpha = f32::from(pixel[3]) * SHADOW_ALPHA;
        shadow.put_pixel(
            (i64::from(x) + inset) as u32,
            (i64::from(y) + inset + SHADOW_DROP) as u32,
            Rgba([0, 0, 0, alpha.round() as u8]),
        );
    }
    let mut canvas = imageops::blur(&shadow, SHADOW_BLUR);
    imageops::overlay(&mut canvas, &body, inset, inset);
    canvas
}

fn main() {
    let folder = std::env::args()
        .nth(1)
        .expect("the iconset folder to write");
    std::fs::create_dir_all(&folder).expect("the iconset folder can be made");
    let icon = icon();
    for size in SIZES {
        for scale in [1, 2] {
            let pixels = size * scale;
            let name = if scale == 2 {
                format!("icon_{size}x{size}@2x.png")
            } else {
                format!("icon_{size}x{size}.png")
            };
            imageops::resize(&icon, pixels, pixels, FilterType::Lanczos3)
                .save(std::path::Path::new(&folder).join(name))
                .expect("the icon saves");
        }
    }
}
