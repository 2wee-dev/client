mod app;
mod date_parse;
mod formula;
pub mod http;
pub mod number_format;
mod runtime;
mod shortcuts;
mod theme;
mod time_parse;
mod token_store;
mod ui;

use std::error::Error;

use app::App;
use http::HttpClient;
use runtime::{restore_terminal, run_app, setup_terminal};

fn main() -> Result<(), Box<dyn Error>> {
    // Base URL: CLI arg > env var > default
    // e.g. "http://2wee.test/terminal" or "http://localhost:3000"
    let base_url = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("TWO_WEE_SERVER").ok())
        .unwrap_or_else(|| String::from("http://localhost:3000"));

    let mut http = HttpClient::new(base_url.clone());
    let mut app = App::new(http.host_url.clone());

    // Load stored token for this server
    if let Some(token) = token_store::load_token(&base_url) {
        http.set_token(Some(token.clone()));
        app.auth_token = Some(token);
    }

    // Set up terminal first so we can show a loading screen
    let mut terminal = setup_terminal()?;

    // Draw full-screen loading state before the first HTTP call
    terminal.draw(|frame| {
        use ratatui::prelude::*;
        use ratatui::widgets::{Block, Paragraph};
        let theme = &app.theme;
        let area = frame.area();
        frame.render_widget(Block::default().style(Style::default().bg(theme.desktop)), area);
        let loading = Paragraph::new(" Loading...")
            .style(Style::default().fg(theme.bar_text).bg(theme.bar_bg).bold())
            .alignment(Alignment::Left);
        let bar = Rect::new(0, area.height.saturating_sub(1), area.width, 1);
        frame.render_widget(loading, bar);
    })?;

    // Fetch the main menu from the server
    runtime::fetch_initial_menu(&mut app, &http);

    let result = run_app(&mut terminal, &mut app, &mut http);
    restore_terminal(&mut terminal)?;

    result
}
