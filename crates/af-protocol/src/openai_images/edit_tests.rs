use bytes::Bytes;

use super::*;
use crate::{ImageInputFidelity, MAX_IMAGE_EDIT_MASK_BYTES};

fn png(width: u32, height: u32, marker: &[u8]) -> Bytes {
    let mut bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR".to_vec();
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(marker);
    Bytes::from(bytes)
}

fn text(name: &str, value: &str) -> OpenAiImageEditFormPart {
    OpenAiImageEditFormPart::text(name.to_owned(), value.to_owned()).unwrap()
}

fn file(name: &str, file_name: &str, content_type: &str, bytes: Bytes) -> OpenAiImageEditFormPart {
    OpenAiImageEditFormPart::file(
        name.to_owned(),
        file_name.to_owned(),
        Some(content_type.to_owned()),
        bytes,
    )
    .unwrap()
}

#[test]
fn parses_multiple_images_and_matching_png_mask_and_rebuilds_safe_multipart() {
    let request = parse_edit_request(
        OpenAiImageEditForm::new(vec![
            text("model", "image-edit-test"),
            text("prompt", "private edit prompt"),
            text("n", "2"),
            text("output_format", "webp"),
            text("response_format", "b64_json"),
            text("style", "natural"),
            text("input_fidelity", "high"),
            text("user", "sdk-user-id"),
            file(
                "image",
                "client-private-name.png",
                "image/png",
                png(1_024, 1_024, b"image-a-secret"),
            ),
            file(
                "image[]",
                "second.png",
                "image/png",
                png(1_024, 1_024, b"image-b-secret"),
            ),
            file(
                "mask",
                "private-mask.png",
                "image/png",
                png(1_024, 1_024, b"mask-secret"),
            ),
        ])
        .unwrap(),
    )
    .unwrap();

    assert_eq!(request.operation(), af_domain::Operation::ImageEdit);
    assert_eq!(request.images().len(), 2);
    assert_eq!(request.total_image_pixels(), 3_145_728);
    assert_eq!(request.mask().unwrap().format(), ImageOutputFormat::Png);
    assert_eq!(request.input_fidelity(), Some(ImageInputFidelity::High));
    let debug = format!("{request:?}");
    assert!(!debug.contains("private edit prompt"));
    assert!(!debug.contains("image-a-secret"));
    assert!(!debug.contains("mask-secret"));
    assert!(!debug.contains("sdk-user-id"));
    assert_eq!(
        request
            .with_model("upstream-image-model".to_owned())
            .unwrap()
            .input_fidelity(),
        Some(ImageInputFidelity::High)
    );

    let body = build_edit_multipart(&request, "anyflows-image-edit-test-boundary").unwrap();
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("name=\"image[]\"; filename=\"image-0.png\""));
    assert!(body.contains("name=\"image[]\"; filename=\"image-1.png\""));
    assert!(body.contains("name=\"mask\"; filename=\"mask.png\""));
    assert!(body.contains("name=\"input_fidelity\"\r\n\r\nhigh"));
    assert!(!body.contains("sdk-user-id"));
    assert!(!body.contains("client-private-name"));
}

#[test]
fn rejects_missing_duplicate_unknown_and_unsupported_fields() {
    let cases = [
        vec![text("prompt", "x")],
        vec![text("model", "m"), text("prompt", "x"), text("prompt", "y")],
        vec![
            text("model", "m"),
            text("prompt", "x"),
            text("unknown", "x"),
        ],
        vec![
            text("model", "m"),
            text("prompt", "x"),
            text("partial_images", "1"),
        ],
    ];
    for (index, parts) in cases.into_iter().enumerate() {
        let form = OpenAiImageEditForm::new(parts).unwrap();
        if index == 0 {
            assert_eq!(
                parse_edit_request(form),
                Err(ParseImageEditRequestError::MissingField)
            );
        } else if index == 1 {
            assert_eq!(
                parse_edit_request(form),
                Err(ParseImageEditRequestError::DuplicateField)
            );
        } else if index == 2 {
            assert_eq!(
                parse_edit_request(form),
                Err(ParseImageEditRequestError::UnknownField)
            );
        } else {
            assert_eq!(
                parse_edit_request(form),
                Err(ParseImageEditRequestError::UnsupportedFeature)
            );
        }
    }
}

