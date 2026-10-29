//! Signed release metadata and package checks. No wallet or mining state.
use ring::signature::{ED25519, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;

pub const MAX_METADATA: u64 = 65_536;
pub const MAX_PACKAGE: u64 = 536_870_912;
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Artifact {
    pub target: String,
    pub file_name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    pub schema_version: u32,
    pub product: String,
    pub channel: String,
    pub version: String,
    pub published_at: String,
    pub expires_at: String,
    pub notes: String,
    pub artifacts: Vec<Artifact>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    signed: String,
    signature: String,
}

pub fn version(s: &str) -> Result<[u32; 3]> {
    let parts: Vec<_> = s.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid release version".into());
    }
    let mut out = [0; 3];
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty()
            || (part.len() > 1 && part.starts_with('0'))
            || !part.bytes().all(|b| b.is_ascii_digit())
        {
            return Err("Invalid release version".into());
        }
        out[i] = part.parse()?;
    }
    Ok(out)
}
pub fn target() -> &'static str {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-amd64"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x64"
    } else {
        "unsupported"
    }
}
fn expected_file(version: &str, target: &str) -> Result<String> {
    Ok(match target {
        "linux-amd64" => format!("ZORU-Miner-{version}-Linux-amd64.deb"),
        "windows-x64" => format!("ZORU-Miner-{version}-Windows-x64.exe"),
        _ => return Err("Unsupported release target".into()),
    })
}

