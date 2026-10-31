//! Embeds the game's runtime files from `public/` (textures, sounds, the menu's tank
//! previews and the icon) in the native apps as a sorted table `src/assets.rs` looks
//! paths up in, on Windows gives the executable its icon, and on iOS a Mac-sized
//! main-thread stack. A page embeds none: its files have content-hashed names that
//! browsers keep across releases, where the Wasm downloads again with each one, and
//! `index.html` fetches them beside the Wasm (`scripts/hashed-assets.ts`).

use std::fmt::Write;
use std::path::{Path, PathBuf};

/// The `public/` folders the native game reads.
const EMBEDDED: &[&str] = &["textures", "audio", "previews", "icons"];

fn collect(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else if path
            .extension()
            .is_some_and(|extension| matches!(extension.to_str(), Some("webp" | "mp3" | "png")))
        {
            files.push(path);
        }
    }
}

/// Write the table of `EMBEDDED`'s files to `OUT_DIR/assets.rs`.
fn embed(public: &Path, out: &Path) {
    let mut files = Vec::new();
    for folder in EMBEDDED {
        let directory = public.join(folder);
        println!("cargo:rerun-if-changed={}", directory.display());
        collect(&directory, &mut files);
    }
    let mut entries: Vec<(String, PathBuf)> = files
        .into_iter()
        .map(|path| {
            let relative = path.strip_prefix(public).expect("under public/");
            let key = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            (key, path)
        })
        .collect();
    entries.sort();
    let mut table = String::from("pub static FILES: &[(&str, &[u8])] = &[\n");
    for (key, path) in &entries {
        writeln!(
            table,
            "    ({key:?}, include_bytes!({:?})),",
            path.display().to_string()
        )
        .expect("writing a String");
    }
    table.push_str("];\n");
    std::fs::write(out.join("assets.rs"), table).expect("OUT_DIR is writable");
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let public = manifest
        .join("../../public")
        .canonicalize()
        .expect("public/ exists");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets it"));
    let page = std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32");
    // Otherwise Cargo reruns this for any change in the package, a page's build too.
    println!("cargo:rerun-if-changed=build.rs");
    if !page {
        embed(&public, &out);
    }

    // iOS gives an app's main thread a 1 MB stack where macOS gives 8 MB, and the
    // engine's start-up (`Session::start` through `Game::create` to `Presentation::new`)
    // takes about 700 KB of it below UIKit's frames: the iPad app overflowed it at
    // launch. Its executable asks for the Mac's main-thread stack instead.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        println!("cargo:rustc-link-arg-bins=-Wl,-stack_size,0x800000");
    }
    // Wasm gives its one stack 1 MB, which the shell's start-up overflowed the same way.
    if page {
        println!("cargo:rustc-link-arg-bins=-zstack-size=0x800000");
    }

    // The executable's icon, from the web app's, at each size Windows draws it (taskbar,
    // title bar, Explorer's views up to 256), so none is scaled down from the largest.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        #[cfg(windows)]
        {
            use image::codecs::ico::{IcoEncoder, IcoFrame};
            let source =
                image::open(public.join("icons/icon-512.png")).expect("the app icon decodes");
            let frames: Vec<IcoFrame> = [16, 24, 32, 48, 64, 128, 256]
                .into_iter()
                .map(|size| {
                    let pixels = source
                        .resize_exact(size, size, image::imageops::FilterType::Lanczos3)
                        .into_rgba8();
                    IcoFrame::as_png(pixels.as_raw(), size, size, image::ExtendedColorType::Rgba8)
                        .expect("an icon frame encodes")
                })
                .collect();
            let path = out.join("icon.ico");
            let file = std::fs::File::create(&path).expect("OUT_DIR is writable");
            IcoEncoder::new(file)
                .encode_images(&frames)
                .expect("the icon encodes");
            let mut resource = winresource::WindowsResource::new();
            resource.set_icon(path.to_str().expect("a UTF-8 path"));
            resource.compile().expect("the icon resource compiles");
        }
    }
}
