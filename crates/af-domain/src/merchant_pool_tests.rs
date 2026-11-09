use crate::{validate_merchant_description, validate_merchant_text};

#[test]
fn merchant_descriptions_allow_lf_and_crlf_but_reject_other_controls() {
    assert!(validate_merchant_description("第一行\n第二行", 2048, true));
    assert!(validate_merchant_description(
        "第一行\r\n第二行",
        2048,
        true
    ));
    assert!(!validate_merchant_description("第一行\t第二行", 2048, true));
    assert!(!validate_merchant_description("第一行\r第二行", 2048, true));
    assert!(!validate_merchant_description(
        "第一行\u{0000}第二行",
        2048,
        true
    ));
    assert!(!validate_merchant_text("第一行\n第二行", 2048, true));
}

#[test]
fn merchant_description_byte_limit_and_trim_boundary_match_single_line_text() {
    assert!(validate_merchant_description("甲\n乙", 7, false));
    assert!(!validate_merchant_description("甲\n乙", 6, false));
    assert!(validate_merchant_description("甲\r\n乙", 8, false));
    assert!(!validate_merchant_description("甲\r\n乙", 7, false));
    assert!(!validate_merchant_description(" 甲\n乙", 2048, false));
    assert!(!validate_merchant_description("甲\n乙 ", 2048, false));
    assert!(validate_merchant_description("", 2048, true));
    assert!(!validate_merchant_description("", 2048, false));
}
