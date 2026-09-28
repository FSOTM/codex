//! Opt-in CLI experiment: bound inline images in sampling requests, retaining originals.

use std::io::Write;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use codex_protocol::error::CodexErr;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::ImageReference;
use codex_protocol::models::ResponseItem;
use sha1::Digest;
use sha1::Sha1;
use crate::context::ContextualUserFragment;
use crate::context::image_history_omission::ImageHistoryOmission;

const ENV: &str = "CODEX_LAB_IMAGE_BUDGET_BYTES";

pub(crate) async fn apply_from_env(
    items: &mut [ResponseItem],
    codex_home: &Path,
) -> Result<(), CodexErr> {
    let Some(value) = std::env::var_os(ENV) else {
        return Ok(());
    };
    let limit = value
        .to_str()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .ok_or_else(|| CodexErr::InvalidRequest(format!("{ENV} must be a positive byte count")))?;
    apply(items, &codex_home.join("image-history-cache"), limit).await
}

pub(crate) async fn apply(
    items: &mut [ResponseItem],
    cache_dir: &Path,
    limit: usize,
) -> Result<(), CodexErr> {
    let mut remaining = limit;
    // Keep a contiguous suffix of images. Do not backfill older small images after eviction.
    let mut evict = false;
    for item in items.iter_mut().rev() {
        match item {
            ResponseItem::Message { content, .. } => {
                for part in content.iter_mut().rev() {
                    if let ContentItem::InputImage {
                        image: ImageReference::Inline { image_url },
                        ..
                    } = part
                        && let Some(text) = replacement(
                            image_url, cache_dir, limit, &mut remaining, &mut evict,
                        )
                        .await?
                    {
                        *part = ContentItem::InputText { text };
                    }
                }
            }
            ResponseItem::FunctionCallOutput { output, .. }
            | ResponseItem::CustomToolCallOutput { output, .. } => {
                if let Some(content) = output.content_items_mut() {
                    for part in content.iter_mut().rev() {
                        if let FunctionCallOutputContentItem::InputImage {
                            image: ImageReference::Inline { image_url },
                            ..
                        } = part
                            && let Some(text) = replacement(
                                image_url, cache_dir, limit, &mut remaining, &mut evict,
                            )
                            .await?
                        {
                            *part = FunctionCallOutputContentItem::InputText { text };
                        }
                    }
                }
            }
            ResponseItem::AdditionalTools { .. }
            | ResponseItem::Reasoning { .. }
            | ResponseItem::AgentMessage { .. }
            | ResponseItem::LocalShellCall { .. }
            | ResponseItem::FunctionCall { .. }
            | ResponseItem::ToolSearchCall { .. }
            | ResponseItem::CustomToolCall { .. }
            | ResponseItem::ToolSearchOutput { .. }
            | ResponseItem::WebSearchCall { .. }
            | ResponseItem::ImageGenerationCall { .. }
            | ResponseItem::Compaction { .. }
            | ResponseItem::ConfigurationUpdate { .. }
            | ResponseItem::CompactionTrigger { .. }
            | ResponseItem::ContextCompaction { .. }
            | ResponseItem::Other => {}
        }
    }
    Ok(())
}

async fn replacement(
    url: &str,
    cache_dir: &Path,
    limit: usize,
    remaining: &mut usize,
    evict: &mut bool,
) -> Result<Option<String>, CodexErr> {
    if !url.starts_with("data:image/") {
        return Ok(None);
    }
    if !*evict && url.len() <= *remaining {
        *remaining -= url.len();
        return Ok(None);
    }
    // Never silently remove the only/newest oversized image before the model sees it.
    if !*evict && *remaining == limit {
        return Err(CodexErr::InvalidRequest(format!(
            "Newest image is {} bytes, exceeding experimental image budget {limit}. \
             Crop/resize it or increase {ENV}; no request was sent.",
            url.len()
        )));
    }
    *evict = true;
    let (prefix, encoded) = url.split_once(',').ok_or_else(|| {
        CodexErr::InvalidRequest("Cannot archive malformed image data URL".to_string())
    })?;
    let extension = match prefix {
        "data:image/png;base64" => "png",
        "data:image/jpeg;base64" => "jpg",
        "data:image/webp;base64" => "webp",
        "data:image/gif;base64" => "gif",
        _ => return Err(CodexErr::InvalidRequest(format!("Unsupported image encoding: {prefix}"))),
    };
    let bytes = STANDARD.decode(encoded).map_err(|error| {
        CodexErr::InvalidRequest(format!("Cannot archive invalid image Base64: {error}"))
    })?;
    let digest = format!("{:x}", Sha1::digest(&bytes));
    let path = cache_dir.join(format!("{digest}.{extension}"));
    let saved_path = path.clone();
    // Atomic, no-clobber writes: a crash must not leave a partial 'original'.
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let directory = path.parent().ok_or_else(|| std::io::Error::other("Missing cache directory"))?;
        std::fs::create_dir_all(directory)?;
        if path.exists() {
            if std::fs::read(&path)? != bytes {
                return Err(std::io::Error::other("Image cache content mismatch; refusing to overwrite"));
            }
            return Ok(());
        }
        let mut temp = tempfile::NamedTempFile::new_in(directory)?;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        if let Err(error) = temp.persist_noclobber(&path)
            && (error.error.kind() != std::io::ErrorKind::AlreadyExists
                || std::fs::read(&path)? != bytes)
        {
            return Err(error.error);
        }
        Ok(())
    })
    .await
    .map_err(|error| CodexErr::InvalidRequest(format!("Image archival task failed: {error}")))??;
    Ok(Some(ImageHistoryOmission { digest, path: saved_path }.body()))
}

#[cfg(test)]
#[path = "image_history_budget_tests.rs"]
mod tests;
