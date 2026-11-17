use robot_common_lib::vla::{VlaError, VlaJoints, VlaOptions};
use robot_third_party_client::{VlaClient, traits::VlaClientTrait};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
};

fn server(status: &str, body: Value) -> (String, std::thread::JoinHandle<Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.to_string().len()
    );
    let thread = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        let (start, size) = loop {
            let mut buffer = [0; 1024];
            let n = stream.read(&mut buffer).unwrap();
            assert_ne!(n, 0);
            bytes.extend_from_slice(&buffer[..n]);
            if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                assert!(headers.starts_with("POST /infer HTTP/1.1"));
                let size = headers
                    .lines()
                    .find_map(|s| {
                        s.to_lowercase()
                            .strip_prefix("content-length: ")
                            .map(|s| s.parse::<usize>().unwrap())
                    })
                    .unwrap();
                break (end + 4, size);
            }
        };
        while bytes.len() < start + size {
            let mut buffer = [0; 1024];
            let n = stream.read(&mut buffer).unwrap();
            assert_ne!(n, 0);
            bytes.extend_from_slice(&buffer[..n]);
        }
        let request = serde_json::from_slice(&bytes[start..start + size]).unwrap();
        stream.write_all(response.as_bytes()).unwrap();
        request
    });
    (url, thread)
}

#[tokio::test]
async fn sends_contract_and_reads_joint_actions_over_http() {
    let (url, server) = server(
        "200 OK",
        json!({"actions":[{"name":["joint"],"position":[0.5]}],"done":true}),
    );
    let client = VlaClient::new(&VlaOptions {
        base_url: url,
        ..Default::default()
    })
    .unwrap();
    let observation = VlaJoints {
        name: vec!["joint".into()],
        position: vec![0.0],
    };
    let result = client.infer("arm", "hold", &observation).await.unwrap();
    assert!(result.done);
    assert_eq!(result.actions[0].position, vec![0.5]);
    assert_eq!(
        server.join().unwrap(),
        json!({"profileId":"arm","task":"hold","observation":{"name":["joint"],"position":[0.0]}})
    );
}

#[tokio::test]
async fn remote_errors_and_malformed_actions_are_unavailable() {
    for (status, body) in [
        ("503 Unavailable", json!({"message":"remote failure"})),
        (
            "200 OK",
            json!({"actions":[{"name":["joint"],"position":[]}],"done":true}),
        ),
        ("200 OK", json!({"actions":[],"done":true,"unexpected":0})),
    ] {
        let (url, server) = server(status, body);
        let client = VlaClient::new(&VlaOptions {
            base_url: url,
            ..Default::default()
        })
        .unwrap();
        let error = client
            .infer(
                "arm",
                "hold",
                &VlaJoints {
                    name: vec!["joint".into()],
                    position: vec![0.0],
                },
            )
            .await
            .err()
            .unwrap();
        assert_eq!(
            error.downcast_ref::<VlaError>(),
            Some(&VlaError::Unavailable)
        );
        server.join().unwrap();
    }
}
