use crate::{
    abi,
    pow::{self, Work},
    rpc::Rpc,
    storage,
};
use alloy_consensus::{
    SignableTransaction, Transaction, TxEnvelope, TxLegacy, transaction::SignerRecoverable,
};
use alloy_eips::eip2718::{Decodable2718, Encodable2718};
use alloy_primitives::{Address, B256, Bytes, TxKind, U256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub const CHAIN: u64 = 6736;
pub const TEST_CHAIN: u64 = 46736;
pub const MAX_TRANSACTION_FEE: u128 = 3_300_000_000_000_000_000;
pub const NATIVE_ID: &str = "f9f432b1851b5c179d2091a96f593aaed50ec7466b74f89301f957a83e56ce1f";
pub const TEST_NATIVE_ID: &str = "c6b5914584c79ad99944f4f77fe813fa2e41373058dadc7c19211e03c886a2fa";
pub const DRAND_HASH: &str = "04f1e9062b8a81f848fded9c12306733282b2727ecced50032187751166ec8c3";
pub const REORG_WINDOW: u64 = 128;
// Start conservatively; learn the full preflight-to-receipt time of this RPC.
const INITIAL_INCLUSION_MARGIN_SECONDS: u64 = 15;
const MIN_INCLUSION_MARGIN_SECONDS: u64 = 5;
const WORK_LEASE_SECONDS: u64 = 5;

#[derive(Default)]
struct InclusionWindow {
    samples: VecDeque<Duration>,
    failovers: u64,
}
impl InclusionWindow {
    fn observe_route(&mut self, failovers: u64) {
        if self.failovers != failovers {
            self.samples.clear();
            self.failovers = failovers;
        }
    }
    fn record(&mut self, elapsed: Duration) {
        self.samples.push_back(elapsed);
        if self.samples.len() > 16 {
            self.samples.pop_front();
        }
    }
    fn margin(&self) -> u64 {
        if self.samples.len() < 3 {
            return INITIAL_INCLUSION_MARGIN_SECONDS;
        }
        let slowest = self.samples.iter().copied().max().unwrap_or_default();
        slowest
            .as_secs()
            .saturating_add(u64::from(slowest.subsec_nanos() > 0))
            .saturating_add(2)
            .clamp(
                MIN_INCLUSION_MARGIN_SECONDS,
                INITIAL_INCLUSION_MARGIN_SECONDS,
            )
    }
}
fn mining_window_open(now: u64, end_time: u64, margin: u64) -> bool {
    now < end_time.saturating_sub(margin)
}

pub(crate) fn work_unavailable(error: &anyhow::Error) -> bool {
    // Exact contract errors for inactive coordinated work; other failures remain errors.
    ["UpgradePending()", "InvalidEpoch()"]
        .iter()
        .any(|signature| {
            error.to_string() == format!("eth_call: RPC error 3 {}", abi::call(signature, &[]))
        })
}

pub const IMPL_SLOT: &str = "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc";

/// Locate the first canonical block in which an account's nonce has advanced
/// past a retained transaction nonce. Historical nonce reads let recovery find
/// replacements regardless of how many blocks have elapsed since submission.
fn first_block_with_consumed_nonce<F>(head: u64, nonce: u64, mut nonce_at: F) -> Result<Option<u64>>
where
    F: FnMut(u64) -> Result<u64>,
{
    if nonce_at(head)? <= nonce {
        return Ok(None);
    }
    if nonce_at(0)? > nonce {
        return Ok(Some(0));
    }

    let mut low = 1;
    let mut high = head;
    while low < high {
        let middle = low + (high - low) / 2;
        if nonce_at(middle)? > nonce {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Ok(Some(low))
}

fn network_identity(mode: &str, chain: u64) -> Result<Option<&'static str>> {
    match (mode, chain) {
        ("mainnet", CHAIN) => Ok(Some(NATIVE_ID)),
        ("testnet", TEST_CHAIN) => Ok(Some(TEST_NATIVE_ID)),
        ("local", CHAIN) => Ok(None),
        _ => bail!("Deployment mode and chain identity do not match"),
    }
}

fn verify_evm_chain(rpc: &Rpc, expected: u64) -> Result<()> {
    let chain = rpc.request("eth_chainId", json!([]))?;
    ensure!(
        abi::quantity(chain.as_str().context("Invalid chain id")?)? == U256::from(expected),
        "Wrong EVM chain: expected {expected}"
    );
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema: String,
    pub chain_id: u64,
    pub mode: String,
    pub token: Address,
    pub ledger: Address,
    pub engine: Address,
    pub beacon: Address,
    pub admin: Address,
    pub owner: Address,
    pub launch_time: u64,
    pub epoch_seconds: u64,
    pub decimals: u8,
    pub deployment_block_number: String,
    pub drand_chain_hash: String,
    pub runtime_code_hashes: BTreeMap<String, B256>,
    pub approved_implementation_code_hashes: BTreeMap<String, Vec<B256>>,
    pub manifest_sha256: String,
    #[serde(default)]
    pub rpc_fallbacks: Vec<String>,
}
fn canonical(value: &Value) -> Result<String> {
    Ok(match value {
        Value::Object(map) => {
            let sorted: BTreeMap<_, _> = map.iter().collect();
            let mut parts = Vec::new();
            for (k, v) in sorted {
                parts.push(format!("{}:{}", serde_json::to_string(k)?, canonical(v)?));
            }
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(canonical)
                .collect::<Result<Vec<_>>>()?
                .join(",")
        ),
        Value::Number(n) => {
            ensure!(!n.is_f64(), "Manifest numbers must be integers");
            n.to_string()
        }
        other => serde_json::to_string(other)?,
    })
}
impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(
            &mut std::io::Read::take(fs::File::open(path)?, 2 * 1024 * 1024 + 1),
            &mut bytes,
        )?;
        ensure!(
            bytes.len() <= 2 * 1024 * 1024,
            "Deployment manifest is too large"
        );
        let mut raw: Value = serde_json::from_slice(&bytes)?;
        let result: Self = serde_json::from_value(raw.clone())?;
        raw.as_object_mut()
            .context("Invalid deployment manifest")?
            .remove("manifestSha256");
        let hash = hex::encode(Sha256::digest(canonical(&raw)?.as_bytes()));
        ensure!(
            hash == result.manifest_sha256,
            "Deployment manifest checksum mismatch"
        );
        ensure!(
            result.schema == "zoru-mainnet-suite-1",
            "Unsupported deployment manifest schema"
        );
        network_identity(&result.mode, result.chain_id)?;
        ensure!(
            result.decimals == 8
                && result.epoch_seconds == 60
                && result.drand_chain_hash == DRAND_HASH,
            "Unsupported mining policy"
        );
        let roles = result.roles();
        let mut addresses: Vec<_> = roles.iter().map(|(_, a)| *a).collect();
        addresses.sort();
        addresses.dedup();
        ensure!(
            addresses.len() == 5 && !addresses.contains(&Address::ZERO),
            "Deployment addresses must be distinct and nonzero"
        );
        ensure!(
            result.owner != Address::ZERO,
            "Deployment owner must be nonzero"
        );
        Ok(result)
    }
    pub fn roles(&self) -> [(&'static str, Address); 5] {
        [
            ("token", self.token),
            ("ledger", self.ledger),
            ("engine", self.engine),
            ("beacon", self.beacon),
            ("admin", self.admin),
        ]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Journal {
    pub chain_id: u64,
    pub account: Address,
    pub nonce: u64,
    pub hash: B256,
    pub raw: String,
    pub state: String,
    pub label: String,
    pub receipt: Option<Value>,
    pub replacement: Option<B256>,
    // Older journals still reconcile receipts/nonces. A proof replay additionally
    // needs the work context that was validated before its original signature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<Work>,
}

fn beacon_epoch_from_calldata(input: &[u8]) -> Result<u64> {
    ensure!(
        input.len() == 164
            && input[..4] == abi::keccak(b"publish(uint48,bytes)")[..4]
            && input[4..30].iter().all(|byte| *byte == 0)
            && U256::from_be_slice(&input[36..68]) == U256::from(64)
            && U256::from_be_slice(&input[68..100]) == U256::from(64),
        "Unexpected drand publication calldata"
    );
    Ok(U256::from_be_slice(&input[4..36]).try_into()?)
}

/// The signed calldata is the sole source of the mining epoch.
/// Check the signature and every identity field before trusting it.
fn signed_mining_epoch(entry: &Journal, recipient: Address) -> Result<u64> {
    let raw = abi::bytes(&entry.raw)?;
    let tx = TxEnvelope::decode_2718_exact(&raw)?;
    ensure!(
        [CHAIN, TEST_CHAIN].contains(&entry.chain_id)
            && tx.chain_id() == Some(entry.chain_id)
            && tx.nonce() == entry.nonce
            && tx.to() == Some(recipient)
            && tx.value() == U256::ZERO
            && abi::keccak(&raw) == entry.hash
            && tx.recover_signer()? == entry.account,
        "Signed mining journal identity mismatch"
    );
    match entry.label.as_str() {
        "publish-drand" => beacon_epoch_from_calldata(tx.input()),
        "mined-proof" => {
            let input = tx.input();
            ensure!(
                input.len() == 100
                    && input[..4] == abi::keccak(b"submit(uint48,address,uint256)")[..4]
                    && input[4..30].iter().all(|byte| *byte == 0)
                    && input[36..68] == abi::address(entry.account)[..],
                "Unexpected proof submission calldata"
            );
            Ok(U256::from_be_slice(&input[4..36]).try_into()?)
        }
        _ => bail!("Unsupported mining journal intent"),
    }
}

/// Never discard a signed journal solely because the RPC rejected it.
/// A competing miner may have published the same beacon: archive the obsolete
/// transaction only after confirming the original hash is absent, the nonce
/// is unchanged on both chain views, and the exact epoch is either cached
/// on-chain or already closed for mining. Keep the original signed bytes.
fn obsolete_mining_journal(
    entry: &Journal,
    latest_nonce: u64,
    pending_nonce: u64,
    transaction_known: bool,
    seed_verified: bool,
    epoch_closed: bool,
) -> bool {
    matches!(entry.label.as_str(), "publish-drand" | "mined-proof")
        && entry.state == "prepared"
        && entry.nonce == latest_nonce
        && entry.nonce == pending_nonce
        && !transaction_known
        && ((entry.label == "publish-drand" && seed_verified) || epoch_closed)
}

fn receipt_location(receipt: &Value, expected_hash: B256) -> Result<(u64, B256)> {
    let hash: B256 = receipt["transactionHash"]
        .as_str()
        .context("Receipt transaction hash missing")?
        .parse()?;
    ensure!(hash == expected_hash, "Receipt transaction hash mismatch");
    let height = abi::quantity(
        receipt["blockNumber"]
            .as_str()
            .context("Receipt block number missing")?,
    )?
    .try_into()?;
    let block_hash: B256 = receipt["blockHash"]
        .as_str()
        .context("Receipt block hash missing")?
        .parse()?;
    ensure!(block_hash != B256::ZERO, "Receipt block hash is zero");
    Ok((height, block_hash))
}

struct WorkSnapshot {
    block: B256,
    observed: Instant,
    timestamp: u64,
    beneficiary: Address,
    work: Option<Work>,
    beacon_missing: bool,
}

impl WorkSnapshot {
    fn epoch_window(&self, manifest: &Manifest) -> Option<(u64, u64)> {
        let epoch = self
            .timestamp
            .checked_sub(manifest.launch_time)?
            .checked_div(manifest.epoch_seconds)?;
        let end = manifest
            .launch_time
            .checked_add(epoch.checked_add(1)?.checked_mul(manifest.epoch_seconds)?)?;
        Some((epoch, end))
    }
    fn pending_beacon_epoch(&self, manifest: &Manifest, margin: u64) -> Option<u64> {
        let age = self.observed.elapsed();
        if self.work.is_some()
            || !self.beacon_missing
            || age >= Duration::from_secs(WORK_LEASE_SECONDS)
        {
            return None;
        }
        let (epoch, end) = self.epoch_window(manifest)?;
        let now = self.timestamp.checked_add(age.as_secs())?;
        mining_window_open(now, end, margin).then_some(epoch)
    }
    fn wait_reason(&self, manifest: &Manifest, margin: u64) -> &'static str {
        let age = self.observed.elapsed();
        if age >= Duration::from_secs(WORK_LEASE_SECONDS) {
            return "stale-work-lease";
        }
        if let Some((_, end)) = self.epoch_window(manifest)
            && !mining_window_open(self.timestamp.saturating_add(age.as_secs()), end, margin)
        {
            return "epoch-safety-window";
        }
        if self.work.is_some() {
            return "none";
        }
        if self.beacon_missing {
            "awaiting-beacon"
        } else {
            "protocol-inactive"
        }
    }
    fn settlement_window_open(&self, manifest: &Manifest, reserve: u64) -> bool {
        let age = self.observed.elapsed();
        if self.beacon_missing || age >= Duration::from_secs(WORK_LEASE_SECONDS) {
            return false;
        }
        self.epoch_window(manifest).is_some_and(|(_, end)| {
            self.timestamp
                .saturating_add(age.as_secs())
                .saturating_add(reserve)
                < end
        })
    }
}

pub struct Client {
    pub cancelled: Arc<AtomicBool>,
    pub manifest: Manifest,
    pub read: Rpc,
    pub submit: Rpc,
    pub native: Option<Rpc>,
    pub journal: PathBuf,
    pub receipts: PathBuf,
    head: Mutex<(u64, Instant)>,
    native_head: Mutex<(u64, Instant)>,
    validated_work: Mutex<Option<(Work, Address)>>,
    work_snapshot: Mutex<Option<WorkSnapshot>>,
    proof_limit: Mutex<U256>,
    inclusion_window: Mutex<InclusionWindow>,
    pub pipeline: Mutex<Value>,
    pub observer: Mutex<Option<crate::Emit>>,
    writer: Mutex<()>,
    pub transactions: Mutex<Vec<Value>>,
    pub fees: Mutex<crate::fees::Fees>,
}
impl Client {
    pub fn new(
        manifest: Manifest,
        read: &str,
        submit: &str,
        native: Option<&str>,
        journal: PathBuf,
        receipts: PathBuf,
    ) -> Result<Arc<Self>> {
        fs::create_dir_all(&journal)?;
        fs::create_dir_all(&receipts)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&journal, fs::Permissions::from_mode(0o700))?;
        }
        let read_rpc = Rpc::new(read, Duration::from_millis(80))?
            .with_fallbacks(&manifest.rpc_fallbacks, manifest.chain_id)?;
        let submit_rpc = Rpc::new(submit, Duration::from_millis(1500))?
            .with_fallbacks(&manifest.rpc_fallbacks, manifest.chain_id)?;
        let result = Arc::new(Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            manifest,
            read: read_rpc,
            submit: submit_rpc,
            native: native
                .map(|url| Rpc::new(url, Duration::from_millis(80)))
                .transpose()?,
            journal,
            receipts,
            head: Mutex::new((0, Instant::now())),
            native_head: Mutex::new((0, Instant::now())),
            validated_work: Mutex::new(None),
            work_snapshot: Mutex::new(None),
            proof_limit: Mutex::new(U256::ZERO),
            inclusion_window: Mutex::new(InclusionWindow::default()),
            pipeline: Mutex::new(json!({"status":"idle","pending":0})),
            observer: Mutex::new(None),
            writer: Mutex::new(()),
            transactions: Mutex::new(Vec::new()),
            fees: Mutex::new(crate::fees::Fees::default()),
        });
        result.identity()?;
        Ok(result)
    }
    pub fn identity(&self) -> Result<()> {
        verify_evm_chain(&self.read, self.manifest.chain_id)?;
        let native = network_identity(&self.manifest.mode, self.manifest.chain_id)?;
        if let (Some(expected), Some(native_rpc)) = (native, self.native.as_ref()) {
            ensure!(
                native_rpc.native()?["chain_id"] == expected,
                "Wrong VEX native network"
            );
        } else if self.manifest.mode == "local" {
            let version = self.read.request("web3_clientVersion", json!([]))?;
            ensure!(
                version
                    .as_str()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains("anvil"),
                "Expected isolated Anvil local chain"
            );
        }
        Ok(())
    }
    pub fn health(&self) -> Result<()> {
        if let (Some(expected), Some(native_rpc)) = (
            network_identity(&self.manifest.mode, self.manifest.chain_id)?,
            self.native.as_ref(),
        ) {
            let info = native_rpc.native()?;
            ensure!(info["chain_id"] == expected, "Wrong VEX native network");
            let head = info["head_block_num"]
                .as_u64()
                .context("Native head is unavailable")?;
            let mut previous = self.native_head.lock().unwrap();
            if previous.0 != head {
                *previous = (head, Instant::now());
            }
            ensure!(
                previous.1.elapsed() < Duration::from_secs(10),
                "Native head is stalled"
            );
        }
        // EVM-only operation verifies identity and tracks the EVM head itself.
        // A frozen relay must not silently authorize further submissions.
        {
            verify_evm_chain(&self.read, self.manifest.chain_id)?;
            let value = self.read.request("eth_blockNumber", json!([]))?;
            let height = abi::quantity(value.as_str().context("Invalid EVM block number")?)?;
            ensure!(height <= U256::from(u64::MAX), "EVM block number overflow");
            let height = height.to::<u64>();
            self.observe_head(height)?;
        }
        Ok(())
    }
    pub fn call(&self, to: Address, signature: &str, words: &[B256]) -> Result<String> {
        self.call_at(to, signature, words, "latest")
    }
    pub fn number(&self, to: Address, sig: &str) -> Result<U256> {
        abi::scalar(&self.call(to, sig, &[])?)
    }
    pub fn address(&self, to: Address, sig: &str) -> Result<Address> {
        abi::abi_address(&self.call(to, sig, &[])?)
    }
    pub fn code(&self, address: Address) -> Result<Vec<u8>> {
        abi::bytes(
            self.read
                .request("eth_getCode", json!([address, "latest"]))?
                .as_str()
                .context("Invalid code response")?,
        )
    }
    pub fn implementations(&self) -> Result<()> {
        for (role, address) in [
            ("ledger", self.manifest.ledger),
            ("engine", self.manifest.engine),
            ("beacon", self.manifest.beacon),
        ] {
            let slot = self
                .read
                .request("eth_getStorageAt", json!([address, IMPL_SLOT, "latest"]))?;
            let word = abi::quantity(slot.as_str().context("Invalid implementation slot")?)?
                .to_be_bytes::<32>();
            let implementation = Address::from_slice(&word[12..]);
            let code = self.code(implementation)?;
            ensure!(
                !code.is_empty()
                    && self
                        .manifest
                        .approved_implementation_code_hashes
                        .get(role)
                        .is_some_and(|v| v.contains(&abi::keccak(&code))),
                "Unapproved {role} implementation"
            );
        }
        Ok(())
    }
    fn verify_owner(&self) -> Result<()> {
        ensure!(
            self.address(self.manifest.token, "owner()")? == self.manifest.owner,
            "Token owner differs from the deployment manifest"
        );
        Ok(())
    }
    pub fn verify(&self) -> Result<U256> {
        self.identity()?;
        let d = &self.manifest;
        for (role, address) in d.roles() {
            let code = self.code(address)?;
            ensure!(
                !code.is_empty() && d.runtime_code_hashes.get(role) == Some(&abi::keccak(&code)),
                "Unexpected {role} contract code"
            );
        }
        self.implementations()?;
        self.verify_owner()?;
        for (sig, expected) in [
            ("token()", d.token),
            ("engine()", d.engine),
            ("beacon()", d.beacon),
            ("protocolAdmin()", d.admin),
        ] {
            ensure!(
                self.address(d.ledger, sig)? == expected,
                "Ledger pointer mismatch"
            );
        }
        ensure!(
            self.address(d.token, "issuer()")? == d.ledger,
            "Issuer must be the Ledger Proxy"
        );
        ensure!(
            self.address(d.admin, "token()")? == d.token,
            "Coordinator token mismatch"
        );
        for (role, proxy) in [
            ("ledger", d.ledger),
            ("engine", d.engine),
            ("beacon", d.beacon),
        ] {
            ensure!(
                self.address(d.admin, &format!("{role}Proxy()"))? == proxy,
                "Coordinator proxy mismatch"
            );
            let mut bytes = vec![0xd6, 0x94];
            bytes.extend_from_slice(proxy.as_slice());
            bytes.push(1);
            let expected = Address::from_slice(&abi::keccak(&bytes).as_slice()[12..]);
            ensure!(
                abi::abi_address(&self.call(
                    d.admin,
                    "adminOf(address)",
                    &[abi::address(proxy)]
                )?)? == expected,
                "ProxyAdmin address mismatch"
            );
            ensure!(
                self.address(expected, "owner()")? == d.admin,
                "ProxyAdmin authority mismatch"
            );
        }
        ensure!(
            self.number(d.token, "MAX_SUPPLY()")? == U256::from(2_100_000_000_000_000u64),
            "Token cap mismatch"
        );
        ensure!(
            self.number(d.token, "decimals()")? == U256::from(8),
            "Token precision mismatch"
        );
        ensure!(
            self.number(d.ledger, "launchTime()")? == U256::from(d.launch_time)
                && self.number(d.ledger, "EPOCH_SECONDS()")? == U256::from(d.epoch_seconds),
            "Clock policy mismatch"
        );
        ensure!(
            !abi::boolean(&self.call(d.admin, "recoveryRequired()", &[])?)?,
            "Owner recovery is required"
        );
        let revision = self.number(d.admin, "suiteRevision()")?;
        ensure!(
            revision == self.number(d.ledger, "moduleGeneration()")?,
            "Upgrade checkpoint mismatch"
        );
        let limit = self.number(d.ledger, "MAX_PROOFS_PER_EPOCH()")?;
        ensure!(
            limit > U256::ZERO && limit <= U256::from(u32::MAX),
            "Unsupported proof limit"
        );
        *self.proof_limit.lock().unwrap() = limit;
        *self.work_snapshot.lock().unwrap() = None;
        Ok(revision)
    }
    pub fn block(&self) -> Result<Value> {
        self.read
            .request("eth_getBlockByNumber", json!(["latest", false]))
    }
    pub fn now(&self) -> Result<u64> {
        Ok(abi::quantity(
            self.block()?["timestamp"]
                .as_str()
                .context("Missing block timestamp")?,
        )?
        .try_into()?)
    }
    pub fn epoch(&self) -> Result<u64> {
        let n = self.number(self.manifest.ledger, "epochNow()")?;
        ensure!(n < (U256::from(1) << 48), "Epoch is out of range");
        Ok(n.try_into()?)
    }
    pub fn work_deadline(&self) -> Option<Instant> {
        self.work_snapshot.lock().unwrap().as_ref().and_then(|s| {
            s.work.as_ref().map(|w| {
                s.observed
                    + Duration::from_secs(
                        w.end_time
                            .saturating_sub(self.inclusion_margin())
                            .saturating_sub(s.timestamp)
                            .min(WORK_LEASE_SECONDS),
                    )
            })
        })
    }
    pub fn inclusion_margin(&self) -> u64 {
        let mut window = self.inclusion_window.lock().unwrap();
        window.observe_route(self.read.failover_count() + self.submit.failover_count());
        window.margin()
    }
    pub(crate) fn pending_beacon_epoch(&self) -> Option<u64> {
        self.work_snapshot
            .lock()
            .unwrap()
            .as_ref()?
            .pending_beacon_epoch(&self.manifest, self.inclusion_margin())
    }
    pub(crate) fn settlement_window_open(&self, reserve: u64) -> bool {
        self.work_snapshot
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|s| s.settlement_window_open(&self.manifest, reserve))
    }
    pub(crate) fn work_wait_reason(&self) -> &'static str {
        self.work_snapshot
            .lock()
            .unwrap()
            .as_ref()
            .map_or("refreshing-work", |s| {
                s.wait_reason(&self.manifest, self.inclusion_margin())
            })
    }
    fn observe_head(&self, height: u64) -> Result<()> {
        let mut previous = self.head.lock().unwrap();
        if previous.0 != height {
            *previous = (height, Instant::now());
        }
        ensure!(
            previous.1.elapsed() < Duration::from_secs(60),
            "EVM head is stalled"
        );
        Ok(())
    }
    pub fn work(&self, beneficiary: Address) -> Result<Option<Work>> {
        let block = self.block()?;
        let height = block["number"].as_str().context("Block height missing")?;
        self.observe_head(abi::quantity(height)?.try_into()?)?;
        let hash: B256 = block["hash"]
            .as_str()
            .context("Block hash missing")?
            .parse()?;
        if let Some(snapshot) = self.work_snapshot.lock().unwrap().as_ref()
            && snapshot.block == hash
            && snapshot.beneficiary == beneficiary
        {
            return Ok(snapshot.work.clone());
        }
        let now: u64 = abi::quantity(
            block["timestamp"]
                .as_str()
                .context("Block timestamp missing")?,
        )?
        .try_into()?;
        let observed = Instant::now();
        let (work, beacon_missing) = self.work_at(beneficiary, now, height)?;
        *self.work_snapshot.lock().unwrap() = Some(WorkSnapshot {
            block: hash,
            observed,
            timestamp: now,
            beneficiary,
            work: work.clone(),
            beacon_missing,
        });
        Ok(work)
    }
    pub(crate) fn call_at(
        &self,
        to: Address,
        signature: &str,
        words: &[B256],
        height: &str,
    ) -> Result<String> {
        let result = self.read.request(
            "eth_call",
            json!([{"to":to,"data":abi::call(signature, words)},height]),
        )?;
        Ok(result.as_str().context("Invalid ABI response")?.to_owned())
    }
    fn work_at(
        &self,
        beneficiary: Address,
        now: u64,
        height: &str,
    ) -> Result<(Option<Work>, bool)> {
        let d = &self.manifest;
        if now < d.launch_time {
            return Ok((None, false));
        }
        let epoch = (now - d.launch_time) / d.epoch_seconds;
        let end_time = d.launch_time + (epoch + 1) * d.epoch_seconds;
        if !mining_window_open(now, end_time, self.inclusion_margin()) {
            return Ok((None, false));
        }
        if abi::boolean(&self.call_at(d.ledger, "admissionPaused()", &[], height)?)? {
            return Ok((None, false));
        }
        if abi::abi_address(&self.call_at(d.token, "issuer()", &[], height)?)? != d.ledger {
            return Ok((None, false));
        }
        let word = abi::uint(U256::from(epoch));
        let info = abi::words(&self.call_at(d.ledger, "epochInfo(uint48)", &[word], height)?)?;
        ensure!(info.len() == 10, "Invalid epoch schema");
        let limit = *self.proof_limit.lock().unwrap();
        ensure!(
            limit > U256::ZERO,
            "Verify the deployment before requesting work"
        );
        if abi::number(info[3]) >= limit || abi::number(info[6]) != U256::ZERO {
            return Ok((None, false));
        }
        // This call also enforces the coordinator's workActive checkpoint.
        // All reads use the same block, so a boundary cannot mix two epochs.
        let target = match self.call_at(d.ledger, "targetForEpoch(uint48)", &[word], height) {
            Ok(value) => abi::scalar(&value)?,
            Err(error) if work_unavailable(&error) => return Ok((None, false)),
            Err(error) => return Err(error),
        };
        let generation =
            abi::scalar(&self.call_at(d.ledger, "moduleGeneration()", &[], height)?)?;
        // After the first proof the Ledger owns the seed. Clearing the beacon
        // cache alone does not invalidate an already opened Ledger epoch.
        let seed = if info[1] != B256::ZERO {
            info[1]
        } else {
            let words =
                abi::words(&self.call_at(d.beacon, "verifiedSeed(uint48)", &[word], height)?)?;
            ensure!(words.len() == 1, "Invalid seed response");
            words[0]
        };
        if seed == B256::ZERO {
            return Ok((None, true));
        }
        let work = Work {
            chain_id: d.chain_id,
            ledger: d.ledger,
            token: d.token,
            generation,
            epoch,
            seed,
            target,
            end_time,
        };
        if self
            .validated_work
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|(previous, who)| previous == &work && *who == beneficiary)
        {
            return Ok((Some(work), false));
        }
        let nonce = (U256::from(1) << 128) + U256::from(7);
        let expected = abi::words(&self.call_at(
            d.ledger,
            "digestFor(uint48,bytes32,address,uint256)",
            &[word, seed, abi::address(beneficiary), abi::uint(nonce)],
            height,
        )?)?;
        ensure!(
            expected.len() == 1 && expected[0] == pow::digest(&work, beneficiary, nonce),
            "Miner and ledger digest domains differ"
        );
        *self.validated_work.lock().unwrap() = Some((work.clone(), beneficiary));
        Ok((Some(work), false))
    }
    pub fn balance(&self, account: Address) -> Result<U256> {
        abi::quantity(
            self.read
                .request("eth_getBalance", json!([account, "latest"]))?
                .as_str()
                .context("Invalid native balance")?,
        )
    }
    pub fn gas_price(&self) -> Result<u128> {
        verify_evm_chain(&self.submit, self.manifest.chain_id)?;
        let quote = self.submit.request("eth_gasPrice", json!([]))?;
        let price: u128 = abi::quantity(quote.as_str().context("Invalid gas price")?)?
            .try_into()
            .context("Gas price exceeds supported range")?;
        ensure!(price > 0, "RPC returned a zero gas price");
        Ok(price)
    }
    pub fn nonce(&self, account: Address, tag: &str) -> Result<u64> {
        Ok(abi::quantity(
            self.read
                .request("eth_getTransactionCount", json!([account, tag]))?
                .as_str()
                .context("Invalid transaction nonce")?,
        )?
        .try_into()?)
    }
    fn journal_path(&self, account: Address, nonce: u64) -> PathBuf {
        self.journal.join(format!("{account:#x}-{nonce:020}.json"))
    }
    pub fn pending_journals(&self, account: Address) -> Result<usize> {
        let mut pending = 0;
        for path in fs::read_dir(&self.journal)? {
            let path = path?.path();
            if path.extension().is_some_and(|e| e == "json") {
                let bytes = match fs::read(path) {
                    Ok(bytes) => bytes,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error.into()),
                };
                let record: Journal = serde_json::from_slice(&bytes)?;
                ensure!(
                    record.account == account && record.chain_id == self.manifest.chain_id,
                    "Journal network or account mismatch"
                );
                if !matches!(
                    record.state.as_str(),
                    "confirmed" | "superseded" | "replaced"
                ) {
                    pending += 1;
                }
            }
        }
        Ok(pending)
    }
    fn write_journal(&self, record: &Journal) -> Result<()> {
        storage::atomic_json(&self.journal_path(record.account, record.nonce), record)
    }
    fn archive_journal(&self, record: &Journal) -> Result<()> {
        let archive = self.journal.join("resolved");
        fs::create_dir_all(&archive)?;
        fs::rename(
            self.journal_path(record.account, record.nonce),
            archive.join(format!(
                "{:#x}-{:020}-{:#x}.json",
                record.account, record.nonce, record.hash
            )),
        )?;
        #[cfg(unix)]
        {
            fs::File::open(&archive)?.sync_all()?;
            fs::File::open(&self.journal)?.sync_all()?;
        }
        Ok(())
    }
    fn canonical_receipt(&self, hash: B256, receipt: &Value) -> Result<bool> {
        if receipt.is_null() {
            return Ok(false);
        }
        let (height, expected_block) = receipt_location(receipt, hash)?;
        let block = self.read.request(
            "eth_getBlockByNumber",
            json!([format!("0x{height:x}"), false]),
        )?;
        if block.is_null() {
            return Ok(false);
        }
        ensure!(
            abi::quantity(
                block["number"]
                    .as_str()
                    .context("Canonical block height missing")?
            )? == U256::from(height),
            "Canonical block height mismatch"
        );
        Ok(block["hash"]
            .as_str()
            .context("Canonical block hash missing")?
            .parse::<B256>()?
            == expected_block)
    }
    pub fn receipt(&self, hash: B256, timeout: Duration) -> Result<Value> {
        let deadline = Instant::now() + timeout;
        let mut health_at = Instant::now();
        let mut delay = Duration::from_millis(400);
        while Instant::now() < deadline {
            if let Ok(receipt) = self
                .read
                .request("eth_getTransactionReceipt", json!([hash]))
                && self.canonical_receipt(hash, &receipt)?
            {
                return Ok(receipt);
            }
            if self.cancelled.load(Ordering::Relaxed) {
                break;
            }
            if Instant::now() >= health_at {
                self.health()?;
                health_at = Instant::now() + Duration::from_secs(4);
            }
            thread::sleep(delay);
            delay = (delay + Duration::from_millis(200)).min(Duration::from_millis(1500));
        }
        bail!("Receipt pending; retain the signed transaction journal")
    }
    pub fn reconcile(&self, account: Address, confirmed: bool) -> Result<()> {
        let latest_nonce = self.nonce(account, "latest")?;
        let head: u64 = abi::quantity(
            self.read
                .request("eth_blockNumber", json!([]))?
                .as_str()
                .context("Invalid head")?,
        )?
        .try_into()?;
        let mut paths = fs::read_dir(&self.journal)?
            .map(|v| v.map(|v| v.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.sort();
        for path in paths {
            if path.extension().and_then(|v| v.to_str()) != Some("json") {
                continue;
            }
            let mut entry: Journal = serde_json::from_slice(&fs::read(path)?)?;
            ensure!(
                entry.chain_id == self.manifest.chain_id && entry.account == account,
                "Journal network or account mismatch"
            );
            if matches!(entry.state.as_str(), "replaced" | "superseded") {
                self.archive_journal(&entry)?;
                self.fees.lock().unwrap().release(entry.hash);
                continue;
            }
            let old = entry
                .receipt
                .as_ref()
                .and_then(|r| r["blockNumber"].as_str())
                .map(abi::quantity)
                .transpose()?
                .map(u64::try_from)
                .transpose()?
                .is_some_and(|n| head >= n.saturating_add(REORG_WINDOW));
            if entry.state == "confirmed" && !confirmed && !old && entry.nonce < latest_nonce {
                continue;
            }
            let receipt = self
                .read
                .request("eth_getTransactionReceipt", json!([entry.hash]))?;
            if self.canonical_receipt(entry.hash, &receipt)? {
                self.fees.lock().unwrap().confirm(entry.hash, &receipt)?;
                let block: u64 = abi::quantity(
                    receipt["blockNumber"]
                        .as_str()
                        .context("Missing receipt block")?,
                )?
                .try_into()?;
                entry.state = "confirmed".into();
                entry.receipt = Some(receipt);
                self.write_journal(&entry)?;
                if head >= block.saturating_add(REORG_WINDOW) {
                    self.archive_journal(&entry)?;
                }
                continue;
            }
            if entry.state == "confirmed" {
                entry.state = "prepared".into();
                entry.receipt = None;
                self.write_journal(&entry)?;
            }
            if self.nonce(account, "latest")? > entry.nonce {
                let replacement_block =
                    first_block_with_consumed_nonce(head, entry.nonce, |block| {
                        self.nonce(account, &format!("0x{block:x}"))
                    })?;
                let mut replacement = None;
                if let Some(block) = replacement_block {
                    let data = self.read.request(
                        "eth_getBlockByNumber",
                        json!([format!("0x{block:x}"), true]),
                    )?;
                    ensure!(!data.is_null(), "Consumed nonce block is unavailable");
                    ensure!(
                        abi::quantity(
                            data["number"]
                                .as_str()
                                .context("Consumed nonce block height missing")?,
                        )? == U256::from(block),
                        "Consumed nonce block height mismatch"
                    );
                    for tx in data["transactions"].as_array().into_iter().flatten() {
                        if tx["from"]
                            .as_str()
                            .is_some_and(|v| v.eq_ignore_ascii_case(&format!("{account:#x}")))
                            && abi::quantity(
                                tx["nonce"]
                                    .as_str()
                                    .context("Consumed transaction nonce missing")?,
                            )? == U256::from(entry.nonce)
                        {
                            replacement = Some(
                                tx["hash"]
                                    .as_str()
                                    .context("Missing hash")?
                                    .parse::<B256>()?,
                            );
                            break;
                        }
                    }
                }
                ensure!(
                    replacement.is_some(),
                    "Consumed nonce could not be reconciled; review the retained journal"
                );
                let replacement_hash = replacement.context("Consumed nonce replacement missing")?;
                ensure!(
                    replacement_hash != entry.hash,
                    "Original consumed transaction receipt is unavailable; retain the journal"
                );
                let replacement_receipt = self
                    .read
                    .request("eth_getTransactionReceipt", json!([replacement_hash]))?;
                ensure!(
                    self.canonical_receipt(replacement_hash, &replacement_receipt)?,
                    "Consumed nonce replacement receipt is not canonical; retain the journal"
                );
                entry.state = "replaced".into();
                entry.replacement = replacement;
                self.write_journal(&entry)?;
                self.fees.lock().unwrap().release(entry.hash);
                continue;
            }
            if matches!(entry.label.as_str(), "publish-drand" | "mined-proof")
                && entry.state == "prepared"
            {
                let is_beacon = entry.label == "publish-drand";
                let recipient = if is_beacon {
                    self.manifest.beacon
                } else {
                    self.manifest.ledger
                };
                let epoch = signed_mining_epoch(&entry, recipient)?;
                let epoch_closed = self.now()?
                    >= self.manifest.launch_time + (epoch + 1) * self.manifest.epoch_seconds;
                let admission_paused =
                    self.number(self.manifest.ledger, "admissionPaused()")? != U256::ZERO;
                let seed_verified = is_beacon
                    && abi::scalar(&self.call(
                        self.manifest.beacon,
                        "verifiedSeed(uint48)",
                        &[abi::uint(U256::from(epoch))],
                    )?)? != U256::ZERO;
                let latest = self.nonce(account, "latest")?;
                let pending = self.nonce(account, "pending")?;
                if latest == entry.nonce && pending == entry.nonce {
                    let known = !self
                        .read
                        .request("eth_getTransactionByHash", json!([entry.hash]))?
                        .is_null();
                    if obsolete_mining_journal(
                        &entry,
                        latest,
                        pending,
                        known,
                        seed_verified,
                        epoch_closed,
                    ) {
                        entry.state = "superseded".into();
                        self.fees.lock().unwrap().release(entry.hash);
                        self.write_journal(&entry)?;
                        self.archive_journal(&entry)?;
                        continue;
                    }
                }
                ensure!(
                    !epoch_closed && !admission_paused && !seed_verified,
                    "Mining context is unavailable; retain the journal until its nonce can be reconciled"
                );
                if !is_beacon {
                    let expected = entry.work.as_ref().context(
                        "Proof journal has no work context; retain it until its receipt or closed epoch can be reconciled"
                    )?;
                    ensure!(
                        expected.epoch == epoch
                            && self.work(account)?.as_ref() == Some(expected)
                            && self
                                .work_deadline()
                                .is_some_and(|deadline| Instant::now() < deadline),
                        "Signed proof work has changed; retain the journal without rebroadcasting"
                    );
                }
            }
            if entry.state == "prepared" && matches!(entry.label.as_str(), "claim" | "seal") {
                ensure!(
                    self.number(self.manifest.ledger, "claimsPaused()")? == U256::ZERO,
                    "Claims are paused; retain the settlement journal without rebroadcasting"
                );
            }
            let envelope = TxEnvelope::decode_2718_exact(&abi::bytes(&entry.raw)?)?;
            self.fees.lock().unwrap().reserve(
                entry.hash,
                U256::from(envelope.gas_limit()) * U256::from(envelope.max_fee_per_gas()),
            )?;
            ensure!(
                self.nonce(account, "pending")? == entry.nonce
                    && self
                        .read
                        .request("eth_getTransactionByHash", json!([entry.hash]))?
                        .is_null(),
                "Transaction is pending; retain the signed journal without rebroadcasting"
            );
            self.identity()?;
            verify_evm_chain(&self.submit, self.manifest.chain_id)?;
            ensure!(!self.cancelled.load(Ordering::SeqCst), "Signing stopped");
            let broadcast = self.broadcast(&entry);
            let receipt = match self.receipt(entry.hash, Duration::from_secs(35)) {
                Ok(receipt) => receipt,
                Err(receipt_error) => return Err(broadcast.err().unwrap_or(receipt_error)),
            };
            entry.state = "confirmed".into();
            self.fees.lock().unwrap().confirm(entry.hash, &receipt)?;
            entry.receipt = Some(receipt);
            self.write_journal(&entry)?;
        }
        Ok(())
    }
    pub fn send(
        &self,
        signer: &PrivateKeySigner,
        label: &str,
        to: Address,
        data: String,
        expected: Option<&Work>,
    ) -> Result<Value> {
        let _writer = self.writer.lock().unwrap();
        self.transaction_phase("preflight", label, None);
        let started = Instant::now();
        let result = self.send_inner(signer, label, to, data, expected);
        if result.is_ok() && label == "mined-proof" && expected.is_some() {
            self.inclusion_window
                .lock()
                .unwrap()
                .record(started.elapsed());
        }
        if result.is_ok() {
            self.transaction_phase("idle", label, None);
        } else {
            let mut pipeline = self.pipeline.lock().unwrap();
            pipeline["status"] = json!(if pipeline["pending"] == 1 {
                "uncertain"
            } else {
                "idle"
            });
        }
        result
    }
    fn broadcast(&self, record: &Journal) -> Result<Value> {
        self.transaction_phase("tx-submission", &record.label, Some(record.hash));
        let result = self
            .submit
            .request("eth_sendRawTransaction", json!([record.raw]));
        if let Err(error) = &result
            && let Some(emit) = self.observer.lock().unwrap().clone()
        {
            let detail: String = error.to_string().chars().take(350).collect();
            emit(
                json!({"event":"warning","warning":format!("Submission response: {detail}; tracking retained transaction {:#x}",record.hash)}),
            );
        }
        self.transaction_phase("receipt-tracking", &record.label, Some(record.hash));
        result
    }
    fn transaction_phase(&self, status: &str, label: &str, hash: Option<B256>) {
        let mut pipeline = self.pipeline.lock().unwrap();
        let hash = if status == "preflight" && pipeline["pending"] == 1 {
            pipeline["hash"]
                .as_str()
                .and_then(|v| v.parse::<B256>().ok())
                .or(hash)
        } else {
            hash
        };
        let data =
            json!({"status":status,"label":label,"hash":hash,"pending":u8::from(hash.is_some())});
        *pipeline = data.clone();
        drop(pipeline);
        if let Some(emit) = self.observer.lock().unwrap().clone() {
            emit(json!({"event":"transaction","data":data}));
        }
    }
    fn send_inner(
        &self,
        signer: &PrivateKeySigner,
        label: &str,
        to: Address,
        data: String,
        expected: Option<&Work>,
    ) -> Result<Value> {
        ensure!(!self.cancelled.load(Ordering::SeqCst), "Signing stopped");
        ensure!(
            [self.manifest.ledger, self.manifest.beacon].contains(&to),
            "Unexpected transaction recipient"
        );
        self.reconcile(signer.address(), false)?;
        self.pipeline.lock().unwrap()["pending"] = json!(0);
        self.health()?;
        self.identity()?;
        self.implementations()?;
        self.verify_owner()?;
        let generation = self.number(self.manifest.ledger, "moduleGeneration()")?;
        if matches!(label, "mined-proof" | "publish-drand") {
            ensure!(
                self.number(self.manifest.ledger, "admissionPaused()")? == U256::ZERO,
                "STALE_WORK: proof admission is paused"
            );
        }
        if matches!(label, "claim" | "seal") {
            ensure!(
                self.number(self.manifest.ledger, "claimsPaused()")? == U256::ZERO,
                "Claims are paused by the protocol"
            );
        }
        ensure!(
            generation == self.number(self.manifest.admin, "suiteRevision()")?
                && self.address(self.manifest.ledger, "protocolAdmin()")? == self.manifest.admin,
            "Upgrade checkpoint changed"
        );
        if let Some(work) = expected {
            ensure!(
                generation == work.generation
                    && self.work(signer.address())?.as_ref() == Some(work)
                    && self
                        .work_deadline()
                        .is_some_and(|deadline| Instant::now() < deadline),
                "STALE_WORK: epoch, seed, target, generation or admission changed"
            );
        }
        let account = signer.address();
        let nonce = self.nonce(account, "pending")?;
        ensure!(
            nonce == self.nonce(account, "latest")?,
            "Another transaction is pending for this account"
        );
        ensure!(
            !self.journal_path(account, nonce).exists(),
            "Signing nonce is already journaled; reconcile before signing again"
        );
        let estimate = self.read.request(
            "eth_estimateGas",
            json!([{"from":account,"to":to,"data":data,"value":"0x0"}]),
        )?;
        let estimated: u64 =
            abi::quantity(estimate.as_str().context("Invalid gas estimate")?)?.try_into()?;
        let gas = estimated.saturating_mul(13) / 10 + 30_000;
        let gas_price = self.gas_price()?;
        let maximum_fee = U256::from(gas) * U256::from(gas_price);
        self.fees.lock().unwrap().check(maximum_fee)?;
        ensure!(
            gas <= 22_000_000
                && maximum_fee <= U256::from(MAX_TRANSACTION_FEE)
                && self.balance(account)? >= maximum_fee,
            "Insufficient bounded gas balance"
        );
        if let Some(work) = expected {
            ensure!(
                self.work(account)?.as_ref() == Some(work)
                    && self
                        .work_deadline()
                        .is_some_and(|deadline| Instant::now() < deadline),
                "STALE_WORK: work changed or inclusion window closing"
            );
        }
        let tx = TxLegacy {
            chain_id: Some(self.manifest.chain_id),
            nonce,
            gas_price,
            gas_limit: gas,
            to: TxKind::Call(to),
            value: U256::ZERO,
            input: Bytes::from(abi::bytes(&data)?),
        };
        ensure!(!self.cancelled.load(Ordering::SeqCst), "Signing stopped");
        let signature = signer.sign_hash_sync(&tx.signature_hash())?;
        let envelope = TxEnvelope::from(tx.into_signed(signature));
        let raw = envelope.encoded_2718();
        let hash = abi::keccak(&raw);
        self.fees.lock().unwrap().reserve(hash, maximum_fee)?;
        if label == "publish-drand" {
            ensure!(
                to == self.manifest.beacon,
                "Unexpected drand publication recipient"
            );
            beacon_epoch_from_calldata(&abi::bytes(&data)?)?;
        }
        let mut record = Journal {
            chain_id: self.manifest.chain_id,
            account,
            nonce,
            hash,
            raw: format!("0x{}", hex::encode(raw)),
            state: "prepared".into(),
            label: label.into(),
            receipt: None,
            replacement: None,
            work: expected.cloned(),
        };
        self.write_journal(&record)?;
        let started = Instant::now();
        let mut result = None;
        for attempt in 0..2 {
            ensure!(
                !self.cancelled.load(Ordering::SeqCst),
                "Signing stopped; retain the signed journal"
            );
            if attempt != 0 {
                ensure!(
                    self.nonce(account, "latest")? == nonce
                        && self.nonce(account, "pending")? == nonce
                        && self
                            .read
                            .request("eth_getTransactionByHash", json!([hash]))?
                            .is_null(),
                    "Transaction outcome is uncertain; reconcile the retained journal before retrying"
                );
                self.identity()?;
                self.implementations()?;
                self.verify_owner()?;
                if matches!(label, "claim" | "seal") {
                    ensure!(
                        self.number(self.manifest.ledger, "claimsPaused()")? == U256::ZERO,
                        "Claims are paused; retain the signed settlement journal"
                    );
                }
                if matches!(label, "mined-proof" | "publish-drand") {
                    ensure!(
                        self.number(self.manifest.ledger, "admissionPaused()")? == U256::ZERO,
                        "STALE_WORK: admission paused; retain the signed journal"
                    );
                    ensure!(
                        self.epoch()? == signed_mining_epoch(&record, to)?,
                        "STALE_WORK: signed epoch expired; retain the journal for reconciliation"
                    );
                }
                if let Some(work) = expected {
                    ensure!(
                        self.work(account)?.as_ref() == Some(work)
                            && self
                                .work_deadline()
                                .is_some_and(|deadline| Instant::now() < deadline),
                        "STALE_WORK: retry context expired; retain the signed journal"
                    );
                }
            }
            let broadcast = self.broadcast(&record);
            match self.receipt(hash, Duration::from_secs(35)) {
                Ok(value) => {
                    result = Some(value);
                    break;
                }
                Err(receipt_error) if attempt == 1 => {
                    return Err(broadcast.err().unwrap_or(receipt_error));
                }
                Err(_) => {}
            }
        }
        let receipt = result.context("Receipt missing")?;
        self.fees.lock().unwrap().confirm(hash, &receipt)?;
        record.state = "confirmed".into();
        record.receipt = Some(receipt.clone());
        self.write_journal(&record)?;
        storage::atomic_json(
            &self.receipts.join(format!("{hash:#x}.json")),
            &json!({"label":label,"hash":hash,"receipt":receipt,"broadcastToReceiptSeconds":started.elapsed().as_secs_f64()}),
        )?;
        let mut transactions = self.transactions.lock().unwrap();
        transactions.push(json!({"label":label,"hash":hash,"receipt":receipt,"latencySeconds":started.elapsed().as_secs_f64()}));
        if transactions.len() > 512 {
            transactions.remove(0);
        }
        ensure!(
            abi::quantity(
                receipt["status"]
                    .as_str()
                    .context("Missing receipt status")?
            )? == U256::from(1),
            "Transaction reverted: {hash:#x}"
        );
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const GAS_PRICE: u128 = 150_000_000_000;

    #[test]
    fn inclusion_window_learns_verified_transaction_times_conservatively() {
        let mut window = InclusionWindow::default();
        assert_eq!(window.margin(), 15);
        for _ in 0..2 {
            window.record(Duration::from_secs(2));
        }
        assert_eq!(window.margin(), 15);
        window.record(Duration::from_millis(3100));
        assert_eq!(window.margin(), 6);
        assert!(mining_window_open(110, 121, window.margin()));
        assert!(!mining_window_open(115, 121, window.margin()));
        window.record(Duration::from_secs(20));
        assert_eq!(window.margin(), 15);
        for _ in 0..20 {
            window.record(Duration::from_secs(1));
        }
        assert_eq!(window.samples.len(), 16);
        assert_eq!(window.margin(), 5);
        window.observe_route(1);
        assert!(window.samples.is_empty());
        assert_eq!(window.margin(), 15);
    }

    fn fixture_manifest() -> Manifest {
        serde_json::from_value(json!({"schema":"zoru-mainnet-suite-1","chainId":CHAIN,"mode":"mainnet",
            "token":Address::repeat_byte(1),"ledger":Address::repeat_byte(2),"engine":Address::repeat_byte(3),
            "beacon":Address::repeat_byte(4),"admin":Address::repeat_byte(5),"owner":Address::repeat_byte(6),
            "launchTime":1,"epochSeconds":60,"decimals":8,"deploymentBlockNumber":"0x0",
            "drandChainHash":DRAND_HASH,"runtimeCodeHashes":{},"approvedImplementationCodeHashes":{},"manifestSha256":"fixture"})).unwrap()
    }
    #[test]
    fn beacon_retry_uses_fresh_missing_work_and_skips_closed_windows() {
        let manifest = fixture_manifest();
        let mut snapshot = WorkSnapshot {
            block: B256::repeat_byte(9),
            observed: Instant::now(),
            timestamp: 70,
            beneficiary: Address::repeat_byte(7),
            work: None,
            beacon_missing: true,
        };
        assert_eq!(
            snapshot.pending_beacon_epoch(&manifest, INITIAL_INCLUSION_MARGIN_SECONDS),
            Some(1)
        );
        snapshot.timestamp = 106; // Epoch ends at 121, with a 15-second margin.
        assert_eq!(
            snapshot.pending_beacon_epoch(&manifest, INITIAL_INCLUSION_MARGIN_SECONDS),
            None
        );
        snapshot.timestamp = 121;
        assert_eq!(
            snapshot.pending_beacon_epoch(&manifest, INITIAL_INCLUSION_MARGIN_SECONDS),
            Some(2)
        );
        snapshot.timestamp = 104;
        snapshot.observed = Instant::now() - Duration::from_secs(3);
        assert_eq!(
            snapshot.pending_beacon_epoch(&manifest, INITIAL_INCLUSION_MARGIN_SECONDS),
            None
        );
        snapshot.timestamp = 70;
        snapshot.observed = Instant::now() - Duration::from_secs(6);
        assert_eq!(
            snapshot.pending_beacon_epoch(&manifest, INITIAL_INCLUSION_MARGIN_SECONDS),
            None
        );
        snapshot.observed = Instant::now();
        snapshot.timestamp = 0;
        assert_eq!(
            snapshot.pending_beacon_epoch(&manifest, INITIAL_INCLUSION_MARGIN_SECONDS),
            None
        );
        snapshot.timestamp = 70;
        snapshot.work = Some(Work {
            chain_id: CHAIN,
            ledger: manifest.ledger,
            token: manifest.token,
            generation: U256::from(1),
            epoch: 1,
            seed: B256::repeat_byte(8),
            target: U256::MAX >> 16,
            end_time: 121,
        });
        assert_eq!(
            snapshot.pending_beacon_epoch(&manifest, INITIAL_INCLUSION_MARGIN_SECONDS),
            None
        );
        assert!(!snapshot.settlement_window_open(&manifest, 20));
        snapshot.beacon_missing = false;
        assert!(snapshot.settlement_window_open(&manifest, 20));
        snapshot.timestamp = 104;
        assert!(!snapshot.settlement_window_open(&manifest, 20));
        snapshot.timestamp = 106;
        assert_eq!(snapshot.wait_reason(&manifest, 15), "epoch-safety-window");
    }
    fn encoded(words: &[B256]) -> Value {
        json!(format!(
            "0x{}",
            words.iter().map(hex::encode).collect::<String>()
        ))
    }
    #[test]
    fn evm_only_mainnet_identity_needs_no_native_endpoint_or_anvil_brand() {
        for chain in [CHAIN, 1] {
            let (url, server) =
                crate::rpc::tests::sequence(vec![("eth_chainId", json!(format!("0x{chain:x}")))]);
            let dir = tempfile::tempdir().unwrap();
            let result = Client::new(
                fixture_manifest(),
                &url,
                &url,
                None,
                dir.path().join("j"),
                dir.path().join("r"),
            );
            assert_eq!(result.is_ok(), chain == CHAIN);
            if let Ok(c) = result {
                *c.head.lock().unwrap() = (10, Instant::now() - Duration::from_secs(61));
                assert!(
                    c.observe_head(10)
                        .unwrap_err()
                        .to_string()
                        .contains("stalled")
                );
                assert!(c.observe_head(11).is_ok());
            }
            server.join().unwrap();
        }
    }
    #[test]
    fn opened_epoch_uses_ledger_seed_and_caches_only_the_same_block() {
        let d = fixture_manifest();
        let beneficiary = Address::repeat_byte(7);
        let work = Work {
            chain_id: CHAIN,
            ledger: d.ledger,
            token: d.token,
            generation: U256::from(7),
            epoch: 1,
            seed: B256::repeat_byte(8),
            target: U256::MAX >> 16,
            end_time: 121,
        };
        let block = json!({"number":"0x10","hash":B256::repeat_byte(9),"timestamp":"0x66"});
        let mut info = [B256::ZERO; 10];
        info[0] = abi::uint(work.target);
        info[1] = work.seed;
        info[3] = abi::uint(U256::from(1));
        info[8] = abi::uint(work.generation);
        let (url, server) = crate::rpc::tests::sequence(vec![
            ("eth_chainId", json!(format!("0x{CHAIN:x}"))),
            ("eth_getBlockByNumber", block.clone()),
            ("eth_call", encoded(&[B256::ZERO])),
            (
                "eth_call",
                encoded(&[abi::address(Address::repeat_byte(2))]),
            ),
            ("eth_call", encoded(&info)),
            ("eth_call", encoded(&[abi::uint(work.target)])),
            ("eth_call", encoded(&[abi::uint(work.generation)])),
            (
                "eth_call",
                encoded(&[pow::digest(
                    &work,
                    beneficiary,
                    (U256::from(1) << 128) + U256::from(7),
                )]),
            ),
            ("eth_getBlockByNumber", block),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let c = Client::new(
            d,
            &url,
            &url,
            None,
            dir.path().join("j"),
            dir.path().join("r"),
        )
        .unwrap();
        *c.proof_limit.lock().unwrap() = U256::from(u32::MAX);
        assert_eq!(c.work(beneficiary).unwrap(), Some(work.clone()));
        assert_eq!(c.work(beneficiary).unwrap(), Some(work));
        assert_eq!(
            c.read.request_count(),
            9,
            "the second read must use the block cache"
        );
        assert!(c.work_deadline().unwrap() <= Instant::now() + Duration::from_secs(5));
        server.join().unwrap();
    }
    #[test]
    fn proof_limits_sealing_and_inactive_issuer_stop_work() {
        for (sealed, revoked) in [(false, false), (true, false), (false, true)] {
            let mut info = [B256::ZERO; 10];
            info[3] = abi::uint(U256::from(if sealed { 0 } else { 5 }));
            info[6] = abi::uint(U256::from(u8::from(sealed)));
            let mut responses = vec![
                ("eth_chainId", json!(format!("0x{CHAIN:x}"))),
                (
                    "eth_getBlockByNumber",
                    json!({"number":"0x10","hash":B256::repeat_byte(9),"timestamp":"0x66"}),
                ),
                ("eth_call", encoded(&[B256::ZERO])),
                (
                    "eth_call",
                    encoded(&[abi::address(Address::repeat_byte(if revoked {
                        99
                    } else {
                        2
                    }))]),
                ),
            ];
            if !revoked {
                responses.push(("eth_call", encoded(&info)));
            }
            let (url, server) = crate::rpc::tests::sequence(responses);
            let dir = tempfile::tempdir().unwrap();
            let c = Client::new(
                fixture_manifest(),
                &url,
                &url,
                None,
                dir.path().join("j"),
                dir.path().join("r"),
            )
            .unwrap();
            *c.proof_limit.lock().unwrap() = U256::from(5);
            assert!(c.work(Address::repeat_byte(7)).unwrap().is_none());
            server.join().unwrap();
        }
    }

    #[test]
    fn network_modes_require_their_exact_chain_and_native_identity() {
        assert_eq!(network_identity("mainnet", CHAIN).unwrap(), Some(NATIVE_ID));
        assert_eq!(
            network_identity("testnet", TEST_CHAIN).unwrap(),
            Some(TEST_NATIVE_ID)
        );
        assert_eq!(network_identity("local", CHAIN).unwrap(), None);
        for (mode, chain) in [
            ("mainnet", TEST_CHAIN),
            ("testnet", CHAIN),
            ("local", TEST_CHAIN),
            ("mainnet", 1),
            ("unknown", CHAIN),
        ] {
            assert!(network_identity(mode, chain).is_err());
        }
    }

    #[test]
    fn receipt_identity_must_bind_the_transaction_and_canonical_location() {
        let hash = B256::repeat_byte(0x12);
        let block = B256::repeat_byte(0x34);
        let receipt = json!({"transactionHash":hash,"blockNumber":"0x42","blockHash":block});
        assert_eq!(receipt_location(&receipt, hash).unwrap(), (66, block));
        assert!(receipt_location(&receipt, B256::repeat_byte(0x56)).is_err());
        for field in ["transactionHash", "blockNumber", "blockHash"] {
            let mut invalid = receipt.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(receipt_location(&invalid, hash).is_err(), "{field}");
        }
        let mut invalid = receipt;
        invalid["blockHash"] = json!(B256::ZERO);
        assert!(receipt_location(&invalid, hash).is_err());
    }

    fn signed_publication(input: Vec<u8>) -> (Journal, Address) {
        let signer = PrivateKeySigner::random();
        let beacon = Address::repeat_byte(0x56);
        let tx = TxLegacy {
            chain_id: Some(CHAIN),
            nonce: 5,
            gas_price: GAS_PRICE,
            gas_limit: 900_000,
            to: beacon.into(),
            value: U256::ZERO,
            input: input.into(),
        };
        let signature = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
        let raw = TxEnvelope::from(tx.into_signed(signature)).encoded_2718();
        (
            Journal {
                chain_id: CHAIN,
                account: signer.address(),
                nonce: 5,
                hash: abi::keccak(&raw),
                raw: format!("0x{}", hex::encode(raw)),
                state: "prepared".into(),
                label: "publish-drand".into(),
                receipt: None,
                replacement: None,
                work: None,
            },
            beacon,
        )
    }

    fn publication_input(epoch: u64) -> Vec<u8> {
        let mut data = abi::bytes(&abi::call(
            "publish(uint48,bytes)",
            &[
                abi::uint(U256::from(epoch)),
                abi::uint(U256::from(64)),
                abi::uint(U256::from(64)),
            ],
        ))
        .unwrap();
        data.extend_from_slice(&[0x12; 64]);
        data
    }

    #[test]
    fn beacon_epoch_comes_from_the_authenticated_signed_transaction() {
        for epoch in [0, 21, (1u64 << 48) - 1] {
            let (entry, beacon) = signed_publication(publication_input(epoch));
            let stored = serde_json::to_value(&entry).unwrap();
            assert!(stored.get("beacon_epoch").is_none());
            let recovered: Journal = serde_json::from_value(stored).unwrap();
            assert_eq!(signed_mining_epoch(&recovered, beacon).unwrap(), epoch);
        }
    }

    #[test]
    fn beacon_recovery_rejects_mismatched_metadata_and_noncanonical_input() {
        let (entry, beacon) = signed_publication(publication_input(21));
        for field in ["account", "nonce", "hash", "chain_id", "raw"] {
            let mut bad = entry.clone();
            match field {
                "account" => bad.account = Address::ZERO,
                "nonce" => bad.nonce += 1,
                "hash" => bad.hash = B256::ZERO,
                "chain_id" => bad.chain_id += 1,
                "raw" => bad.raw.push_str("00"),
                _ => unreachable!(),
            }
            assert!(signed_mining_epoch(&bad, beacon).is_err(), "{field}");
        }
        assert!(signed_mining_epoch(&entry, Address::ZERO).is_err());
        for index in [0, 4, 29, 36, 67, 68, 99] {
            let mut input = publication_input(21);
            input[index] ^= 1;
            let (bad, beacon) = signed_publication(input);
            assert!(signed_mining_epoch(&bad, beacon).is_err(), "byte {index}");
        }
        let (bad, beacon) = signed_publication(publication_input(1u64 << 48));
        assert!(signed_mining_epoch(&bad, beacon).is_err());
    }

    #[test]
    fn obsolete_mining_journal_requires_verified_obsolescence_and_unused_nonce() {
        let mut entry = Journal {
            chain_id: CHAIN,
            account: Address::repeat_byte(0x85),
            nonce: 5,
            hash: B256::repeat_byte(0x12),
            raw: "0x00".into(),
            state: "prepared".into(),
            label: "publish-drand".into(),
            receipt: None,
            replacement: None,
            work: None,
        };
        assert!(obsolete_mining_journal(&entry, 5, 5, false, true, false));
        assert!(obsolete_mining_journal(&entry, 5, 5, false, false, true));
        assert!(!obsolete_mining_journal(&entry, 5, 5, true, false, true));
        assert!(!obsolete_mining_journal(&entry, 4, 5, false, true, false));
        assert!(!obsolete_mining_journal(&entry, 5, 6, false, true, false));
        assert!(!obsolete_mining_journal(&entry, 5, 5, true, true, false));
        assert!(!obsolete_mining_journal(&entry, 5, 5, false, false, false));
        entry.state = "confirmed".into();
        assert!(!obsolete_mining_journal(&entry, 5, 5, false, true, false));
        entry.state = "prepared".into();
        entry.label = "mined-proof".into();
        assert!(!obsolete_mining_journal(&entry, 5, 5, false, true, false));
    }

    #[test]
    fn nonce_reconciliation_finds_consuming_block_beyond_recent_window() {
        let head = 20_000;
        let nonce = 2_800;
        let consumed_at = 3_907;
        let mut calls = 0;
        let block = first_block_with_consumed_nonce(head, nonce, |height| {
            calls += 1;
            Ok(if height >= consumed_at {
                nonce + 1
            } else {
                nonce
            })
        })
        .unwrap();
        assert_eq!(block, Some(consumed_at));
        assert!(
            calls <= 18,
            "binary search should use logarithmic RPC reads"
        );
    }

    #[test]
    fn nonce_reconciliation_handles_unconsumed_and_genesis_nonces() {
        assert_eq!(
            first_block_with_consumed_nonce(100, 5, |_| Ok(5)).unwrap(),
            None
        );
        assert_eq!(
            first_block_with_consumed_nonce(100, 5, |height| {
                Ok(if height == 0 { 6 } else { 7 })
            })
            .unwrap(),
            Some(0)
        );
        assert_eq!(
            first_block_with_consumed_nonce(100, 5, |height| {
                Ok(if height == 100 { 6 } else { 5 })
            })
            .unwrap(),
            Some(100)
        );
    }

    #[test]
    fn mining_work_closes_at_the_relay_inclusion_safety_margin() {
        let end = 10_000;
        let cutoff = end - INITIAL_INCLUSION_MARGIN_SECONDS;
        assert!(mining_window_open(
            cutoff - 1,
            end,
            INITIAL_INCLUSION_MARGIN_SECONDS
        ));
        assert!(!mining_window_open(
            cutoff,
            end,
            INITIAL_INCLUSION_MARGIN_SECONDS
        ));
        assert!(!mining_window_open(
            cutoff + 1,
            end,
            INITIAL_INCLUSION_MARGIN_SECONDS
        ));
        assert!(!mining_window_open(
            end,
            end,
            INITIAL_INCLUSION_MARGIN_SECONDS
        ));
        assert!(!mining_window_open(
            u64::MAX,
            end,
            INITIAL_INCLUSION_MARGIN_SECONDS
        ));
    }

    #[test]
    fn canonical_manifest_keys_are_sorted() {
        assert_eq!(
            canonical(&json!({"z":3,"a":{"b":2,"a":1}})).unwrap(),
            "{\"a\":{\"a\":1,\"b\":2},\"z\":3}"
        );
        assert!(canonical(&json!(1.5)).is_err());
    }
    #[test]
    fn eip155_signature_recovers_expected_signer() {
        let signer = PrivateKeySigner::random();
        let tx = TxLegacy {
            chain_id: Some(CHAIN),
            nonce: 7,
            gas_price: GAS_PRICE,
            gas_limit: 21_000,
            to: Address::repeat_byte(1).into(),
            value: U256::ZERO,
            input: Bytes::new(),
        };
        let signature = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
        assert_eq!(
            signature
                .recover_address_from_prehash(&tx.signature_hash())
                .unwrap(),
            signer.address()
        );
        let encoded = TxEnvelope::from(tx.into_signed(signature)).encoded_2718();
        assert!(encoded[0] >= 0xc0, "EIP-155 envelope must be untagged RLP");
    }
}
