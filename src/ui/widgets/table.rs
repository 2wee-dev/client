use ratatui::{
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame,
};

use two_wee_shared::{ColumnAlign, ColumnDef, ColumnWidth, TableSpec};

use crate::theme::Theme;

/// State for a table widget — tracks selection and scroll offset.
#[derive(Debug, Clone)]
pub struct TableState {
    pub selected: usize,
    pub scroll_offset: usize,
}

impl TableState {
    pub fn new() -> Self {
        Self {
            selected: 0,
            scroll_offset: 0,
        }
    }

    pub fn select_next(&mut self, row_count: usize) {
        if row_count == 0 {
            return;
        }
        if self.selected + 1 < row_count {
            self.selected += 1;
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
    }

    pub fn select_last(&mut self, row_count: usize) {
        if row_count > 0 {
            self.selected = row_count - 1;
        }
    }

    pub fn page_down(&mut self, row_count: usize, visible_rows: usize) {
        if row_count == 0 {
            return;
        }
        self.selected = (self.selected + visible_rows).min(row_count - 1);
    }

    pub fn page_up(&mut self, visible_rows: usize) {
        self.selected = self.selected.saturating_sub(visible_rows);
    }

    /// Ensure the selected row is visible given the viewport height.
    fn ensure_visible(&mut self, visible_rows: usize) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + visible_rows {
            self.scroll_offset = self.selected - visible_rows + 1;
        }
    }
}

/// Draw a table from a TableSpec inside the given area.
/// Draw a table. When `focused` is Some(false), selection highlight is suppressed.
pub fn draw_table(
    frame: &mut Frame,
    area: Rect,
    spec: &TableSpec,
    state: &mut TableState,
    theme: &Theme,
    title: Option<&str>,
) {
    draw_table_focused(frame, area, spec, state, theme, title, true);
}

