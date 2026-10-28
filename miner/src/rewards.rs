use alloy_primitives::U256;
use anyhow::{Result, ensure};

/// Mirrors Ledger.claim() Math.mulDiv partitioning for an individual accepted proof.
/// With the ZORU emission cap the products fit U256.
pub fn proof_reward(budget: U256, proof_count: U256, index_plus_one: U256) -> Result<U256> {
    ensure!(proof_count > U256::ZERO, "Epoch proof count cannot be zero");
    ensure!(
        index_plus_one > U256::ZERO && index_plus_one <= proof_count,
        "Invalid accepted proof index"
    );
    let current = budget
        .checked_mul(index_plus_one)
        .ok_or_else(|| anyhow::anyhow!("Reward multiplication overflow"))?
        / proof_count;
    let previous = budget
        .checked_mul(index_plus_one - U256::from(1))
        .ok_or_else(|| anyhow::anyhow!("Reward multiplication overflow"))?
        / proof_count;
    Ok(current - previous)
}

pub fn split_global_rewards(allocated: U256, funded: U256, paid: U256) -> Result<(U256, U256)> {
    ensure!(
        allocated >= funded && funded >= paid,
        "Invalid Ledger reward accounting"
    );
    Ok((allocated - funded, funded - paid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_exactly_distributes_epoch_budget() {
        let budget = U256::from(500_000_000u64);
        for n in [1u64, 2, 3, 6, 60, 401, 10_000] {
            let mut total = U256::ZERO;
            for i in 1..=n {
                total += proof_reward(budget, U256::from(n), U256::from(i)).unwrap();
            }
            assert_eq!(total, budget);
        }
    }

    #[test]
    fn rejects_invalid_state_and_proof_indices() {
        assert!(proof_reward(U256::from(5), U256::ZERO, U256::from(1)).is_err());
        assert!(proof_reward(U256::from(5), U256::from(1), U256::ZERO).is_err());
        assert!(proof_reward(U256::from(5), U256::from(1), U256::from(2)).is_err());
        assert!(split_global_rewards(U256::from(5), U256::from(6), U256::ZERO).is_err());
        assert!(split_global_rewards(U256::from(6), U256::from(5), U256::from(6)).is_err());
    }

    #[test]
    fn global_partition_matches_ledger_accounting() {
        let (pending, unclaimed) = split_global_rewards(
            U256::from(49_500_000_000u64),
            U256::from(500_000_000u64),
            U256::from(500_000_000u64),
        )
        .unwrap();
        assert_eq!(pending, U256::from(49_000_000_000u64));
        assert_eq!(unclaimed, U256::ZERO);
    }
}
