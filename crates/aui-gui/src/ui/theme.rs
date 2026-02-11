use gpui::{Hsla, Rgba, hsla, rgb};

pub fn app_bg() -> Rgba {
    rgb(0xf6f7f9)
}

pub fn surface() -> Rgba {
    rgb(0xffffff)
}

pub fn surface_2() -> Rgba {
    rgb(0xf3f4f6)
}

pub fn surface_3() -> Rgba {
    rgb(0xf8fafc)
}

pub fn text() -> Rgba {
    rgb(0x0b1220)
}

pub fn muted_text() -> Rgba {
    rgb(0x334155)
}

pub fn subtle_text() -> Rgba {
    rgb(0x64748b)
}

pub fn border() -> Hsla {
    hsla(0.0, 0.0, 0.0, 0.08)
}

pub fn border_strong() -> Hsla {
    hsla(0.0, 0.0, 0.0, 0.14)
}

pub fn accent() -> Hsla {
    hsla(0.57, 0.75, 0.50, 0.95)
}

pub fn accent_bg() -> Hsla {
    hsla(0.57, 0.75, 0.50, 0.12)
}

pub fn danger() -> Hsla {
    hsla(0.0, 0.7, 0.5, 0.95)
}

pub fn danger_bg() -> Hsla {
    hsla(0.0, 0.7, 0.5, 0.12)
}
