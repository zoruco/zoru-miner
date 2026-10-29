use anyhow::{Result, bail, ensure};
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::Read,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub struct Rpc {
    pub url: String,
    http: Client,
    rate: Mutex<Instant>,
    interval: Duration,
    id: AtomicU64,
    pub latencies: Mutex<VecDeque<f64>>,
}
impl Rpc {
    pub fn new(url: &str, interval: Duration) -> Result<Self> {
        let parsed = reqwest::Url::parse(url)?;
        ensure!(
            parsed.scheme() == "https"
                || (parsed.scheme() == "http" && parsed.host_str() == Some("127.0.0.1")),
            "Use HTTPS or a local RPC connection"
        );
        // VEX's HTTP/1 RPC requires conventional header casing.
        Ok(Self {
            url: url.into(),
            http: Client::builder()
                .http1_title_case_headers()
                .timeout(Duration::from_secs(12))
                .build()?,
            rate: Mutex::new(Instant::now() - interval),
            interval,
            id: AtomicU64::new(1),
            latencies: Mutex::new(VecDeque::new()),
        })
    }
    pub fn request(&self, method: &str, params: Value) -> Result<Value> {
        {
            let mut last = self.rate.lock().unwrap();
            let interval = if method == "eth_sendRawTransaction" {
                self.interval
            } else {
                self.interval.min(Duration::from_millis(80))
            };
            let wait = interval.saturating_sub(last.elapsed());
            if !wait.is_zero() {
                thread::sleep(wait);
            }
            *last = Instant::now();
        }
        let start = Instant::now();
        let id = self.id.fetch_add(1, Ordering::Relaxed);
        let result = (|| {
            let response = self
                .http
                .post(&self.url)
                .json(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
                .send()
                .map_err(|_| anyhow::anyhow!("RPC transport unavailable"))?;
            ensure!(
                response.status().is_success(),
                "{method}: RPC returned HTTP {}",
                response.status().as_u16()
            );
            let mut bytes = Vec::new();
            response.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() <= 8 * 1024 * 1024,
                "RPC response exceeded the limit"
            );
            let value: Value = serde_json::from_slice(&bytes)?;
            ensure!(
                value["jsonrpc"] == "2.0"
                    && value["id"] == id
                    && (value.get("result").is_some() ^ value.get("error").is_some()),
                "{method}: invalid JSON-RPC response envelope"
            );
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let detail = if message.contains("has insufficient ram") {
                    ": native RAM capacity is exhausted"
                } else if message.contains("min_inclusion_price") {
                    ": gas price is below the relay inclusion minimum"
                } else {
                    ""
                };
                let data = error
                    .get("data")
                    .and_then(Value::as_str)
                    .filter(|v| {
                        v.starts_with("0x") && v.len() <= 66 && hex::decode(&v[2..]).is_ok()
                    })
                    .unwrap_or_default();
                bail!(
                    "{method}: RPC error {} {data}{detail}",
                    error.get("code").and_then(Value::as_i64).unwrap_or(0)
                );
            }
            value
                .get("result")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("RPC result missing"))
        })();
        let mut times = self.latencies.lock().unwrap();
        times.push_back(start.elapsed().as_secs_f64());
        if times.len() > 2048 {
            times.pop_front();
        }
        result
    }
    pub fn native(&self) -> Result<Value> {
        let response = self
            .http
            .post(format!(
                "{}/v1/chain/get_info",
                self.url.trim_end_matches('/')
            ))
            .json(&json!({}))
            .send()
            .map_err(|_| anyhow::anyhow!("Native RPC transport unavailable"))?;
        ensure!(
            response.status().is_success(),
            "Native RPC returned HTTP {}",
            response.status().as_u16()
        );
        let mut bytes = Vec::new();
        response.take(65537).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 65536,
            "Native RPC response exceeded the limit"
        );
        Ok(serde_json::from_slice(&bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener};

    fn serve_response(body: Value) -> (Rpc, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut data = Vec::new();
            let mut byte = [0u8; 1];
            while !data.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                data.push(byte[0]);
            }
            let headers = String::from_utf8(data).unwrap();
            assert!(headers.contains("\r\nContent-Length:"));
            assert!(headers.contains("\r\nContent-Type: application/json"));
            let size = headers
                .lines()
                .find_map(|v| v.strip_prefix("Content-Length: "))
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let mut request_body = vec![0; size];
            stream.read_exact(&mut request_body).unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&request_body).unwrap()["method"],
                "eth_chainId"
            );
            let body = body.to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let rpc = Rpc::new(&format!("http://{address}"), Duration::ZERO).unwrap();
        (rpc, server)
    }

    #[test]
    fn vex_http_headers_are_title_case() {
        let (rpc, server) = serve_response(json!({"jsonrpc":"2.0","id":1,"result":"0x1a50"}));
        assert_eq!(rpc.request("eth_chainId", json!([])).unwrap(), "0x1a50");
        server.join().unwrap();
    }

    #[test]
    fn mismatched_or_ambiguous_rpc_responses_are_rejected() {
        for response in [
            json!({"jsonrpc":"2.0","id":2,"result":"0x1a50"}),
            json!({"id":1,"result":"0x1a50"}),
            json!({"jsonrpc":"2.0","id":1,"result":"0x1a50","error":{"code":-32000}}),
            json!({"jsonrpc":"2.0","id":1}),
        ] {
            let (rpc, server) = serve_response(response);
            assert!(
                rpc.request("eth_chainId", json!([]))
                    .unwrap_err()
                    .to_string()
                    .contains("invalid JSON-RPC response envelope")
            );
            server.join().unwrap();
        }
    }

    #[test]
    fn native_resource_and_inclusion_failures_are_identifiable_without_echoing_rpc_text() {
        for (message, expected) in [
            (
                "account vex.evm has insufficient ram; needs 5995936 bytes has 5995719 bytes",
                "native RAM capacity is exhausted",
            ),
            (
                "assertion failure with message: inclusion price must >= min_inclusion_price",
                "gas price is below the relay inclusion minimum",
            ),
        ] {
            let (rpc, server) = serve_response(
                json!({"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":message}}),
            );
            let error = rpc
                .request("eth_chainId", json!([]))
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected));
            assert!(!error.contains(message));
            server.join().unwrap();
        }
    }

    #[test]
    #[ignore = "requires an explicitly selected public RPC; sends read-only identity requests"]
    fn public_rpc_identity_readonly() {
        let url = std::env::var("ZORU_PUBLIC_RPC").expect("ZORU_PUBLIC_RPC");
        let expected: u64 = std::env::var("ZORU_PUBLIC_CHAIN_ID")
            .expect("ZORU_PUBLIC_CHAIN_ID")
            .parse()
            .unwrap();
        let rpc = Rpc::new(&url, Duration::from_millis(150)).unwrap();
        let chain = rpc.request("eth_chainId", json!([])).unwrap();
        assert_eq!(
            u64::from_str_radix(chain.as_str().unwrap().trim_start_matches("0x"), 16).unwrap(),
            expected
        );
        let height = rpc.request("eth_blockNumber", json!([])).unwrap();
        assert!(u64::from_str_radix(height.as_str().unwrap().trim_start_matches("0x"), 16).is_ok());
    }
}
