//! An iPad's hardware keyboard as game keys. winit 0.30 hears an iPad's keyboard only
//! as text for a focused field, never as keys going down and up, so winit's view
//! controller takes UIKit's presses (`ios::listen_for_keys`) and this turns them into
//! the presses and releases winit gives the Mac. Elsewhere winit has the keys and this
//! reads nothing.

use winit::event::ElementState;
use winit::keyboard::KeyCode;

/// The keys the game reads (`Controls::key`), by their USB HID usage, which is what
/// UIKit's `UIKeyboardHIDUsage` gives.
#[cfg(target_os = "ios")]
const KEYS: [(KeyCode, isize); 30] = [
    (KeyCode::KeyW, 0x1a),
    (KeyCode::KeyA, 0x04),
    (KeyCode::KeyS, 0x16),
    (KeyCode::KeyD, 0x07),
    (KeyCode::ArrowUp, 0x52),
    (KeyCode::ArrowDown, 0x51),
    (KeyCode::ArrowLeft, 0x50),
    (KeyCode::ArrowRight, 0x4f),
    (KeyCode::Space, 0x2c),
    (KeyCode::KeyQ, 0x14),
    (KeyCode::KeyE, 0x08),
    (KeyCode::KeyV, 0x19),
    (KeyCode::KeyN, 0x11),
    (KeyCode::Escape, 0x29),
    (KeyCode::Digit1, 0x1e),
    (KeyCode::Digit2, 0x1f),
    (KeyCode::Digit3, 0x20),
    (KeyCode::Digit4, 0x21),
    (KeyCode::Digit5, 0x22),
    (KeyCode::Numpad1, 0x59),
    (KeyCode::Numpad2, 0x5a),
    (KeyCode::Numpad3, 0x5b),
    (KeyCode::Numpad4, 0x5c),
    (KeyCode::Numpad5, 0x5d),
    // Command, Control and Option on either side make the keys shortcuts.
    (KeyCode::ControlLeft, 0xe0),
    (KeyCode::AltLeft, 0xe2),
    (KeyCode::SuperLeft, 0xe3),
    (KeyCode::ControlRight, 0xe4),
    (KeyCode::AltRight, 0xe6),
    (KeyCode::SuperRight, 0xe7),
];

/// The modifier keys among [`KEYS`].
#[cfg(target_os = "ios")]
fn is_modifier(code: KeyCode) -> bool {
    use KeyCode::*;
    matches!(
        code,
        ControlLeft | AltLeft | SuperLeft | ControlRight | AltRight | SuperRight
    )
}

/// The keyboard's keys as last read.
#[derive(Default)]
pub struct HardwareKeys {
    #[cfg(target_os = "ios")]
    held: [bool; KEYS.len()],
}

impl HardwareKeys {
    /// The keys that went down or up since the last call, and whether a modifier is
    /// held now; `None` where winit reports the keys itself.
    #[cfg(target_os = "ios")]
    pub fn changes(
        &mut self,
        window: &winit::window::Window,
    ) -> Option<(Vec<(KeyCode, ElementState)>, bool)> {
        crate::ios::hold_key_focus(window);
        let mut changes = Vec::new();
        for (usage, down) in crate::ios::take_presses() {
            let Some(index) = KEYS.iter().position(|(_, key)| *key == usage) else {
                continue;
            };
            // UIKit reports a press once; a release of a key never seen down is dropped.
            if self.held[index] != down {
                self.held[index] = down;
                let state = if down {
                    ElementState::Pressed
                } else {
                    ElementState::Released
                };
                changes.push((KEYS[index].0, state));
            }
        }
        let command = KEYS
            .iter()
            .zip(self.held)
            .any(|((code, _), down)| down && is_modifier(*code));
        Some((changes, command))
    }

    #[cfg(not(target_os = "ios"))]
    pub fn changes(
        &mut self,
        _window: &winit::window::Window,
    ) -> Option<(Vec<(KeyCode, ElementState)>, bool)> {
        None
    }
}
