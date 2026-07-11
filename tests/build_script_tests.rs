#[path = "../build_support.rs"]
mod build_support;

#[test]
fn release_build_reuses_a_prebuilt_frontend_when_available() {
    assert!(!build_support::frontend_build_is_required(true, true, true));
    assert!(build_support::frontend_build_is_required(true, false, true));
    assert!(build_support::frontend_build_is_required(true, true, false));
}
