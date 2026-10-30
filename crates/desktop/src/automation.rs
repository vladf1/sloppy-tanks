//! The development hook browser checks drive the page through, on a page opened with
//! `?automation` (`--automation`). The canvas shows only pixels, so a check reads here
//! what the shell shows and where its widgets are, then plays it with real mouse,
//! keyboard and touch input at those coordinates. `window.sloppyShell` has:
//!
//! - `state()`: the latest presented frame's [`State`];
//! - `widgets()`: that frame's labelled widgets ([`Widget`]), read from egui's AccessKit
//!   tree, their rectangles in CSS pixels;
//! - `frame(extras)`: a promise of the next presented frame's state, with the engine's
//!   JSON for each of `extras` ([`Extra`]: `"hud"`, `"stats"`, `"nerd"`, `"debug"`,
//!   `"snapshot"`) as `state.extras`;
//! - `hold(ms)`: hold the shell's clock at `ms`, so each frame steps the game to the
//!   time the check picks; `hold(null)` lets it run again;
//! - `debug(name, ...args)`: a promise of what a debug [`Command`] returned (up to three
//!   arguments), run before the next frame.
//!
//! The old page's development `window.sloppy` (`35afd91`'s `src/diagnostics.ts`) is the
//! model. Natively, and in a page without the flag, all of this does nothing.

// Natively nothing reads a page's checks: only the tests use the parsing.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use serde::Serialize;
use serde_json::Value;

/// What a check reads of a presented frame.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// Presented frames since the page loaded.
    pub frame: u64,
    /// The shell's clock for this frame, milliseconds.
    pub now: f64,
    /// `"single"` or `"room"`: which engine has the window.
    pub engine: &'static str,
    /// A round is on screen (not Battle Setup).
    pub in_round: bool,
    /// `"ready"`, `"playing"`, `"paused"` or `"results"`.
    pub phase: &'static str,
    pub alive: bool,
    pub first_person: bool,
    /// First person waits for a click to lock the cursor again (the cockpit says so).
    pub aim_hint: bool,
    /// Battle Setup's Multiplayer tab shows.
    pub multiplayer_tab: bool,
    /// The arena for Battle Setup's choices is ready (GO starts at once).
    pub ready: bool,
    /// Battle Setup's status line.
    pub status: String,
    /// The Settings dialog is open.
    pub settings: bool,
    /// The player's touch controls show (a wreck keeps only the zoom pair, which does
    /// not count).
    pub touch_controls: bool,
    /// The touch screen layout is on.
    pub touch_screen: bool,
    /// Where the touch controls sit on a touch screen (they are painted, not widgets).
    pub touch_layout: Option<TouchControls>,
    /// The drive stick's push (x right, z down) while a finger holds it.
    pub stick_push: Option<[f64; 2]>,
    /// A finger on the arena aims and fires.
    pub arena_finger: bool,
    /// The shell stopped and shows why.
    pub failure: Option<String>,
    /// The renderer's own error (`Game::error`).
    pub error: Option<String>,
    /// The saved choices.
    pub prefs: Value,
    /// The room: its code and the room menu's phase.
    pub room: Option<Value>,
    /// The frame's parts in milliseconds (`frame_log::FrameParts`), single player's: the engine's
    /// frame and its simulation and render times, egui's layout and tessellation
    /// (`overlay`) and drawing the overlay and showing the frame (`present`).
    pub parts: Option<Value>,
    /// The engine's JSON a `frame(extras)` asked for.
    pub extras: serde_json::Map<String, Value>,
}

/// A rectangle on screen, in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Area {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl From<egui::Rect> for Area {
    fn from(rect: egui::Rect) -> Self {
        Self {
            x: rect.left(),
            y: rect.top(),
            width: rect.width(),
            height: rect.height(),
        }
    }
}

/// The touch controls' places (`touch::TouchLayout`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TouchControls {
    pub stick: Area,
    /// The mine button's centre, where the edition has one.
    pub mine: Option<[f32; 2]>,
    pub zoom_in: Area,
    pub zoom_out: Area,
}

