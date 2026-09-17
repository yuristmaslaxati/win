//! First-launch "who does this computer belong to?" prompt.
//!
//! The name is asked once, stored locally, and never editable from inside the
//! app (per product requirement: no settings UI, no deviceId/computerName).
//! It is appended as a `deviceName` query parameter on the configured web
//! app's URL so the remote site can tell machines apart.

use std::path::PathBuf;
use tauri::{AppHandle, Manager, Url};

const DEVICE_NAME_FILE: &str = "device-name.txt";
const DEVICE_NAME_SCHEME: &str = "pake-device-name";
const DEVICE_NAME_SUBMIT_PATH: &str = "set";
/// Query key the prompt page's `pake-device-name:set?name=...` handoff uses.
const DEVICE_NAME_SUBMIT_KEY: &str = "name";
/// Query key appended to the real target URL, per the product spec.
const DEVICE_NAME_QUERY_KEY: &str = "deviceName";
const MAX_DEVICE_NAME_CHARS: usize = 80;

fn device_name_path(app: &AppHandle) -> std::io::Result<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::NotFound, err.to_string()))?
        .join(DEVICE_NAME_FILE))
}

/// The name entered once on first launch. `None` means the prompt has not
/// been completed yet (fresh install, or the local file was removed).
pub fn read_stored_device_name(app: &AppHandle) -> Option<String> {
    let path = device_name_path(app).ok()?;
    let content = std::fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Persist the device name once. There is deliberately no matching "clear" or
/// "update" helper: this name identifies which physical machine the install
/// belongs to and is not meant to change after the first run.
pub fn write_device_name(app: &AppHandle, name: &str) -> std::io::Result<()> {
    let path = device_name_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, name.trim())
}

/// RFC 3986 percent-encoding (unreserved characters pass through, everything
/// else becomes `%XX`, including space as `%20`). Used instead of the `url`
/// crate's form-urlencoded helpers, which would encode space as `+`.
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Append `deviceName=<value>` to `url`, using `&` when `url` already has a
/// query string and `?` otherwise.
pub fn append_device_name_query(url: &str, device_name: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!(
        "{url}{separator}{DEVICE_NAME_QUERY_KEY}={}",
        percent_encode(device_name)
    )
}

/// The self-contained first-run prompt page, loaded as a `data:` URL so it
/// needs no bundled frontend assets, network access, or extra IPC surface.
pub fn prompt_page_url() -> Url {
    let html = include_str!("../inject/device_name_prompt.html");
    let encoded = percent_encode(html);
    Url::parse(&format!("data:text/html;charset=utf-8,{encoded}"))
        .expect("the device-name prompt page must encode to a valid data: URL")
}

/// True when `url` is the prompt page's "save" navigation
/// (`pake-device-name:set?name=...`, sent by `device_name_prompt.html`), and
/// returns the submitted name when it is non-empty.
pub fn extract_submitted_device_name(url: &Url) -> Option<String> {
    if url.scheme() != DEVICE_NAME_SCHEME || url.path() != DEVICE_NAME_SUBMIT_PATH {
        return None;
    }
    let (_, value) = url
        .query_pairs()
        .find(|(key, _)| key == DEVICE_NAME_SUBMIT_KEY)?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.chars().take(MAX_DEVICE_NAME_CHARS).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_question_mark_when_url_has_no_query() {
        assert_eq!(
            append_device_name_query("https://etizim.uz/portal-1", "Mahmud PC"),
            "https://etizim.uz/portal-1?deviceName=Mahmud%20PC"
        );
    }

    #[test]
    fn appends_ampersand_when_url_already_has_a_query() {
        assert_eq!(
            append_device_name_query("https://etizim.uz/portal-1?ref=x", "Kassa-01"),
            "https://etizim.uz/portal-1?ref=x&deviceName=Kassa-01"
        );
    }

    #[test]
    fn percent_encodes_reserved_characters() {
        assert_eq!(
            append_device_name_query("https://example.com", "A&B=C"),
            "https://example.com?deviceName=A%26B%3DC"
        );
    }

    #[test]
    fn extracts_the_submitted_name() {
        let url = Url::parse("pake-device-name:set?name=Mahmud%20PC").unwrap();
        assert_eq!(
            extract_submitted_device_name(&url),
            Some("Mahmud PC".to_string())
        );
    }

    #[test]
    fn ignores_unrelated_navigations() {
        let url = Url::parse("https://etizim.uz/portal-1").unwrap();
        assert_eq!(extract_submitted_device_name(&url), None);
    }

    #[test]
    fn rejects_blank_submitted_names() {
        let url = Url::parse("pake-device-name:set?name=%20%20").unwrap();
        assert_eq!(extract_submitted_device_name(&url), None);
    }

    #[test]
    fn prompt_page_url_encodes_to_a_valid_data_url() {
        let url = prompt_page_url();
        assert_eq!(url.scheme(), "data");
    }
}
