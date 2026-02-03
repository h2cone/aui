#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarThumb {
    pub show_thumb: bool,
    pub thumb_top_px: f32,
    pub thumb_height_px: f32,
}

pub fn compute_thumb(
    viewport_height_px: f32,
    scroll_max_height_px: f32,
    offset_y_px: f32,
) -> ScrollbarThumb {
    let viewport_height_px = viewport_height_px.max(0.0);
    let scroll_max_height_px = scroll_max_height_px.max(0.0);

    if viewport_height_px <= 0.0 {
        return ScrollbarThumb {
            show_thumb: false,
            thumb_top_px: 0.0,
            thumb_height_px: 0.0,
        };
    }

    if scroll_max_height_px <= 0.5 {
        return ScrollbarThumb {
            show_thumb: false,
            thumb_top_px: 0.0,
            thumb_height_px: viewport_height_px,
        };
    }

    let content_height_px = viewport_height_px + scroll_max_height_px;
    let mut thumb_height_px = (viewport_height_px * viewport_height_px) / content_height_px;
    thumb_height_px = thumb_height_px.clamp(24.0, viewport_height_px);

    let scroll_progress =
        (-offset_y_px).clamp(0.0, scroll_max_height_px) / scroll_max_height_px.max(1.0);
    let thumb_top_px = scroll_progress * (viewport_height_px - thumb_height_px);

    ScrollbarThumb {
        show_thumb: true,
        thumb_top_px: thumb_top_px.max(0.0),
        thumb_height_px,
    }
}

pub fn offset_for_thumb_top(
    viewport_height_px: f32,
    scroll_max_height_px: f32,
    thumb_height_px: f32,
    thumb_top_px: f32,
) -> f32 {
    let viewport_height_px = viewport_height_px.max(0.0);
    let scroll_max_height_px = scroll_max_height_px.max(0.0);
    let thumb_height_px = thumb_height_px.max(0.0);

    if viewport_height_px <= 0.0 || scroll_max_height_px <= 0.5 {
        return 0.0;
    }

    let track_range_px = (viewport_height_px - thumb_height_px).max(0.0);
    if track_range_px <= 0.0 {
        return 0.0;
    }

    let thumb_top_px = thumb_top_px.clamp(0.0, track_range_px);
    let progress = thumb_top_px / track_range_px;
    -progress * scroll_max_height_px
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollbar_thumb_hidden_when_not_scrollable() {
        let thumb = compute_thumb(200.0, 0.0, 0.0);
        assert!(!thumb.show_thumb);
        assert_eq!(thumb.thumb_top_px, 0.0);
        assert_eq!(thumb.thumb_height_px, 200.0);
    }

    #[test]
    fn scrollbar_thumb_tracks_scroll_position() {
        let top = compute_thumb(200.0, 200.0, 0.0);
        assert!(top.show_thumb);
        assert!((top.thumb_height_px - 100.0).abs() < 0.01);
        assert!((top.thumb_top_px - 0.0).abs() < 0.01);

        let bottom = compute_thumb(200.0, 200.0, -200.0);
        assert!(bottom.show_thumb);
        assert!((bottom.thumb_height_px - 100.0).abs() < 0.01);
        assert!((bottom.thumb_top_px - 100.0).abs() < 0.01);
    }

    #[test]
    fn scrollbar_thumb_respects_min_height() {
        let thumb = compute_thumb(100.0, 900.0, -450.0);
        assert!(thumb.show_thumb);
        assert!((thumb.thumb_height_px - 24.0).abs() < 0.01);
        assert!(thumb.thumb_top_px >= 0.0);
    }

    #[test]
    fn scrollbar_thumb_clamps_offset_to_range() {
        let below_top = compute_thumb(200.0, 200.0, 999.0);
        assert!((below_top.thumb_top_px - 0.0).abs() < 0.01);

        let past_bottom = compute_thumb(200.0, 200.0, -999.0);
        assert!((past_bottom.thumb_top_px - 100.0).abs() < 0.01);
    }

    #[test]
    fn offset_for_thumb_top_maps_track_to_scroll() {
        let thumb = compute_thumb(200.0, 200.0, 0.0);
        assert!((thumb.thumb_height_px - 100.0).abs() < 0.01);

        let top = offset_for_thumb_top(200.0, 200.0, thumb.thumb_height_px, 0.0);
        assert!((top - 0.0).abs() < 0.01);

        let bottom = offset_for_thumb_top(200.0, 200.0, thumb.thumb_height_px, 999.0);
        assert!((bottom - (-200.0)).abs() < 0.01);
    }
}
