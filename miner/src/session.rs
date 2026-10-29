use crate::{
    abi,
    history::{History, proof_record},
    pow::{self, Work},
    protocol::{Client, DRAND_HASH, Manifest},
    storage::{self, WriterLock},
    wallet,
};
use alloy_primitives::{Address, B256, U256};
use alloy_signer_local::PrivateKeySigner;
use anyhow::{Context, Result, ensure};
use rand::RngCore;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::PathBuf,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub type Emit = Arc<dyn Fn(Value) + Send + Sync>;
#[derive(Clone)]
pub struct Options {
    pub manifest: PathBuf,
    pub read: String,
    pub submit: String,
    pub native: String,
    pub wallet: Option<PathBuf>,
    pub expected_wallet: Option<Address>,
    pub observer: Option<Address>,
    pub sync_on_connect: bool,
    pub fee_budget: Option<U256>,
    pub state_dir: PathBuf,
}
#[derive(Clone, Copy)]
pub struct Run {
    pub workers: usize,
    pub cpu_budget: f64,
    pub seconds: u64,
    pub max_accepted: u64,
    pub auto_claim_after: u64,
}
#[derive(Clone)]
struct Candidate {
    work: Work,
    nonce: U256,
    digest: B256,
}
// The bounded channel holds under 16 KiB; keep proof results allocation-free.
#[allow(clippy::large_enum_variant)]
enum Task {
    Proof(Candidate),
    Beacon(u64),
}

pub struct Session {
    pub client: Arc<Client>,
    signer: Option<PrivateKeySigner>,
    pub account: Address,
    pub stop: Arc<AtomicBool>,
    pub pause: Arc<AtomicBool>,
    work: Arc<RwLock<Option<Work>>>,
    stats: Mutex<Value>,
    history: History,
    state_dir: PathBuf,
    _lock: Option<WriterLock>,
    emit: Emit,
    revision: Mutex<U256>,
}
fn topic(signature: &str) -> B256 {
    abi::keccak(signature.as_bytes())
}
fn topic_address(address: Address) -> String {
    format!("{:#x}", abi::address(address))
}
fn receipt_event<'a>(receipt: &'a Value, contract: Address, signature: &str) -> Result<&'a Value> {
    let expected_topic = format!("{:#x}", topic(signature));
    let expected_address = format!("{contract:#x}");
    let mut events = receipt["logs"]
        .as_array()
        .context("Receipt logs missing")?
        .iter()
        .filter(|log| {
            log["address"]
                .as_str()
                .is_some_and(|value| value.eq_ignore_ascii_case(&expected_address))
                && log["topics"][0]
                    .as_str()
                    .is_some_and(|value| value.eq_ignore_ascii_case(&expected_topic))
        });
    let event = events.next().context("Expected receipt event missing")?;
    ensure!(
        events.next().is_none(),
        "Receipt contains duplicate expected events"
    );
    Ok(event)
}
fn int(value: &Value) -> Result<U256> {
    abi::quantity(value.as_str().context("Expected RPC quantity")?)
}
fn set_counter(stats: &mut Value, name: &str, delta: u64) {
    stats[name] = json!(stats[name].as_u64().unwrap_or(0).saturating_add(delta));
}
fn short_error(error: &anyhow::Error) -> String {
    error.to_string().chars().take(350).collect()
}

// A stale candidate is discarded before broadcast; it is not a rejected
// on-chain proof. This distinction is shared by the CLI and desktop backend.
fn is_stale_candidate(message: &str) -> bool {
    message.starts_with("STALE_WORK: ")
}

// A public mining outcome is based on verified ProofAccepted receipts already
// counted by submit_candidate, not on ad-hoc interpretation by service scripts.
fn mining_outcome(state: &Value) -> &'static str {
    let accepted = state["accepted"].as_u64().unwrap_or(0);
    let rejected = state["rejected"].as_u64().unwrap_or(0);
    let errors = state["errors"].as_array().is_some_and(|a| !a.is_empty());
    if accepted > 0 {
        if rejected > 0 || errors {
            "accepted-with-errors"
        } else {
            "accepted"
        }
    } else if rejected > 0 || errors {
        "failed"
    } else {
        "no-proof"
    }
}

fn beacon_publication(epoch: u64, signature: &[u8]) -> Result<String> {
    ensure!(
        epoch < 1u64 << 48 && signature.len() == 64,
        "Invalid beacon publication"
    );
    let mut encoded = abi::call(
        "publish(uint48,bytes)",
        &[
            abi::uint(U256::from(epoch)),
            abi::uint(U256::from(64)),
            abi::uint(U256::from(64)),
        ],
    );
    encoded.push_str(&hex::encode(signature));
    Ok(encoded)
}

fn preflight_beacon(
    rpc: &crate::rpc::Rpc,
    beacon: Address,
    epoch: u64,
    signature: &[u8],
) -> Result<()> {
    let result = rpc.request(
        "eth_call",
        json!([{"to":beacon,"data":beacon_publication(epoch, signature)?},"latest"]),
    )?;
    let observed = abi::bytes(
        result
            .as_str()
            .context("Invalid beacon verification response")?,
    )?;
    ensure!(
        observed == Sha256::digest(signature).as_slice(),
        "Beacon verification response mismatch"
    );
    Ok(())
}

fn fetch_scheduled_beacon<F>(
    endpoints: &[&str],
    round: U256,
    mut authenticate: F,
) -> Result<(Value, Vec<u8>)>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    let http = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    for endpoint in endpoints {
        let response = (|| -> Result<(Value, Vec<u8>)> {
            let mut bytes = Vec::new();
            http.get(format!("{endpoint}/{DRAND_HASH}/public/{round}"))
                .send()?
                .error_for_status()?
                .take(16385)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 16384, "Beacon response too large");
            let data: Value = serde_json::from_slice(&bytes)?;
            ensure!(
                data["round"]
                    .as_u64()
                    .is_some_and(|v| U256::from(v) == round),
                "Wrong drand round"
            );
            let signature = hex::decode(
                data["signature"]
                    .as_str()
                    .context("Missing drand signature")?,
            )?;
            ensure!(
                signature.len() == 64
                    && hex::encode(Sha256::digest(&signature))
                        == data["randomness"].as_str().unwrap_or_default(),
                "Invalid beacon encoding"
            );
            authenticate(&signature)?;
            Ok((data, signature))
        })();
        if let Ok(verified_encoding) = response {
            return Ok(verified_encoding);
        }
    }
    Err(anyhow::anyhow!(
        "Scheduled drand round is unavailable or invalid"
    ))
}

