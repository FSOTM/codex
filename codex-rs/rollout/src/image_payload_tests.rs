use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn image_event_copy_is_removed_only_after_artifact_is_saved() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("generated.png");
    let original: RolloutItem = serde_json::from_value(json!({
        "type":"event_msg", "payload":{
            "type":"image_generation_end", "call_id":"test", "status":"completed",
            "revised_prompt":null, "result":"original-base64", "saved_path":path
        }
    })).unwrap();
    let mut missing = original.clone();
    strip_saved_image_event_payload(&mut missing);
    assert_eq!(serde_json::to_value(&missing).unwrap(), serde_json::to_value(&original).unwrap());
    std::fs::write(&path, b"saved-artifact").unwrap();
    let mut persisted = original.clone();
    strip_saved_image_event_payload(&mut persisted);
    let mut expected = serde_json::to_value(&original).unwrap();
    expected["payload"]["result"] = json!("");
    assert_eq!(serde_json::to_value(&persisted).unwrap(), expected);
    // Existing readers can deserialize the new record without a new schema or migration.
    let roundtrip: RolloutItem = serde_json::from_value(expected.clone()).unwrap();
    assert_eq!(serde_json::to_value(roundtrip).unwrap(), expected);
}

#[test]
fn image_event_copy_is_removed_from_paginated_history_without_changing_live_event() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let original: RolloutItem = serde_json::from_value(json!({
        "type":"event_msg", "payload":{
            "type":"item_completed", "thread_id":"11111111-1111-4111-8111-111111111111",
            "turn_id":"test-turn", "completed_at_ms":1,
            "item": {"type":"Extension", "kind":"image_gen.generation", "id":"image",
                "status":"completed", "revisedPrompt":null, "result":"original-base64",
                "savedPath":file.path()}
        }
    })).unwrap();
    let mut persisted = original.clone();
    strip_saved_image_event_payload(&mut persisted);
    let mut expected = serde_json::to_value(&original).unwrap();
    expected["payload"]["item"]["result"] = json!("");
    assert_eq!(serde_json::to_value(&persisted).unwrap(), expected);
    assert_eq!(serde_json::to_value(&original).unwrap()["payload"]["item"]["result"], json!("original-base64"));
}
