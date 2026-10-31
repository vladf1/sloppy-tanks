//! Typing in a page: the browser's own text entry under egui's text fields, as eframe's
//! web "text agent" does it. winit 0.30's web backend has no IME or on-screen keyboard,
//! and its canvas cancels every key's default, so on the canvas alone Cmd+V fires no
//! `paste`, a copy never reaches the browser's clipboard and a phone or an iPad shows no
//! keyboard. While an egui text field has the keyboard, a hidden `<input>` at its caret
//! has the page's focus instead of the canvas:
//!
//! - all of the field's keyboard arrives through the element, so nothing arrives twice:
//!   its `input` and composition events become egui's text and IME events, its key
//!   presses egui's key events, and the canvas, out of focus, hears no keys at all;
//! - its `paste` becomes egui's paste, and a copy or cut puts the field's selection on
//!   the clipboard inside the browser's event (the one place Safari lets a page write
//!   it), egui's cut then deleting it from the field;
//! - a press let go on a field focuses the element at once, inside the browser's
//!   gesture, which is when iOS brings up its keyboard; a frame focuses it otherwise,
//!   and moves it to the caret, where a soft keyboard scrolls the page to keep it shown.
//!
//! Once no field has the keyboard the canvas takes the focus back, and the keys drive
//! the tank again. Natively winit and egui-winit type (the system's keyboard and IME)
//! and copy (arboard, where it reaches a clipboard) as before; there only the rules
//! below build, for their tests.

// Natively only the tests use the rules.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use egui::{Id, Key, Modifiers, OutputCommand, Pos2};

#[cfg(target_arch = "wasm32")]
pub use page::TextEntry;

/// Whether a press let go at `point` lands on an enabled text field, by the last pass's
/// layout, before egui has seen the release: the topmost widget there that takes clicks,
/// on the topmost layer (egui's own hit test), is one. A tap too quick for a frame to
/// see the finger counts too.
pub fn field_at(context: &egui::Context, point: Pos2) -> bool {
    let Some(layer) = context.layer_id_at(point) else {
        return false;
    };
    // A layer drawn scaled (a phone's shrunk sheet) keeps its widgets in its own space.
    let local = context
        .layer_transform_from_global(layer)
        .map_or(point, |transform| transform * point);
    let top = context.viewport(|viewport| {
        viewport
            .prev_pass
            .widgets
            .get_layer(layer)
            .filter(|widget| {
                widget.enabled
                    && widget.sense.senses_click()
                    && widget.interact_rect.contains(local)
            })
            .last()
            .map(|widget| widget.id)
    });
    top.is_some_and(|id| egui::text_edit::TextEditState::load(context, id).is_some())
}

/// egui's modifiers for a browser key event's: Cmd is the command key on a Mac (and an
/// iPad), Ctrl elsewhere, as egui-winit maps winit's.
pub fn modifiers(alt: bool, ctrl: bool, shift: bool, meta: bool, mac: bool) -> Modifiers {
    Modifiers {
        alt,
        ctrl,
        shift,
        mac_cmd: mac && meta,
        command: if mac { meta } else { ctrl },
    }
}

/// Whether a browser `key` is a clipboard shortcut, which the browser answers with its
/// `copy`, `cut` or `paste` event: egui hears those rather than the key, as egui-winit
/// gives it a copy, cut or paste for them.
pub fn clipboard_shortcut(key: &str, modifiers: Modifiers) -> bool {
    modifiers.command_only() && matches!(key, "c" | "C" | "x" | "X" | "v" | "V")
}

