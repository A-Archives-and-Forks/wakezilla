use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use wakezilla::client_server;
use wakezilla::shutdown_auth::{
    generate_key, sign_request, NONCE_HEADER, SIGNATURE_HEADER, TIMESTAMP_HEADER,
};

#[tokio::test]
async fn secure_health_rejects_unsigned_request_when_key_is_configured() {
    let app = client_server::build_router(Some(generate_key()));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health/secure")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn secure_health_accepts_request_signed_with_configured_key() {
    let key = generate_key();
    let signed = sign_request(&key, "GET", "/health/secure").unwrap();
    let app = client_server::build_router(Some(key));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health/secure")
                .header(TIMESTAMP_HEADER, signed.timestamp)
                .header(NONCE_HEADER, signed.nonce)
                .header(SIGNATURE_HEADER, signed.signature)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn configured_client_rejects_unsigned_shutdown() {
    let app = client_server::build_router(Some(generate_key()));
    let response = app
        .oneshot(
            Request::post("/machines/turn-off")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn public_health_remains_available_with_key_configured() {
    let app = client_server::build_router(Some(generate_key()));
    let response = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
