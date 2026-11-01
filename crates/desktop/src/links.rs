//! Links (`play-modes.ts`, `join-screen.ts`, `client.ts`): a page reads its query as the
//! app's command line (`main.rs`), and its address then follows Battle Setup's tab and
//! the room played, so a reload comes back there: `?multiplayer` on the Multiplayer tab,
//! `?room=<CODE>` in a room or back from one that ended, neither in single player. A
//! room's invite link is that address. The app has no address; its invite link opens
//! the web version.

use crate::tasks::Background;

/// Where the player is, as the page's address keeps it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place<'a> {
    Single,
    Multiplayer,
    Room(&'a str),
}

/// The page's query (without its `?`) as command-line arguments: `?debug&map=harbor`
/// reads as `--debug --map harbor`, values decoded as `URLSearchParams` decodes them.
#[cfg(any(target_arch = "wasm32", test))]
pub fn query_arguments(query: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = decoded(name);
        if !name.is_empty() {
            arguments.push(format!("--{name}"));
        }
        let value = decoded(value);
        if !value.is_empty() {
            arguments.push(value);
        }
    }
    arguments
}

/// A query component's text: `+` is a space, `%XX` a byte of UTF-8.
#[cfg(any(target_arch = "wasm32", test))]
fn decoded(component: &str) -> String {
    let bytes = component.as_bytes();
    let mut text = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%')
            .then(|| component.get(index + 1..index + 3))
            .flatten()
            .filter(|hex| hex.bytes().all(|digit| digit.is_ascii_hexdigit()))
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (bytes[index], escaped) {
            (_, Some(byte)) => {
                text.push(byte);
                index += 3;
                continue;
            }
            (b'+', None) => text.push(b' '),
            (byte, None) => text.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&text).into_owned()
}

/// `query` (without its `?`) as the address of `place`, its other parameters as they
/// were: a room names itself in place of the Multiplayer tab (`roomAddress`), the tab
/// drops the room it left, and single player drops both.
pub fn query_for(query: &str, place: Place) -> String {
    let mut pairs: Vec<String> = query
        .split('&')
        .filter(|pair| {
            let name = pair.split_once('=').map_or(*pair, |(name, _)| name);
            !pair.is_empty() && name != "multiplayer" && name != "room"
        })
        .map(str::to_owned)
        .collect();
    match place {
        Place::Single => {}
        Place::Multiplayer => pairs.push("multiplayer".into()),
        Place::Room(room) => pairs.push(format!("room={room}")),
    }
    pairs.join("&")
}

/// The web version an invite from the app opens: the site of the servers it lists.
#[cfg(not(target_arch = "wasm32"))]
fn web_site() -> &'static str {
    if crate::directory::ENVIRONMENT == "dev" {
        "https://dev.sloppytanks.com/"
    } else {
        "https://sloppytanks.com/"
    }
}

/// Keep the page's address on `place` without a new history entry; the app has none.
pub fn show(place: Place) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(window) = web_sys::window() else {
            return;
        };
        let (Ok(history), Some(address)) = (window.history(), address(&window, place)) else {
            return;
        };
        if window.location().href().ok().as_deref() != Some(address.as_str()) {
            let _ =
                history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&address));
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = place;
}

/// The page's own address as `place` keeps it, with its other parameters and fragment.
#[cfg(target_arch = "wasm32")]
fn address(window: &web_sys::Window, place: Place) -> Option<String> {
    let location = window.location();
    let query = query_for(location.search().ok()?.trim_start_matches('?'), place);
    let mut address = location.origin().ok()? + &location.pathname().ok()?;
    if !query.is_empty() {
        address = address + "?" + &query;
    }
    Some(address + &location.hash().ok()?)
}

/// The link that opens Battle Setup with `room` selected for a friend: in a page its
/// own address in the room (the old page's `location.href`), from the app the web
/// version's.
pub fn invite_link(room: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|window| address(&window, Place::Room(room)))
            .unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        format!("{}?{}", web_site(), query_for("", Place::Room(room)))
    }
}

/// The apps' App Store page, which the web version points to (App Store Connect's Apple
/// ID for the app; the page shows the app once Apple releases it).
#[cfg(target_arch = "wasm32")]
pub const APP_STORE: &str = "https://apps.apple.com/app/sloppy-tanks/id6829385069";