impl From<&crate::touch::TouchLayout> for TouchControls {
    fn from(layout: &crate::touch::TouchLayout) -> Self {
        Self {
            stick: layout.stick.into(),
            mine: layout.mine.map(|mine| [mine.x, mine.y]),
            zoom_in: layout.zoom_in.into(),
            zoom_out: layout.zoom_out.into(),
        }
    }
}

/// A labelled widget on screen.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Widget {
    pub label: String,
    /// AccessKit's role, as `"Button"` or `"TextInput"`.
    pub role: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub enabled: bool,
    /// A card or switch that is on (`None`: it has no such state).
    pub selected: Option<bool>,
    /// A text field's text.
    pub value: Option<String>,
}

/// The engine JSON a frame can carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extra {
    Hud,
    Stats,
    Nerd,
    Debug,
    Snapshot,
}

impl Extra {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "hud" => Self::Hud,
            "stats" => Self::Stats,
            "nerd" => Self::Nerd,
            "debug" => Self::Debug,
            "snapshot" => Self::Snapshot,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Hud => "hud",
            Self::Stats => "stats",
            Self::Nerd => "nerd",
            Self::Debug => "debug",
            Self::Snapshot => "snapshot",
        }
    }
}

/// A debug action, the old `window.sloppy` method of the same name.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// GO, as Battle Setup's button presses it, without moving the pointer the
    /// player's turret follows.
    Start,
    Autoplay(bool),
    Overview(bool),
    AutoRounds(bool),
    Zoom(f64),
    Reflections(f32),
    Stress,
    Collapse,
    GiveAmmo(f64),
    KillHuman,
    Soak(f64),
    /// Switch passes and effects off for a same-page A/B measurement, as JSON
    /// (`Game::debug_set_off`): a string, or an object the check passes.
    Off(String),
    /// A room's transport delay, milliseconds (`?latency`, `?jitter`, `?stall`).
    Latency {
        latency: f64,
        jitter: f64,
        stall: f64,
    },
}

impl Command {
    /// `name(...args)` as a check calls it.
    pub fn parse(name: &str, args: &[Value]) -> Result<Self, String> {
        let number = |index: usize, default: f64| {
            args.get(index)
                .map_or(Some(default), Value::as_f64)
                .ok_or_else(|| format!("{name}: argument {index} is not a number"))
        };
        let flag = || {
            args.first()
                .map_or(Some(true), Value::as_bool)
                .ok_or_else(|| format!("{name}: the argument is not true or false"))
        };
        Ok(match name {
            "start" => Self::Start,
            "autoplay" => Self::Autoplay(flag()?),
            "overview" => Self::Overview(flag()?),
            "autoRounds" => Self::AutoRounds(flag()?),
            "zoom" => Self::Zoom(
                args.first()
                    .and_then(Value::as_f64)
                    .ok_or("zoom: the zoom is not a number")?,
            ),
            "reflections" => Self::Reflections(number(0, 1.0)? as f32),
            "stress" => Self::Stress,
            "collapse" => Self::Collapse,
            "giveAmmo" => Self::GiveAmmo(number(0, 5.0)?),
            "killHuman" => Self::KillHuman,
            "soak" => Self::Soak(number(0, 1200.0)?),
            "off" => Self::Off(match args.first() {
                Some(Value::String(json)) => json.clone(),
                Some(value) => value.to_string(),
                None => "{}".into(),
            }),
            "latency" => Self::Latency {
                latency: number(0, 0.0)?,
                jitter: number(1, 0.0)?,
                stall: number(2, 0.0)?,
            },
            _ => return Err(format!("no debug command {name}")),
        })
    }
}

