use alloy_primitives::{Address, B256, U256};
use anyhow::{Result, ensure};
use tiny_keccak::{Hasher, Keccak};

pub fn keccak(bytes: &[u8]) -> B256 {
    let mut value = [0u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(bytes);
    hasher.finalize(&mut value);
    value.into()
}
pub fn uint(value: U256) -> B256 {
    value.to_be_bytes::<32>().into()
}
pub fn address(value: Address) -> B256 {
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(value.as_slice());
    word.into()
}
pub fn call(signature: &str, words: &[B256]) -> String {
    let mut data = keccak(signature.as_bytes()).as_slice()[..4].to_vec();
    for word in words {
        data.extend_from_slice(word.as_slice());
    }
    format!("0x{}", hex::encode(data))
}
pub fn bytes(value: &str) -> Result<Vec<u8>> {
    ensure!(value.starts_with("0x"), "RPC data must have a hex prefix");
    Ok(hex::decode(&value[2..])?)
}
pub fn words(value: &str) -> Result<Vec<B256>> {
    let data = bytes(value)?;
    ensure!(data.len() % 32 == 0, "Malformed ABI response");
    Ok(data.chunks_exact(32).map(B256::from_slice).collect())
}
pub fn number(value: B256) -> U256 {
    U256::from_be_slice(value.as_slice())
}
pub fn scalar(value: &str) -> Result<U256> {
    let data = words(value)?;
    ensure!(data.len() == 1, "Expected one ABI word");
    Ok(number(data[0]))
}
pub fn boolean(value: &str) -> Result<bool> {
    let n = scalar(value)?;
    ensure!(n <= U256::from(1), "Non-canonical ABI boolean");
    Ok(n == U256::from(1))
}
pub fn abi_address(value: &str) -> Result<Address> {
    let data = words(value)?;
    ensure!(
        data.len() == 1 && data[0].as_slice()[..12] == [0u8; 12],
        "Malformed ABI address"
    );
    Ok(Address::from_slice(&data[0].as_slice()[12..]))
}
pub fn quantity(value: &str) -> Result<U256> {
    Ok(if let Some(hex) = value.strip_prefix("0x") {
        U256::from_str_radix(if hex.is_empty() { "0" } else { hex }, 16)?
    } else {
        U256::from_str_radix(value, 10)?
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ethereum_keccak_not_sha3() {
        assert_eq!(
            format!("{:#x}", keccak(b"")),
            "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
    }
    #[test]
    fn reject_noncanonical_bool_and_address() {
        assert!(boolean(&format!("{:#x}", uint(U256::from(2)))).is_err());
        assert!(abi_address(&format!("{:#x}", B256::repeat_byte(1))).is_err());
    }
}
