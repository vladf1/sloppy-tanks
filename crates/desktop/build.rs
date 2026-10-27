//! Embeds the game's runtime files from `public/` (textures, sounds, the menu's tank
//! previews and the icon) as a sorted table `src/assets.rs` looks paths up in, and on
//! Windows gives the executable its icon.

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

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let public = manifest
        .join("../../public")
        .canonicalize()
        .expect("public/ exists");
    let mut files = Vec::new();
    for folder in EMBEDDED {
        let directory = public.join(folder);
        println!("cargo:rerun-if-changed={}", directory.display());
        collect(&directory, &mut files);
    }
    let mut entries: Vec<(String, PathBuf)> = files
        .into_iter()
        .map(|path| {
            let relative = path.strip_prefix(&public).expect("under public/");
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
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets it"));
    std::fs::write(out.join("assets.rs"), table).expect("OUT_DIR is writable");

    // The executable's icon, from the web app's (Windows' largest icon size is 256).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        #[cfg(windows)]
        {
            let icon = image::open(public.join("icons/icon-512.png"))
                .expect("the app icon decodes")
                .resize(256, 256, image::imageops::FilterType::Lanczos3);
            let path = out.join("icon.ico");
            icon.save_with_format(&path, image::ImageFormat::Ico)
                .expect("the icon encodes");
            let mut resource = winresource::WindowsResource::new();
            resource.set_icon(path.to_str().expect("a UTF-8 path"));
            resource.compile().expect("the icon resource compiles");
        }
    }
}
