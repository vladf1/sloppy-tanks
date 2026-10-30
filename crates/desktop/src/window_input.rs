//! The window's input for whichever engine has it, single player's `Session` or a
//! room's `RoomSession`: the keyboard, the mouse, fingers and an iPad's hardware
//! keyboard reach egui and the same [`Controls`], and what a key asks of the shell
//! comes back as a [`Request`]. The sessions keep only what differs between them:
//! where the battle stands ([`Battle`]) and what a shortcut, a hidden window or Quit
//! does there.

use sloppy_render::presentation::input::InputFrame;
use winit::event::{ElementState, Touch, TouchPhase, WindowEvent};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::Window;

use crate::app::{css_size, now_ms};
use crate::hardware_keys::HardwareKeys;
use crate::input::{Controls, Shortcut, TouchInput};
use crate::overlay::Overlay;
use crate::pointer_lock::{self, PointerLock};
use crate::touch::{Fingers, Play, Showing};
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
    /// The cursor lock for first person's mouse look.
    pointer: PointerLock,
    /// Where the cursor last moved to, in points.
    cursor: Option<egui::Pos2>,
    /// Which fingers the menus own, on a touch screen.
    fingers: Fingers,
    /// The last press was a finger's: with touch play, a finger turns the first-person
    /// view, which a pointer lock would freeze; a mouse on the same device (a touch
    /// laptop) still looks with the lock (`fingerLooks`).
    finger_last: bool,
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
        if let WindowEvent::Touch(touch) = event {
            crate::touch::touched();
            if touch.phase == TouchPhase::Started {
                self.finger_last = true;
            }
            if crate::touch::screen()
                && !self.route_touch(window, overlay, touch, battle.touch.as_ref())
            {
                return None;
            }
        }
        if let WindowEvent::MouseInput {
            state: ElementState::Pressed,
            ..
        } = event
        {
            self.finger_last = false;
        }
        let keys_to_menus = !battle.keys || overlay.typing();
        let locked = self.pointer.held();
        let consumed = overlay.event(event, !locked, keys_to_menus);
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
                    return self.key(
                        window,
                        overlay,
                        code,
                        event.state,
                        event.repeat,
                        battle.playing,
                    );
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (width, height, scale) = css_size(window);
                self.controls
                    .aim_at((position.x, position.y), (width * scale, height * scale));
                self.cursor = Some(egui::pos2(
                    (position.x / scale) as f32,
                    (position.y / scale) as f32,
                ));
            }
            // A turned phone or a browser's bars changed the screen under the fingers:
            // the stick and the shot let go until they press again (`resized`).
            WindowEvent::Resized(_) => self.controls.touch = TouchInput::default(),
            // A press on a menu or the HUD's buttons is not a shot, nor is the click on
            // the arena that takes a freed cursor back; a release always reaches the
            // controls, so fire never sticks. egui learns where the cursor is at its next
            // frame, so a cursor that jumped onto a button since is found by its place.
            WindowEvent::MouseInput { state, button, .. }
                if *state == ElementState::Released
                    || (battle.playing
                        && (locked
                            || !(consumed
                                || overlay.wants_pointer()
                                || self.cursor.is_some_and(|cursor| {
                                    crate::touch::over_ui(&overlay.context, cursor)
                                })))) =>
            {
                if *state == ElementState::Pressed
                    && !locked
                    && let Some(grab) = self.pointer.press(battle.playing, now_ms())
                {
                    // Within the press, which a browser wants before it locks again.
                    self.grab(window, grab);
                } else {
                    self.controls.button(*button, *state);
                }
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
            .filter_map(|(code, state)| self.key(window, overlay, code, state, false, playing))
            .collect()
    }

    /// A key went down or up. `playing` gates the ammo keys. Esc with the cursor locked
    /// only frees it (`pointer_lock.rs`).
    fn key(
        &mut self,
        window: &Window,
        overlay: &Overlay,
        code: KeyCode,
        state: ElementState,
        repeat: bool,
        playing: bool,
    ) -> Option<Request> {
        // Typing goes to the field; a release still reaches the controls, so a key
        // held when a field took focus never sticks. (In a page the field's own element
        // takes the keys from the canvas, whose `Focused(false)` lets go of held ones:
        // `text_entry.rs`.)
        if overlay.typing() && state == ElementState::Pressed {
            return None;
        }
        if code == KeyCode::KeyQ && self.modifiers.super_key() {
            return Some(Request::Quit);
        }
        let shortcut = self.controls.key(code, state, repeat, playing)?;
        if shortcut == Shortcut::Escape
            && let Some(grab) = self.pointer.escape(now_ms())
        {
            self.grab(window, grab);
            return None;
        }
        Some(Request::Shortcut(shortcut))
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
        self.controls.stick_turns = crate::touch::screen() && first_person;
        self.controls.take_input()
    }

    /// What the touch controls show this frame, while any show on a touch screen.
    pub fn touch_view(&self, showing: Showing, first_person: bool) -> Option<TouchView> {
        (crate::touch::screen() && showing != Showing::Nothing).then_some(TouchView {
            input: self.controls.touch,
            first_person,
            learned: self.fingers.learned,
            acting: showing == Showing::All,
        })
    }

    /// Lock the cursor while first person steers with the mouse (`wanted`), or let it
    /// go; a room keeps asking while its player is down.
    pub fn hold_pointer(&mut self, window: &Window, wanted: bool) {
        self.hold_pointer_steering(window, wanted, true);
    }

    /// [`Self::hold_pointer`] for a player who can act only while `steering`: a death
    /// keeps a held lock but asks for none, so a cursor Esc freed picks the next tank.
    /// With touch play a finger turns the view, which a lock would freeze, until a mouse
    /// presses again.
    pub fn hold_pointer_steering(&mut self, window: &Window, wanted: bool, steering: bool) {
        let finger_looks = self.finger_last && crate::touch::screen();
        let browser = pointer_lock::browser_locked(window);
        if let Some(grab) = self
            .pointer
            .frame(wanted && !finger_looks, steering, browser, now_ms())
        {
            self.grab(window, grab);
        }
    }

    /// The cursor is locked: raw mouse travel turns the first-person view.
    pub fn pointer_locked(&self) -> bool {
        self.pointer.held()
    }

    /// First person waits for a click on the arena to lock the cursor again.
    pub fn aim_waits_for_click(&self) -> bool {
        self.pointer.waits_for_click()
    }

    fn grab(&mut self, window: &Window, grab: pointer_lock::Grab) {
        if !pointer_lock::apply(window, grab) {
            self.pointer.refused();
        }
    }
}
