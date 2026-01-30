use std::ops::Range;
use std::time::Instant;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, EntityInputHandler, FocusHandle, Focusable,
    FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ShapedLine, SharedString, UTF16Selection, Window, prelude::*,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::actions::*;
use crate::text_element::TextElement;

pub struct TextInput {
    pub focus_handle: FocusHandle,
    pub content: SharedString,
    pub placeholder: SharedString,
    pub compact: bool,
    pub selected_range: Range<usize>,
    pub selection_reversed: bool,
    pub marked_range: Option<Range<usize>>,
    pub last_lines: Vec<ShapedLine>,
    pub last_line_starts: Vec<usize>,
    pub last_line_height: Option<Pixels>,
    pub last_bounds: Option<Bounds<Pixels>>,
    pub is_selecting: bool,
    pub cursor_phase: f32,
    pub last_tick: Instant,
    pub sent_messages: Vec<SharedString>,
    pub history_index: Option<usize>,
    pub history_draft: SharedString,
}

impl TextInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: "Type a message to the assistant.".into(),
            compact: false,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_lines: Vec::new(),
            last_line_starts: Vec::new(),
            last_line_height: None,
            last_bounds: None,
            is_selecting: false,
            cursor_phase: 0.0,
            last_tick: Instant::now(),
            sent_messages: Vec::new(),
            history_index: None,
            history_draft: "".into(),
        }
    }

    pub fn new_compact(cx: &mut Context<Self>, placeholder: impl Into<SharedString>) -> Self {
        let mut input = Self::new(cx);
        input.compact = true;
        input.placeholder = placeholder.into();
        input
    }

    pub fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    pub fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    pub fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    pub fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    pub fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx)
    }

    pub fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    pub fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    pub fn insert_line_break(
        &mut self,
        _: &InsertLineBreak,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_range(None, "\n", window, cx)
    }

    pub fn take_submission(&mut self) -> Option<SharedString> {
        let trimmed = self.content.trim_end_matches(['\n', '\r']);
        if trimmed.trim().is_empty() {
            return None;
        }

        let message: SharedString = trimmed.to_string().into();
        self.sent_messages.push(message.clone());
        if self.sent_messages.len() > 8 {
            self.sent_messages.remove(0);
        }
        self.content = "".into();
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
        self.is_selecting = false;
        self.history_index = None;
        self.history_draft = "".into();
        Some(message)
    }

    pub fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    pub fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    pub fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;

        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx)
        }
    }

    pub fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    pub fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    pub fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    pub fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    pub fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    pub fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    pub fn history_prev(&mut self, _: &HistoryPrev, _: &mut Window, cx: &mut Context<Self>) {
        if self.sent_messages.is_empty() {
            return;
        }

        let next_index = match self.history_index {
            None => {
                self.history_draft = std::mem::take(&mut self.content);
                self.sent_messages.len().saturating_sub(1)
            }
            Some(index) => index.saturating_sub(1),
        };

        self.history_index = Some(next_index);
        self.content = self.sent_messages[next_index].clone();
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        cx.notify();
    }

    pub fn history_next(&mut self, _: &HistoryNext, _: &mut Window, cx: &mut Context<Self>) {
        let Some(current) = self.history_index else {
            return;
        };
        let next_index = current + 1;
        if next_index >= self.sent_messages.len() {
            self.history_index = None;
            self.content = std::mem::take(&mut self.history_draft);
        } else {
            self.history_index = Some(next_index);
            self.content = self.sent_messages[next_index].clone();
        }
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        cx.notify();
    }

    pub fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify()
    }

    pub fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    pub fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line_height)) = (self.last_bounds.as_ref(), self.last_line_height)
        else {
            return 0;
        };
        if self.last_line_starts.is_empty() || self.last_lines.is_empty() {
            return 0;
        }
        let line_starts = self.last_line_starts.as_slice();
        let lines = self.last_lines.as_slice();
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        let line_ix = ((position.y - bounds.top()) / line_height).floor().max(0.0) as usize;
        let line_ix = line_ix.min(lines.len().saturating_sub(1));
        let line = &lines[line_ix];
        let line_start = line_starts.get(line_ix).copied().unwrap_or_default();
        let local_index = line.closest_index_for_x(position.x - bounds.left());
        (line_start + local_index).min(self.content.len())
    }

    pub fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    pub fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    pub fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    pub fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    pub fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    pub fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    pub fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    pub fn line_info_for_offset(&self, offset: usize) -> Option<(usize, usize, usize)> {
        if self.last_lines.is_empty() {
            return None;
        }

        for (line_ix, line_start) in self.last_line_starts.iter().copied().enumerate() {
            let line_len = self.last_lines.get(line_ix)?.len();
            let line_end = line_start + line_len;
            if offset <= line_end || line_ix + 1 == self.last_lines.len() {
                return Some((line_ix, line_start, line_len));
            }
        }

        None
    }

    pub fn cursor_alpha(&self) -> f32 {
        let triangle = 1.0 - (self.cursor_phase * 2.0 - 1.0).abs();
        0.35 + 0.65 * triangle
    }

    pub fn tick_cursor(&mut self, window: &mut Window) {
        if !self.focus_handle.is_focused(window) {
            self.cursor_phase = 0.0;
            self.last_tick = Instant::now();
            return;
        }

        let now = Instant::now();
        let delta = now.duration_since(self.last_tick);
        self.last_tick = now;
        self.cursor_phase = (self.cursor_phase + delta.as_secs_f32() * 1.4) % 1.0;
        window.request_animation_frame();
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.history_index.is_some() {
            self.history_index = None;
            self.history_draft = self.content.clone();
        }
        let marked = self.marked_range.take();
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(marked)
            .unwrap_or_else(|| self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.history_index.is_some() {
            self.history_index = None;
            self.history_draft = self.content.clone();
        }
        let marked = self.marked_range.take();
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(marked)
            .unwrap_or_else(|| self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let line_height = self.last_line_height?;
        let (line_ix, line_start, line_len) = self.line_info_for_offset(range.start)?;
        let line = self.last_lines.get(line_ix)?;
        let start_x = line.x_for_index(range.start.saturating_sub(line_start));
        let end_offset = if range.end <= line_start + line_len {
            range.end.saturating_sub(line_start)
        } else {
            line_len
        };
        let end_x = line.x_for_index(end_offset);
        let y = bounds.top() + line_height * line_ix as f32;
        Some(Bounds::from_corners(
            gpui::point(bounds.left() + start_x, y),
            gpui::point(bounds.left() + end_x, y + line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let utf8_index = self.index_for_mouse_position(point);
        Some(self.offset_to_utf16(utf8_index))
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::ui::theme;
        use gpui::{BoxShadow, div, hsla, point, px};

        self.tick_cursor(window);
        let is_focused = self.focus_handle.is_focused(window);
        let border_color = if is_focused {
            theme::border_strong()
        } else {
            theme::border()
        };
        let shadow = if is_focused {
            vec![BoxShadow {
                color: hsla(0.55, 0.85, 0.62, 0.14),
                offset: point(px(0.), px(6.)),
                blur_radius: px(18.),
                spread_radius: px(-10.),
            }]
        } else {
            Vec::new()
        };

        let (line_height, text_size, padding_x, padding_y) = if self.compact {
            (px(20.), px(13.), px(10.), px(6.))
        } else {
            (px(28.), px(18.), px(18.), px(12.))
        };

        div()
            .flex()
            .w_full()
            .key_context("TextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::insert_line_break))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::history_prev))
            .on_action(cx.listener(Self::history_next))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .line_height(line_height)
            .text_size(text_size)
            .font_weight(FontWeight::MEDIUM)
            .font_family("Cascadia Code")
            .text_color(theme::text())
            .bg(theme::surface_3())
            .border_1()
            .border_color(border_color)
            .rounded_xl()
            .px(padding_x)
            .py(padding_y)
            .shadow(shadow)
            .hover(|style| style.border_color(theme::border_strong()))
            .child(TextElement { input: cx.entity() })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn text_input_history_roundtrip(cx: &mut TestAppContext) {
        let (input, cx) = cx.add_window_view(|_, cx| TextInput::new(cx));
        cx.focus(&input);

        input.update_in(cx, |input, window, cx| {
            input.replace_text_in_range(None, "first", window, cx);
        });
        let first = input
            .update(cx, |input, _| input.take_submission())
            .expect("first submission");
        assert_eq!(first.as_ref(), "first");

        input.update_in(cx, |input, window, cx| {
            input.replace_text_in_range(None, "second", window, cx);
        });
        let second = input
            .update(cx, |input, _| input.take_submission())
            .expect("second submission");
        assert_eq!(second.as_ref(), "second");

        input.update_in(cx, |input, window, cx| {
            input.replace_text_in_range(None, "draft", window, cx);
        });

        input.update_in(cx, |input, window, cx| {
            input.history_prev(&HistoryPrev, window, cx);
        });
        let (content, index, draft) = input.read_with(cx, |input, _| {
            (
                input.content.clone(),
                input.history_index,
                input.history_draft.clone(),
            )
        });
        assert_eq!(content.as_ref(), "second");
        assert_eq!(index, Some(1));
        assert_eq!(draft.as_ref(), "draft");

        input.update_in(cx, |input, window, cx| {
            input.history_prev(&HistoryPrev, window, cx);
        });
        let (content, index) =
            input.read_with(cx, |input, _| (input.content.clone(), input.history_index));
        assert_eq!(content.as_ref(), "first");
        assert_eq!(index, Some(0));

        input.update_in(cx, |input, window, cx| {
            input.history_next(&HistoryNext, window, cx);
        });
        let (content, index) =
            input.read_with(cx, |input, _| (input.content.clone(), input.history_index));
        assert_eq!(content.as_ref(), "second");
        assert_eq!(index, Some(1));

        input.update_in(cx, |input, window, cx| {
            input.history_next(&HistoryNext, window, cx);
        });
        let (content, index) =
            input.read_with(cx, |input, _| (input.content.clone(), input.history_index));
        assert_eq!(content.as_ref(), "draft");
        assert_eq!(index, None);
    }

    #[gpui::test]
    fn text_input_selection_reverses(cx: &mut TestAppContext) {
        let (input, cx) = cx.add_window_view(|_, cx| TextInput::new(cx));

        input.update(cx, |input, cx| {
            input.content = "hello".into();
            input.move_to(5, cx);
            input.select_to(3, cx);
        });
        let (range, reversed) = input.read_with(cx, |input, _| {
            (input.selected_range.clone(), input.selection_reversed)
        });
        assert_eq!(range, 3..5);
        assert!(reversed);

        input.update(cx, |input, cx| {
            input.select_to(6, cx);
        });
        let (range, reversed) = input.read_with(cx, |input, _| {
            (input.selected_range.clone(), input.selection_reversed)
        });
        assert_eq!(range, 5..6);
        assert!(!reversed);
    }
}