/// Whether a key pressed in the text entry keeps the browser's own action: typing,
/// whose `input` event carries the text, the clipboard's shortcuts, whose events egui
/// hears, and the browser's own shortcuts (reload, a new tab, the developer tools). The
/// keys that edit or leave a field are egui's alone: the element moving its own caret
/// would do nothing, Tab would move the page's focus off it, and the browser's undo
/// would bring back text the element already handed over.
pub fn keeps_browser_action(key: &str, modifiers: Modifiers) -> bool {
    match key {
        "Tab" | "Enter" | "Escape" | "Backspace" | "Delete" | "ArrowLeft" | "ArrowRight"
        | "ArrowUp" | "ArrowDown" | "Home" | "End" | "PageUp" | "PageDown" => false,
        _ if modifiers.command => !matches!(key, "a" | "A" | "z" | "Z" | "y" | "Y"),
        _ => true,
    }
}

/// Whether a key pressed on the canvas is the browser's: one held with Cmd, Ctrl or
/// Alt (reload, the address bar, zoom, back, find), as the old page's controls left
/// them (`typingOrShortcut`), or a function key other than F11, the game's fullscreen
/// (reload, the developer tools). winit's canvas cancels every key's default, so the
/// page keeps these from reaching it at all; the modifier keys themselves still do,
/// which keeps winit's modifiers right.
pub fn browser_shortcut(code: &str, ctrl: bool, meta: bool, alt: bool) -> bool {
    let modifier = ["Control", "Meta", "Alt", "Shift", "OS"]
        .iter()
        .any(|name| code.starts_with(name));
    let function_key = code
        .strip_prefix('F')
        .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()));
    !modifier && (ctrl || meta || alt || (function_key && code != "F11"))
}

/// The browser's key event as egui's: its key by name (`"a"`, `"Enter"`) and its
/// physical key by code (`"KeyA"`), which egui's shortcuts fall back on.
pub fn key_event(
    key: &str,
    code: &str,
    pressed: bool,
    modifiers: Modifiers,
) -> Option<egui::Event> {
    Some(egui::Event::Key {
        key: Key::from_name(key)?,
        physical_key: Key::from_name(code),
        pressed,
        // egui fills it in.
        repeat: false,
        modifiers,
    })
}

/// The focused field's selected text, which a copy or a cut puts on the clipboard
/// before egui has run again: egui reports a selection once it settles (the pointer is
/// up), and a field whose cursor stops spanning text, or that loses the keyboard, has
/// none.
#[derive(Default)]
pub struct Selection {
    field: Option<Id>,
    text: String,
}

impl Selection {
    /// Follow a pass's `commands`; returns the selection now.
    pub fn after_pass(&mut self, context: &egui::Context, commands: &[OutputCommand]) -> &str {
        let focused = context.memory(|memory| memory.focused());
        for command in commands {
            if let OutputCommand::TextSelectionSettled(text) = command {
                self.field = focused;
                self.text.clone_from(text);
            }
        }
        let spans = focused
            .filter(|id| self.field == Some(*id))
            .and_then(|id| egui::text_edit::TextEditState::load(context, id))
            .and_then(|state| state.cursor.char_range())
            .is_some_and(|range| !range.is_empty());
        if !spans {
            self.field = None;
            self.text.clear();
        }
        &self.text
    }
}

#[cfg(target_arch = "wasm32")]
mod page {
    use std::cell::RefCell;
    use std::rc::Rc;

