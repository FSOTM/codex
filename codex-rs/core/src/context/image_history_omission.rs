use super::ContextualUserFragment;
use codex_protocol::models::ContentItemKind;
use std::path::PathBuf;

pub(crate) struct ImageHistoryOmission {
    pub(crate) digest: String,
    pub(crate) path: PathBuf,
}

impl ContextualUserFragment for ImageHistoryOmission {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind("image_history_omission".to_string())
    }

    fn role(&self) -> &'static str {
        "user"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        ("<image_history_omission>", "</image_history_omission>")
    }

    fn body(&self) -> String {
        format!(
            "Image {} omitted from this request to bound transfer size. Original retained at {}. \
             Earlier observations remain valid, but pixels are not currently visible. \
             Use view_image on this path when visual details are needed; do not guess unseen details.",
            self.digest,
            self.path.display()
        )
    }
}