#[test]
fn rejects_duplicate_mask_invalid_mime_signature_and_mask_dimensions() {
    let valid = || png(1_024, 1_024, b"payload");
    let duplicate_mask = OpenAiImageEditForm::new(vec![
        text("model", "m"),
        text("prompt", "x"),
        file("image", "a.png", "image/png", valid()),
        file("mask", "mask.png", "image/png", valid()),
        file("mask", "mask2.png", "image/png", valid()),
    ])
    .unwrap();
    assert_eq!(
        parse_edit_request(duplicate_mask),
        Err(ParseImageEditRequestError::DuplicateField)
    );

    for invalid_file in [
        file("image", "a.jpg", "image/jpeg", valid()),
        file("image", "a.png", "image/jpeg", valid()),
        file(
            "image",
            "a.png",
            "image/png",
            Bytes::from_static(b"not-an-image"),
        ),
    ] {
        let form =
            OpenAiImageEditForm::new(vec![text("model", "m"), text("prompt", "x"), invalid_file])
                .unwrap();
        assert_eq!(
            parse_edit_request(form),
            Err(ParseImageEditRequestError::InvalidValue)
        );
    }

    let mismatched_mask = OpenAiImageEditForm::new(vec![
        text("model", "m"),
        text("prompt", "x"),
        file("image", "a.png", "image/png", valid()),
        file("mask", "mask.png", "image/png", png(2_048, 1_024, b"mask")),
    ])
    .unwrap();
    assert_eq!(
        parse_edit_request(mismatched_mask),
        Err(ParseImageEditRequestError::InvalidValue)
    );
}

#[test]
fn accepts_indexed_image_fields_and_rejects_duplicate_indexes_and_oversized_masks() {
    let first = png(16, 16, b"first");
    let second = png(16, 16, b"second");
    let request = parse_edit_request(
        OpenAiImageEditForm::new(vec![
            text("model", "m"),
            text("prompt", "x"),
            file("image[1]", "second.png", "image/png", second.clone()),
            file("image[0]", "first.png", "image/png", first.clone()),
        ])
        .unwrap(),
    )
    .unwrap();
    assert_eq!(request.images().len(), 2);
    assert_eq!(request.images()[0].bytes(), &first);
    assert_eq!(request.images()[1].bytes(), &second);
    let encoded =
        String::from_utf8_lossy(&build_edit_multipart(&request, "image-index-boundary").unwrap())
            .into_owned();
    assert_eq!(encoded.matches("name=\"image[]\"").count(), 2);

    let duplicate_index = OpenAiImageEditForm::new(vec![
        file("image[0]", "first.png", "image/png", png(16, 16, b"first")),
        file(
            "image[0]",
            "second.png",
            "image/png",
            png(16, 16, b"second"),
        ),
    ])
    .unwrap();
    assert_eq!(
        parse_edit_request(duplicate_index),
        Err(ParseImageEditRequestError::DuplicateField)
    );

    let mixed_image_styles = OpenAiImageEditForm::new(vec![
        text("model", "m"),
        text("prompt", "x"),
        file("image[0]", "first.png", "image/png", first),
        file("image", "second.png", "image/png", second),
    ])
    .unwrap();
    assert_eq!(
        parse_edit_request(mixed_image_styles),
        Err(ParseImageEditRequestError::InvalidValue)
    );

    let mut mask = png(16, 16, b"mask").to_vec();
    for size in [MAX_IMAGE_EDIT_MASK_BYTES - 1, MAX_IMAGE_EDIT_MASK_BYTES] {
        mask.resize(size, 0);
        let form = OpenAiImageEditForm::new(vec![file(
            "mask",
            "mask.png",
            "image/png",
            Bytes::from(mask.clone()),
        )]);
        assert_eq!(form.is_ok(), size < MAX_IMAGE_EDIT_MASK_BYTES);
    }
}

#[test]
fn enforces_form_budgets_and_boundary_validation() {
    let text_part = OpenAiImageEditFormPart::text(
        "prompt".to_owned(),
        "x".repeat(MAX_IMAGE_EDIT_FORM_TEXT_BYTES + 1),
    );
    assert!(text_part.is_err());

    let mut parts = vec![text("model", "m"), text("prompt", "x")];
    parts.push(file(
        "image",
        "a.png",
        "image/png",
        png(1_024, 1_024, b"boundary-secret"),
    ));
    let request = parse_edit_request(OpenAiImageEditForm::new(parts).unwrap()).unwrap();
    assert_eq!(
        build_edit_multipart(&request, "bad boundary"),
        Err(BuildImageEditRequestError::InvalidBoundary)
    );
    assert_eq!(
        build_edit_multipart(&request, "boundary-secret"),
        Err(BuildImageEditRequestError::BoundaryCollision)
    );
}

#[test]
fn rejects_total_file_budget_overflow() {
    let first = Bytes::from(vec![0_u8; 15 * 1_024 * 1_024]);
    let second = Bytes::from(vec![0_u8; 15 * 1_024 * 1_024 + 1]);
    let parts = vec![
        file("image", "a.png", "image/png", first),
        file("image", "b.png", "image/png", second),
    ];
    assert_eq!(
        OpenAiImageEditForm::new(parts),
        Err(OpenAiImageEditFormError::FileBudgetExceeded)
    );
}
