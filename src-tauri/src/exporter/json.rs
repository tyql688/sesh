use std::fs;
use std::path::Path;

use crate::models::SessionDetail;

pub fn export_json(detail: &SessionDetail, output_path: &Path) -> Result<(), String> {
    let json = serde_json::to_string_pretty(detail)
        .map_err(|e| format!("failed to serialize session: {e}"))?;
    let json = super::redact_home_path(&json);

    fs::write(output_path, json).map_err(|e| format!("failed to write file: {e}"))?;

    Ok(())
}
