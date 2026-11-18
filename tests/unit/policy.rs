use vt_tray::core::policy::{
    CredentialStatus, DIRECT_UPLOAD_LIMIT, MAX_UPLOAD_LIMIT, UploadRoute, auto_check_allowed,
    consent_satisfied, requires_large_warning, upload_route,
};

#[test]
fn upload_routes_follow_service_limits() {
    assert_eq!(upload_route(1).unwrap(), UploadRoute::Direct);
    assert_eq!(
        upload_route(DIRECT_UPLOAD_LIMIT).unwrap(),
        UploadRoute::Direct
    );
    assert_eq!(
        upload_route(DIRECT_UPLOAD_LIMIT + 1).unwrap(),
        UploadRoute::UploadUrl
    );
    assert_eq!(
        upload_route(MAX_UPLOAD_LIMIT).unwrap(),
        UploadRoute::UploadUrl
    );
    assert!(upload_route(MAX_UPLOAD_LIMIT + 1).is_err());
    assert!(upload_route(0).is_err());
}

#[test]
fn large_file_warning_threshold_is_exclusive() {
    assert!(!requires_large_warning(200 * 1024 * 1024));
    assert!(requires_large_warning(200 * 1024 * 1024 + 1));
}

#[test]
fn consent_requires_a_timestamp() {
    assert!(!consent_satisfied(None));
    assert!(!consent_satisfied(Some(0)));
    assert!(consent_satisfied(Some(1)));
}

#[test]
fn automatic_checks_require_opt_in_and_budget() {
    assert!(!auto_check_allowed(
        false,
        false,
        CredentialStatus::Present,
        10
    ));
    assert!(!auto_check_allowed(
        true,
        true,
        CredentialStatus::Present,
        10
    ));
    assert!(!auto_check_allowed(
        true,
        false,
        CredentialStatus::Absent,
        10
    ));
    assert!(!auto_check_allowed(
        true,
        false,
        CredentialStatus::Present,
        0
    ));
    assert!(auto_check_allowed(
        true,
        false,
        CredentialStatus::Present,
        1
    ));
}
