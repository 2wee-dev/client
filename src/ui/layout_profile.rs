use ratatui::layout::Rect;

pub struct MainMenuLayout {
    pub top_bar: Rect,
    pub top_menu_line: Rect,
    pub bottom_bar: Rect,
    pub panel: Rect,
    pub cols_area: Rect,
    pub grid: Rect,
}

pub fn main_menu_layout(area: Rect) -> MainMenuLayout {
    let top_bar = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    let top_menu_line = Rect {
        x: area.x,
        y: area.y.saturating_add(1),
        width: area.width,
        height: 1,
    };
    let bottom_bar = Rect {
        x: area.x,
        y: area.y.saturating_add(area.height.saturating_sub(1)),
        width: area.width,
        height: 1,
    };

    // Fixed DOS-like profile for classic 80x24 and above.
    let panel = if area.width >= 80 && area.height >= 24 {
        Rect {
            x: area.x.saturating_add(2),
            y: area.y.saturating_add(4),
            width: area.width.saturating_sub(4),
            height: area.height.saturating_sub(8),
        }
    } else {
        Rect {
            x: area.x.saturating_add(3),
            y: area.y.saturating_add(5),
            width: area.width.saturating_sub(7),
            height: area.height.saturating_sub(10),
        }
    };

    let cols_area = Rect {
        x: panel.x.saturating_add(2),
        y: panel.y.saturating_add(3),
        width: panel.width.saturating_sub(4),
        height: 1,
    };
    let grid = Rect {
        x: panel.x.saturating_add(2),
        y: panel.y.saturating_add(5),
        width: panel.width.saturating_sub(4),
        height: panel.height.saturating_sub(8),
    };

    MainMenuLayout {
        top_bar,
        top_menu_line,
        bottom_bar,
        panel,
        cols_area,
        grid,
    }
}
