use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

fn image_url(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(bytes))
}

fn user(url: &str) -> ResponseItem {
    serde_json::from_value(json!({
        "type": "message", "role": "user",
        "content": [{"type":"input_image", "image_url":url}]
    })).unwrap()
}

fn tool(url: &str) -> ResponseItem {
    serde_json::from_value(json!({
        "type":"custom_tool_call_output", "call_id":"view-test",
        "output":[{"type":"input_image", "image_url":url}]
    })).unwrap()
}

#[tokio::test]
async fn image_history_budget_archives_old_user_and_tool_images_without_rewriting_source() {
    let directory = tempfile::tempdir().unwrap();
    let old = image_url(b"old-image-bytes");
    let latest = image_url(b"latest-image-bytes");
    let original = vec![user(&old), tool(&old), user(&latest)];
    let mut input = original.clone();
    apply(&mut input, directory.path(), latest.len()).await.unwrap();
    let digest = format!("{:x}", Sha1::digest(b"old-image-bytes"));
    let path = directory.path().join(format!("{digest}.png"));
    assert_eq!(std::fs::read(&path).unwrap(), b"old-image-bytes");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    let notice = ImageHistoryOmission { digest, path }.body();
    let expected = vec![
        serde_json::from_value(json!({"type":"message", "role":"user", "content":[{"type":"input_text","text":notice}]})).unwrap(),
        serde_json::from_value(json!({"type":"custom_tool_call_output", "call_id":"view-test", "output":[{"type":"input_text","text":notice}]})).unwrap(),
        user(&latest),
    ];
    assert_eq!(input, expected);
    assert_eq!(original, vec![user(&old), tool(&old), user(&latest)]);
    let mut resumed = original.clone();
    apply(&mut resumed, directory.path(), latest.len()).await.unwrap();
    assert_eq!(resumed, input);
}

#[tokio::test]
async fn image_history_budget_keeps_newest_reread_and_preserves_file_references() {
    let directory = tempfile::tempdir().unwrap();
    let first = image_url(b"first");
    let second = image_url(b"second");
    let file: ResponseItem = serde_json::from_value(json!({"type":"message", "role":"user", "content":[{"type":"input_image", "file_id":"file-existing"}]})).unwrap();
    let mut input = vec![user(&first), tool(&second), tool(&first), file.clone()];
    apply(&mut input, directory.path(), first.len()).await.unwrap();
    assert_eq!(&input[2..], &[tool(&first), file]);
    let serialized = serde_json::to_string(&input).unwrap();
    assert_eq!(serialized.matches("data:image/").count(), 1);
}

#[tokio::test]
async fn image_history_budget_fails_before_omitting_a_newest_oversized_image() {
    let directory = tempfile::tempdir().unwrap();
    let original = vec![user(&image_url(b"oversized"))];
    let mut input = original.clone();
    assert!(apply(&mut input, directory.path(), 1).await.unwrap_err().to_string().contains("Newest image"));
    assert_eq!(input, original);
}

#[tokio::test]
async fn image_history_budget_refuses_a_corrupt_cache_entry() {
    let directory = tempfile::tempdir().unwrap();
    let old = image_url(b"original");
    let newest = image_url(b"newest");
    let digest = format!("{:x}", Sha1::digest(b"original"));
    std::fs::write(directory.path().join(format!("{digest}.png")), b"corrupt").unwrap();
    let mut input = vec![user(&old), user(&newest)];
    assert!(apply(&mut input, directory.path(), newest.len()).await.unwrap_err().to_string().contains("mismatch"));
}
