use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, GlobalElementId, IntoElement,
    LayoutId, PaintQuad, Pixels, ShapedLine, SharedString, Style, TextRun, UnderlineStyle, Window,
    fill, hsla, point, px, relative, rgba, size,
};

use crate::text_input::TextInput;

pub struct TextElement {
    pub input: Entity<TextInput>,
}

pub struct PrepaintState {
    pub lines: Vec<ShapedLine>,
    pub line_starts: Vec<usize>,
    pub line_height: Pixels,
    pub cursor: Option<PaintQuad>,
    pub selection: Vec<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let input = self.input.read(cx);
        let display_text = if input.content.is_empty() {
            input.placeholder.clone()
        } else {
            input.content.clone()
        };
        let line_count = display_text.split('\n').count().max(1) as f32;
        let line_height = window.line_height();
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (line_height * line_count).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();
        let is_focused = input.focus_handle.is_focused(window);

        let (display_text, text_color, marked_range) = if content.is_empty() {
            (input.placeholder.clone(), hsla(0.0, 0.0, 0.0, 0.35), None)
        } else {
            (content, style.color, input.marked_range.clone())
        };

        let mut line_starts = Vec::new();
        let mut line_texts: Vec<SharedString> = Vec::new();
        let mut start = 0;
        for line in display_text.split('\n') {
            line_starts.push(start);
            line_texts.push(line.to_string().into());
            start += line.len() + 1;
        }

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let mut lines = Vec::with_capacity(line_texts.len());

        for (line_ix, line_text) in line_texts.iter().enumerate() {
            let line_start = line_starts[line_ix];
            let line_len = line_text.len();
            let run = TextRun {
                len: line_len,
                font: style.font(),
                color: text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let runs = if let Some(marked_range) = marked_range.as_ref() {
                let line_end = line_start + line_len;
                let mark_start = marked_range.start.max(line_start);
                let mark_end = marked_range.end.min(line_end);
                if mark_start < mark_end {
                    let before = mark_start.saturating_sub(line_start);
                    let marked = mark_end.saturating_sub(mark_start);
                    let after = line_len.saturating_sub(before + marked);
                    vec![
                        TextRun {
                            len: before,
                            ..run.clone()
                        },
                        TextRun {
                            len: marked,
                            underline: Some(UnderlineStyle {
                                color: Some(run.color),
                                thickness: px(1.0),
                                wavy: false,
                            }),
                            ..run.clone()
                        },
                        TextRun {
                            len: after,
                            ..run.clone()
                        },
                    ]
                    .into_iter()
                    .filter(|run| run.len > 0)
                    .collect()
                } else {
                    vec![run]
                }
            } else {
                vec![run]
            };

            lines.push(
                window
                    .text_system()
                    .shape_line(line_text.clone(), font_size, &runs, None),
            );
        }

        let mut selection = Vec::new();
        let selection_color = if is_focused {
            rgba(0x0d948833)
        } else {
            rgba(0x0d94881a)
        };
        let find_line_info = |offset: usize| {
            for (line_ix, line_start) in line_starts.iter().copied().enumerate() {
                let line_len = lines.get(line_ix).map(|line| line.len()).unwrap_or(0);
                let line_end = line_start + line_len;
                if offset <= line_end || line_ix + 1 == line_starts.len() {
                    return (line_ix, line_start, line_len);
                }
            }
            (0, 0, 0)
        };

        let cursor = if selected_range.is_empty() {
            let (line_ix, line_start, line_len) = find_line_info(cursor);
            let line = lines.get(line_ix);
            let x = line
                .map(|line| line.x_for_index(cursor.saturating_sub(line_start).min(line_len)))
                .unwrap_or(px(0.));
            let y = bounds.top() + line_height * line_ix as f32;
            Some(fill(
                Bounds::new(point(bounds.left() + x, y), size(px(2.), line_height)),
                hsla(0.48, 0.65, 0.45, input.cursor_alpha()),
            ))
        } else {
            let selection_start = selected_range.start.min(display_text.len());
            let selection_end = selected_range.end.min(display_text.len());
            let (start_line_ix, _, _) = find_line_info(selection_start);
            let (end_line_ix, _, _) = find_line_info(selection_end);
            for line_ix in start_line_ix..=end_line_ix {
                let line = &lines[line_ix];
                let line_start = line_starts[line_ix];
                let line_len = line.len();
                let start_offset = if line_ix == start_line_ix {
                    selection_start.saturating_sub(line_start).min(line_len)
                } else {
                    0
                };
                let end_offset = if line_ix == end_line_ix {
                    selection_end.saturating_sub(line_start).min(line_len)
                } else {
                    line_len
                };
                if start_offset == end_offset {
                    continue;
                }
                let left = line.x_for_index(start_offset);
                let right = line.x_for_index(end_offset);
                let y = bounds.top() + line_height * line_ix as f32;
                selection.push(fill(
                    Bounds::from_corners(
                        point(bounds.left() + left, y),
                        point(bounds.left() + right, y + line_height),
                    ),
                    selection_color,
                ));
            }
            None
        };
        PrepaintState {
            lines,
            line_starts,
            line_height,
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        for quad in prepaint.selection.drain(..) {
            window.paint_quad(quad);
        }
        let lines = std::mem::take(&mut prepaint.lines);
        let line_starts = std::mem::take(&mut prepaint.line_starts);
        let line_height = prepaint.line_height;
        for (line_ix, line) in lines.iter().enumerate() {
            let y = bounds.top() + line_height * line_ix as f32;
            line.paint(point(bounds.left(), y), line_height, window, cx)
                .unwrap();
        }

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            input.last_lines = lines;
            input.last_line_starts = line_starts;
            input.last_line_height = Some(line_height);
            input.last_bounds = Some(bounds);
        });
    }
}
