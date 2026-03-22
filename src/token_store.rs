use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn token_dir() -> PathBuf {
    dirs::home_dir().unwrap().join(".2wee")
}

/// Deterministic short hash of the server URL, used as the token filename.
fn server_hash(server_url: &str) -> String {
    let mut hasher = DefaultHasher::new();
    server_url.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn token_path(server_url: &str) -> PathBuf {
    token_dir().join("tokens").join(server_hash(server_url))
}

pub fn store_token(server_url: &str, token: &str) {
    let path = token_path(server_url);
    let _ = fs::create_dir_all(path.parent().unwrap());
    let _ = fs::write(&path, token);
    // Owner-only read/write (chmod 600)
    let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
}

pub fn load_token(server_url: &str) -> Option<String> {
    // Try new per-server path first, fall back to legacy single-token path
    let path = token_path(server_url);
    fs::read_to_string(&path)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            // Legacy: ~/.2wee/token (migrate on next store)
            let legacy = token_dir().join("token");
            fs::read_to_string(legacy).ok().filter(|s| !s.trim().is_empty())
        })
}

pub fn clear_token(server_url: &str) {
    let _ = fs::remove_file(token_path(server_url));
    // Also clean up legacy file if it exists
    let _ = fs::remove_file(token_dir().join("token"));
}
