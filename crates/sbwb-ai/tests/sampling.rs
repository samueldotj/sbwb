//! A newer Claude model refuses `temperature` with a 400 error; the
//! request SBWB sends must leave it out and still reach a validated reply.
//! A stand-in server on localhost plays the Messages API.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use sbwb_ai::{check_page, prompt, Budget, Connection, PagePayload, Provider};

/// Answer `n` requests like a model that rejects sampling settings, and
/// send each request body back to the test.
fn stand_in(n: usize) -> (String, mpsc::Receiver<serde_json::Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(n) {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut len = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let (status, reply) = if body.get("temperature").is_some() {
                (
                    "400 Bad Request",
                    serde_json::json!({"type": "error", "error": {"type": "invalid_request_error", "message": "`temperature` is deprecated for this model."}}),
                )
            } else {
                (
                    "200 OK",
                    serde_json::json!({
                        "content": [
                            {"type": "thinking", "thinking": "", "signature": "x"},
                            {"type": "text", "text": "{\"suggestions\":[{\"id\":1,\"before\":\"carricd\",\"after\":\"carried\",\"reason\":\"OCR\"}]}"}
                        ],
                        "stop_reason": "end_turn",
                        "usage": {"input_tokens": 900, "output_tokens": 40}
                    }),
                )
            };
            tx.send(body).unwrap();
            let reply = reply.to_string();
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            )
            .unwrap();
        }
    });
    (format!("http://{addr}"), rx)
}

#[test]
fn newer_claude_models_get_no_temperature() {
    let (base, bodies) = stand_in(1);
    // the only test in this binary, so setting the override is safe
    std::env::set_var("SBWB_AI_ANTHROPIC_BASE", &base);
    let conn = Connection {
        provider: Provider::Anthropic,
        model: "claude-sonnet-5".into(),
        api_key: "test".into(),
    };
    let payload = PagePayload {
        page_number: 1,
        words: vec!["The".into(), "carricd".into(), "it.".into()],
    };
    let out = check_page(
        &conn,
        &prompt::system(None),
        &payload,
        &Budget::default(),
        &AtomicBool::new(false),
    )
    .expect("the request is accepted");
    let body = bodies.recv().unwrap();
    assert!(body.get("temperature").is_none());
    assert_eq!(body["max_tokens"], 16000);
    // the thinking block is skipped and the text block validated
    assert_eq!(out.suggestions.len(), 1);
    assert_eq!(out.suggestions[0].after, "carried");
    assert_eq!(out.usage.output_tokens, Some(40));
}
