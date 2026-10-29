use alloy_primitives::{B256, U256};
use anyhow::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::Path, sync::Mutex};

#[derive(Clone, Serialize, Deserialize)]
pub struct Proof {
    pub digest: B256,
    pub epoch: u64,
    pub generation: U256,
    pub hash: B256,
    pub block: u64,
    pub claimed: bool,
    pub reward: Option<U256>,
    pub canonical: bool,
    pub claim_block: Option<u64>,
    /// 1-based proof position from the accepted-proof event.
    pub index_plus_one: U256,
}
pub struct History {
    db: Mutex<Connection>,
}
impl History {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL;
          CREATE TABLE IF NOT EXISTS proofs(digest TEXT PRIMARY KEY,status INTEGER NOT NULL,data TEXT NOT NULL);
          CREATE INDEX IF NOT EXISTS unclaimed ON proofs(status);
          CREATE INDEX IF NOT EXISTS proof_height ON proofs(json_extract(data,'$.block'));
          CREATE INDEX IF NOT EXISTS claim_height ON proofs(json_extract(data,'$.claim_block'));
          CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);")?;
        Ok(Self { db: Mutex::new(db) })
    }
    pub fn put(&self, proof: &Proof) -> Result<()> {
        self.db.lock().unwrap().execute("INSERT INTO proofs VALUES(?1,?2,?3) ON CONFLICT(digest) DO UPDATE SET status=excluded.status,data=excluded.data",
            params![format!("{:#x}",proof.digest),if !proof.canonical{2}else if proof.claimed{1}else{0},serde_json::to_string(proof)?])?;
        Ok(())
    }
    pub fn get(&self, digest: B256) -> Result<Option<Proof>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare("SELECT data FROM proofs WHERE digest=?1")?;
        let mut rows = stmt.query(params![format!("{digest:#x}")])?;
        Ok(if let Some(row) = rows.next()? {
            Some(serde_json::from_str(&row.get::<_, String>(0)?)?)
        } else {
            None
        })
    }
    pub fn pending(&self, limit: usize) -> Result<Vec<Proof>> {
        let db = self.db.lock().unwrap();
        let mut statement =
            db.prepare("SELECT data FROM proofs WHERE status=0 ORDER BY rowid LIMIT ?1")?;
        let values = statement
            .query_map([limit as i64], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(values
            .into_iter()
            .map(|s| serde_json::from_str(&s))
            .collect::<serde_json::Result<_>>()?)
    }
    pub fn recent(&self, height: u64, after: &str) -> Result<Vec<Proof>> {
        let db = self.db.lock().unwrap();
        let mut statement=db.prepare("SELECT data FROM proofs WHERE digest>?1 AND (json_extract(data,'$.block')>=?2 OR json_extract(data,'$.claim_block')>=?2) ORDER BY digest LIMIT 256")?;
        let values = statement
            .query_map(params![after, height], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(values
            .into_iter()
            .map(|s| serde_json::from_str(&s))
            .collect::<serde_json::Result<_>>()?)
    }
    pub fn paid(&self) -> Result<U256> {
        let db = self.db.lock().unwrap();
        let mut statement = db.prepare("SELECT data FROM proofs WHERE status=1")?;
        let mut result = U256::ZERO;
        for row in statement.query_map([], |r| r.get::<_, String>(0))? {
            let proof: Proof = serde_json::from_str(&row?)?;
            if let Some(value) = proof.reward {
                result = result
                    .checked_add(value)
                    .ok_or_else(|| anyhow::anyhow!("Reward total overflow"))?;
            }
        }
        Ok(result)
    }
    pub fn cursor(&self) -> Result<Option<u64>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .db
            .lock()
            .unwrap()
            .query_row("SELECT value FROM metadata WHERE key='cursor'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .map(|v| v.parse())
            .transpose()?)
    }
    pub fn set_cursor(&self, value: u64) -> Result<()> {
        self.db.lock().unwrap().execute("INSERT INTO metadata VALUES('cursor',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[value.to_string()])?;
        Ok(())
    }
    pub fn raw_count(&self) -> Result<u64> {
        Ok(self
            .db
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM proofs", [], |r| r.get(0))?)
    }
}

pub fn proof_record(log: &Value) -> Result<Proof> {
    use crate::abi;
    let digest = log["topics"][2]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing proof digest"))?
        .parse()?;
    let epoch = abi::quantity(log["topics"][1].as_str().unwrap_or(""))?.try_into()?;
    let data = abi::words(log["data"].as_str().unwrap_or(""))?;
    anyhow::ensure!(data.len() == 2, "Invalid proof event");
    Ok(Proof {
        digest,
        epoch,
        generation: abi::number(data[1]),
        hash: log["transactionHash"].as_str().unwrap_or("").parse()?,
        block: abi::quantity(log["blockNumber"].as_str().unwrap_or(""))?.try_into()?,
        claimed: false,
        reward: None,
        canonical: true,
        claim_block: None,
        index_plus_one: abi::number(data[0]) + U256::from(1),
    })
}
