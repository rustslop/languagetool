use og_api::server::run_server;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn test_run_server_serves_the_version_endpoint() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let server = tokio::spawn(run_server(port));

    let mut stream = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(stream) = tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
                return stream;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("server must start accepting connections");

    stream
        .write_all(b"GET /v2/version HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();

    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut response))
        .await
        .expect("response must arrive")
        .unwrap();

    let response = String::from_utf8_lossy(&response);
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "unexpected status line in {response}"
    );
    assert!(response.contains("OpenGrammar"), "{response}");
    assert!(response.contains("\"apiVersion\":2"), "{response}");

    server.abort();
}
