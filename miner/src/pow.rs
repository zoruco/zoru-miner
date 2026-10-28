use crate::abi::{address, keccak, uint};
use alloy_primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Work {
    pub chain_id: u64,
    pub ledger: Address,
    pub token: Address,
    pub generation: U256,
    pub epoch: u64,
    pub seed: B256,
    pub target: U256,
    pub end_time: u64,
}

pub fn preimage(work: &Work, beneficiary: Address, nonce: U256) -> [u8; 288] {
    let words = [
        keccak(b"ZORU-FLOWPOW-PROOF-1"),
        uint(U256::from(work.chain_id)),
        address(work.ledger),
        address(work.token),
        uint(work.generation),
        uint(U256::from(work.epoch)),
        work.seed,
        address(beneficiary),
        uint(nonce),
    ];
    let mut data = [0u8; 288];
    for (index, word) in words.iter().enumerate() {
        data[index * 32..(index + 1) * 32].copy_from_slice(word.as_slice());
    }
    data
}
pub fn digest(work: &Work, beneficiary: Address, nonce: U256) -> B256 {
    keccak(&preimage(work, beneficiary, nonce))
}
pub fn valid(digest: B256, target: U256) -> bool {
    U256::from_be_slice(digest.as_slice()) < target
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_target_and_full_nonce() {
        assert!(!valid(
            B256::repeat_byte(1),
            U256::from_be_slice(B256::repeat_byte(1).as_slice())
        ));
        let value = U256::MAX;
        assert_eq!(uint(value).as_slice(), &[255u8; 32]);
    }
    #[test]
    fn every_domain_component_changes_work() {
        let work = Work {
            chain_id: 6736,
            ledger: Address::repeat_byte(1),
            token: Address::repeat_byte(2),
            generation: U256::from(1) << 128,
            epoch: (1u64 << 48) - 1,
            seed: B256::repeat_byte(3),
            target: U256::MAX >> 16,
            end_time: 0,
        };
        let who = Address::repeat_byte(4);
        let base = digest(&work, who, U256::MAX);
        let mut other = work.clone();
        other.generation += U256::from(1);
        assert_ne!(digest(&other, who, U256::MAX), base);
        other = work.clone();
        other.chain_id += 1;
        assert_ne!(digest(&other, who, U256::MAX), base);
        assert_ne!(digest(&work, Address::repeat_byte(5), U256::MAX), base);
        assert_ne!(digest(&work, who, U256::MAX - U256::from(1)), base);
        assert_eq!(&preimage(&work, who, U256::MAX)[256..], &[255u8; 32]);
    }
}
