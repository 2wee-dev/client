use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::theme::Theme;

/// Render a single-line bottom bar with an optional left-side text and right-aligned hints.
///
/// - `left_text`: optional message or context hint shown on the left
/// - `left_highlight`: true for status messages (bold gold), false for context hints (muted)
/// - `is_error`: true for validation errors (bold red)
/// - `hints`: hotkey hints shown right-aligned (e.g. "Enter View  Esc Back")
pub fn draw_bottom_bar(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    left_text: Option<&str>,
    left_highlight: bool,
    is_error: bool,
    hints: &str,
) {
    // Error state: entire bar turns red with bright yellow text
    let bar_style = if is_error {
        Style::default().bg(theme.error_bar_bg)
    } else {
        Style::default().bg(theme.bar_bg)
    };
    let hints_len = hints.chars().count() as u16;

    let line = if let Some(left) = left_text {
        let left_msg = format!(" {}", left);
        let left_len = left_msg.chars().count() as u16;
        let gap = area.width.saturating_sub(left_len + hints_len);
        let left_style = if is_error {
            bar_style.fg(theme.error_bar_fg).add_modifier(Modifier::BOLD)
        } else if left_highlight {
            bar_style.fg(theme.status_text).add_modifier(Modifier::BOLD)
        } else {
            bar_style.fg(theme.bar_text)
        };
        let hints_style = if is_error {
            bar_style.fg(theme.error_bar_fg)
        } else {
            bar_style.fg(theme.bar_text)
        };
        Line::from(vec![
            Span::styled(left_msg, left_style),
            Span::styled(" ".repeat(gap as usize), bar_style),
            Span::styled(hints, hints_style),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " ".repeat(area.width.saturating_sub(hints_len) as usize),
                bar_style,
            ),
            Span::styled(hints, bar_style.fg(theme.bar_text)),
        ])
    };

    frame.render_widget(Paragraph::new(line).style(bar_style), area);
}
