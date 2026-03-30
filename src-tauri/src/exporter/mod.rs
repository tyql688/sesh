pub mod html;
pub mod json;
pub mod markdown;
mod templates;

use std::path::Path;

use crate::models::SessionDetail;

/// Replace the user's home directory path with `~` for privacy in exports.
/// Uses trailing separator to avoid partial-word matches (e.g. `/Users/john` vs `/Users/johnson`).
pub(crate) fn redact_home_path(content: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_with_sep = format!("{}/", home.display());
        content.replace(home_with_sep.as_str(), "~/")
    } else {
        content.to_string()
    }
}

pub fn export(detail: &SessionDetail, format: &str, output_path: &str) -> Result<(), String> {
    let path = Path::new(output_path);
    match format {
        "json" => json::export_json(detail, path),
        "markdown" | "md" => markdown::export_markdown(detail, path),
        "html" => html::export_html(detail, path),
        _ => Err(format!("unsupported export format: {format}")),
    }
}
