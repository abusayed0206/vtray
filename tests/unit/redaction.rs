use vt_tray::core::errors::redact_error_text;

#[test]
fn redacts_header_like_error_text() {
    let text = "request failed x-apikey=abc123 other";
    let redacted = redact_error_text(text);
    assert!(!redacted.contains("abc123"));
    assert!(redacted.contains("[redacted]"));
}

#[test]
fn leaves_hashes_alone() {
    let hash = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    assert_eq!(redact_error_text(hash), hash);
}