impl Session {
    pub fn connect(options: Options, password: Zeroizing<String>, emit: Emit) -> Result<Arc<Self>> {
        let manifest = Manifest::load(&options.manifest)?;
        let signer = options
            .wallet
            .as_ref()
            .map(|path| wallet::unlock(path, password))
            .transpose()?;
        let account = signer
            .as_ref()
            .map(|v| v.address())
            .or(options.observer)
            .unwrap_or(Address::ZERO);
        ensure!(
            options.observer.is_none() || signer.is_none(),
            "Use --account for read-only inspection without a wallet"
        );
        if let Some(expected) = options.expected_wallet {
            ensure!(
                account == expected,
                "Wallet does not match --expected-wallet"
            );
        }
        let root = storage::default_root()?;
        let lock = if signer.is_some() {
            Some(WriterLock::acquire(
                &root.join("signers"),
                &format!("{account:#x}"),
            )?)
        } else {
            None
        };
        let journal = root
            .join("signers")
            .join(&manifest.mode)
            .join(format!("{account:#x}"))
            .join("private-journal");
        fs::create_dir_all(&options.state_dir)?;
        let client = Client::new(
            manifest,
            &options.read,
            &options.submit,
            &options.native,
            journal,
            options.state_dir.join("receipts"),
        )?;
        client.fees.lock().unwrap().set_limit(options.fee_budget)?;
        let revision = client.verify()?;
        if signer.is_some() {
            client.reconcile(account, true)?;
        }
        let history = History::open(
            &options
                .state_dir
                .join(format!("{:#x}-{account:#x}.sqlite", client.manifest.ledger)),
        )?;
        let paid = history.paid()?;
        let result = Arc::new(Self {
            stop: client.cancelled.clone(),
            client,
            signer,
            account,
            pause: Arc::new(AtomicBool::new(false)),
            work: Arc::new(RwLock::new(None)),
            stats: Mutex::new(
                json!({"status":"connected","address":if account==Address::ZERO{String::new()}else{format!("{account:#x}")},"chainId":6736,"hashes":0,"validFound":0,"accepted":0,"rejected":0,"staleBeforeBroadcast":0,"resourcePauses":0,"hashesPerSecond":0,"rewardPaidBaseUnits":paid.to_string(),"contexts":[],"events":[],"beaconsPublished":[],"errors":[],"warnings":[],"runOutcome":"not-started"}),
            ),
            history,
            state_dir: options.state_dir,
            _lock: lock,
            emit,
            revision: Mutex::new(revision),
        });
        if result.account != Address::ZERO && options.sync_on_connect {
            result.stats.lock().unwrap()["status"] = json!("syncing-history");
            result.publish()?;
            if result.signer.is_none() {
                result.inspect()?;
            }
            result.sync_history()?;
            result.stats.lock().unwrap()["status"] = json!("connected");
        }
        result.publish()?;
        Ok(result)
    }
    pub fn publish(&self) -> Result<()> {
        let mut data = {
            let mut stats = self.stats.lock().unwrap();
            for key in [
                "errors",
                "warnings",
                "events",
                "contexts",
                "beaconsPublished",
            ] {
                if let Some(items) = stats[key].as_array_mut() {
                    let max = if key == "events" { 256 } else { 64 };
                    if items.len() > max {
                        items.drain(..items.len() - max);
                    }
                }
            }
            stats.clone()
        };
        data["fees"] = self.client.fees.lock().unwrap().snapshot();
        if self.signer.is_some() {
            storage::atomic_json(&self.state_dir.join("progress.json"), &data)?;
        }
        (self.emit)(json!({"event":"state","data":data}));
        Ok(())
    }
    fn error(&self, error: anyhow::Error, attempt: bool) {
        let text = short_error(&error);
        if text.starts_with("FEE_BUDGET_EXHAUSTED:") {
            self.stop.store(true, Ordering::SeqCst);
        }
        if text == "Signing stopped" && self.stop.load(Ordering::Relaxed) {
            return;
        }
        let stale = is_stale_candidate(&text);
        let mut stats = self.stats.lock().unwrap();
        if stale {
            set_counter(&mut stats, "staleBeforeBroadcast", 1);
            stats["warnings"]
                .as_array_mut()
                .unwrap()
                .push(json!({"message":text}));
        } else {
            if attempt {
                set_counter(&mut stats, "rejected", 1);
            }
            stats["errors"]
                .as_array_mut()
                .unwrap()
                .push(json!({"message":text}));
        }
        drop(stats);
        if stale {
            (self.emit)(json!({"event":"warning","warning":text}));
        } else {
            (self.emit)(json!({"event":"error","error":text}));
        }
    }
    pub fn inspect(&self) -> Result<Value> {
        let c = &self.client;
        let d = &c.manifest;
        let inspected_at = Instant::now();
        let block = c
            .read
            .request("eth_getBlockByNumber", json!(["latest", false]))?;
        let height = block["number"].as_str().context("Block height missing")?;
        let call = |to: Address, signature: &str, words: &[B256]| -> Result<String> {
            let result = c.read.request(
                "eth_call",
                json!([{"to":to,"data":abi::call(signature, words)},height]),
            )?;
            Ok(result.as_str().context("Invalid ABI response")?.to_owned())
        };
        let number = |to, signature| abi::scalar(&call(to, signature, &[])?);
        let allocated = number(d.ledger, "allocatedSupply()")?;
        let funded = number(d.ledger, "fundedSupply()")?;
        let paid = number(d.ledger, "paidSupply()")?;
        let (pending, unclaimed) = crate::rewards::split_global_rewards(allocated, funded, paid)?;
        let total_supply = number(d.token, "totalSupply()")?;
        let lifetime_minted = number(d.token, "lifetimeMinted()")?;
        let total_proofs = number(d.ledger, "totalProofs()")?;
        let emission_index = number(d.ledger, "emissionIndex()")?;
        let halving_epochs = number(d.ledger, "HALVING_EPOCHS()")?;
        let next_budget = abi::scalar(&call(
            d.ledger,
            "budgetForIndex(uint256)",
            &[abi::uint(emission_index)],
        )?)?;
        let next_halving = (emission_index / halving_epochs + U256::from(1)) * halving_epochs;
        let following_budget = abi::scalar(&call(
            d.ledger,
            "budgetForIndex(uint256)",
            &[abi::uint(next_halving)],
        )?)?;
        let chain_time = int(&block["timestamp"])?;
        let mining_launched = chain_time >= U256::from(d.launch_time);
        let current_epoch = if mining_launched {
            Some(number(d.ledger, "epochNow()")?.to_string())
        } else {
            None
        };
        let claims_paused = abi::boolean(&call(d.ledger, "claimsPaused()", &[])?)?;
        let admission_paused = abi::boolean(&call(d.ledger, "admissionPaused()", &[])?)?;
        let mut data = json!({
            "chainId":6736,
            "transactionFeeCapWei":crate::protocol::MAX_TRANSACTION_FEE.to_string(),
            "blockNumber":int(&block["number"])?.to_string(),
            "blockHash":block["hash"],
            "blockTimestamp":chain_time.to_string(),
            "emissionIndex":emission_index.to_string(),
            "halvingEpochs":halving_epochs.to_string(),
            "halvingProgressEpochs":(emission_index % halving_epochs).to_string(),
            "epochsUntilHalving":(next_halving - emission_index).to_string(),
            "halvingEra":(emission_index / halving_epochs).to_string(),
            "followingEraBudgetBaseUnits":following_budget.to_string(),
            "epochSeconds":d.epoch_seconds.to_string(),
            "nextEmissionBudgetBaseUnits":next_budget.to_string(),
            "capBaseUnits":number(d.ledger, "CAP()")?.to_string(),
            "contracts":{"token":d.token,"ledger":d.ledger,"engine":d.engine,"beacon":d.beacon,"admin":d.admin},
            "totalSupplyBaseUnits":total_supply.to_string(),
            "lifetimeMintedBaseUnits":lifetime_minted.to_string(),
            "allocatedSupplyBaseUnits":allocated.to_string(),
            "fundedSupplyBaseUnits":funded.to_string(),
            "paidSupplyBaseUnits":paid.to_string(),
            "globalPendingBaseUnits":pending.to_string(),
            "globalUnclaimedBaseUnits":unclaimed.to_string(),
            "globalClaimedBaseUnits":paid.to_string(),
            "totalAcceptedProofs":total_proofs.to_string(),
            "currentEpoch":current_epoch,
            "miningLaunched":mining_launched,
            "launchTime":d.launch_time.to_string(),
            "claimsPaused":claims_paused,
            "admissionPaused":admission_paused
        });
        if let Some(epoch) = &current_epoch {
            let word = abi::uint(abi::quantity(epoch)?);
            let info = abi::words(&call(d.ledger, "epochInfo(uint48)", &[word])?)?;
            ensure!(info.len() == 10, "Invalid epoch schema");
            data["currentEpochBudgetBaseUnits"] = if abi::number(info[0]) == U256::ZERO {
                Value::Null
            } else {
                json!(abi::number(info[2]).to_string())
            };
            data["currentEpochProofs"] = json!(abi::number(info[3]).to_string());
            let target = call(d.ledger, "targetForEpoch(uint48)", &[word]);
            data["target"] = match target {
                Ok(value) => json!(abi::scalar(&value)?.to_string()),
                Err(error)
                    if admission_paused
                        || claims_paused
                        || error.to_string().ends_with("0xd5b25b63") =>
                {
                    Value::Null
                }
                Err(error) => return Err(error),
            };
            if let Some(target) = data["target"].as_str() {
                let target = abi::quantity(target)?;
                if target != U256::ZERO {
                    data["expectedHashes"] = json!((U256::MAX / target).to_string());
                }
            }
            data["beaconVerified"] = json!(
                abi::scalar(&call(d.beacon, "verifiedSeed(uint48)", &[word])?)? != U256::ZERO
            );
        }
        data["fees"] = c.fees.lock().unwrap().snapshot();
        if self.account != Address::ZERO {
            data["address"] = json!(self.account);
            data["nativeBalanceWei"] = json!(
                int(&c
                    .read
                    .request("eth_getBalance", json!([self.account, height]))?)?
                .to_string()
            );
            data["tokenBalanceBaseUnits"] = json!(
                abi::scalar(&call(
                    d.token,
                    "balanceOf(address)",
                    &[abi::address(self.account)]
                )?)?
                .to_string()
            );
            // A wallet's pending and unclaimed rewards must be computed by proof,
            // not from global supply. A single epoch budget is shared by all proofs.
            // The local history is synced on connection and after settlement.
            let proofs = self.history.pending(10_001)?;
            let head: u64 = int(&block["number"])?.try_into()?;
            let synced_at = self.history.cursor()?.unwrap_or(0);
            let complete = proofs.len() <= 10_000
                && synced_at >= head.saturating_sub(crate::protocol::REORG_WINDOW);
            let mut cached_epochs = std::collections::BTreeMap::new();
            let mut my_pending = U256::ZERO;
            let mut my_unclaimed = U256::ZERO;
            for proof in proofs.iter().take(10_000).filter(|p| p.canonical) {
                let index = proof.index_plus_one;
                let epoch = if let Some(value) = cached_epochs.get(&proof.epoch) {
                    *value
                } else {
                    let state = abi::words(&call(
                        d.ledger,
                        "epochInfo(uint48)",
                        &[abi::uint(U256::from(proof.epoch))],
                    )?)?;
                    ensure!(
                        state.len() == 10,
                        "Ledger epoch schema differs from this release"
                    );
                    let value = (
                        abi::number(state[2]),
                        abi::number(state[3]),
                        abi::number(state[6]) == U256::from(1),
                    );
                    cached_epochs.insert(proof.epoch, value);
                    value
                };
                let reward = crate::rewards::proof_reward(epoch.0, epoch.1, index)?;
                if epoch.2 {
                    my_unclaimed = my_unclaimed
                        .checked_add(reward)
                        .context("Unclaimed reward overflow")?;
                } else {
                    my_pending = my_pending
                        .checked_add(reward)
                        .context("Pending reward overflow")?;
                }
            }
            let my_claimed = self.history.paid()?;
            data["myPendingBaseUnits"] = json!(my_pending.to_string());
            data["myUnclaimedBaseUnits"] = json!(my_unclaimed.to_string());
            data["myClaimedBaseUnits"] = json!(my_claimed.to_string());
            data["myRewardsHistoryComplete"] = json!(complete);
            data["myPendingProofsCounted"] = json!(proofs.len().min(10_000));
            data["myProofCount"] = json!(self.history.raw_count()?.to_string());
            data["myClaimedProofs"] = json!(self.history.claimed_count()?.to_string());
            let mut rows = Vec::new();
            for proof in self.history.latest(100)? {
                let mut row = serde_json::to_value(&proof)?;
                row["epochStart"] =
                    json!((d.launch_time + proof.epoch * d.epoch_seconds).to_string());
                rows.push(row);
            }
            data["proofs"] = json!(rows);
        }
        let canonical = c
            .read
            .request("eth_getBlockByNumber", json!([height, false]))?;
        ensure!(
            block["hash"].as_str().is_some() && canonical["hash"] == block["hash"],
            "Inspection block changed; refresh the snapshot"
        );
        let times = c.read.latencies.lock().unwrap();
        let latency =
            times.iter().rev().take(32).copied().sum::<f64>() / times.len().clamp(1, 32) as f64;
        drop(times);
        data["rpcLatencyMilliseconds"] = json!(latency * 1000.0);
        data["inspectionMilliseconds"] = json!(inspected_at.elapsed().as_millis() as u64);
        (self.emit)(json!({"event":"inspection","data":data}));
        Ok(data)
    }
    pub fn sync_history(&self) -> Result<()> {
        let c = &self.client;
        let d = &c.manifest;
        let initial = abi::quantity(&d.deployment_block_number)?
            .try_into()
            .context("Invalid deployment height")?;
        let head: u64 = int(&c.read.request("eth_blockNumber", json!([]))?)?.try_into()?;
        let mut start = self
            .history
            .cursor()?
            .unwrap_or(initial)
            .min(head)
            .saturating_sub(crate::protocol::REORG_WINDOW)
            .max(initial);
        let mut after = String::new();
        loop {
            let recent = self.history.recent(start, &after)?;
            if recent.is_empty() {
                break;
            }
            for mut proof in recent {
                after = format!("{:#x}", proof.digest);
                let record = self.proof_state(proof.digest)?;
                proof.canonical = record.2 != U256::ZERO && record.0 == self.account;
                proof.claimed = record.3;
                if !proof.canonical || !proof.claimed {
                    proof.reward = None;
                    proof.claim_block = None;
                }
                self.history.put(&proof)?;
            }
        }
        let deadline = Instant::now() + Duration::from_secs(90);
        while start <= head && Instant::now() < deadline && !self.stop.load(Ordering::Relaxed) {
            let end = head.min(start.saturating_add(9_999));
            let logs=c.read.request("eth_getLogs",json!([{"address":d.ledger,"fromBlock":format!("0x{start:x}"),"toBlock":format!("0x{end:x}"),"topics":[topic("ProofAccepted(uint48,bytes32,address,uint256,uint256)"),Value::Null,Value::Null,topic_address(self.account)]}]))?;
            for log in logs.as_array().context("Invalid event history")? {
                let mut proof = proof_record(log)?;
                let record = self.proof_state(proof.digest)?;
                proof.canonical = record.2 != U256::ZERO && record.0 == self.account;
                proof.claimed = record.3;
                if let Some(old) = self.history.get(proof.digest)? {
                    proof.reward = old.reward;
                    proof.claim_block = old.claim_block;
                }
                self.history.put(&proof)?;
            }
            let claims=c.read.request("eth_getLogs",json!([{"address":d.ledger,"fromBlock":format!("0x{start:x}"),"toBlock":format!("0x{end:x}"),"topics":[topic("RewardClaimed(uint48,bytes32,address,uint256)"),Value::Null,Value::Null,topic_address(self.account)]}]))?;
            for log in claims.as_array().context("Invalid claim history")? {
                let digest = log["topics"][2]
                    .as_str()
                    .context("Missing claim digest")?
                    .parse()?;
                if let Some(mut proof) = self.history.get(digest)? {
                    proof.claimed = true;
                    proof.reward = Some(abi::scalar(log["data"].as_str().unwrap_or_default())?);
                    proof.claim_block = Some(int(&log["blockNumber"])?.try_into()?);
                    self.history.put(&proof)?;
                }
            }
            self.history.set_cursor(end)?;
            if end == u64::MAX {
                break;
            }
            start = end + 1;
        }
        self.stats.lock().unwrap()["rewardPaidBaseUnits"] = json!(self.history.paid()?.to_string());
        Ok(())
    }
    fn proof_state(&self, digest: B256) -> Result<(Address, u64, U256, bool)> {
        let data = abi::words(&self.client.call(
            self.client.manifest.ledger,
            "proofs(bytes32)",
            &[digest],
        )?)?;
        ensure!(data.len() == 4, "Invalid proof record");
        ensure!(
            abi::number(data[3]) <= U256::from(1),
            "Invalid claim status"
        );
        Ok((
            Address::from_slice(&data[0].as_slice()[12..]),
            abi::number(data[1]).try_into()?,
            abi::number(data[2]),
            abi::number(data[3]) == U256::from(1),
        ))
    }
    fn publish_beacon(&self, epoch: u64) -> Result<()> {
        let signer = self.signer.as_ref().context("Unlock a mainnet wallet")?;
        let c = &self.client;
        let d = &c.manifest;
        let word = abi::uint(U256::from(epoch));
        if abi::scalar(&c.call(d.beacon, "verifiedSeed(uint48)", &[word])?)? != U256::ZERO {
            return Ok(());
        }
        let round = abi::scalar(&c.call(d.beacon, "expectedRound(uint48)", &[word])?)?;
        let fetched = fetch_scheduled_beacon(
            &["https://api.drand.sh", "https://api2.drand.sh"],
            round,
            |signature| preflight_beacon(&c.read, d.beacon, epoch, signature),
        );
        let (data, signature) = match fetched {
            Ok(beacon) => beacon,
            Err(error) => {
                if abi::scalar(&c.call(d.beacon, "verifiedSeed(uint48)", &[word])?)? != U256::ZERO {
                    return Ok(());
                }
                return Err(error);
            }
        };
        if c.epoch()? != epoch {
            return Ok(());
        }
        let encoded = beacon_publication(epoch, &signature)?;
        match c.send(signer, "publish-drand", d.beacon, encoded, None) {
            Ok(receipt) => {
                self.stats.lock().unwrap()["beaconsPublished"].as_array_mut().unwrap().push(json!({"epoch":epoch,"round":round.to_string(),"hash":receipt["transactionHash"],"signature":data["signature"],"randomness":data["randomness"]}));
                Ok(())
            }
            Err(error) => {
                if abi::scalar(&c.call(d.beacon, "verifiedSeed(uint48)", &[word])?)? != U256::ZERO {
                    Ok(())
                } else {
                    Err(error)
                }
            }
        }
    }
    fn submit_candidate(&self, candidate: Candidate) -> Result<()> {
        let signer = self.signer.as_ref().context("Unlock a mainnet wallet")?;
        let current = self.work.read().unwrap().clone();
        ensure!(
            current.as_ref() == Some(&candidate.work),
            "STALE_WORK: context changed before submission"
        );
        let data = abi::call(
            "submit(uint48,address,uint256)",
            &[
                abi::uint(U256::from(candidate.work.epoch)),
                abi::address(self.account),
                abi::uint(candidate.nonce),
            ],
        );
        let started = Instant::now();
        let receipt = self.client.send(
            signer,
            "mined-proof",
            self.client.manifest.ledger,
            data,
            Some(&candidate.work),
        )?;
        let mut log = receipt_event(
            &receipt,
            self.client.manifest.ledger,
            "ProofAccepted(uint48,bytes32,address,uint256,uint256)",
        )?
        .clone();
        log["transactionHash"] = receipt["transactionHash"].clone();
        log["blockNumber"] = receipt["blockNumber"].clone();
        let proof = proof_record(&log)?;
        ensure!(
            proof.digest == candidate.digest
                && proof.epoch == candidate.work.epoch
                && proof.generation == candidate.work.generation,
            "Accepted proof differs from submitted work"
        );
        ensure!(
            log["topics"][3]
                .as_str()
                .is_some_and(|v| v.eq_ignore_ascii_case(&topic_address(self.account))),
            "Proof beneficiary differs"
        );
        self.history.put(&proof)?;
        let gas_used: u64 = int(&receipt["gasUsed"])?.try_into()?;
        let mut stats = self.stats.lock().unwrap();
        set_counter(&mut stats, "accepted", 1);
        stats["events"].as_array_mut().unwrap().push(json!({"hash":proof.hash,"digest":proof.digest,"epoch":proof.epoch,"generation":proof.generation.to_string(),"gasUsed":gas_used,"candidateToReceiptSeconds":started.elapsed().as_secs_f64()}));
        Ok(())
    }
    pub fn begin_action(&self) {
        self.stop.store(false, Ordering::SeqCst);
        self.pause.store(false, Ordering::SeqCst);
    }
    pub fn settle(&self) -> Result<Value> {
        self.settle_batch(100, true)
    }
    fn settle_batch(&self, limit: usize, inspect: bool) -> Result<Value> {
        let signer = self.signer.as_ref().context("Unlock a mainnet wallet")?;
        if inspect {
            self.sync_history()?;
        }
        let mut claims = Vec::new();
        for mut proof in self.history.pending(limit)? {
            if self.stop.load(Ordering::Relaxed) {
                break;
            }
            let record = self.proof_state(proof.digest)?;
            if record.2 == U256::ZERO || record.0 != self.account {
                proof.canonical = false;
                self.history.put(&proof)?;
                continue;
            }
            if record.3 {
                proof.claimed = true;
                self.history.put(&proof)?;
                continue;
            }
            let state = abi::words(&self.client.call(
                self.client.manifest.ledger,
                "epochInfo(uint48)",
                &[abi::uint(U256::from(proof.epoch))],
            )?)?;
            ensure!(
                state.len() == 10,
                "Ledger epoch schema differs from this release"
            );
            if abi::number(state[6]) == U256::ZERO {
                let closes = abi::number(state[9]);
                if U256::from(self.client.now()?) < closes {
                    continue;
                }
                match self.client.send(
                    signer,
                    "seal",
                    self.client.manifest.ledger,
                    abi::call("seal(uint48)", &[abi::uint(U256::from(proof.epoch))]),
                    None,
                ) {
                    Ok(_) => {}
                    Err(e) => {
                        let fresh = abi::words(&self.client.call(
                            self.client.manifest.ledger,
                            "epochInfo(uint48)",
                            &[abi::uint(U256::from(proof.epoch))],
                        )?)?;
                        if abi::number(fresh[6]) != U256::from(1) {
                            return Err(e);
                        }
                    }
                }
            }
            match self.client.send(
                signer,
                "claim",
                self.client.manifest.ledger,
                abi::call("claim(bytes32)", &[proof.digest]),
                None,
            ) {
                Ok(receipt) => {
                    let log = receipt_event(
                        &receipt,
                        self.client.manifest.ledger,
                        "RewardClaimed(uint48,bytes32,address,uint256)",
                    )?;
                    ensure!(
                        log["topics"][1].as_str().map(abi::quantity).transpose()?
                            == Some(U256::from(proof.epoch))
                            && log["topics"][2].as_str().is_some_and(
                                |v| v.eq_ignore_ascii_case(&format!("{:#x}", proof.digest))
                            )
                            && log["topics"][3].as_str().is_some_and(
                                |v| v.eq_ignore_ascii_case(&topic_address(self.account))
                            ),
                        "Claim event differs from the accepted proof"
                    );
                    proof.reward = Some(abi::scalar(
                        log["data"].as_str().context("Claim reward missing")?,
                    )?);
                    proof.claimed = true;
                    proof.claim_block = Some(int(&receipt["blockNumber"])?.try_into()?);
                    self.history.put(&proof)?;
                    self.stats.lock().unwrap()["rewardPaidBaseUnits"] =
                        json!(self.history.paid()?.to_string());
                    self.publish()?;
                    claims.push(json!({"digest":proof.digest,"hash":receipt["transactionHash"],"amountBaseUnits":proof.reward.unwrap().to_string()}));
                }
                Err(error) => {
                    if self.proof_state(proof.digest)?.3 {
                        proof.claimed = true;
                        self.history.put(&proof)?;
                    } else {
                        return Err(error);
                    }
                }
            }
        }
        self.stats.lock().unwrap()["rewardPaidBaseUnits"] = json!(self.history.paid()?.to_string());
        if inspect {
            self.inspect()?;
        }
        self.publish()?;
        Ok(json!({"claims":claims}))
    }
    pub fn run(self: &Arc<Self>, options: Run) -> Result<Value> {
        ensure!(self.signer.is_some(), "Unlock a mainnet wallet");
        ensure!(
            [1, 2, 4].contains(&options.workers)
                && options.cpu_budget > 0.0
                && options.cpu_budget <= 2.0,
            "Choose 1,2 or4 workers and at most2 CPU cores"
        );
        {
            let mut stats = self.stats.lock().unwrap();
            for name in [
                "accepted",
                "rejected",
                "validFound",
                "hashes",
                "resourcePauses",
                "autoClaimed",
                "staleBeforeBroadcast",
            ] {
                stats[name] = json!(0);
            }
            for name in [
                "events",
                "errors",
                "warnings",
                "contexts",
                "beaconsPublished",
            ] {
                stats[name] = json!([]);
            }
        }
        let started = Instant::now();
        let cpu_begin = process_cpu();
        let hashes = Arc::new(AtomicU64::new(0));
        let found = Arc::new(AtomicU64::new(0));
        let resource = Arc::new(AtomicBool::new(false));
        let claiming = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel::<Task>(64);
        let run = self.clone();
        let work_tx = tx.clone();
        let poll_resource = resource.clone();
        let poll = thread::spawn(move || {
            while !run.stop.load(Ordering::Relaxed) {
                match run.client.work(run.account) {
                    Ok(fresh) => {
                        if let Some(ref work) = fresh {
                            let changed = run.work.read().unwrap().as_ref() != Some(work);
                            if changed {
                                let verify = (|| -> Result<()> {
                                    let mut rev = run.revision.lock().unwrap();
                                    if *rev != work.generation {
                                        *rev = run.client.verify()?;
                                        ensure!(
                                            *rev == work.generation,
                                            "Generation changed during refresh"
                                        );
                                    }
                                    Ok(())
                                })();
                                if let Err(error) = verify {
                                    run.error(error, false);
                                    *run.work.write().unwrap() = None;
                                    thread::sleep(Duration::from_secs(1));
                                    continue;
                                }
                                run.stats.lock().unwrap()["contexts"].as_array_mut().unwrap().push(json!({"epoch":work.epoch,"generation":work.generation.to_string(),"target":work.target.to_string(),"seed":work.seed}));
                            }
                        }
                        *run.work.write().unwrap() = fresh.clone();
                        if fresh.is_none()
                            && !run.pause.load(Ordering::Relaxed)
                            && !poll_resource.load(Ordering::Relaxed)
                            && let Ok(epoch) = run.client.epoch()
                        {
                            let _ = work_tx.try_send(Task::Beacon(epoch));
                        }
                    }
                    Err(error) => {
                        *run.work.write().unwrap() = None;
                        run.error(error, false)
                    }
                }
                thread::sleep(Duration::from_millis(700));
            }
        });
        let run = self.clone();
        let writer_resource = resource.clone();
        let writer_claiming = claiming.clone();
        let accepted_limit = options.max_accepted;
        let writer = thread::spawn(move || {
            let mut claim_retry = Instant::now();
            let mut claims_remaining = 0;
            while !run.stop.load(Ordering::Relaxed) {
                // Gate the only transaction writer before it consumes another
                // queued result. The UI polling interval must not add a proof.
                if accepted_limit > 0
                    && run.stats.lock().unwrap()["accepted"].as_u64().unwrap_or(0) >= accepted_limit
                {
                    run.stop.store(true, Ordering::Relaxed);
                    break;
                }
                if run.pause.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }
                if options.auto_claim_after > 0 && Instant::now() >= claim_retry {
                    let pending = match run.history.pending_count() {
                        Ok(count) => count,
                        Err(error) => {
                            run.error(error, false);
                            0
                        }
                    };
                    if claims_remaining == 0 && pending >= options.auto_claim_after {
                        claims_remaining = options.auto_claim_after;
                    }
                    if claims_remaining == 0 {
                        claim_retry = Instant::now() + Duration::from_secs(2);
                    } else {
                        writer_claiming.store(true, Ordering::Relaxed);
                        let mut completed = 0;
                        match run.settle_batch(1, false) {
                            Ok(result) => {
                                let count = result["claims"].as_array().map_or(0, Vec::len);
                                completed = count as u64;
                                claims_remaining = claims_remaining.saturating_sub(completed);
                                if count > 0 {
                                    set_counter(
                                        &mut run.stats.lock().unwrap(),
                                        "autoClaimed",
                                        count as u64,
                                    );
                                    (run.emit)(json!({"event":"auto-claims","data":result}));
                                }
                            }
                            Err(error) => run.error(error, false),
                        }
                        writer_claiming.store(false, Ordering::Relaxed);
                        run.stats.lock().unwrap()["autoClaimRemaining"] = json!(claims_remaining);
                        claim_retry = Instant::now()
                            + Duration::from_secs(if completed > 0 { 5 } else { 30 });
                        if run.stop.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                }
                if writer_resource.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }
                match rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(Task::Proof(candidate)) => {
                        if let Err(error) = run.submit_candidate(candidate) {
                            run.error(error, true)
                        }
                    }
                    Ok(Task::Beacon(epoch)) => {
                        let missing = run.work.read().unwrap().is_none();
                        if missing && let Err(error) = run.publish_beacon(epoch) {
                            run.error(error, false)
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => break,
                }
            }
        });
        let mut workers = Vec::new();
        let duty = (options.cpu_budget / options.workers as f64).min(0.8);
        for _ in 0..options.workers {
            let run = self.clone();
            let tx = tx.clone();
            let hashes = hashes.clone();
            let found = found.clone();
            let resources = resource.clone();
            let claiming = claiming.clone();
            workers.push(thread::spawn(move || {
                let mut raw = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut raw);
                let mut nonce = U256::from_be_bytes(raw);
                while !run.stop.load(Ordering::Relaxed) {
                    if run.pause.load(Ordering::Relaxed)
                        || resources.load(Ordering::Relaxed)
                        || claiming.load(Ordering::Relaxed)
                    {
                        thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                    let work = run.work.read().unwrap().clone();
                    let Some(work) = work else {
                        thread::sleep(Duration::from_millis(50));
                        continue;
                    };
                    let mut preimage = pow::preimage(&work, run.account, nonce);
                    let slice = Instant::now();
                    let deadline = slice + Duration::from_secs_f64(0.01 * duty);
                    let mut count = 0;
                    loop {
                        for _ in 0..32 {
                            preimage[256..].copy_from_slice(&nonce.to_be_bytes::<32>());
                            let digest = abi::keccak(&preimage);
                            count += 1;
                            if pow::valid(digest, work.target) {
                                found.fetch_add(1, Ordering::Relaxed);
                                let _ = tx.try_send(Task::Proof(Candidate {
                                    work: work.clone(),
                                    nonce,
                                    digest,
                                }));
                            }
                            nonce = nonce.wrapping_add(U256::from(1));
                        }
                        if Instant::now() >= deadline || run.stop.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                    hashes.fetch_add(count, Ordering::Relaxed);
                    if let Some(rest) = Duration::from_millis(10).checked_sub(slice.elapsed()) {
                        thread::sleep(rest)
                    }
                }
            }));
        }
        {
            let mut stats = self.stats.lock().unwrap();
            stats["workers"] = json!(options.workers);
            stats["cpuBudgetCores"] = json!(options.cpu_budget);
            stats["autoClaimAfter"] = json!(options.auto_claim_after);
        }
        let mut runtime_error = None;
        loop {
            if self.stop.load(Ordering::Relaxed)
                || (options.seconds > 0
                    && started.elapsed() >= Duration::from_secs(options.seconds))
            {
                break;
            }
            let blocked = resource_blocked();
            resource.store(blocked, Ordering::Relaxed);
            let mut stats = self.stats.lock().unwrap();
            if options.max_accepted > 0
                && stats["accepted"].as_u64().unwrap_or(0) >= options.max_accepted
            {
                break;
            }
            stats["status"] = json!(if self.pause.load(Ordering::Relaxed) {
                "paused"
            } else if claiming.load(Ordering::Relaxed) {
                "claiming"
            } else if blocked {
                "paused-resource"
            } else if self.work.read().unwrap().is_none() {
                "waiting-beacon-or-recovery"
            } else {
                "mining"
            });
            if blocked {
                set_counter(&mut stats, "resourcePauses", 1);
            }
            stats["hashes"] = json!(hashes.load(Ordering::Relaxed));
            stats["validFound"] = json!(found.load(Ordering::Relaxed));
            stats["elapsedSeconds"] = json!(started.elapsed().as_secs_f64());
            stats["hashesPerSecond"] = json!(
                hashes.load(Ordering::Relaxed) as f64 / started.elapsed().as_secs_f64().max(0.001)
            );
            drop(stats);
            if let Err(error) = self.publish() {
                runtime_error = Some(error);
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }
        self.stop.store(true, Ordering::Relaxed);
        for worker in workers {
            let _ = worker.join();
        }
        let _ = poll.join();
        let _ = writer.join();
        if let Some(error) = runtime_error {
            return Err(error);
        }
        let mut data = {
            let mut stats = self.stats.lock().unwrap();
            stats["status"] = json!("stopped");
            stats["elapsedSeconds"] = json!(started.elapsed().as_secs_f64());
            stats["hashes"] = json!(hashes.load(Ordering::Relaxed));
            stats["hashesPerSecond"] = json!(
                hashes.load(Ordering::Relaxed) as f64 / started.elapsed().as_secs_f64().max(0.001)
            );
            stats["processCpuSeconds"] = json!(process_cpu() - cpu_begin);
            stats["runOutcome"] = json!(mining_outcome(&stats));
            stats.clone()
        };
        data["fees"] = self.client.fees.lock().unwrap().snapshot();
        storage::atomic_json(&self.state_dir.join("summary.json"), &data)?;
        self.publish()?;
        (self.emit)(json!({"event":"mine-result","data":data}));
        Ok(data)
    }
}

pub fn resource_blocked() -> bool {
    #[cfg(target_os = "linux")]
    {
        let count = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) }.max(1) as f64;
        let overloaded = fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|s| {
                s.split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<f64>().ok())
            })
            .is_some_and(|v| v > count);
        let low = fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|v| v.starts_with("MemAvailable:"))
                    .and_then(|v| v.split_whitespace().nth(1))
                    .and_then(|v| v.parse::<u64>().ok())
            })
            .is_some_and(|v| v < 1_048_576);
        overloaded || low
    }
    #[cfg(not(target_os = "linux"))]
    false
}
pub fn process_cpu() -> f64 {
    #[cfg(unix)]
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) == 0 {
            return usage.ru_utime.tv_sec as f64
                + usage.ru_utime.tv_usec as f64 / 1e6
                + usage.ru_stime.tv_sec as f64
                + usage.ru_stime.tv_usec as f64 / 1e6;
        }
    }
    0.0
}

