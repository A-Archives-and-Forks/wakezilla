use std::time::Duration;

use wakezilla::shutdown_auth::{
    generate_key, sign_request_at, ReplayGuard, SignedRequestHeaders, VerifyError,
};

const NOW: u64 = 1_700_000_000;

#[test]
fn valid_signature_is_accepted_once() {
    let key = generate_key();
    let headers = sign_request_at(&key, "POST", "/machines/turn-off", NOW).unwrap();
    let mut guard = ReplayGuard::new(Duration::from_secs(120), 16);

    guard
        .verify(&key, "POST", "/machines/turn-off", &headers, NOW)
        .expect("valid signature should be accepted");
}

#[test]
fn signature_from_another_key_is_rejected() {
    let expected_key = generate_key();
    let other_key = generate_key();
    let headers = sign_request_at(&other_key, "POST", "/machines/turn-off", NOW).unwrap();
    let mut guard = ReplayGuard::new(Duration::from_secs(120), 16);

    let error = guard
        .verify(&expected_key, "POST", "/machines/turn-off", &headers, NOW)
        .unwrap_err();

    assert_eq!(error, VerifyError::InvalidSignature);
}

#[test]
fn expired_timestamp_is_rejected() {
    let key = generate_key();
    let headers = sign_request_at(&key, "POST", "/machines/turn-off", NOW - 61).unwrap();
    let mut guard = ReplayGuard::new(Duration::from_secs(120), 16);

    let error = guard
        .verify(&key, "POST", "/machines/turn-off", &headers, NOW)
        .unwrap_err();

    assert_eq!(error, VerifyError::TimestampOutsideWindow);
}

#[test]
fn replayed_nonce_is_rejected() {
    let key = generate_key();
    let headers = sign_request_at(&key, "POST", "/machines/turn-off", NOW).unwrap();
    let mut guard = ReplayGuard::new(Duration::from_secs(120), 16);

    guard
        .verify(&key, "POST", "/machines/turn-off", &headers, NOW)
        .unwrap();
    let error = guard
        .verify(&key, "POST", "/machines/turn-off", &headers, NOW)
        .unwrap_err();

    assert_eq!(error, VerifyError::Replay);
}

#[test]
fn malformed_key_is_rejected_before_signing() {
    let error = sign_request_at("YWJj", "GET", "/health/secure", NOW).unwrap_err();
    assert!(error.to_string().contains("32 bytes"));
}

#[test]
fn generated_key_is_url_safe_and_256_bits() {
    let key = generate_key();
    assert_eq!(key.len(), 43);
    assert!(key
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')));
}

#[test]
fn headers_are_plain_serializable_values() {
    let headers = SignedRequestHeaders {
        timestamp: "1700000000".into(),
        nonce: "nonce".into(),
        signature: "signature".into(),
    };
    assert_eq!(headers.timestamp, "1700000000");
}
