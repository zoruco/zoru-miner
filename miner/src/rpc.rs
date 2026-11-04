use anyhow::{Result, bail, ensure};
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::Read,
    sync::{
        Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
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
    requests: AtomicU64,
    endpoints: Vec<String>,
    active: AtomicUsize,
    expected_chain: Option<u64>,
    failovers: AtomicU64,
    pub latencies: Mutex<VecDeque<f64>>,
}
impl Rpc {
    fn validate_url(url: &str) -> Result<()> {
        let parsed = reqwest::Url::parse(url)?;
        ensure!(
            parsed.username().is_empty()
                && parsed.password().is_none()
                && parsed.query().is_none()
                && parsed.fragment().is_none()
                && (parsed.scheme() == "https"
                    || (parsed.scheme() == "http"
                        && matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")))),
            "Use HTTPS or a local RPC connection"
        );
        Ok(())
    }
    pub fn new(url: &str, interval: Duration) -> Result<Self> {
        Self::validate_url(url)?;
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
            requests: AtomicU64::new(0),
            endpoints: vec![url.into()],
            active: AtomicUsize::new(0),
            expected_chain: None,
            failovers: AtomicU64::new(0),
            latencies: Mutex::new(VecDeque::new()),
        })
    }
    pub fn with_fallbacks(mut self, urls: &[String], expected_chain: u64) -> Result<Self> {
        ensure!(urls.len() <= 3, "Too many RPC fallback endpoints");
        for url in urls {
            Self::validate_url(url)?;
            if !self.endpoints.contains(url) {
                self.endpoints.push(url.clone());
            }
        }
        self.expected_chain = Some(expected_chain);
        Ok(self)
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
        let index = self.active.load(Ordering::Relaxed);
        let result = self.post(&self.endpoints[index], method, &params);
        let result = match result {
            Err(error) if self.expected_chain.is_some() && transport_failure(&error) => {
                let mut fallback = None;
                for distance in 1..self.endpoints.len() {
                    let candidate = (index + distance) % self.endpoints.len();
                    if let Ok(identity) =
                        self.post(&self.endpoints[candidate], "eth_chainId", &json!([]))
                    {
                        let chain = identity
                            .as_str()
                            .and_then(|value| value.strip_prefix("0x"))
                            .and_then(|value| u64::from_str_radix(value, 16).ok());
                        if chain == self.expected_chain {
                            fallback = Some(candidate);
                            break;
                        }
                    }
                }
                if let Some(candidate) = fallback {
                    self.active.store(candidate, Ordering::Relaxed);
                    self.failovers.fetch_add(1, Ordering::Relaxed);
                    // A failed send may already be accepted. Let the journal
                    // reconcile its receipt before any repeat broadcast.
                    if method == "eth_sendRawTransaction" {
                        Err(error)
                    } else {
                        self.post(&self.endpoints[candidate], method, &params)
                    }
                } else {
                    Err(error)
                }
            }
            other => other,
        };
        let mut times = self.latencies.lock().unwrap();
        times.push_back(start.elapsed().as_secs_f64());
        if times.len() > 2048 {
            times.pop_front();
        }
        result
    }
    fn post(&self, url: &str, method: &str, params: &Value) -> Result<Value> {
        self.requests.fetch_add(1, Ordering::Relaxed);
        let id = self.id.fetch_add(1, Ordering::Relaxed);
        let response = self
            .http
            .post(url)
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
            } else if message.contains("max_transaction_cpu_usage") {
                ": native execution CPU limit exceeded"
            } else {
                ""
            };
            let data = error
                .get("data")
                .and_then(Value::as_str)
                .filter(|v| v.starts_with("0x") && v.len() <= 66 && hex::decode(&v[2..]).is_ok())
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
    }
    pub fn native(&self) -> Result<Value> {
        self.requests.fetch_add(1, Ordering::Relaxed);
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

    pub fn request_count(&self) -> u64 {
        self.requests.load(Ordering::Relaxed)
    }
    pub fn active_url(&self) -> &str {
        &self.endpoints[self.active.load(Ordering::Relaxed)]
    }
    pub fn failover_count(&self) -> u64 {
        self.failovers.load(Ordering::Relaxed)
    }
}