#[cfg(test)]
mod beacon_tests {
    use super::*;
    use std::{io::Write, net::TcpListener};

    fn endpoint(body: Value) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).unwrap() > 0);
            let body = body.to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        });
        (url, handle)
    }

    fn official_response() -> Value {
        let signature = "139a803860ff6d842b822b8b37eaeccc4e278dd44440535a976530a1a503e85e11d84b824f18e608a85e627e6b1b08322060cbcf7eca1ea78489252907e8473a";
        json!({"round":1000000,"signature":signature,"randomness":"4da14d73e62d0c909d143e61a2adce866f6be3e7a9d58b1e32a95c87e780569c"})
    }

    #[test]
    fn invalid_primary_beacon_uses_valid_secondary() {
        let valid = official_response();
        for bad in [
            json!({"round":1000020}),
            json!({"round":1000000,"signature":"00","randomness":"00"}),
            json!({"round":1000000,"signature":"zz"}),
            json!({"round":1000000,"signature":"00".repeat(64),"randomness":"00".repeat(32)}),
            Value::Null,
        ] {
            let (primary, a) = endpoint(bad);
            let (secondary, b) = endpoint(valid.clone());
            let result =
                fetch_scheduled_beacon(&[&primary, &secondary], U256::from(1000000), |_| Ok(()));
            assert!(
                result.is_ok(),
                "invalid primary must not suppress a usable secondary: {result:?}"
            );
            let (data, signature) = result.unwrap();
            assert_eq!(data, valid);
            assert_eq!(signature.len(), 64);
            a.join().unwrap();
            b.join().unwrap();
        }
    }

    #[test]
    fn all_invalid_beacons_return_error_without_a_signer() {
        let (a, first) = endpoint(json!({"round":999999}));
        let (b, second) = endpoint(json!({"round":1000000,"signature":"zz"}));
        assert!(fetch_scheduled_beacon(&[&a, &b], U256::from(1000000), |_| Ok(())).is_err());
        first.join().unwrap();
        second.join().unwrap();
    }

    #[test]
    fn well_encoded_forgery_does_not_suppress_authenticated_secondary() {
        let valid = official_response();
        let forged_signature = [vec![0; 31], vec![1], vec![0; 31], vec![2]].concat();
        let bad = json!({"round":1000000,"signature":hex::encode(&forged_signature),"randomness":hex::encode(Sha256::digest(&forged_signature))});
        let (a, first) = endpoint(bad);
        let (b, second) = endpoint(valid.clone());
        let mut checked = 0;
        let (selected, _) = fetch_scheduled_beacon(&[&a, &b], U256::from(1000000), |signature| {
            checked += 1;
            ensure!(
                signature != forged_signature,
                "simulated contract rejection"
            );
            Ok(())
        })
        .unwrap();
        assert_eq!(checked, 2);
        assert_eq!(selected, valid);
        first.join().unwrap();
        second.join().unwrap();
    }

    #[test]
    #[ignore = "requires an explicit read-only network beacon fixture"]
    fn authenticated_beacon_failover_readonly() {
        let path = std::env::var("ZORU_BEACON_READ_TEST").expect("read-only fixture path");
        let fixture: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let rpc =
            crate::rpc::Rpc::new(fixture["read"].as_str().unwrap(), Duration::from_millis(80))
                .unwrap();
        assert_eq!(
            abi::quantity(
                rpc.request("eth_chainId", json!([]))
                    .unwrap()
                    .as_str()
                    .unwrap()
            )
            .unwrap(),
            U256::from(crate::protocol::CHAIN)
        );
        let epoch = fixture["epoch"].as_u64().unwrap();
        let round = U256::from(fixture["round"].as_u64().unwrap());
        let beacon: Address = fixture["beacon"].as_str().unwrap().parse().unwrap();
        let forged_signature = [vec![0; 31], vec![1], vec![0; 31], vec![2]].concat();
        let invalid = json!({"round":fixture["round"],"signature":hex::encode(&forged_signature),"randomness":hex::encode(Sha256::digest(&forged_signature))});
        let valid = json!({"round":fixture["round"],"signature":fixture["signature"],"randomness":fixture["randomness"]});
        let (a, first) = endpoint(invalid);
        let (b, second) = endpoint(valid.clone());
        let before = rpc.request("eth_call", json!([{"to":beacon,"data":abi::call("verifiedSeed(uint48)",&[abi::uint(U256::from(epoch))])},"latest"])).unwrap();
        let (selected, _) = fetch_scheduled_beacon(&[&a, &b], round, |signature| {
            preflight_beacon(&rpc, beacon, epoch, signature)
        })
        .unwrap();
        assert_eq!(selected, valid);
        let after = rpc.request("eth_call", json!([{"to":beacon,"data":abi::call("verifiedSeed(uint48)",&[abi::uint(U256::from(epoch))])},"latest"])).unwrap();
        assert_eq!(
            before, after,
            "read-only preflight must not persist a beacon"
        );
        first.join().unwrap();
        second.join().unwrap();
    }
}

