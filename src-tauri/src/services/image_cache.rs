use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::models::{Message, Provider};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

pub trait ImageCacheProvider: Send + Sync {
    fn extract_image_paths(&self, messages: &[Message]) -> Vec<String>;
}

// ---------------------------------------------------------------------------
// Claude implementation
// ---------------------------------------------------------------------------

pub struct ClaudeImageCacheProvider;

impl ImageCacheProvider for ClaudeImageCacheProvider {
    fn extract_image_paths(&self, messages: &[Message]) -> Vec<String> {
        use crate::providers::claude::images::extract_image_source_segments;

        let mut paths = Vec::new();
        for msg in messages {
            for segment in extract_image_source_segments(&msg.content) {
                if let Some(path) = extract_path_from_segment(&segment) {
                    paths.push(path.to_string());
                }
            }
        }
        paths
    }
}

fn extract_path_from_segment(segment: &str) -> Option<&str> {
    let trimmed = segment.strip_prefix("[Image: source: ")?;
    let path = trimmed.strip_suffix(']')?;
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    Some(path)
}

// ---------------------------------------------------------------------------
// Provider lookup helpers
// ---------------------------------------------------------------------------

pub fn image_cache_provider_for(provider: &Provider) -> Option<Box<dyn ImageCacheProvider>> {
    match provider {
        Provider::Claude => Some(Box::new(ClaudeImageCacheProvider)),
        _ => None,
    }
}

pub fn image_cache_provider_for_key(key: &str) -> Option<Box<dyn ImageCacheProvider>> {
    Provider::parse(key).and_then(|p| image_cache_provider_for(&p))
}

// ---------------------------------------------------------------------------
// ImageCacheService
// ---------------------------------------------------------------------------

pub struct ImageCacheService {
    cache_dir: PathBuf,
}

impl ImageCacheService {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            cache_dir: data_dir.join("images"),
        }
    }

    pub fn cache_name(original_path: &str) -> String {
        let hash = Sha256::digest(original_path.as_bytes());
        let hex = format!("{hash:x}");
        let ext = Path::new(original_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png");
        format!("{hex}.{ext}")
    }

    pub fn cache_images(&self, provider: &dyn ImageCacheProvider, messages: &[Message]) {
        let paths = provider.extract_image_paths(messages);
        if paths.is_empty() {
            return;
        }
        if let Err(e) = std::fs::create_dir_all(&self.cache_dir) {
            log::warn!("failed to create image cache dir: {e}");
            return;
        }
        for path in &paths {
            let cache_name = Self::cache_name(path);
            let cache_path = self.cache_dir.join(&cache_name);
            if cache_path.exists() {
                continue;
            }
            let original = Path::new(path);
            if !original.exists() {
                continue;
            }
            if let Err(e) = std::fs::copy(original, &cache_path) {
                log::warn!("failed to cache image {path}: {e}");
            }
        }
    }

    pub fn resolve_cached_path(&self, original_path: &str) -> Option<PathBuf> {
        let cache_name = Self::cache_name(original_path);
        let cache_path = self.cache_dir.join(&cache_name);
        if cache_path.exists() {
            Some(cache_path)
        } else {
            None
        }
    }

    pub fn cleanup_images(&self, provider: &dyn ImageCacheProvider, messages: &[Message]) {
        let paths = provider.extract_image_paths(messages);
        for path in &paths {
            let cache_name = Self::cache_name(path);
            let cache_path = self.cache_dir.join(&cache_name);
            if cache_path.exists() {
                if let Err(e) = std::fs::remove_file(&cache_path) {
                    log::warn!(
                        "failed to remove cached image {}: {e}",
                        cache_path.display()
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::MessageRole;
    use std::io::Write;
    use tempfile::TempDir;

    fn msg(content: &str) -> Message {
        Message {
            role: MessageRole::Assistant,
            content: content.to_string(),
            timestamp: None,
            tool_name: None,
            tool_input: None,
            tool_metadata: None,
            token_usage: None,
            model: None,
            usage_hash: None,
        }
    }

    #[test]
    fn claude_extracts_image_paths() {
        let provider = ClaudeImageCacheProvider;
        let messages = vec![
            msg("Here is the result [Image: source: /tmp/test-image-cache/sess/1.png] done"),
            msg("Another [Image: source: /tmp/screenshot.jpg] and [Image: source: /tmp/test-image-cache/sess/2.png]"),
            msg("No images here"),
        ];
        let paths = provider.extract_image_paths(&messages);
        assert_eq!(
            paths,
            vec![
                "/tmp/test-image-cache/sess/1.png",
                "/tmp/screenshot.jpg",
                "/tmp/test-image-cache/sess/2.png",
            ]
        );
    }

    #[test]
    fn claude_returns_empty_for_no_images() {
        let provider = ClaudeImageCacheProvider;
        let messages = vec![msg("just text"), msg("more text")];
        assert!(provider.extract_image_paths(&messages).is_empty());
    }

    #[test]
    fn cache_name_is_deterministic() {
        let name = ImageCacheService::cache_name("/tmp/test-image-cache/sess/1.png");
        assert_eq!(
            name,
            ImageCacheService::cache_name("/tmp/test-image-cache/sess/1.png")
        );
        assert!(name.ends_with(".png"));
        assert_eq!(name.len(), 64 + 4); // 64 hex + ".png"
    }

    #[test]
    fn cache_name_defaults_to_png_for_no_extension() {
        let name = ImageCacheService::cache_name("/some/path/noext");
        assert!(name.ends_with(".png"));
    }

    #[test]
    fn cache_and_resolve_round_trip() {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path().join("data");
        let service = ImageCacheService::new(&data_dir);

        let img_dir = tmp.path().join("images");
        std::fs::create_dir_all(&img_dir).unwrap();
        let img_path = img_dir.join("test.png");
        let mut f = std::fs::File::create(&img_path).unwrap();
        f.write_all(b"fake png data").unwrap();
        let img_path_str = img_path.to_str().unwrap();

        assert!(service.resolve_cached_path(img_path_str).is_none());

        let provider = ClaudeImageCacheProvider;
        let messages = vec![msg(&format!("[Image: source: {img_path_str}]"))];
        service.cache_images(&provider, &messages);

        let cached = service.resolve_cached_path(img_path_str);
        assert!(cached.is_some());
        assert_eq!(std::fs::read(cached.unwrap()).unwrap(), b"fake png data");

        service.cleanup_images(&provider, &messages);
        assert!(service.resolve_cached_path(img_path_str).is_none());
    }

    #[test]
    fn cache_skips_missing_original() {
        let tmp = TempDir::new().unwrap();
        let service = ImageCacheService::new(tmp.path());
        let provider = ClaudeImageCacheProvider;
        let messages = vec![msg("[Image: source: /nonexistent/path/img.png]")];
        service.cache_images(&provider, &messages);
        assert!(service
            .resolve_cached_path("/nonexistent/path/img.png")
            .is_none());
    }
}
