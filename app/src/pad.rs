//! Controller-Eingabe (XInput/DirectInput via gilrs) + Tastatur-Ersatz.
use gilrs::{Axis, Button, Gilrs};

pub struct Input {
    pub buttons: u32,
    pub l2: u8,
    pub r2: u8,
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
}

pub struct Pad {
    gilrs: Option<Gilrs>,
}

fn axis(v: f32) -> i16 {
    let v = if v.abs() < 0.08 { 0.0 } else { v };
    (v.clamp(-1.0, 1.0) * 32767.0) as i16
}

impl Pad {
    pub fn new() -> Self {
        Self { gilrs: Gilrs::new().ok() }
    }

    pub fn connected(&self) -> bool {
        self.gilrs.as_ref().map(|g| g.gamepads().next().is_some()).unwrap_or(false)
    }

    pub fn poll(&mut self) -> Input {
        let mut inp = Input { buttons: 0, l2: 0, r2: 0, lx: 0, ly: 0, rx: 0, ry: 0 };
        let Some(g) = self.gilrs.as_mut() else { return inp };
        while g.next_event().is_some() {}
        let Some((_, pad)) = g.gamepads().next() else { return inp };
        let map: [(Button, u32); 16] = [
            (Button::South, 1 << 0),
            (Button::East, 1 << 1),
            (Button::West, 1 << 2),
            (Button::North, 1 << 3),
            (Button::DPadLeft, 1 << 4),
            (Button::DPadRight, 1 << 5),
            (Button::DPadUp, 1 << 6),
            (Button::DPadDown, 1 << 7),
            (Button::LeftTrigger, 1 << 8),
            (Button::RightTrigger, 1 << 9),
            (Button::LeftThumb, 1 << 10),
            (Button::RightThumb, 1 << 11),
            (Button::Start, 1 << 12),
            (Button::Select, 1 << 13),
            (Button::Mode, 1 << 15),
            (Button::Unknown, 0),
        ];
        for (b, mask) in map {
            if mask != 0 && pad.is_pressed(b) {
                inp.buttons |= mask;
            }
        }
        let trig = |b: Button, ax: Axis| -> u8 {
            let from_btn = pad.button_data(b).map(|d| d.value()).unwrap_or(0.0);
            let from_ax = pad.axis_data(ax).map(|d| d.value()).unwrap_or(0.0);
            (from_btn.max(from_ax).clamp(0.0, 1.0) * 255.0) as u8
        };
        inp.l2 = trig(Button::LeftTrigger2, Axis::LeftZ);
        inp.r2 = trig(Button::RightTrigger2, Axis::RightZ);
        let ax = |a: Axis| pad.axis_data(a).map(|d| d.value()).unwrap_or(0.0);
        inp.lx = axis(ax(Axis::LeftStickX));
        inp.ly = axis(-ax(Axis::LeftStickY));
        inp.rx = axis(ax(Axis::RightStickX));
        inp.ry = axis(-ax(Axis::RightStickY));
        inp
    }
}