pub fn draw_table_focused(
    frame: &mut Frame,
    area: Rect,
    spec: &TableSpec,
    state: &mut TableState,
    theme: &Theme,
    title: Option<&str>,
    focused: bool,
) {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(theme.content_bg).fg(theme.card_border));
    if let Some(t) = title {
        block = block.title(t);
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 || inner.width < 4 {
        return;
    }

    // 1-char horizontal padding
    let padded = Rect {
        x: inner.x + 1,
        y: inner.y,
        width: inner.width.saturating_sub(2),
        height: inner.height,
    };

    // Build column constraints
    let constraints: Vec<Constraint> = spec
        .columns
        .iter()
        .map(|col| match &col.width {
            ColumnWidth::Fixed(w) => Constraint::Length(*w),
            ColumnWidth::Fill(_) => Constraint::Fill(1),
        })
        .collect();

    // Render header manually — bold text, no special background, no underline
    let header_style = Style::default().bg(theme.content_bg).fg(theme.table_header_fg)
        .add_modifier(Modifier::BOLD);
    let pad_style = Style::default().bg(theme.content_bg);
    let header_cells: Vec<ratatui::text::Line> = spec
        .columns
        .iter()
        .enumerate()
        .map(|(ci, col)| {
            let label = &col.label;
            let label_len = label.chars().count();
            let col_width = constraints.get(ci).and_then(|c| match c {
                Constraint::Length(w) => Some(*w as usize),
                _ => None,
            });
            let align = col.align;

            match (align, col_width) {
                (ColumnAlign::Right, Some(w)) if label_len < w => {
                    let pad = " ".repeat(w - label_len);
                    ratatui::text::Line::from(vec![
                        Span::styled(pad, pad_style),
                        Span::styled(label.clone(), header_style),
                    ])
                }
                (ColumnAlign::Center, Some(w)) if label_len < w => {
                    let left = (w - label_len) / 2;
                    let right = w - label_len - left;
                    ratatui::text::Line::from(vec![
                        Span::styled(" ".repeat(left), pad_style),
                        Span::styled(label.clone(), header_style),
                        Span::styled(" ".repeat(right), pad_style),
                    ])
                }
                _ => ratatui::text::Line::from(Span::styled(label.clone(), header_style)),
            }
        })
        .collect();

    // Render header using a zero-row Table so column widths are consistent
    let header_table = Table::new(Vec::<Row>::new(), &constraints)
        .header(Row::new(header_cells))
        .style(Style::default().bg(theme.content_bg));
    frame.render_widget(header_table, Rect { height: 1, ..padded });

    // Separator line
    let sep_y = padded.y + 1;
    let sep = "─".repeat(padded.width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(sep, Style::default().fg(theme.card_border).bg(theme.content_bg))),
        Rect { x: padded.x, y: sep_y, width: padded.width, height: 1 },
    );

    // Data area below separator
    let data_area = Rect {
        x: padded.x,
        y: sep_y + 1,
        width: padded.width,
        height: padded.height.saturating_sub(2), // header + separator
    };
    let visible_rows = data_area.height as usize;

    state.ensure_visible(visible_rows);

    // Visible rows slice
    let end = (state.scroll_offset + visible_rows).min(spec.rows.len());
    let visible = &spec.rows[state.scroll_offset..end];

    // Index of the selected row within the visible slice (if visible)
    let selected_vi = if focused && spec.selectable && state.selected >= state.scroll_offset && state.selected < end {
        Some(state.selected - state.scroll_offset)
    } else {
        None
    };

    // Build ratatui rows
    let rows: Vec<Row> = visible
        .iter()
        .map(|row| {
            let cells: Vec<Span> = row
                .values
                .iter()
                .enumerate()
                .map(|(ci, val)| {
                    let col = spec.columns.get(ci);
                    let aligned = align_value(val, col, &constraints, ci);
                    Span::styled(aligned, Style::default().fg(theme.table_text))
                })
                .collect();

            Row::new(cells)
        })
        .collect();

    let table = Table::new(rows, &constraints)
        .highlight_style(Style::default().bg(theme.table_selected_bg).fg(theme.table_selected_fg))
        .style(Style::default().bg(theme.content_bg));

    let mut ratatui_state = ratatui::widgets::TableState::default();
    if let Some(vi) = selected_vi {
        ratatui_state.select(Some(vi));
    }
    frame.render_stateful_widget(table, data_area, &mut ratatui_state);

    // Status bar: pagination info
    if spec.row_count > 0 {
        let total_pages = (spec.row_count + visible_rows.max(1) - 1) / visible_rows.max(1);
        let current_page = state.scroll_offset / visible_rows.max(1) + 1;
        let status = format!(
            " {}/{} rows  Page {} of {} ",
            state.selected + 1,
            spec.row_count,
            current_page,
            total_pages
        );
        let status_area = Rect {
            x: area.x + area.width.saturating_sub(status.len() as u16 + 2),
            y: area.y + area.height.saturating_sub(1),
            width: (status.len() as u16).min(area.width),
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(status).style(Style::default().bg(theme.content_bg).fg(theme.field_readonly)),
            status_area,
        );
    }
}

fn align_value(value: &str, col: Option<&ColumnDef>, constraints: &[Constraint], ci: usize) -> String {
    let align = col.map(|c| c.align).unwrap_or(ColumnAlign::Left);
    let col_width = constraints.get(ci).and_then(|c| match c {
        Constraint::Length(w) => Some(*w as usize),
        _ => None,
    });
    match align {
        ColumnAlign::Right => {
            if let Some(w) = col_width {
                let val_len = value.chars().count();
                if val_len < w {
                    format!("{}{}", " ".repeat(w - val_len), value)
                } else {
                    value.to_string()
                }
            } else {
                value.to_string()
            }
        }
        ColumnAlign::Center => {
            if let Some(w) = col_width {
                let val_len = value.chars().count();
                if val_len < w {
                    let pad_left = (w - val_len) / 2;
                    let pad_right = w - val_len - pad_left;
                    format!("{}{}{}", " ".repeat(pad_left), value, " ".repeat(pad_right))
                } else {
                    value.to_string()
                }
            } else {
                value.to_string()
            }
        }
        _ => value.to_string(),
    }
}
