use std::io::ErrorKind;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use wakezilla::forward;
use wakezilla::shutdown_auth::{
    generate_key, ReplayGuard, SignedRequestHeaders, NONCE_HEADER, SIGNATURE_HEADER,
    TIMESTAMP_HEADER,
};

#[tokio::test]
async fn turn_off_remote_machine_sends_post_request() {
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(err) if err.kind() == ErrorKind::PermissionDenied => {
            eprintln!(
                "skipping forward test because binding TCP sockets is not permitted: {}",
                err
            );
            return;
        }
        Err(err) => panic!("failed to bind http test listener: {err}"),
    };
    let addr = listener.local_addr().expect("failed to read listener addr");

    let received = Arc::new(Mutex::new(None));
    let received_clone = received.clone();

    let server_task = tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = vec![0u8; 1024];
            if let Ok(n) = socket.read(&mut buf).await {
                if n > 0 {
                    let request = String::from_utf8_lossy(&buf[..n]).to_string();
                    *received_clone.lock().await = Some(request);
                }
            }
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .await;
        }
    });

    let key = generate_key();
    forward::turn_off_remote_machine(&addr.ip().to_string(), addr.port(), Some(&key))
        .await
        .expect("turn_off_remote_machine should succeed");

    server_task.await.expect("server task panicked");

    let request = received.lock().await.clone().expect("no request captured");
    assert!(request.starts_with("POST /machines/turn-off"));

    let header = |name: &str| {
        request.lines().find_map(|line| {
            let (candidate, value) = line.split_once(':')?;
            candidate
                .eq_ignore_ascii_case(name)
                .then(|| value.trim().to_string())
        })
    };
    let signed = SignedRequestHeaders {
        timestamp: header(TIMESTAMP_HEADER).expect("timestamp header missing"),
        nonce: header(NONCE_HEADER).expect("nonce header missing"),
        signature: header(SIGNATURE_HEADER).expect("signature header missing"),
    };
    ReplayGuard::default()
        .verify(
            &key,
            "POST",
            "/machines/turn-off",
            &signed,
            wakezilla::shutdown_auth::unix_timestamp(),
        )
        .expect("proxy request should have a valid signature");

    let host_line = request
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("host:"))
        .unwrap_or_else(|| panic!("Host header missing in request: {request}"));

    let host_value = host_line.split_once(':').map(|(_, value)| value.trim());
    let expected_ip = addr.ip().to_string();
    let expected_with_port = format!("{}:{}", expected_ip, addr.port());
    assert!(
        matches!(host_value, Some(value) if value.eq_ignore_ascii_case(&expected_ip) || value.eq_ignore_ascii_case(&expected_with_port)),
        "unexpected host header: {host_line}"
    );
}

#[tokio::test]
async fn verify_remote_client_sends_signed_secure_health_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let key = generate_key();
    let key_for_server = key.clone();

    let server_task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 2048];
        let n = socket.read(&mut buf).await.unwrap();
        let request = String::from_utf8_lossy(&buf[..n]);
        assert!(request.starts_with("GET /health/secure"));
        let header = |name: &str| {
            request.lines().find_map(|line| {
                let (candidate, value) = line.split_once(':')?;
                candidate
                    .eq_ignore_ascii_case(name)
                    .then(|| value.trim().to_string())
            })
        };
        let signed = SignedRequestHeaders {
            timestamp: header(TIMESTAMP_HEADER).unwrap(),
            nonce: header(NONCE_HEADER).unwrap(),
            signature: header(SIGNATURE_HEADER).unwrap(),
        };
        ReplayGuard::default()
            .verify(
                &key_for_server,
                "GET",
                "/health/secure",
                &signed,
                wakezilla::shutdown_auth::unix_timestamp(),
            )
            .unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .await
            .unwrap();
    });

    let status = forward::verify_remote_client(&addr.ip().to_string(), addr.port(), &key).await;
    assert_eq!(status, forward::ClientVerification::Verified);
    server_task.await.unwrap();
}

#[tokio::test]
async fn verify_remote_client_distinguishes_key_mismatch() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 1024];
        let _ = socket.read(&mut buf).await.unwrap();
        socket
            .write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n")
            .await
            .unwrap();
    });

    let status =
        forward::verify_remote_client(&addr.ip().to_string(), addr.port(), &generate_key()).await;
    assert_eq!(status, forward::ClientVerification::KeyMismatch);
    server_task.await.unwrap();
}

#[tokio::test]
async fn turn_off_remote_machine_returns_error_for_rejected_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 1024];
        let _ = socket.read(&mut buf).await.unwrap();
        socket
            .write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n")
            .await
            .unwrap();
    });

    let result = forward::turn_off_remote_machine(
        &addr.ip().to_string(),
        addr.port(),
        Some(&generate_key()),
    )
    .await;

    assert!(result.is_err());
    server_task.await.unwrap();
}