pub fn verify_metadata(bytes: &[u8], now: i64) -> Result<Release> {
    let key = hex::decode(include_str!("../../config/update-public-key.hex").trim())?;
    verify_with_key(bytes, &key, now)
}
fn verify_with_key(bytes: &[u8], key: &[u8], now: i64) -> Result<Release> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_METADATA {
        return Err("Update information exceeds limit".into());
    }
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    let payload = hex::decode(envelope.signed)?;
    let signature = hex::decode(envelope.signature)?;
    UnparsedPublicKey::new(&ED25519, key)
        .verify(&payload, &signature)
        .map_err(|_| "Update signature is invalid")?;
    let release: Release = serde_json::from_slice(&payload)?;
    version(&release.version)?;
    if release.schema_version != 1
        || release.product != "zoru-miner"
        || release.channel != "stable"
        || release.notes.len() > 8192
        || release.artifacts.is_empty()
        || release.artifacts.len() > 8
    {
        return Err("Update information is invalid".into());
    }
    let published = chrono::DateTime::parse_from_rfc3339(&release.published_at)?.timestamp();
    let expires = chrono::DateTime::parse_from_rfc3339(&release.expires_at)?.timestamp();
    if published > now + 300
        || expires <= now
        || expires <= published
        || expires - published > 90 * 86400
    {
        return Err("Update information has expired or has an invalid date".into());
    }
    let mut targets = std::collections::BTreeSet::new();
    for a in &release.artifacts {
        let url = reqwest::Url::parse(&a.url)?;
        let expected_url = format!(
            "https://github.com/zoruco/zoru-miner/releases/download/v{}/{}",
            release.version, a.file_name
        );
        if !targets.insert(&a.target)
            || a.file_name != expected_file(&release.version, &a.target)?
            || a.size == 0
            || a.size > MAX_PACKAGE
            || a.sha256.len() != 64
            || !a.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.query().is_some()
            || a.url != expected_url
        {
            return Err("Update package information is invalid".into());
        }
    }
    Ok(release)
}
pub fn artifact(release: &Release) -> Result<&Artifact> {
    release
        .artifacts
        .iter()
        .find(|a| a.target == target())
        .ok_or_else(|| "No update is available for this operating system".into())
}
pub fn read_metadata(file: impl Read) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take(MAX_METADATA + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_METADATA {
        return Err("Update information exceeds limit".into());
    }
    Ok(bytes)
}
pub fn check_package(mut file: impl Read, expected: &Artifact) -> Result<()> {
    let mut count = 0u64;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65_536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count = count.checked_add(n as u64).ok_or("Update size overflow")?;
        if count > expected.size {
            return Err("Update package size differs".into());
        }
        hash.update(&buffer[..n]);
    }
    if count != expected.size
        || hex::encode(hash.finalize()) != expected.sha256.to_ascii_lowercase()
    {
        return Err("Update package checksum differs".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    fn fixture() -> (Ed25519KeyPair, Release, i64) {
        // RFC 8032 test-only seed. It is never a release signing key.
        let key = Ed25519KeyPair::from_seed_unchecked(
            &hex::decode("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")
                .unwrap(),
        )
        .unwrap();
        let now = 1793000000;
        let release = Release { schema_version:1, product:"zoru-miner".into(), channel:"stable".into(), version:"9.0.0".into(),
            published_at:chrono::DateTime::from_timestamp(now-10,0).unwrap().to_rfc3339(), expires_at:chrono::DateTime::from_timestamp(now+600,0).unwrap().to_rfc3339(), notes:"Update test".into(),
            artifacts:vec![Artifact { target:"linux-amd64".into(),file_name:"ZORU-Miner-9.0.0-Linux-amd64.deb".into(),url:"https://github.com/zoruco/zoru-miner/releases/download/v9.0.0/ZORU-Miner-9.0.0-Linux-amd64.deb".into(),size:3,sha256:hex::encode(Sha256::digest(b"abc")) }] };
        (key, release, now)
    }
    fn signed(key: &Ed25519KeyPair, release: &Release) -> Vec<u8> {
        let bytes = serde_json::to_vec(release).unwrap();
        serde_json::to_vec(&serde_json::json!({"signed":hex::encode(&bytes),"signature":hex::encode(key.sign(&bytes).as_ref())})).unwrap()
    }
    #[test]
    fn signed_release_accepts_exact_bytes_and_rejects_tampering_or_wrong_key() {
        let (key, r, now) = fixture();
        let bytes = signed(&key, &r);
        assert_eq!(
            verify_with_key(&bytes, key.public_key().as_ref(), now)
                .unwrap()
                .version,
            "9.0.0"
        );
        assert!(verify_with_key(&bytes, &[7; 32], now).is_err());
        let mut envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let mut payload = hex::decode(envelope["signed"].as_str().unwrap()).unwrap();
        payload[10] ^= 1;
        envelope["signed"] = serde_json::json!(hex::encode(payload));
        assert!(
            verify_with_key(
                &serde_json::to_vec(&envelope).unwrap(),
                key.public_key().as_ref(),
                now
            )
            .is_err()
        );
    }
    #[test]
    fn signed_metadata_rejects_expiry_wrong_product_paths_duplicates_and_insecure_urls() {
        let (key, r, now) = fixture();
        for change in 0..8 {
            let mut bad = r.clone();
            match change {
                0 => bad.expires_at = bad.published_at.clone(),
                1 => bad.product = "other".into(),
                2 => bad.artifacts[0].file_name = "../../update.deb".into(),
                3 => bad.artifacts.push(bad.artifacts[0].clone()),
                4 => bad.artifacts[0].url = "http://example.com/update.deb".into(),
                5 => bad.artifacts[0].size = MAX_PACKAGE + 1,
                6 => bad.artifacts[0].url = bad.artifacts[0].url.replace("/zoruco/", "/other/"),
                _ => {
                    bad.published_at = chrono::DateTime::from_timestamp(now + 301, 0)
                        .unwrap()
                        .to_rfc3339()
                }
            }
            assert!(verify_with_key(&signed(&key, &bad), key.public_key().as_ref(), now).is_err());
        }
        assert!(verify_with_key(&signed(&key, &r), key.public_key().as_ref(), now + 1000).is_err());
    }
    #[test]
    fn checksum_requires_complete_exact_package() {
        let (_, r, _) = fixture();
        let a = &r.artifacts[0];
        check_package(&b"abc"[..], a).unwrap();
        for bytes in [&b"ab"[..], &b"abcd"[..], &b"abd"[..]] {
            assert!(check_package(bytes, a).is_err());
        }
    }
    #[test]
    fn stable_versions_have_an_unambiguous_order() {
        assert!(version("1.10.0").unwrap() > version("1.9.9").unwrap());
        for v in ["1.0", "1.0.0-beta", "01.0.0", "1.0.0/../../x", "1.0.-1"] {
            assert!(version(v).is_err());
        }
    }
    #[test]
    fn metadata_reads_are_bounded_before_parsing() {
        assert!(read_metadata(std::io::repeat(b'x').take(MAX_METADATA + 1)).is_err());
        assert_eq!(read_metadata(&b"{}"[..]).unwrap(), b"{}");
    }
}
