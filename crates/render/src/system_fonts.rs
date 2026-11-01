//! The system's font files for the CSS families the game names, picked as a browser
//! on the same machine would: `sans-serif` is Arial (Helvetica's metric twin, on
//! every Mac, Windows and iPad install), Georgia and Impact are themselves. Nothing
//! ships with the binary; a machine without a file gets `None` and the caller goes
//! without.

/// The weight and slant a CSS font asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontStyle {
    /// 400 regular, 700 bold, 900 black.
    pub weight: u16,
    pub italic: bool,
}

/// Font directories, searched in order.
#[cfg(target_os = "macos")]
const FONT_DIRECTORIES: &[&str] = &[
    "/System/Library/Fonts/Supplemental",
    "/System/Library/Fonts",
    "/Library/Fonts",
];
#[cfg(target_os = "windows")]
const FONT_DIRECTORIES: &[&str] = &["C:\\Windows\\Fonts"];
/// iOS keeps the system face in `Core` and the web's families in `WebFonts`.
#[cfg(target_os = "ios")]
const FONT_DIRECTORIES: &[&str] = &[
    "/System/Library/Fonts/Core",
    "/System/Library/Fonts/CoreUI",
    "/System/Library/Fonts/WebFonts",
];
/// A page has no font files to read.
#[cfg(target_arch = "wasm32")]
const FONT_DIRECTORIES: &[&str] = &[];
#[cfg(not(any(
    target_os = "macos",
    target_os = "windows",
    target_os = "ios",
    target_arch = "wasm32"
)))]
const FONT_DIRECTORIES: &[&str] = &["/usr/share/fonts/truetype/msttcorefonts"];

/// Candidate file names (macOS's first, then Windows', then iOS's) for `family` in
/// `style`, and whether each is a true italic.
fn candidates(family: &str, style: FontStyle) -> Vec<(&'static str, bool)> {
    let black = style.weight >= 800;
    let bold = style.weight >= 600;
    match family.to_ascii_lowercase().as_str() {
        "system-ui" => {
            #[cfg(target_os = "macos")]
            return vec![("SFNS.ttf", false)];
            #[cfg(target_os = "ios")]
            return vec![("SFUI.ttf", false)];
            #[cfg(not(any(target_os = "macos", target_os = "ios")))]
            if bold {
                vec![("segoeuib.ttf", false), ("arialbd.ttf", false)]
            } else {
                vec![("segoeui.ttf", false), ("arial.ttf", false)]
            }
        }
        "monospace" => vec![
            ("SFNSMono.ttf", false),
            ("consola.ttf", false),
            ("SFUIMono.ttf", false),
        ],
        "georgia" => match (bold, style.italic) {
            (true, true) => vec![
                ("Georgia Bold Italic.ttf", true),
                ("georgiaz.ttf", true),
                ("GeorgiaBoldItalic.ttf", true),
            ],
            (true, false) => vec![
                ("Georgia Bold.ttf", false),
                ("georgiab.ttf", false),
                ("GeorgiaBold.ttf", false),
            ],
            (false, true) => vec![
                ("Georgia Italic.ttf", true),
                ("georgiai.ttf", true),
                ("GeorgiaItalic.ttf", true),
            ],
            (false, false) => vec![("Georgia.ttf", false), ("georgia.ttf", false)],
        },
        "impact" => vec![("Impact.ttf", false), ("impact.ttf", false)],
        // Arial Black has no italic; the caller slants it. iOS has no Arial Black, and
        // its bold is the nearest face.
        _ if black => vec![
            ("Arial Black.ttf", false),
            ("ariblk.ttf", false),
            ("ArialBold.ttf", false),
        ],
        _ => match (bold, style.italic) {
            (true, true) => vec![
                ("Arial Bold Italic.ttf", true),
                ("arialbi.ttf", true),
                ("ArialBoldItalic.ttf", true),
            ],
            (true, false) => vec![
                ("Arial Bold.ttf", false),
                ("arialbd.ttf", false),
                ("ArialBold.ttf", false),
            ],
            (false, true) => vec![
                ("Arial Italic.ttf", true),
                ("ariali.ttf", true),
                ("ArialItalic.ttf", true),
            ],
            (false, false) => vec![("Arial.ttf", false), ("arial.ttf", false)],
        },
    }
}

/// The bytes of the system font for `family` in `style`, and whether it is a true
/// italic (an upright face for an italic request needs slanting).
pub fn system_font(family: &str, style: FontStyle) -> Option<(Vec<u8>, bool)> {
    // The iOS simulator runs on the Mac's file system, with the simulated system
    // under this root.
    let root = std::env::var_os("IPHONE_SIMULATOR_ROOT");
    candidates(family, style)
        .into_iter()
        .find_map(|(file, italic)| {
            FONT_DIRECTORIES.iter().find_map(|directory| {
                let directory = match &root {
                    Some(root) => {
                        std::path::Path::new(root).join(directory.trim_start_matches('/'))
                    }
                    None => std::path::PathBuf::from(directory),
                };
                std::fs::read(directory.join(file))
                    .ok()
                    .map(|bytes| (bytes, italic))
            })
        })
}
