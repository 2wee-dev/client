use std::fmt::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct HttpClient {
    client: reqwest::Client,
    /// Full base URL including path prefix (e.g. "http://2wee.test/terminal").
    /// Used for the two hardcoded entry points (/menu/main, /auth/login).
    pub base_url: String,
    /// Scheme + host only (e.g. "http://2wee.test").
    /// Used to resolve server-returned absolute paths.
    pub host_url: String,
    token: Option<String>,
    /// Set by the loading indicator thread when it draws directly to the
    /// terminal. The event loop checks this and forces a full redraw so
    /// ratatui paints over the raw crossterm output.
    needs_redraw: Arc<AtomicBool>,
}

#[derive(Debug)]
pub enum HttpError {
    Unauthorized,
    NotFound,
    ServerError(String),
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpError::Unauthorized => write!(f, "Unauthorized"),
            HttpError::NotFound => write!(f, "Not found"),
            HttpError::ServerError(msg) => write!(f, "{}", msg),
        }
    }
}

impl HttpClient {
    pub fn new(base_url: String) -> Self {
        let host_url = extract_host(&base_url);
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            base_url,
            host_url,
            token: None,
            needs_redraw: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Resolve a server-returned absolute path (e.g. "/terminal/menu/main")
    /// into a full URL by prepending the host.
    pub fn resolve(&self, path: &str) -> String {
        format!("{}{}", self.host_url, path)
    }

    pub fn set_token(&mut self, token: Option<String>) {
        self.token = token;
    }

    /// Returns true (once) if a loading indicator was drawn directly to the
    /// terminal and ratatui needs a full redraw to paint over it.
    pub fn take_needs_redraw(&self) -> bool {
        self.needs_redraw.swap(false, Ordering::Relaxed)
    }

    /// Build a URL with an optional search query parameter.
    /// Strips any existing query string and appends `?query=<encoded>` if non-empty.
    pub fn search_url(base_url: &str, query: &str) -> String {
        let clean = base_url.split('?').next().unwrap_or(base_url);
        if query.is_empty() {
            clean.to_string()
        } else {
            let mut encoded = String::with_capacity(query.len() * 2);
            for c in query.chars() {
                match c {
                    ' ' => encoded.push('+'),
                    '&' | '#' | '?' | '=' | '+' | '%' => {
                        write!(encoded, "%{:02X}", c as u32).unwrap();
                    }
                    _ => encoded.push(c),
                }
            }
            format!("{}?query={}", clean, encoded)
        }
    }

    fn build_request(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.request(method, url);
        if let Some(ref token) = self.token {
            req = req.bearer_auth(token);
        }
        req
    }

    fn run_blocking<F, T>(&self, future: F) -> Result<T, HttpError>
    where
        F: std::future::Future<Output = Result<T, HttpError>>,
    {
        let done = Arc::new(AtomicBool::new(false));
        let done2 = done.clone();
        let redraw_flag = self.needs_redraw.clone();

        // Detached thread: sleeps 500ms then shows "Loading..." if still waiting.
        // No join — fast requests pay only a negligible spawn cost, no blocking.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            if !done2.load(Ordering::Relaxed) {
                use crossterm::{cursor, execute, style::{self, Attribute}, terminal as ct};
                use std::io;
                let (cols, rows) = ct::size().unwrap_or((80, 24));
                let msg = format!(" {:<width$}", "Loading...", width = (cols as usize).saturating_sub(1));
                let _ = execute!(
                    io::stdout(),
                    cursor::MoveTo(0, rows - 1),
                    style::SetAttribute(Attribute::Bold),
                    style::SetForegroundColor(style::Color::Rgb { r: 224, g: 224, b: 224 }),
                    style::SetBackgroundColor(style::Color::Rgb { r: 46, g: 107, b: 115 }),
                    style::Print(&msg),
                    style::SetAttribute(Attribute::Reset),
                );
                redraw_flag.store(true, Ordering::Relaxed);
            }
        });

        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| HttpError::ServerError(e.to_string()))?
            .block_on(future);

        done.store(true, Ordering::Relaxed);
        result
    }

    async fn check_response(&self, resp: reqwest::Response) -> Result<reqwest::Response, HttpError> {
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(HttpError::Unauthorized);
        }
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(HttpError::NotFound);
        }
        if !resp.status().is_success() {
            return Err(HttpError::ServerError(format!("Server error: {}", resp.status())));
        }
        Ok(resp)
    }

    pub fn get_screen(&self, url: &str) -> Result<two_wee_shared::ScreenContract, HttpError> {
        self.run_blocking(async {
            let resp = self.build_request(reqwest::Method::GET, url)
                .send().await
                .map_err(|e| HttpError::ServerError(format!("HTTP error: {}", e)))?;
            let resp = self.check_response(resp).await?;
            resp.json().await
                .map_err(|e| HttpError::ServerError(format!("JSON error: {}", e)))
        })
    }

    pub fn post_save(
        &self,
        url: &str,
        changeset: &two_wee_shared::SaveChangeset,
    ) -> Result<two_wee_shared::ScreenContract, HttpError> {
        self.run_blocking(async {
            let req = self.build_request(reqwest::Method::POST, url).json(changeset);
            let resp = req.send().await
                .map_err(|e| HttpError::ServerError(format!("HTTP error: {}", e)))?;
            let resp = self.check_response(resp).await?;
            resp.json().await
                .map_err(|e| HttpError::ServerError(format!("JSON error: {}", e)))
        })
    }

    pub fn post_delete(
        &self,
        url: &str,
        request: &two_wee_shared::DeleteRequest,
    ) -> Result<two_wee_shared::ScreenContract, HttpError> {
        self.run_blocking(async {
            let req = self.build_request(reqwest::Method::POST, url).json(request);
            let resp = req.send().await
                .map_err(|e| HttpError::ServerError(format!("HTTP error: {}", e)))?;
            let resp = self.check_response(resp).await?;
            resp.json().await
                .map_err(|e| HttpError::ServerError(format!("JSON error: {}", e)))
        })
    }

    pub fn get_validate(
        &self,
        url: &str,
    ) -> Result<two_wee_shared::ValidateResponse, HttpError> {
        self.run_blocking(async {
            let resp = self.build_request(reqwest::Method::GET, url)
                .send().await
                .map_err(|e| HttpError::ServerError(format!("HTTP error: {}", e)))?;
            let resp = self.check_response(resp).await?;
            resp.json().await
                .map_err(|e| HttpError::ServerError(format!("JSON error: {}", e)))
        })
    }

    pub fn post_action(
        &self,
        url: &str,
        request: &two_wee_shared::ActionRequest,
    ) -> Result<two_wee_shared::ActionResponse, HttpError> {
        self.run_blocking(async {
            let req = self.build_request(reqwest::Method::POST, url).json(request);
            let resp = req.send().await
                .map_err(|e| HttpError::ServerError(format!("HTTP error: {}", e)))?;
            let resp = self.check_response(resp).await?;
            resp.json().await
                .map_err(|e| HttpError::ServerError(format!("JSON error: {}", e)))
        })
    }

    pub fn post_auth(
        &self,
        url: &str,
        request: &two_wee_shared::AuthRequest,
    ) -> Result<two_wee_shared::AuthResponse, HttpError> {
        self.run_blocking(async {
            let req = self.build_request(reqwest::Method::POST, url).json(request);
            let resp = req.send().await
                .map_err(|e| HttpError::ServerError(format!("HTTP error: {}", e)))?;
            let resp = self.check_response(resp).await?;
            resp.json().await
                .map_err(|e| HttpError::ServerError(format!("JSON error: {}", e)))
        })
    }
}

/// Extract "scheme://host[:port]" from a URL, stripping any path.
fn extract_host(url: &str) -> String {
    // Find the end of "scheme://host[:port]"
    if let Some(rest) = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://")) {
        let scheme_end = url.len() - rest.len(); // length of "http://" or "https://"
        let host_end = rest.find('/').map(|i| scheme_end + i).unwrap_or(url.len());
        url[..host_end].to_string()
    } else {
        url.to_string()
    }
}
