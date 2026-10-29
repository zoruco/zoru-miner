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
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

pub const CHAIN: u64 = 6736;
pub const MAX_TRANSACTION_FEE: u128 = 3_300_000_000_000_000_000;
pub const NATIVE_ID: &str = "f9f432b1851b5c179d2091a96f593aaed50ec7466b74f89301f957a83e56ce1f";
pub const DRAND_HASH: &str = "04f1e9062b8a81f848fded9c12306733282b2727ecced50032187751166ec8c3";
pub const REORG_WINDOW: u64 = 128;

// Custom error InvalidEpoch() on the pinned ZoruLedger Solidity ABI.
// 0xd5b25b63 = keccak256("InvalidEpoch()")[..4]. This is NOT a
// general-purpose revert suppressor: callers must recheck epochNow() too.
fn is_epoch_boundary_revert(error: &anyhow::Error) -> bool {
    error.to_string() == "eth_call: RPC error 3 0xd5b25b63"
}

pub const IMPL_SLOT: &str = "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc";

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
            result.schema == "zoru-mainnet-suite-1" && result.chain_id == CHAIN,
            "Only ZORU mainnet chain 6736 is enabled"
        );
        ensure!(
            matches!(result.mode.as_str(), "local" | "mainnet"),
            "Unknown network mode"
        );
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

/// The signed calldata is the sole source of the publication epoch.
/// Check the signature and every identity field before trusting it.
fn signed_beacon_epoch(entry: &Journal, beacon: Address) -> Result<u64> {
    let raw = abi::bytes(&entry.raw)?;
    let tx = TxEnvelope::decode_2718_exact(&raw)?;
    ensure!(
        entry.chain_id == CHAIN
            && tx.chain_id() == Some(CHAIN)
            && tx.nonce() == entry.nonce
            && tx.to() == Some(beacon)
            && tx.value() == U256::ZERO
            && abi::keccak(&raw) == entry.hash
            && tx.recover_signer()? == entry.account,
        "Signed beacon journal identity mismatch"
    );
    beacon_epoch_from_calldata(tx.input())
}

/// Never discard a signed journal solely because the RPC rejected it.
/// A competing miner may have published the same beacon: archive the obsolete
/// transaction only after confirming the original hash is absent, the nonce
/// is unchanged on both chain views, and the exact epoch is either cached
/// on-chain or already closed for mining. Keep the original signed bytes.
fn obsolete_beacon_journal(
    entry: &Journal,
    latest_nonce: u64,
    pending_nonce: u64,
    transaction_known: bool,
    seed_verified: bool,
    epoch_closed: bool,
) -> bool {
    entry.label == "publish-drand"
        && entry.state == "prepared"
        && entry.nonce == latest_nonce
        && entry.nonce == pending_nonce
        && !transaction_known
        && (seed_verified || epoch_closed)
}

