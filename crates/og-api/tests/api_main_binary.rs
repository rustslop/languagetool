use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const PORT: u16 = 8081;

fn spawn_server() -> Child {
    Command::new(env!("CARGO_BIN_EXE_og-server"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("og-server binary must be built alongside the tests")
}

fn wait_for_exit(child: &mut Child, secs: u64) -> ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().expect("child must be waitable") {
            return status;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    panic!("og-server did not exit within {secs}s");
}

fn connect_with_retry(child: &mut Child) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().expect("child must be waitable") {
            panic!("og-server exited before serving anything: {status}");
        }
        if let Ok(stream) = TcpStream::connect(("127.0.0.1", PORT)) {
            return stream;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    panic!("og-server did not start listening on port {PORT}");
}

#[test]
fn og_server_binary_serves_http_and_fails_cleanly_on_a_taken_port() {
    let listener = TcpListener::bind(("127.0.0.1", PORT)).expect("port must be free");
    let mut child = spawn_server();
    let status = wait_for_exit(&mut child, 30);
    assert_eq!(status.code(), Some(1), "bind failure must exit non-zero");
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("stderr must be piped")
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(stderr.contains("Server error"), "stderr was: {stderr}");
    drop(listener);

    let mut child = spawn_server();
    let mut stream = connect_with_retry(&mut child);
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream
        .write_all(b"GET /v2/version HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let response = String::from_utf8_lossy(&response);
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "unexpected status line in {response}"
    );
    assert!(response.contains("OpenGrammar"), "{response}");
    assert!(response.contains("\"apiVersion\":2"), "{response}");
    child.kill().unwrap();
    child.wait().unwrap();
}
