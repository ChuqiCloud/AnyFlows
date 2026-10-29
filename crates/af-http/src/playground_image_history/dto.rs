use af_admin::{ImageHistoryEntry, ImageHistoryEntryParts};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PlaygroundImageHistoryItem)]
pub(crate) struct ImageHistoryItemDto {
    id: String,
    model: String,
    prompt: String,
    revised_prompt: Option<String>,
    request_id: Option<String>,
    format: String,
    byte_size: u64,
    thumbnail_format: Option<String>,
    thumbnail_byte_size: Option<u64>,
    created_at: i64,
}

impl ImageHistoryItemDto {
    pub(super) fn from_application(entry: ImageHistoryEntry) -> Self {
        let ImageHistoryEntryParts {
            id,
            model,
            prompt,
            revised_prompt,
            request_id,
            format,
            byte_size,
            thumbnail_format,
            thumbnail_byte_size,
            created_at,
        } = entry.into_parts();
        Self {
            id,
            model,
            prompt,
            revised_prompt,
            request_id,
            format,
            byte_size,
            thumbnail_format,
            thumbnail_byte_size,
            created_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PlaygroundImageHistoryListResponse)]
pub(crate) struct ImageHistoryListResponse {
    images: Vec<ImageHistoryItemDto>,
}

impl ImageHistoryListResponse {
    pub(super) fn from_application(entries: Vec<ImageHistoryEntry>) -> Self {
        Self {
            images: entries
                .into_iter()
                .map(ImageHistoryItemDto::from_application)
                .collect(),
        }
    }
}
