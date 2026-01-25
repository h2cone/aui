mod actions;
#[allow(dead_code)]
mod agent;
mod app;
#[allow(dead_code)]
mod config;
#[allow(dead_code)]
mod logger;
#[allow(dead_code)]
mod session;
mod text_element;
mod text_input;
#[allow(dead_code)]
mod ui;

use gpui::{
    App, Application, Bounds, Focusable, KeyBinding, WindowBounds, WindowOptions, prelude::*, px,
    size,
};

use actions::*;
use app::AuiApp;

fn main() {
    logger::init();
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1180.), px(760.0)), cx);
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, None),
            KeyBinding::new("delete", Delete, None),
            KeyBinding::new("left", Left, None),
            KeyBinding::new("right", Right, None),
            KeyBinding::new("up", HistoryPrev, None),
            KeyBinding::new("down", HistoryNext, None),
            KeyBinding::new("shift-left", SelectLeft, None),
            KeyBinding::new("shift-right", SelectRight, None),
            KeyBinding::new("cmd-a", SelectAll, None),
            KeyBinding::new("ctrl-a", SelectAll, None),
            KeyBinding::new("cmd-v", Paste, None),
            KeyBinding::new("ctrl-v", Paste, None),
            KeyBinding::new("cmd-c", Copy, None),
            KeyBinding::new("ctrl-c", Copy, None),
            KeyBinding::new("cmd-x", Cut, None),
            KeyBinding::new("ctrl-x", Cut, None),
            KeyBinding::new("shift-enter", InsertLineBreak, None),
            KeyBinding::new("enter", Submit, None),
            KeyBinding::new("cmd-enter", Submit, None),
            KeyBinding::new("ctrl-enter", Submit, None),
            KeyBinding::new("cmd-o", AttachFiles, None),
            KeyBinding::new("ctrl-o", AttachFiles, None),
            KeyBinding::new("cmd-s", ExportSession, None),
            KeyBinding::new("ctrl-s", ExportSession, None),
            KeyBinding::new("escape", ClearAttachments, None),
            KeyBinding::new("home", Home, None),
            KeyBinding::new("end", End, None),
            KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, None),
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("ctrl-q", Quit, None),
        ]);

        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| cx.new(|cx| AuiApp::new(cx)),
            )
            .unwrap();

        window
            .update(cx, |view, window, cx| {
                window.focus(&view.text_input.focus_handle(cx));
                cx.activate(true);
            })
            .unwrap();
        cx.on_action(|_: &Quit, cx| cx.quit());
    });
}
