use vt_tray::core::types::LookupSummary;
use vt_tray::ui::strings::{consent_body, result_body};

#[test]
fn consent_text_carries_every_required_element() {
    let text = consent_body("tool.exe", "1.2 KB", "abc123", false);
    for required in ["tool.exe", "1.2 KB", "abc123", "security community"] {
        assert!(text.contains(required), "consent text lacks {required:?}");
    }
    assert!(!text.contains("200 MB"));
    assert!(consent_body("big.iso", "300 MB", "def", true).contains("larger than 200 MB"));
}

#[test]
fn error_messages_are_plain_and_hide_technical_detail() {
    use vt_tray::core::errors::{ApiError, AppError};
    use vt_tray::core::types::QuotaDenial;
    use vt_tray::ui::strings::error_message;

    let network = AppError::Api(ApiError::Network(
        "io: Connection refused (os error 10061)".into(),
    ));
    let text = error_message(&network);
    assert!(text.contains("could not reach VirusTotal"));
    assert!(!text.contains("10061"));

    let quota = error_message(&AppError::QuotaDenied(QuotaDenial::Day));
    assert!(quota.contains("00:00 UTC"));
    let slow = error_message(&AppError::Api(ApiError::RateLimited {
        retry_after: Some(90),
    }));
    assert!(slow.contains("90 seconds"));
    let outage = error_message(&AppError::Api(ApiError::Http {
        status: 503,
        message: "upstream connect error".into(),
    }));
    assert!(!outage.contains("upstream"));
}

#[test]
fn result_text_is_advisory() {
    let summary = LookupSummary {
        sha256: "abc".into(),
        size_bytes: 1,
        malicious: 0,
        suspicious: 0,
        undetected: 60,
        harmless: 0,
        timeout: 0,
        reputation: None,
        type_description: None,
        meaningful_name: None,
        first_submission_at: None,
        last_analysis_at: None,
    };
    let text = result_body(&summary, "a.exe", "1 B", "unknown", true);
    assert!(text.contains("no detections"));
    assert!(text.contains("advisory"));
    assert!(text.contains("Source: local cache"));
}

#[test]
fn quota_warnings_say_what_happens_and_when_it_resets() {
    use vt_tray::core::quota::QuotaWarning;
    use vt_tray::ui::strings::quota_warning_body;

    assert!(quota_warning_body(QuotaWarning::AutoPaused).contains("100 remain for manual"));
    assert!(quota_warning_body(QuotaWarning::DayLow { remaining: 7 }).contains("Only 7"));
    assert!(quota_warning_body(QuotaWarning::DayLow { remaining: 0 }).contains("used up"));
    assert!(quota_warning_body(QuotaWarning::MonthLow { remaining: 900 }).contains("this month"));
}

#[test]
fn removal_text_lists_every_step_and_any_failure() {
    use vt_tray::ui::strings::{REMOVE_APP_BODY, removed_summary};

    for step in ["startup", "Explorer menu", "API key", "local data"] {
        assert!(
            REMOVE_APP_BODY.contains(step),
            "confirmation lacks {step:?}"
        );
    }
    assert!(REMOVE_APP_BODY.contains("not touched"));
    let folder = std::path::Path::new(r"C:\Users\Test\AppData\Local\VTTray");
    let clean = removed_summary(&[], folder);
    assert!(clean.contains("removed") && clean.contains(r"AppData\Local\VTTray"));
    let partial = removed_summary(&["API key: access denied".to_string()], folder);
    assert!(partial.contains("some steps failed") && partial.contains("access denied"));
}

#[test]
fn folder_labels_hide_the_long_path_prefix() {
    use std::path::Path;
    use vt_tray::ui::strings::{display_path, menu_remove_folder};

    assert_eq!(display_path(Path::new(r"\\?\D:\vt\watch")), r"D:\vt\watch");
    assert_eq!(
        display_path(Path::new(r"\\?\UNC\server\share\dir")),
        r"\\server\share\dir"
    );
    assert_eq!(display_path(Path::new(r"C:\plain")), r"C:\plain");
    assert_eq!(
        menu_remove_folder(Path::new(r"\\?\E:\usb"), true),
        r"Remove: E:\usb (unavailable)"
    );
}
