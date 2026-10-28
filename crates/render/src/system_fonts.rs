//! The system's font files for the CSS families the game names, picked as a browser
//! on the same machine would: `sans-serif` is Arial (Helvetica's metric twin, on
//! every Mac and Windows install), Georgia and Impact are themselves. Nothing ships
//! with the binary; a machine without a file gets `None` and the caller goes without.

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
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const FONT_DIRECTORIES: &[&str] = &["/usr/share/fonts/truetype/msttcorefonts"];

/// Candidate file names (macOS's first, then Windows') for `family` in `style`, and
/// whether each is a true italic.
fn candidates(family: &str, style: FontStyle) -> Vec<(&'static str, bool)> {
    let black = style.weight >= 800;
    let bold = style.weight >= 600;
    match family.to_ascii_lowercase().as_str() {
        "system-ui" => {
            #[cfg(target_os = "macos")]
            return vec![("SFNS.ttf", false)];
            #[cfg(not(target_os = "macos"))]
            if bold {
                vec![("segoeuib.ttf", false), ("arialbd.ttf", false)]
            } else {
                vec![("segoeui.ttf", false), ("arial.ttf", false)]
            }
        }
        "monospace" => vec![("SFNSMono.ttf", false), ("consola.ttf", false)],
        "georgia" => match (bold, style.italic) {
            (true, true) => vec![("Georgia Bold Italic.ttf", true), ("georgiaz.ttf", true)],
            (true, false) => vec![("Georgia Bold.ttf", false), ("georgiab.ttf", false)],
            (false, true) => vec![("Georgia Italic.ttf", true), ("georgiai.ttf", true)],
            (false, false) => vec![("Georgia.ttf", false), ("georgia.ttf", false)],
        },
        "impact" => vec![("Impact.ttf", false), ("impact.ttf", false)],
        // Arial Black has no italic; the caller slants it.
        _ if black => vec![("Arial Black.ttf", false), ("ariblk.ttf", false)],
        _ => match (bold, style.italic) {
            (true, true) => vec![("Arial Bold Italic.ttf", true), ("arialbi.ttf", true)],
            (true, false) => vec![("Arial Bold.ttf", false), ("arialbd.ttf", false)],
            (false, true) => vec![("Arial Italic.ttf", true), ("ariali.ttf", true)],
            (false, false) => vec![("Arial.ttf", false), ("arial.ttf", false)],
        },
    }
}

/// The bytes of the system font for `family` in `style`, and whether it is a true
/// italic (an upright face for an italic request needs slanting).
pub fn system_font(family: &str, style: FontStyle) -> Option<(Vec<u8>, bool)> {
    candidates(family, style)
        .into_iter()
        .find_map(|(file, italic)| {
            FONT_DIRECTORIES.iter().find_map(|directory| {
                let path = std::path::Path::new(directory).join(file);
                std::fs::read(path).ok().map(|bytes| (bytes, italic))
            })
        })
}