#[cfg(test)]
mod mining_outcome_tests {
    use super::{is_stale_candidate, mining_outcome, receipt_event, topic};
    use alloy_primitives::Address;
    use serde_json::json;

    #[test]
    fn receipt_events_require_one_matching_contract_and_signature() {
        let ledger = Address::repeat_byte(0x12);
        let signature = "RewardClaimed(uint48,bytes32,address,uint256)";
        let event = json!({"address":ledger,"topics":[topic(signature)],"data":"0x"});
        assert!(receipt_event(&json!({"logs":[event.clone()]}), ledger, signature).is_ok());
        assert!(
            receipt_event(
                &json!({"logs":[event.clone()]}),
                Address::repeat_byte(0x34),
                signature
            )
            .is_err()
        );
        assert!(
            receipt_event(
                &json!({"logs":[event.clone()]}),
                ledger,
                "ProofAccepted(uint48,bytes32,address,uint256,uint256)"
            )
            .is_err()
        );
        assert!(receipt_event(&json!({"logs":[event.clone(),event]}), ledger, signature).is_err());
        assert!(receipt_event(&json!({"logs":[]}), ledger, signature).is_err());
    }

    #[test]
    fn stale_before_broadcast_is_warning_not_rejected_proof() {
        assert!(is_stale_candidate(
            "STALE_WORK: context changed before submission"
        ));
        assert!(is_stale_candidate(
            "STALE_WORK: epoch or generation changed"
        ));
        assert!(!is_stale_candidate("eth_call: RPC error 3 0xd5b25b63"));
        assert!(!is_stale_candidate("BAD_STALE_WORK_FORGERY"));
    }

    #[test]
    fn public_outcome_distinguishes_receipts_errors_and_timeout() {
        assert_eq!(
            mining_outcome(
                &json!({"accepted":1,"rejected":0,"errors":[],"staleBeforeBroadcast":6})
            ),
            "accepted"
        );
        assert_eq!(
            mining_outcome(&json!({"accepted":1,"rejected":1,"errors":[{"message":"rpc failed"}]})),
            "accepted-with-errors"
        );
        assert_eq!(
            mining_outcome(&json!({"accepted":0,"rejected":0,"errors":[]})),
            "no-proof"
        );
        assert_eq!(
            mining_outcome(
                &json!({"accepted":0,"rejected":0,"errors":[{"message":"drand unavailable"}]})
            ),
            "failed"
        );
        assert_eq!(
            mining_outcome(&json!({"accepted":0,"rejected":1,"errors":[]})),
            "failed"
        );
    }
}
