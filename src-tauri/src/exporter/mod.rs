pub mod html;
pub mod json;
pub mod markdown;

use std::path::Path;

use crate::models::SessionDetail;

pub fn export(detail: &SessionDetail, format: &str, output_path: &str) -> Result<(), String> {
    let path = Path::new(output_path);
    match format {
        "json" => json::export_json(detail, path),
        "markdown" | "md" => markdown::export_markdown(detail, path),
        "html" => html::export_html(detail, path),
        _ => Err(format!("unsupported export format: {format}")),
    }
}