/// The labelled widgets of `context`'s AccessKit tree for its last pass, on screen in
/// egui's points (a page's CSS pixels): a layer drawn smaller to fit a phone moves its
/// widgets with it (`paint::shrink_about`).
pub fn widgets(context: &egui::Context, update: &egui::accesskit::TreeUpdate) -> Vec<Widget> {
    use egui::accesskit::Toggled;
    let layers: std::collections::HashMap<u64, egui::LayerId> = context.viewport(|viewport| {
        viewport
            .prev_pass
            .widgets
            .layers()
            .flat_map(|(layer, rects)| rects.iter().map(move |widget| (widget.id.value(), *layer)))
            .collect()
    });
    update
        .nodes
        .iter()
        .filter_map(|(id, node)| {
            let bounds = node.bounds()?;
            let value = node.value().map(str::to_owned);
            let label = node
                .label()
                .or(node.placeholder())
                .map(str::to_owned)
                .or_else(|| value.clone())?;
            let mut rect = egui::Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            );
            if let Some(transform) = layers
                .get(&id.0)
                .and_then(|layer| context.layer_transform_to_global(*layer))
            {
                rect = transform * rect;
            }
            Some(Widget {
                label,
                role: format!("{:?}", node.role()),
                x: rect.min.x.into(),
                y: rect.min.y.into(),
                width: rect.width().into(),
                height: rect.height().into(),
                enabled: !node.is_disabled(),
                selected: node.toggled().map(|toggled| toggled == Toggled::True),
                value,
            })
        })
        .collect()
}

/// A frame's phase as checks name it.
pub fn phase_name(phase: sloppy_core::sim::MatchPhase) -> &'static str {
    use sloppy_core::sim::MatchPhase;
    match phase {
        MatchPhase::Ready => "ready",
        MatchPhase::Playing => "playing",
        MatchPhase::Paused => "paused",
        MatchPhase::Results => "results",
    }
}

pub use page::*;

#[cfg(target_arch = "wasm32")]
mod page {
    use std::cell::RefCell;

    use js_sys::{Array, Function, Object, Promise, Reflect};
    use serde_json::Value;
    use wasm_bindgen::prelude::*;

    use super::{Command, Extra, State, Widget};

    /// A check waiting on a command's answer.
    pub struct Pending {
        pub command: Command,
        resolve: Function,
        reject: Function,
    }

    impl Pending {
        /// Answer the check with what the command returned, or why it could not run.
        pub fn answer(self, result: Result<Value, String>) {
            let _ = match result {
                Ok(value) => self.resolve.call1(&JsValue::NULL, &to_js(&value)),
                Err(error) => self
                    .reject
                    .call1(&JsValue::NULL, &js_sys::Error::new(&error).into()),
            };
        }
    }

    #[derive(Default)]
    struct Hook {
        frame: u64,
        state: String,
        widgets: String,
        waiters: Vec<(Vec<Extra>, Function)>,
        commands: Vec<Pending>,
        clock: Option<f64>,
    }

    thread_local! {
        static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    }

    fn with<T>(f: impl FnOnce(&mut Hook) -> T) -> Option<T> {
        HOOK.with_borrow_mut(|hook| hook.as_mut().map(f))
    }

    fn to_js(value: &Value) -> JsValue {
        serde_json::to_string(value)
            .ok()
            .and_then(|json| js_sys::JSON::parse(&json).ok())
            .unwrap_or(JsValue::UNDEFINED)
    }

    /// The hook is on: the page was opened with `?automation`.
    pub fn enabled() -> bool {
        HOOK.with_borrow(Option::is_some)
    }

