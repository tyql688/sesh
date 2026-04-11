pub fn normalize_gemini_message(text: &str, project_path: &str) -> String {
    use super::images::{looks_like_image_path, resolve_gemini_image_path};

    if !text.contains(".png")
        && !text.contains(".jpg")
        && !text.contains(".jpeg")
        && !text.contains(".gif")
        && !text.contains(".webp")
        && !text.contains(".bmp")
    {
        return text.to_string();
    }

    text.lines()
        .map(|line| {
            let trimmed = line.trim();
            // Try the whole line first
            if let Some(image_path) = resolve_gemini_image_path(trimmed, project_path) {
                return format!("[Image: source: {image_path}]");
            }
            // Handle "@ path/to/image.png some caption text" -- extract the path token
            let token = trimmed.strip_prefix('@').unwrap_or(trimmed).trim();
            if let Some(space_idx) = token.find(|c: char| c.is_whitespace()) {
                let path_part = &token[..space_idx];
                let rest = token[space_idx..].trim();
                if looks_like_image_path(path_part) {
                    let full_raw = if trimmed.starts_with('@') {
                        format!("@{path_part}")
                    } else {
                        path_part.to_string()
                    };
                    if let Some(image_path) = resolve_gemini_image_path(&full_raw, project_path) {
                        return if rest.is_empty() {
                            format!("[Image: source: {image_path}]")
                        } else {
                            format!("[Image: source: {image_path}]\n{rest}")
                        };
                    }
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
