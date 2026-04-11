pub mod html;
pub mod json;
pub mod markdown;
mod templates;
mod tool_html;

use std::path::Path;

use crate::models::SessionDetail;
use crate::provider_utils::shorten_home_path;

/// Replace home-directory paths with `~` for privacy in exports.
///
/// Keep this as a compatibility wrapper so all Rust display/privacy path
/// handling still goes through `provider_utils::shorten_home_path`.
pub(crate) fn redact_home_path(content: &str) -> String {
    shorten_home_path(content)
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