/// Open one of the site's own pages (`privacy.html`, `support.html`) in the browser: a
/// page opens it beside itself, under its own base, and the app opens the web version's.
pub fn open_site_page(context: &egui::Context, page: &str) {
    #[cfg(target_arch = "wasm32")]
    open_url(context, page);
    #[cfg(not(target_arch = "wasm32"))]
    open_url(context, &format!("{}{page}", web_site()));
}

/// Open `url` in the browser, from a page in a new tab.
pub fn open_url(context: &egui::Context, url: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = context;
        if let Some(window) = web_sys::window() {
            let _ = window.open_with_url_and_target(url, "_blank");
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    context.open_url(egui::OpenUrl::new_tab(url));
}

/// Put `text` on the clipboard; the answer says whether it took it. The app copies
/// through egui at once; a page writes the browser's clipboard, which may refuse (no
/// secure context, or the click too long ago for Safari).
pub fn copy_text(context: &egui::Context, text: String) -> Background<bool> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        context.copy_text(text);
        Background::ready(true)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = context;
        let clipboard = web_sys::window()
            .map(|window| window.navigator())
            .and_then(|navigator| js_sys::Reflect::get(&navigator, &"clipboard".into()).ok())
            .filter(|clipboard| !clipboard.is_undefined())
            .map(wasm_bindgen::JsCast::unchecked_into::<web_sys::Clipboard>);
        let Some(clipboard) = clipboard else {
            return Background::ready(false);
        };
        Background::spawn(async move {
            wasm_bindgen_futures::JsFuture::from(clipboard.write_text(&text))
                .await
                .is_ok()
        })
    }
}

/// Load the page again, for a new game version or renderer (the RELOAD the engine
/// offers a page, never the app).
pub fn reload() {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

/// Whether this run takes the development parameters: `--directory` and the rooms'
/// transport delay and comparisons. The app's command line always does; a page only on
/// this machine or the dev site, where the old page offered them.
pub fn development() -> bool {
    cfg!(not(target_arch = "wasm32")) || local_page() || crate::directory::ENVIRONMENT == "dev"
}

/// Whether this is a page served from this machine (the development server or a local
/// preview); the app is no page.
pub fn local_page() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|window| window.location().hostname().ok())
            .is_some_and(|host| crate::directory::is_local_host(&host))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_reads_as_the_command_line() {
        assert_eq!(
            query_arguments("debug&directory=http%3A%2F%2F127.0.0.1%3A8784&room=abcd2345"),
            [
                "--debug",
                "--directory",
                "http://127.0.0.1:8784",
                "--room",
                "abcd2345"
            ]
        );
        assert_eq!(
            query_arguments("map=stress-test&&latency=120&name=Iron+Jack%20%E2%9C%93"),
            [
                "--map",
                "stress-test",
                "--latency",
                "120",
                "--name",
                "Iron Jack ✓"
            ]
        );
        // A stray `%` stays as it is.
        assert_eq!(query_arguments("room=%zz%+1%4"), ["--room", "%zz% 1%4"]);
        assert!(query_arguments("").is_empty());
    }

    #[test]
    fn the_address_keeps_the_tab_or_the_room_and_every_other_parameter() {
        let query = "debug&multiplayer=&directory=http%3A%2F%2F127.0.0.1%3A8784";
        assert_eq!(
            query_for(query, Place::Room("ABCD2345")),
            "debug&directory=http%3A%2F%2F127.0.0.1%3A8784&room=ABCD2345"
        );
        assert_eq!(
            query_for("room=ABCD2345&latency=120", Place::Multiplayer),
            "latency=120&multiplayer"
        );
        assert_eq!(
            query_for("multiplayer&room=ABCD2345&map=harbor", Place::Single),
            "map=harbor"
        );
        assert_eq!(query_for("", Place::Single), "");
        // A parameter that only starts with a kept name is another parameter.
        assert_eq!(query_for("roomy=1", Place::Single), "roomy=1");
    }

    #[test]
    fn the_app_invites_to_the_web_version() {
        let link = invite_link("ABCD2345");
        assert_eq!(link, format!("{}?room=ABCD2345", web_site()));
        assert!(web_site().starts_with("https://"), "{link}");
    }
}
