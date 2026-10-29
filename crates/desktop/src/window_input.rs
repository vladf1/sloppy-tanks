//! The window's input for whichever engine has it, single player's `Session` or a
//! room's `RoomSession`: the keyboard, the mouse, fingers and an iPad's hardware
//! keyboard reach egui and the same [`Controls`], and what a key asks of the shell
//! comes back as a [`Request`]. The sessions keep only what differs between them:
//! where the battle stands ([`Battle`]) and what a shortcut, a hidden window or Quit
//! does there.

use sloppy_render::presentation::input::InputFrame;
use winit::event::{ElementState, Touch, WindowEvent};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::Window;

use crate::app::{drawable_size, grab_pointer};
use crate::hardware_keys::HardwareKeys;
use crate::input::{Controls, Shortcut};
use crate::overlay::Overlay;
use crate::touch::{Fingers, Play};
use crate::ui::touch::TouchView;

/// Where the battle stands, as input needs to know it.
pub struct Battle {
    /// The battle has the keyboard: keys drive the tank rather than reach a menu.
    pub keys: bool,
    /// The player can act: alive in a running battle. Gates a shot, the ammo keys and
    /// the wheel's ammunition step.
    pub playing: bool,
    /// A battle is on screen, so the wheel zooms or steps the ammunition.
    pub wheel: bool,
    /// The touch controls, while they are live.
    pub touch: Option<Play>,
}

/// What input asks of the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Shortcut(Shortcut),
    /// Cmd+Q.
    Quit,
}

#[derive(Default)]
pub struct WindowInput {
    pub controls: Controls,
    modifiers: ModifiersState,
    /// The cursor is locked for first person's mouse look.
    pub cursor_grabbed: bool,
    /// Which fingers the menus own, on a touch screen.
    fingers: Fingers,
    /// An iPad's keyboard, read each frame.
    hardware_keys: HardwareKeys,
}

impl WindowInput {
    /// Hand a window event to egui and the controls; returns what a key asked for.
    /// Window events that are not input (a resize, occlusion, focus, close) reach
    /// egui here and are the session's to act on.
    pub fn event(
        &mut self,
        window: &Window,
        overlay: &mut Overlay,
        event: &WindowEvent,
        battle: &Battle,
    ) -> Option<Request> {
        if crate::TOUCH
            && let WindowEvent::Touch(touch) = event
            && !self.route_touch(window, overlay, touch, battle.touch.as_ref())
        {
            return None;
        }
        let keys_to_menus = !battle.keys || overlay.typing();
        let consumed = overlay.event(event, !self.cursor_grabbed, keys_to_menus);
        match event {
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                self.controls.set_command(
                    self.modifiers.super_key()
                        || self.modifiers.control_key()
                        || self.modifiers.alt_key(),
                );
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    return self.key(overlay, code, event.state, event.repeat, battle.playing);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let size = drawable_size(window);
                self.controls.aim_at(
                    (position.x, position.y),
                    (f64::from(size.width), f64::from(size.height)),
                );
            }
            // A press on a menu or the HUD's buttons is not a shot; a release always
            // reaches the controls, so fire never sticks.
            WindowEvent::MouseInput { state, button, .. }
                if *state == ElementState::Released
                    || (battle.playing
                        && (self.cursor_grabbed || !(consumed || overlay.wants_pointer()))) =>
            {
                self.controls.button(*button, *state);
            }
            WindowEvent::MouseWheel { delta, .. } if !overlay.wants_pointer() && battle.wheel => {
                self.controls
                    .wheel(*delta, self.modifiers.shift_key(), battle.playing);
            }
            _ => {}
        }
        None
    }

    /// What an iPad's keyboard pressed and released since the last frame asked for.
    pub fn hardware_keys(
        &mut self,
        window: &Window,
        overlay: &Overlay,
        playing: bool,
    ) -> Vec<Request> {
        let Some((keys, command)) = self.hardware_keys.changes(window) else {
            return Vec::new();
        };
        self.controls.set_command(command);
        keys.into_iter()
            .filter_map(|(code, state)| self.key(overlay, code, state, false, playing))
            .collect()
    }

    /// A key went down or up. `playing` gates the ammo keys.
    fn key(
        &mut self,
        overlay: &Overlay,
        code: KeyCode,
        state: ElementState,
        repeat: bool,
        playing: bool,
    ) -> Option<Request> {
        // Typing goes to the field; a release still reaches the controls, so a key
        // held when a field took focus never sticks.
        if overlay.typing() && state == ElementState::Pressed {
            return None;
        }
        if code == KeyCode::KeyQ && self.modifiers.super_key() {
            return Some(Request::Quit);
        }
        self.controls
            .key(code, state, repeat, playing)
            .map(Request::Shortcut)
    }

    /// Hand a finger to the touch controls or the menus; true when egui should see it.
    fn route_touch(
        &mut self,
        window: &Window,
        overlay: &Overlay,
        touch: &Touch,
        play: Option<&Play>,
    ) -> bool {
        let context = &overlay.context;
        self.fingers.route(
            touch,
            window.scale_factor(),
            play,
            |point| crate::touch::over_ui(context, point),
            context.viewport_rect().size(),
            &mut self.controls,
        )
    }

    /// This frame's input for the engine. First person on a touch screen turns the view
    /// with the stick's sideways push.
    pub fn take_input(&mut self, first_person: bool) -> InputFrame {
        self.controls.stick_turns = crate::TOUCH && first_person;
        self.controls.take_input()
    }

    /// What the touch controls show this frame, while they are live.
    pub fn touch_view(&self, live: bool, first_person: bool) -> Option<TouchView> {
        live.then_some(TouchView {
            input: self.controls.touch,
            first_person,
            learned: self.fingers.learned,
        })
    }

    /// Lock the cursor while first person steers with the mouse, or let it go. A finger
    /// turns the view on a touch screen, which has no cursor to lock.
    pub fn hold_pointer(&mut self, window: &Window, wanted: bool) {
        grab_pointer(window, wanted && !crate::TOUCH, &mut self.cursor_grabbed);
    }
}