fn transport_failure(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message == "RPC transport unavailable" || message.contains(": RPC returned HTTP ")
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

    fn sequence(responses: Vec<(&'static str, Value)>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for (method, result) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut headers = Vec::new();
                while !headers.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    headers.push(byte[0]);
                }
                let size = String::from_utf8(headers)
                    .unwrap()
                    .lines()
                    .find_map(|line| line.strip_prefix("Content-Length: "))
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                let mut body = vec![0; size];
                stream.read_exact(&mut body).unwrap();
                let request: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(request["method"], method);
                let response =
                    json!({"jsonrpc":"2.0","id":request["id"],"result":result}).to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
            }
        });
        (url, server)
    }

    #[test]
    fn transport_failure_switches_reads_only_after_verifying_the_fallback_chain() {
        let primary = TcpListener::bind("127.0.0.1:0").unwrap();
        let unavailable = format!("http://{}", primary.local_addr().unwrap());
        let (secondary, server) = sequence(vec![
            ("eth_chainId", json!("0x1a50")),
            ("eth_blockNumber", json!("0x42")),
            ("eth_blockNumber", json!("0x43")),
        ]);
        drop(primary);
        let rpc = Rpc::new(&unavailable, Duration::ZERO)
            .unwrap()
            .with_fallbacks(std::slice::from_ref(&secondary), 6736)
            .unwrap();
        assert_eq!(rpc.request("eth_blockNumber", json!([])).unwrap(), "0x42");
        assert_eq!(rpc.request("eth_blockNumber", json!([])).unwrap(), "0x43");
        assert_eq!(rpc.active_url(), secondary);
        assert_eq!(rpc.failover_count(), 1);
        assert_eq!(rpc.request_count(), 4);
        server.join().unwrap();
    }

    #[test]
    fn a_fallback_on_another_chain_is_never_used_for_the_requested_operation() {
        let primary = TcpListener::bind("127.0.0.1:0").unwrap();
        let unavailable = format!("http://{}", primary.local_addr().unwrap());
        let (secondary, server) = sequence(vec![("eth_chainId", json!("0xb690"))]);
        drop(primary);
        let rpc = Rpc::new(&unavailable, Duration::ZERO)
            .unwrap()
            .with_fallbacks(&[secondary], 6736)
            .unwrap();
        assert!(rpc.request("eth_blockNumber", json!([])).is_err());
        assert_eq!(rpc.active_url(), unavailable);
        assert_eq!(rpc.failover_count(), 0);
        server.join().unwrap();
    }

    #[test]
    fn a_failed_send_selects_recovery_endpoint_without_replaying_the_transaction() {
        let primary = TcpListener::bind("127.0.0.1:0").unwrap();
        let unavailable = format!("http://{}", primary.local_addr().unwrap());
        let (secondary, server) = sequence(vec![("eth_chainId", json!("0x1a50"))]);
        drop(primary);
        let rpc = Rpc::new(&unavailable, Duration::ZERO)
            .unwrap()
            .with_fallbacks(std::slice::from_ref(&secondary), 6736)
            .unwrap();
        assert!(
            rpc.request("eth_sendRawTransaction", json!(["0x1234"]))
                .is_err()
        );
        assert_eq!(rpc.active_url(), secondary);
        assert_eq!(
            rpc.request_count(),
            2,
            "receipt reconciliation must precede any repeat broadcast"
        );
        server.join().unwrap();
    }

    #[test]
    fn fallback_configuration_rejects_credentials_and_unbounded_endpoint_lists() {
        assert!(
            Rpc::new("http://127.0.0.1:1", Duration::ZERO)
                .unwrap()
                .with_fallbacks(&["https://user:password@example.com/rpc".into()], 6736)
                .is_err()
        );
        assert!(
            Rpc::new("http://127.0.0.1:1", Duration::ZERO)
                .unwrap()
                .with_fallbacks(&vec!["https://example.com/rpc".into(); 4], 6736)
                .is_err()
        );
    }
}