pub struct Client {
    pub manifest: Manifest,
    pub read: Rpc,
    pub submit: Rpc,
    pub native: Rpc,
    pub journal: PathBuf,
    pub receipts: PathBuf,
    head: Mutex<(u64, Instant)>,
    pub transactions: Mutex<Vec<Value>>,
}
impl Client {
    pub fn new(
        manifest: Manifest,
        read: &str,
        submit: &str,
        native: &str,
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
        let result = Arc::new(Self {
            manifest,
            read: Rpc::new(read, Duration::from_millis(80))?,
            submit: Rpc::new(submit, Duration::from_millis(1500))?,
            native: Rpc::new(native, Duration::from_millis(80))?,
            journal,
            receipts,
            head: Mutex::new((0, Instant::now())),
            transactions: Mutex::new(Vec::new()),
        });
        result.identity()?;
        Ok(result)
    }
    pub fn identity(&self) -> Result<()> {
        let chain = self.read.request("eth_chainId", json!([]))?;
        ensure!(
            abi::quantity(chain.as_str().context("Invalid chain id")?)? == U256::from(CHAIN),
            "Wrong EVM chain: expected 6736"
        );
        if self.manifest.mode == "local" {
            let version = self.read.request("web3_clientVersion", json!([]))?;
            ensure!(
                version
                    .as_str()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains("anvil"),
                "Expected isolated Anvil local chain"
            );
        } else {
            ensure!(
                self.native.native()?["chain_id"] == NATIVE_ID,
                "Wrong VEX native network"
            );
        }
        Ok(())
    }
    pub fn health(&self) -> Result<()> {
        if self.manifest.mode == "mainnet" {
            let info = self.native.native()?;
            ensure!(info["chain_id"] == NATIVE_ID, "Wrong VEX native network");
            let head = info["head_block_num"]
                .as_u64()
                .context("Native head is unavailable")?;
            let mut previous = self.head.lock().unwrap();
            if previous.0 != head {
                *previous = (head, Instant::now());
            }
            ensure!(
                previous.1.elapsed() < Duration::from_secs(10),
                "Native head is stalled"
            );
        }
        Ok(())
    }
    pub fn call(&self, to: Address, signature: &str, words: &[B256]) -> Result<String> {
        let value = self.read.request(
            "eth_call",
            json!([{"to":to,"data":abi::call(signature,words)},"latest"]),
        )?;
        Ok(value.as_str().context("Invalid ABI response")?.into())
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
    pub fn work(&self, beneficiary: Address) -> Result<Option<Work>> {
        let d = &self.manifest;
        let now = self.now()?;
        if now < d.launch_time {
            return Ok(None);
        }
        let epoch = self.epoch()?;
        let generation = self.number(d.ledger, "moduleGeneration()")?;
        // eth_call is executed at "latest". A block may cross the 60-second
        // boundary after epochNow() was read but before targetForEpoch() is
        // evaluated. The Ledger correctly reverts InvalidEpoch() in that case.
        // Discard only that verified epoch transition; do NOT suppress an
        // InvalidEpoch in the same epoch or any other revert.
        let target = match self.call(
            d.ledger,
            "targetForEpoch(uint48)",
            &[abi::uint(U256::from(epoch))],
        ) {
            Ok(result) => abi::scalar(&result)?,
            Err(error) if is_epoch_boundary_revert(&error) => {
                if self.epoch()? != epoch {
                    return Ok(None);
                }
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        let seed = abi::words(&self.call(
            d.beacon,
            "verifiedSeed(uint48)",
            &[abi::uint(U256::from(epoch))],
        )?)?;
        ensure!(seed.len() == 1, "Invalid seed response");
        if seed[0] == B256::ZERO {
            return Ok(None);
        }
        let work = Work {
            chain_id: CHAIN,
            ledger: d.ledger,
            token: d.token,
            generation,
            epoch,
            seed: seed[0],
            target,
            end_time: d.launch_time + (epoch + 1) * d.epoch_seconds,
        };
        let nonce = (U256::from(1) << 128) + U256::from(7);
        let expected = abi::words(&self.call(
            d.ledger,
            "digestFor(uint48,bytes32,address,uint256)",
            &[
                abi::uint(U256::from(epoch)),
                work.seed,
                abi::address(beneficiary),
                abi::uint(nonce),
            ],
        )?)?;
        ensure!(
            expected.len() == 1 && expected[0] == pow::digest(&work, beneficiary, nonce),
            "Miner and ledger digest domains differ"
        );
        Ok(Some(work))
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
        let quote = self.read.request("eth_gasPrice", json!([]))?;
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
    pub fn receipt(&self, hash: B256, timeout: Duration) -> Result<Value> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(receipt) = self
                .read
                .request("eth_getTransactionReceipt", json!([hash]))
                && !receipt.is_null()
            {
                let block = self.read.request(
                    "eth_getBlockByNumber",
                    json!([receipt["blockNumber"], false]),
                )?;
                if block["hash"] == receipt["blockHash"] {
                    return Ok(receipt);
                }
            }
            self.health()?;
            thread::sleep(Duration::from_millis(400));
        }
        bail!("Receipt pending; retain the signed transaction journal")
    }
    pub fn reconcile(&self, account: Address, confirmed: bool) -> Result<()> {
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
                entry.chain_id == CHAIN && entry.account == account,
                "Journal network or account mismatch"
            );
            if matches!(entry.state.as_str(), "replaced" | "superseded-beacon") {
                self.archive_journal(&entry)?;
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
            if entry.state == "confirmed" && !confirmed && !old {
                continue;
            }
            let receipt = self
                .read
                .request("eth_getTransactionReceipt", json!([entry.hash]))?;
            let canonical = if receipt.is_null() {
                false
            } else {
                self.read.request(
                    "eth_getBlockByNumber",
                    json!([receipt["blockNumber"], false]),
                )?["hash"]
                    == receipt["blockHash"]
            };
            if canonical {
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
            if self.nonce(account, "latest")? > entry.nonce {
                let head: u64 = abi::quantity(
                    self.read
                        .request("eth_blockNumber", json!([]))?
                        .as_str()
                        .context("Invalid head")?,
                )?
                .try_into()?;
                let mut replacement = None;
                for block in (head.saturating_sub(32)..=head).rev() {
                    let data = self.read.request(
                        "eth_getBlockByNumber",
                        json!([format!("0x{block:x}"), true]),
                    )?;
                    for tx in data["transactions"].as_array().into_iter().flatten() {
                        if tx["from"]
                            .as_str()
                            .is_some_and(|v| v.eq_ignore_ascii_case(&format!("{account:#x}")))
                            && abi::quantity(tx["nonce"].as_str().unwrap_or("0x0"))?
                                == U256::from(entry.nonce)
                        {
                            replacement = Some(
                                tx["hash"]
                                    .as_str()
                                    .context("Missing hash")?
                                    .parse::<B256>()?,
                            );
                        }
                    }
                    if replacement.is_some() {
                        break;
                    }
                }
                ensure!(
                    replacement.is_some(),
                    "Consumed nonce could not be reconciled; review the retained journal"
                );
                entry.state = "replaced".into();
                entry.replacement = replacement;
                self.write_journal(&entry)?;
                continue;
            }
            if entry.label == "publish-drand" && entry.state == "prepared" {
                let epoch = signed_beacon_epoch(&entry, self.manifest.beacon)?;
                let latest = self.nonce(account, "latest")?;
                let pending = self.nonce(account, "pending")?;
                if latest == entry.nonce && pending == entry.nonce {
                    let known = !self
                        .read
                        .request("eth_getTransactionByHash", json!([entry.hash]))?
                        .is_null();
                    if !known {
                        let seed_verified = abi::scalar(&self.call(
                            self.manifest.beacon,
                            "verifiedSeed(uint48)",
                            &[abi::uint(U256::from(epoch))],
                        )?)? != U256::ZERO;
                        let epoch_closed = self.now()?
                            >= self.manifest.launch_time
                                + (epoch + 1) * self.manifest.epoch_seconds;
                        if obsolete_beacon_journal(
                            &entry,
                            latest,
                            pending,
                            known,
                            seed_verified,
                            epoch_closed,
                        ) {
                            entry.state = "superseded-beacon".into();
                            self.write_journal(&entry)?;
                            self.archive_journal(&entry)?;
                            continue;
                        }
                    }
                }
            }
            let broadcast = self
                .submit
                .request("eth_sendRawTransaction", json!([entry.raw]));
            let receipt = match self.receipt(entry.hash, Duration::from_secs(35)) {
                Ok(receipt) => receipt,
                Err(receipt_error) => return Err(broadcast.err().unwrap_or(receipt_error)),
            };
            entry.state = "confirmed".into();
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
        ensure!(
            [self.manifest.ledger, self.manifest.beacon].contains(&to),
            "Unexpected transaction recipient"
        );
        self.reconcile(signer.address(), false)?;
        self.health()?;
        self.identity()?;
        self.implementations()?;
        self.verify_owner()?;
        let generation = self.number(self.manifest.ledger, "moduleGeneration()")?;
        ensure!(
            generation == self.number(self.manifest.admin, "suiteRevision()")?
                && self.address(self.manifest.ledger, "protocolAdmin()")? == self.manifest.admin,
            "Upgrade checkpoint changed"
        );
        if let Some(work) = expected {
            ensure!(
                generation == work.generation
                    && self.epoch()? == work.epoch
                    && self.now()? < work.end_time.saturating_sub(8),
                "STALE_WORK: epoch or generation changed"
            );
        }
        let account = signer.address();
        let nonce = self.nonce(account, "pending")?;
        ensure!(
            nonce == self.nonce(account, "latest")?,
            "Another transaction is pending for this account"
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
        ensure!(
            gas <= 22_000_000
                && maximum_fee <= U256::from(MAX_TRANSACTION_FEE)
                && self.balance(account)? >= maximum_fee,
            "Insufficient bounded gas balance"
        );
        if let Some(work) = expected {
            ensure!(
                self.epoch()? == work.epoch && self.now()? < work.end_time.saturating_sub(5),
                "STALE_WORK: inclusion window closing"
            );
        }
        let tx = TxLegacy {
            chain_id: Some(CHAIN),
            nonce,
            gas_price,
            gas_limit: gas,
            to: TxKind::Call(to),
            value: U256::ZERO,
            input: Bytes::from(abi::bytes(&data)?),
        };
        let signature = signer.sign_hash_sync(&tx.signature_hash())?;
        let envelope = TxEnvelope::from(tx.into_signed(signature));
        let raw = envelope.encoded_2718();
        let hash = abi::keccak(&raw);
        if label == "publish-drand" {
            ensure!(
                to == self.manifest.beacon,
                "Unexpected drand publication recipient"
            );
            beacon_epoch_from_calldata(&abi::bytes(&data)?)?;
        }
        let mut record = Journal {
            chain_id: CHAIN,
            account,
            nonce,
            hash,
            raw: format!("0x{}", hex::encode(raw)),
            state: "prepared".into(),
            label: label.into(),
            receipt: None,
            replacement: None,
        };
        self.write_journal(&record)?;
        let started = Instant::now();
        let mut result = None;
        for attempt in 0..2 {
            let broadcast = self
                .submit
                .request("eth_sendRawTransaction", json!([record.raw]));
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
            assert_eq!(signed_beacon_epoch(&recovered, beacon).unwrap(), epoch);
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
            assert!(signed_beacon_epoch(&bad, beacon).is_err(), "{field}");
        }
        assert!(signed_beacon_epoch(&entry, Address::ZERO).is_err());
        for index in [0, 4, 29, 36, 67, 68, 99] {
            let mut input = publication_input(21);
            input[index] ^= 1;
            let (bad, beacon) = signed_publication(input);
            assert!(signed_beacon_epoch(&bad, beacon).is_err(), "byte {index}");
        }
        let (bad, beacon) = signed_publication(publication_input(1u64 << 48));
        assert!(signed_beacon_epoch(&bad, beacon).is_err());
    }

    #[test]
    fn obsolete_beacon_journal_requires_verified_obsolescence_and_unused_nonce() {
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
        };
        assert!(obsolete_beacon_journal(&entry, 5, 5, false, true, false));
        assert!(obsolete_beacon_journal(&entry, 5, 5, false, false, true));
        assert!(!obsolete_beacon_journal(&entry, 5, 5, true, false, true));
        assert!(!obsolete_beacon_journal(&entry, 4, 5, false, true, false));
        assert!(!obsolete_beacon_journal(&entry, 5, 6, false, true, false));
        assert!(!obsolete_beacon_journal(&entry, 5, 5, true, true, false));
        assert!(!obsolete_beacon_journal(&entry, 5, 5, false, false, false));
        entry.state = "confirmed".into();
        assert!(!obsolete_beacon_journal(&entry, 5, 5, false, true, false));
        entry.state = "prepared".into();
        entry.label = "mined-proof".into();
        assert!(!obsolete_beacon_journal(&entry, 5, 5, false, true, false));
    }

    #[test]
    fn only_exact_invalid_epoch_selector_is_a_boundary_candidate() {
        assert!(is_epoch_boundary_revert(&anyhow::anyhow!(
            "eth_call: RPC error 3 0xd5b25b63"
        )));
        for unrelated in [
            "eth_call: RPC error 3 0x09bde339",
            "eth_call: RPC error 3 0xd5b25b64",
            "eth_estimateGas: RPC error 3 0xd5b25b63",
            "eth_call: RPC transport unavailable",
        ] {
            assert!(!is_epoch_boundary_revert(&anyhow::anyhow!("{unrelated}")));
        }
    }

    #[test]
    fn epoch_boundary_matches_deployed_contract_signature() {
        assert_eq!(
            &format!("{:#x}", abi::keccak("InvalidEpoch()".as_bytes()))[..10],
            "0xd5b25b63"
        );
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
    fn legacy_signature_recovers_expected_signer() {
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