    /// Put `window.sloppyShell` on the page; overlays then build their AccessKit trees.
    pub fn enable() {
        HOOK.set(Some(Hook::default()));
        let shell = Object::new();
        let set = |name: &str, function: JsValue| {
            let _ = Reflect::set(&shell, &name.into(), &function);
        };
        set(
            "state",
            Closure::<dyn Fn() -> JsValue>::new(|| {
                with(|hook| js_sys::JSON::parse(&hook.state).unwrap_or(JsValue::NULL))
                    .unwrap_or(JsValue::NULL)
            })
            .into_js_value(),
        );
        set(
            "widgets",
            Closure::<dyn Fn() -> JsValue>::new(|| {
                with(|hook| js_sys::JSON::parse(&hook.widgets).unwrap_or(JsValue::NULL))
                    .unwrap_or(JsValue::NULL)
            })
            .into_js_value(),
        );
        set(
            "frame",
            Closure::<dyn Fn(JsValue) -> Promise>::new(|extras: JsValue| {
                let names = if Array::is_array(&extras) {
                    Array::from(&extras)
                } else {
                    Array::new()
                };
                let extras: Vec<Extra> = names
                    .iter()
                    .filter_map(|name| name.as_string())
                    .filter_map(|name| Extra::parse(&name))
                    .collect();
                Promise::new(&mut |resolve, _| {
                    with(|hook| hook.waiters.push((extras.clone(), resolve)));
                })
            })
            .into_js_value(),
        );
        set(
            "hold",
            Closure::<dyn Fn(JsValue)>::new(|ms: JsValue| {
                with(|hook| hook.clock = ms.as_f64());
            })
            .into_js_value(),
        );
        set(
            "debug",
            Closure::<dyn Fn(JsValue, JsValue, JsValue, JsValue) -> Promise>::new(
                |name: JsValue, a: JsValue, b: JsValue, c: JsValue| {
                    let name = name.as_string().unwrap_or_default();
                    let args: Vec<Value> = [a, b, c]
                        .iter()
                        .take_while(|arg| !arg.is_undefined())
                        .map(|arg| {
                            js_sys::JSON::stringify(arg)
                                .ok()
                                .and_then(|json| serde_json::from_str(&String::from(json)).ok())
                                .unwrap_or_default()
                        })
                        .collect();
                    let command = Command::parse(&name, &args);
                    Promise::new(&mut |resolve, reject| match command.clone() {
                        Ok(command) => {
                            with(|hook| {
                                hook.commands.push(Pending {
                                    command,
                                    resolve,
                                    reject,
                                })
                            });
                        }
                        Err(error) => {
                            let _ =
                                reject.call1(&JsValue::NULL, &js_sys::Error::new(&error).into());
                        }
                    })
                },
            )
            .into_js_value(),
        );
        if let Some(window) = web_sys::window() {
            let _ = Reflect::set(&window, &"sloppyShell".into(), &shell);
        }
    }

    /// The shell's clock: the time a check holds it at, else the page's.
    pub fn now() -> f64 {
        with(|hook| hook.clock)
            .flatten()
            .unwrap_or_else(crate::app::now_ms)
    }

    /// The debug commands checks sent since the last frame.
    pub fn take_commands() -> Vec<Pending> {
        with(|hook| std::mem::take(&mut hook.commands)).unwrap_or_default()
    }

    /// The overlay's AccessKit tree for this frame.
    pub fn seen(context: &egui::Context, update: &egui::accesskit::TreeUpdate) {
        if enabled() {
            let widgets: Vec<Widget> = super::widgets(context, update);
            let json = serde_json::to_string(&widgets).unwrap_or_default();
            with(|hook| hook.widgets = json);
        }
    }

