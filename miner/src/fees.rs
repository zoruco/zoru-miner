use crate::abi;
use alloy_primitives::{B256, U256};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Fees {
    limit: Option<U256>,
    spent: U256,
    pending: BTreeMap<B256, U256>,
    last_maximum: Option<U256>,
}

impl Fees {
    pub fn set_limit(&mut self, limit: Option<U256>) -> Result<()> {
        ensure!(
            limit != Some(U256::ZERO),
            "Choose a positive session fee limit"
        );
        ensure!(
            self.pending.is_empty() && self.spent == U256::ZERO,
            "The session fee limit is already active"
        );
        self.limit = limit;
        Ok(())
    }

    pub fn reserve(&mut self, hash: B256, maximum: U256) -> Result<()> {
        if let Some(previous) = self.pending.get(&hash) {
            ensure!(*previous == maximum, "Transaction fee reservation changed");
            return Ok(());
        }
        self.check(maximum)?;
        self.pending.insert(hash, maximum);
        self.last_maximum = Some(maximum);
        Ok(())
    }

    pub fn check(&self, maximum: U256) -> Result<()> {
        let committed = self
            .spent
            .checked_add(self.reserved())
            .context("Fee accounting overflow")?;
        if let Some(limit) = self.limit {
            ensure!(
                maximum <= limit.saturating_sub(committed),
                "FEE_BUDGET_EXHAUSTED: the next transaction exceeds the remaining session allowance"
            );
        }
        Ok(())
    }

    pub fn confirm(&mut self, hash: B256, receipt: &Value) -> Result<()> {
        if let Some(maximum) = self.pending.get(&hash).copied() {
            let actual = actual_fee(receipt)?;
            ensure!(
                actual <= maximum,
                "Receipt fee exceeds its signed reservation"
            );
            self.spent = self
                .spent
                .checked_add(actual)
                .context("Fee accounting overflow")?;
            self.pending.remove(&hash);
        }
        Ok(())
    }

    pub fn release(&mut self, hash: B256) {
        self.pending.remove(&hash);
    }

    fn reserved(&self) -> U256 {
        self.pending
            .values()
            .copied()
            .fold(U256::ZERO, U256::saturating_add)
    }

    pub fn snapshot(&self) -> Value {
        let reserved = self.reserved();
        json!({
            "limitWei":self.limit.map(|v|v.to_string()),
            "spentWei":self.spent.to_string(),
            "reservedWei":reserved.to_string(),
            "remainingWei":self.limit.map(|v|v.saturating_sub(self.spent).saturating_sub(reserved).to_string()),
            "lastMaximumFeeWei":self.last_maximum.map(|v|v.to_string())
        })
    }
}

pub fn actual_fee(receipt: &Value) -> Result<U256> {
    let gas = abi::quantity(
        receipt["gasUsed"]
            .as_str()
            .context("Receipt gas usage missing")?,
    )?;
    let price = abi::quantity(
        receipt["effectiveGasPrice"]
            .as_str()
            .context("Receipt gas price missing")?,
    )?;
    gas.checked_mul(price).context("Receipt fee overflow")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_and_recovered_receipts_charge_a_signed_intent_once() {
        let hash = B256::repeat_byte(1);
        let mut fees = Fees::default();
        fees.set_limit(Some(U256::from(100))).unwrap();
        fees.reserve(hash, U256::from(70)).unwrap();
        fees.reserve(hash, U256::from(70)).unwrap();
        assert_eq!(fees.snapshot()["reservedWei"], "70");
        let receipt = json!({"gasUsed":"0x3","effectiveGasPrice":"0x14"});
        fees.confirm(hash, &receipt).unwrap();
        fees.confirm(hash, &receipt).unwrap();
        assert_eq!(fees.snapshot()["spentWei"], "60");
        assert_eq!(fees.snapshot()["remainingWei"], "40");
        assert!(fees.reserve(B256::repeat_byte(2), U256::from(41)).is_err());
        fees.reserve(B256::repeat_byte(2), U256::from(40)).unwrap();
        fees.release(B256::repeat_byte(2));
        assert_eq!(fees.snapshot()["remainingWei"], "40");
    }

    #[test]
    fn an_invalid_receipt_preserves_the_original_reservation() {
        let hash = B256::repeat_byte(1);
        let mut fees = Fees::default();
        fees.reserve(hash, U256::from(10)).unwrap();
        assert!(
            fees.confirm(hash, &json!({"gasUsed":"0xb","effectiveGasPrice":"0x1"}))
                .is_err()
        );
        assert_eq!(fees.snapshot()["reservedWei"], "10");
        assert_eq!(fees.snapshot()["spentWei"], "0");
    }
}