    use egui::{Event, ImeEvent, OutputCommand};
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};
    use web_sys::{
        ClipboardEvent, CompositionEvent, Document, FocusEvent, HtmlCanvasElement,
        HtmlInputElement, InputEvent, KeyboardEvent,
    };

    use super::Selection;

    /// One of the element's event listeners, kept until the element goes.
    type Listener = Closure<dyn FnMut(web_sys::Event)>;

    /// The key code browsers give a key an IME is composing with (Android's keyboards
    /// give it to most keys): its text comes with the composition's events.
    const IME_KEY_CODE: u32 = 229;

    /// The element's attributes: plain text, nothing the browser or a soft keyboard
    /// adds on its own (capitals, corrections, suggestions, autofill), a Done key, and
    /// out of the page's Tab order (egui's Tab moves between egui's widgets).
    const ATTRIBUTES: [(&str, &str); 10] = [
        ("id", "sloppy-text-entry"),
        ("type", "text"),
        ("inputmode", "text"),
        ("enterkeyhint", "done"),
        ("autocapitalize", "off"),
        ("autocomplete", "off"),
        ("autocorrect", "off"),
        ("spellcheck", "false"),
        ("tabindex", "-1"),
        ("aria-label", "Text field"),
    ];

    /// Invisible but focusable (`display: none` or `visibility: hidden` would take no
    /// focus and bring up no keyboard), and never in a finger's way: the canvas under it
    /// takes every touch. A 16 px font keeps iOS from zooming in on focus.
    const STYLE: &str = "position: absolute; left: 0; top: 0; width: 1px; height: 1px; \
        margin: 0; padding: 0; border: 0; outline: none; opacity: 0; \
        background: transparent; color: transparent; caret-color: transparent; \
        font-size: 16px; pointer-events: none; overflow: hidden;";

    /// What the element heard since the overlay last took it, and what its events share
    /// with the overlay. A listener borrows it while it runs: the overlay never holds it
    /// across a call that dispatches one (focus, blur).
    #[derive(Default)]
    struct Heard {
        /// egui's events, in the order they happened.
        events: Vec<Event>,
        /// The page's focus left the element for nothing of the page's (iOS's Done put
        /// the keyboard away): the field lets go of the keyboard.
        dismissed: bool,
        /// The focused field's selection, for a copy or cut (`Selection`).
        selection: String,
        /// What a cut put on the clipboard: egui's copy of the same text is not written
        /// again outside the browser's event.
        cut: Option<String>,
    }

    pub struct TextEntry {
        input: HtmlInputElement,
        canvas: HtmlCanvasElement,
        document: Document,
        heard: Rc<RefCell<Heard>>,
        listeners: Vec<(&'static str, Listener)>,
        /// The document's listener that keeps the browser's shortcuts from the canvas.
        shortcuts: Option<Listener>,
        selection: Selection,
        /// The caret the element sits at, in points.
        caret: Option<egui::Rect>,
    }

    impl TextEntry {
        /// The element over `canvas`, in the page's body; `mac` maps Cmd as egui's
        /// command key. None in a page without a body.
        pub fn new(canvas: HtmlCanvasElement, mac: bool) -> Option<Self> {
            let document = canvas.owner_document()?;
            let input: HtmlInputElement = document.create_element("input").ok()?.dyn_into().ok()?;
            for (name, value) in ATTRIBUTES {
                input.set_attribute(name, value).ok()?;
            }
            input.set_attribute("style", STYLE).ok()?;
            document.body()?.append_child(&input).ok()?;
            let mut entry = Self {
                input,
                canvas,
                document,
                heard: Rc::default(),
                listeners: Vec::new(),
                shortcuts: None,
                selection: Selection::default(),
                caret: None,
            };
            entry.listen_for_keys(mac);
            entry.listen_for_text();
            entry.listen_for_clipboard();
            entry.listen_for_focus();
            entry.listen_for_shortcuts();
            Some(entry)
        }

        /// Whether the element has the page's focus.
        pub fn has_focus(&self) -> bool {
            self.document
                .active_element()
                .is_some_and(|element| same(&element, &self.input))
        }

        /// Take the page's focus now, as a press lets go on a field: inside the browser's
        /// gesture, iOS brings up its keyboard.
        pub fn focus(&self) {
            if !self.has_focus() {
                let _ = self.input.focus();
            }
        }

        /// Hand what the element heard to egui's next pass, in order with the window's
        /// events: the overlay calls this before it gives egui a window event, and
        /// before each pass.
        pub fn hand_over(&self, raw: &mut egui::RawInput) {
            let mut heard = self.heard.borrow_mut();
            for event in heard.events.drain(..) {
                if let Event::WindowFocused(focused) = event {
                    raw.focused = focused;
                }
                raw.events.push(event);
            }
        }

        /// Before a pass: what the element heard goes to egui, and a field whose keyboard
        /// the player put away lets go, the canvas taking the keys back.
        pub fn before_pass(&self, context: &egui::Context, raw: &mut egui::RawInput) {
            self.hand_over(raw);
            let dismissed = std::mem::take(&mut self.heard.borrow_mut().dismissed);
            if dismissed {
                context.memory_mut(egui::Memory::stop_text_input);
                let body = self.document.body();
                let idle = self
                    .document
                    .active_element()
                    .is_none_or(|element| body.is_some_and(|body| same(&element, &body)));
                if idle {
                    let _ = self.canvas.focus();
                }
            }
        }

        /// After a pass: put what egui copied on the clipboard, keep the field's
        /// selection for a copy, and follow egui's text field: the element sits at its
        /// caret with the page's focus while one has the keyboard, and the canvas has
        /// the focus once none does. The copies leave `output`, so egui-winit's own
        /// clipboard (a string of its own in a page) never holds one.
        pub fn after_pass(&mut self, context: &egui::Context, output: &mut egui::PlatformOutput) {
            let cut = self.heard.borrow_mut().cut.take();
            output.commands.retain(|command| match command {
                OutputCommand::CopyText(text) => {
                    if cut.as_ref() != Some(text) {
                        write_clipboard(text);
                    }
                    false
                }
                _ => true,
            });
            let selection = self.selection.after_pass(context, &output.commands);
            {
                let mut heard = self.heard.borrow_mut();
                if heard.selection != selection {
                    heard.selection = selection.to_owned();
                }
            }
            match &output.ime {
                Some(ime) => {
                    self.place(ime.cursor_rect);
                    self.focus();
                }
                None => {
                    self.caret = None;
                    if self.has_focus() {
                        let _ = self.canvas.focus();
                    }
                }
            }
        }

        /// Sit at the caret, `rect` in points (the canvas's CSS pixels), so a soft
        /// keyboard scrolls the page to keep the field above it and an IME shows its
        /// candidates there.
        fn place(&mut self, rect: egui::Rect) {
            if self.caret == Some(rect) {
                return;
            }
            self.caret = Some(rect);
            let window = web_sys::window();
            let scroll = window.as_ref().map_or((0.0, 0.0), |window| {
                (
                    window.scroll_x().unwrap_or(0.0),
                    window.scroll_y().unwrap_or(0.0),
                )
            });
            let canvas = self.canvas.get_bounding_client_rect();
            let style = self.input.style();
            let left = canvas.left() + scroll.0 + f64::from(rect.left());
            let top = canvas.top() + scroll.1 + f64::from(rect.top());
            let _ = style.set_property("left", &format!("{left}px"));
            let _ = style.set_property("top", &format!("{top}px"));
            let _ = style.set_property("height", &format!("{}px", rect.height().max(1.0)));
        }

        fn listen(&mut self, kind: &'static str, handler: impl FnMut(web_sys::Event) + 'static) {
            let closure = Listener::new(handler);
            if self
                .input
                .add_event_listener_with_callback(kind, closure.as_ref().unchecked_ref())
                .is_ok()
            {
                self.listeners.push((kind, closure));
            }
        }

        /// Keys reach egui as keys; the editing keys lose the browser's action, which
        /// egui takes (`keeps_browser_action`).
        fn listen_for_keys(&mut self, mac: bool) {
            for (kind, pressed) in [("keydown", true), ("keyup", false)] {
                let heard = self.heard.clone();
                self.listen(kind, move |event| {
                    let event: KeyboardEvent = event.unchecked_into();
                    if event.is_composing() || event.key_code() == IME_KEY_CODE {
                        return;
                    }
                    let key = event.key();
                    let modifiers = super::modifiers(
                        event.alt_key(),
                        event.ctrl_key(),
                        event.shift_key(),
                        event.meta_key(),
                        mac,
                    );
                    if pressed && !super::keeps_browser_action(&key, modifiers) {
                        event.prevent_default();
                    }
                    if super::clipboard_shortcut(&key, modifiers) {
                        return;
                    }
                    if let Some(egui_event) =
                        super::key_event(&key, &event.code(), pressed, modifiers)
                    {
                        heard.borrow_mut().events.push(egui_event);
                    }
                });
            }
        }

        /// Typed text and an IME's compositions. The element hands over everything it
        /// takes and stays empty, so it has nothing of its own to undo, select or
        /// autocorrect.
        fn listen_for_text(&mut self) {
            let heard = self.heard.clone();
            let input = self.input.clone();
            self.listen("input", move |event| {
                let event: InputEvent = event.unchecked_into();
                // A composition's text comes with its own events.
                if event.is_composing() {
                    return;
                }
                let value = input.value();
                input.set_value("");
                match event.input_type().as_str() {
                    // Safari's input after a composition ends, which the composition's
                    // end already handed over; and the element's own history.
                    "insertCompositionText"
                    | "insertFromComposition"
                    | "historyUndo"
                    | "historyRedo" => {}
                    _ => {
                        let text = event
                            .data()
                            .filter(|data| !data.is_empty())
                            .unwrap_or(value);
                        if !text.is_empty() && !text.chars().any(char::is_control) {
                            heard.borrow_mut().events.push(Event::Text(text));
                        }
                    }
                }
            });
            let heard = self.heard.clone();
            self.listen("compositionupdate", move |event| {
                let event: CompositionEvent = event.unchecked_into();
                heard
                    .borrow_mut()
                    .events
                    .push(Event::Ime(ImeEvent::Preedit {
                        text: event.data().unwrap_or_default(),
                        active_range_chars: None,
                    }));
            });
            let heard = self.heard.clone();
            let input = self.input.clone();
            self.listen("compositionend", move |event| {
                let event: CompositionEvent = event.unchecked_into();
                // Safari's input event that follows finds nothing left to hand over.
                input.set_value("");
                heard.borrow_mut().events.push(Event::Ime(ImeEvent::Commit(
                    event.data().unwrap_or_default(),
                )));
            });
        }

        /// The clipboard's events. A paste never reaches the element (its own `input`
        /// would hand the text over twice). A copy or cut writes the field's selection
        /// into the event's clipboard data, which only then reaches the clipboard; with
        /// nothing selected the browser keeps its clipboard, as a native field does.
        fn listen_for_clipboard(&mut self) {
            let heard = self.heard.clone();
            self.listen("paste", move |event| {
                let event: ClipboardEvent = event.unchecked_into();
                event.prevent_default();
                let text = event
                    .clipboard_data()
                    .and_then(|data| data.get_data("text/plain").ok())
                    .unwrap_or_default()
                    .replace("\r\n", "\n");
                if !text.is_empty() {
                    heard.borrow_mut().events.push(Event::Paste(text));
                }
            });
            for (kind, cut) in [("copy", false), ("cut", true)] {
                let heard = self.heard.clone();
                self.listen(kind, move |event| {
                    let event: ClipboardEvent = event.unchecked_into();
                    let mut heard = heard.borrow_mut();
                    if heard.selection.is_empty() {
                        return;
                    }
                    let Some(data) = event.clipboard_data() else {
                        return;
                    };
                    if data.set_data("text/plain", &heard.selection).is_err() {
                        return;
                    }
                    event.prevent_default();
                    if cut {
                        heard.cut = Some(heard.selection.clone());
                        heard.events.push(Event::Cut);
                    }
                });
            }
        }

        /// A browser shortcut pressed on the canvas (`browser_shortcut`) stops on its way
        /// down to it, so winit, which cancels the default of every key it hears, never
        /// does: Cmd+R reloads and Cmd+plus zooms as on any page.
        fn listen_for_shortcuts(&mut self) {
            let canvas = self.canvas.clone();
            let closure = Listener::new(move |event: web_sys::Event| {
                let event: &KeyboardEvent = event.unchecked_ref();
                let on_canvas = event.target().is_some_and(|target| same(&target, &canvas));
                if on_canvas
                    && super::browser_shortcut(
                        &event.code(),
                        event.ctrl_key(),
                        event.meta_key(),
                        event.alt_key(),
                    )
                {
                    event.stop_propagation();
                }
            });
            if self
                .document
                .add_event_listener_with_callback_and_bool(
                    "keydown",
                    closure.as_ref().unchecked_ref(),
                    true,
                )
                .is_ok()
            {
                self.shortcuts = Some(closure);
            }
        }

        /// The element's focus as egui's window focus: the page's window losing it
        /// while a field types (another tab, another app), or the player putting the
        /// keyboard away. Focus moving between the element and the canvas is neither:
        /// the canvas's own focus events say the rest.
        fn listen_for_focus(&mut self) {
            let heard = self.heard.clone();
            self.listen("focus", move |_| {
                heard.borrow_mut().events.push(Event::WindowFocused(true));
            });
            let heard = self.heard.clone();
            let canvas = self.canvas.clone();
            let document = self.document.clone();
            self.listen("blur", move |event| {
                let event: FocusEvent = event.unchecked_into();
                if event
                    .related_target()
                    .is_some_and(|target| same(&target, &canvas))
                {
                    return;
                }
                let mut heard = heard.borrow_mut();
                if document.has_focus().unwrap_or(false) {
                    heard.dismissed = true;
                } else {
                    heard.events.push(Event::WindowFocused(false));
                }
            });
        }
    }

    /// The element goes with the overlay; the canvas keeps the keys if it had them.
    impl Drop for TextEntry {
        fn drop(&mut self) {
            if self.has_focus() {
                let _ = self.canvas.focus();
            }
            for (kind, closure) in &self.listeners {
                let _ = self
                    .input
                    .remove_event_listener_with_callback(kind, closure.as_ref().unchecked_ref());
            }
            if let Some(closure) = &self.shortcuts {
                let _ = self.document.remove_event_listener_with_callback_and_bool(
                    "keydown",
                    closure.as_ref().unchecked_ref(),
                    true,
                );
            }
            self.input.remove();
        }
    }

    /// Write `text` on the browser's clipboard, which a page outside a secure context
    /// has none of, and which may refuse it (Safari outside the player's gesture).
    fn write_clipboard(text: &str) {
        let clipboard = web_sys::window()
            .map(|window| window.navigator())
            .and_then(|navigator| js_sys::Reflect::get(&navigator, &"clipboard".into()).ok())
            .filter(|clipboard| !clipboard.is_undefined())
            .map(JsCast::unchecked_into::<web_sys::Clipboard>);
        if let Some(clipboard) = clipboard {
            let written = clipboard.write_text(text);
            wasm_bindgen_futures::spawn_local(async move {
                let _ = wasm_bindgen_futures::JsFuture::from(written).await;
            });
        }
    }

    /// Whether two of the page's objects are one.
    fn same(one: &impl AsRef<JsValue>, other: &impl AsRef<JsValue>) -> bool {
        one.as_ref() == other.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, PointerButton, Rect, pos2};

    use super::*;

    const SCREEN: Rect = Rect::from_min_max(Pos2::ZERO, pos2(800.0, 600.0));
    const FIELD: Rect = Rect::from_min_max(pos2(100.0, 100.0), pos2(300.0, 130.0));
    const BUTTON: Rect = Rect::from_min_max(pos2(100.0, 200.0), pos2(300.0, 230.0));

    /// A screen with a text field over a button, as the Multiplayer tab's room code
    /// field sits beside its FIND.
    struct Screen {
        context: egui::Context,
        text: String,
        time: f64,
        /// A dialog covers the field.
        dialog: bool,
    }

    impl Screen {
        fn new(text: &str) -> Self {
            let context = egui::Context::default();
            crate::ui::theme::install(&context);
            let mut screen = Self {
                context,
                text: text.into(),
                time: 0.0,
                dialog: false,
            };
            // A new area shows from its second pass, once egui has measured it.
            for _ in 0..2 {
                screen.pass(Vec::new());
            }
            screen
        }

        fn pass(&mut self, events: Vec<Event>) -> egui::FullOutput {
            self.time += 1.0 / 60.0;
            let input = egui::RawInput {
                screen_rect: Some(SCREEN),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let text = &mut self.text;
            let dialog = self.dialog;
            let mut output = self.context.run_pass(input, |context| {
                egui::Area::new(Id::unique("screen"))
                    .fixed_pos(Pos2::ZERO)
                    .show(context, |ui| {
                        ui.put(FIELD, egui::TextEdit::singleline(text));
                        ui.put(BUTTON, egui::Button::new("FIND"));
                    });
                if dialog {
                    egui::Area::new(Id::unique("dialog"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(FIELD.min - egui::vec2(20.0, 20.0))
                        .show(context, |ui| {
                            ui.allocate_exact_size(
                                FIELD.size() + egui::vec2(40.0, 40.0),
                                egui::Sense::click(),
                            );
                        });
                }
            });
            // No painter takes the font atlas here.
            output.textures_delta.clear();
            output
        }

        /// Click at `point`, over two passes as a mouse's press and release come.
        fn click(&mut self, point: Pos2) {
            let button = |pressed| Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            self.pass(vec![Event::PointerMoved(point), button(true)]);
            self.pass(vec![button(false)]);
        }
    }

    fn press(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// iOS brings up its keyboard only for a focus inside the tap's own event, before
    /// egui has seen the tap: the last pass must say a field is under it, wherever the
    /// pointer was (a quick tap shows no frame of the finger).
    #[test]
    fn a_release_on_a_field_is_one_that_focuses_it() {
        let mut screen = Screen::new("");
        let (on_field, on_button) = (FIELD.center(), BUTTON.center());
        assert!(field_at(&screen.context, on_field));
        assert!(field_at(
            &screen.context,
            FIELD.left_top() + egui::vec2(2.0, 2.0)
        ));
        assert!(
            !field_at(&screen.context, on_button),
            "a button is no field"
        );
        assert!(
            !field_at(&screen.context, pos2(600.0, 500.0)),
            "nor is nothing"
        );

        // A tap that moves the focused field's caret.
        screen.click(on_field);
        screen.pass(vec![Event::PointerGone]);
        assert!(screen.context.text_edit_focused());
        assert!(field_at(&screen.context, on_field));

        // A dialog over the field takes the tap.
        screen.dialog = true;
        for _ in 0..2 {
            screen.pass(Vec::new());
        }
        assert!(!field_at(&screen.context, on_field), "the dialog is on top");
    }

    /// A copy writes the clipboard inside the browser's event, before egui runs again:
    /// the selection it takes is the one the last pass settled.
    #[test]
    fn a_copy_takes_the_selection_the_last_pass_settled() {
        let mut screen = Screen::new("Tank driver");
        let mut selection = Selection::default();
        let mut pass = |screen: &mut Screen, events| {
            let output = screen.pass(events);
            selection
                .after_pass(&screen.context, &output.platform_output.commands)
                .to_owned()
        };
        screen.click(FIELD.center());
        assert_eq!(pass(&mut screen, Vec::new()), "", "a click selects nothing");
        let all = press(Key::A, Modifiers::COMMAND);
        assert_eq!(pass(&mut screen, vec![all.clone()]), "Tank driver");
        assert_eq!(pass(&mut screen, Vec::new()), "Tank driver", "it stays");
        let right = press(Key::ArrowRight, Modifiers::NONE);
        assert_eq!(pass(&mut screen, vec![right]), "", "the caret moved");

        assert_eq!(pass(&mut screen, vec![all]), "Tank driver");
        let escape = press(Key::Escape, Modifiers::NONE);
        assert_eq!(pass(&mut screen, vec![escape]), "", "the field let go");
        assert!(!screen.context.text_edit_focused());
    }

    #[test]
    fn typing_and_the_clipboard_keep_the_browser_and_editing_keys_are_egui_s() {
        let none = Modifiers::NONE;
        let cmd = modifiers(false, false, false, true, true);
        assert!(cmd.command && cmd.mac_cmd);
        for key in ["a", "Z", " ", "7", "é", "Dead"] {
            assert!(keeps_browser_action(key, none), "{key} types");
        }
        for key in ["v", "c", "x"] {
            assert!(keeps_browser_action(key, cmd), "Cmd+{key} fires its event");
            assert!(clipboard_shortcut(key, cmd));
        }
        assert!(keeps_browser_action("r", cmd), "reload");
        for key in [
            "Tab",
            "Enter",
            "Escape",
            "Backspace",
            "Delete",
            "ArrowLeft",
            "End",
        ] {
            assert!(!keeps_browser_action(key, none), "{key} is egui's");
        }
        for key in ["a", "z", "Z", "y"] {
            assert!(!keeps_browser_action(key, cmd), "Cmd+{key} is egui's");
        }

        // Ctrl is no command key on a Mac, where Ctrl+V pastes nowhere; elsewhere it is.
        let mac_ctrl = modifiers(false, true, false, false, true);
        assert!(!clipboard_shortcut("v", mac_ctrl));
        let ctrl = modifiers(false, true, false, false, false);
        assert!(ctrl.command && !ctrl.mac_cmd);
        assert!(clipboard_shortcut("V", ctrl) && clipboard_shortcut("c", ctrl));
        let shift_ctrl = modifiers(false, true, true, false, false);
        assert!(!clipboard_shortcut("v", shift_ctrl));
        assert!(!keeps_browser_action("z", shift_ctrl), "redo is egui's");
    }

    #[test]
    fn the_browser_keeps_its_shortcuts_and_the_game_its_keys() {
        for (code, ctrl, meta, alt) in [
            ("KeyR", false, true, false),
            ("KeyR", true, false, false),
            ("Equal", false, true, false),
            ("BracketLeft", false, true, false),
            ("ArrowLeft", false, false, true),
            ("F5", false, false, false),
            ("F12", false, false, false),
        ] {
            assert!(
                browser_shortcut(code, ctrl, meta, alt),
                "{code} is the browser's"
            );
        }
        for code in ["KeyW", "Space", "Digit2", "Escape", "Tab", "F11", "KeyF"] {
            assert!(
                !browser_shortcut(code, false, false, false),
                "{code} is the game's"
            );
        }
        // Pressing Cmd itself still tells winit that it is down.
        assert!(!browser_shortcut("MetaLeft", false, true, false));
        assert!(!browser_shortcut("ControlRight", true, false, false));
    }

    #[test]
    fn a_browser_key_names_egui_s_key_and_its_physical_key() {
        let Some(Event::Key {
            key, physical_key, ..
        }) = key_event("q", "KeyA", true, Modifiers::NONE)
        else {
            panic!("a letter is a key");
        };
        // An AZERTY layout's A key types q.
        assert_eq!((key, physical_key), (Key::Q, Some(Key::A)));
        assert!(key_event("Enter", "NumpadEnter", false, Modifiers::NONE).is_some());
        assert!(key_event("Unidentified", "", true, Modifiers::NONE).is_none());
        assert!(key_event("Shift", "ShiftLeft", true, Modifiers::NONE).is_none());
    }
}