    /// A frame was presented showing `state`; `extra` gives the engine JSON asked for.
    pub fn presented(mut state: State, mut extra: impl FnMut(Extra) -> Value) {
        let Some(waiters) = with(|hook| {
            hook.frame += 1;
            std::mem::take(&mut hook.waiters)
        }) else {
            return;
        };
        let mut extras = serde_json::Map::new();
        for wanted in waiters.iter().flat_map(|(wanted, _)| wanted) {
            extras
                .entry(wanted.name())
                .or_insert_with(|| extra(*wanted));
        }
        state.frame = with(|hook| hook.frame).unwrap_or_default();
        state.now = now();
        let plain = serde_json::to_value(&state).unwrap_or_default();
        with(|hook| hook.state = plain.to_string());
        for (wanted, resolve) in waiters {
            let mut answer = plain.clone();
            if let Some(map) = answer.get_mut("extras").and_then(Value::as_object_mut) {
                for extra in wanted {
                    if let Some(value) = extras.get(extra.name()) {
                        map.insert(extra.name().into(), value.clone());
                    }
                }
            }
            let _ = resolve.call1(&JsValue::NULL, &to_js(&answer));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod page {
    use serde_json::Value;

    use super::{Command, Extra, State};

    /// A check waiting on a command's answer; natively there are none.
    pub struct Pending {
        pub command: Command,
    }

    impl Pending {
        pub fn answer(self, _result: Result<Value, String>) {}
    }

    pub fn enabled() -> bool {
        false
    }

    pub fn enable() {}

    pub fn now() -> f64 {
        crate::app::now_ms()
    }

    pub fn take_commands() -> Vec<Pending> {
        Vec::new()
    }

    pub fn seen(_context: &egui::Context, _update: &egui::accesskit::TreeUpdate) {}

    pub fn presented(_state: State, _extra: impl FnMut(Extra) -> Value) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A GO! button laid out at (20, 30) in an area whose layer draws at `scale` about
    /// the origin: the widget as listed.
    fn go_button(scale: f32) -> Widget {
        let context = egui::Context::default();
        context.enable_accesskit();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 300.0),
            )),
            ..Default::default()
        };
        // An area lays itself out unseen on its first pass.
        let mut output = egui::FullOutput::default();
        for _ in 0..2 {
            output.textures_delta.clear();
            output = context.run_pass(input.clone(), |context| {
                let area = egui::Area::new(egui::Id::unique("setup"))
                    .fixed_pos(egui::pos2(20.0, 30.0))
                    .show(context, |ui| {
                        let response =
                            ui.allocate_response(egui::vec2(120.0, 40.0), egui::Sense::click());
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::accesskit::Role::Button, true, "GO!")
                        });
                    });
                crate::ui::paint::shrink_about(
                    context,
                    area.response.layer_id,
                    egui::Pos2::ZERO,
                    scale,
                );
            });
        }
        output.textures_delta.clear();
        let update = output.platform_output.accesskit_update.expect("a tree");
        widgets(&context, &update)
            .into_iter()
            .find(|widget| widget.label == "GO!")
            .expect("GO!")
    }

    #[test]
    fn a_labelled_button_is_listed_where_egui_laid_it_out() {
        let go = go_button(1.0);
        assert_eq!(go.role, "Button");
        assert!(go.enabled);
        assert_eq!((go.x, go.y, go.width, go.height), (20.0, 30.0, 120.0, 40.0));
    }

    #[test]
    fn a_shrunk_layer_lists_its_widgets_where_they_show() {
        let go = go_button(0.5);
        assert_eq!((go.x, go.y, go.width, go.height), (10.0, 15.0, 60.0, 20.0));
    }

    #[test]
    fn commands_read_like_the_old_debug_api() {
        assert_eq!(Command::parse("killHuman", &[]), Ok(Command::KillHuman));
        assert_eq!(Command::parse("start", &[]), Ok(Command::Start));
        assert_eq!(Command::parse("giveAmmo", &[]), Ok(Command::GiveAmmo(5.0)));
        assert_eq!(
            Command::parse("overview", &[Value::Bool(false)]),
            Ok(Command::Overview(false))
        );
        assert_eq!(
            Command::parse("latency", &[120.into(), 10.into()]),
            Ok(Command::Latency {
                latency: 120.0,
                jitter: 10.0,
                stall: 0.0
            })
        );
        assert_eq!(
            Command::parse("off", &[serde_json::json!({ "backdrop": true })]),
            Ok(Command::Off(r#"{"backdrop":true}"#.into()))
        );
        assert!(Command::parse("zoom", &["far".into()]).is_err());
        assert!(Command::parse("teleport", &[]).is_err());
    }
}
