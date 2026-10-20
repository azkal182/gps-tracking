use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use rand::{RngCore, rngs::OsRng};
use sha1::Sha1;
use subtle::ConstantTimeEq;

type HmacSha1 = Hmac<Sha1>;

pub(crate) fn new_secret() -> Vec<u8> {
    let mut value = vec![0; 20];
    OsRng.fill_bytes(&mut value);
    value
}

pub(crate) fn base32_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in input {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

#[cfg(test)]
pub(crate) fn base32_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in input.bytes() {
        let value = match byte.to_ascii_uppercase() {
            b'A'..=b'Z' => byte.to_ascii_uppercase() - b'A',
            b'2'..=b'7' => byte - b'2' + 26,
            b'=' | b' ' | b'-' => continue,
            _ => return None,
        };
        buffer = (buffer << 5) | u32::from(value);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    (!out.is_empty()).then_some(out)
}

pub(crate) fn totp(secret: &[u8], step: u64) -> String {
    let mut mac = <HmacSha1 as Mac>::new_from_slice(secret).expect("HMAC accepts any key size");
    mac.update(&step.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest[19] & 0x0f);
    let binary = (u32::from(digest[offset] & 0x7f) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);
    format!("{:06}", binary % 1_000_000)
}

pub(crate) fn matching_step(
    secret: &[u8],
    code: &str,
    current: u64,
    last: Option<i64>,
) -> Option<u64> {
    if code.len() != 6 || !code.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    (-1i64..=1).find_map(|offset| {
        let step = current as i64 + offset;
        if step < 0 || last.is_some_and(|accepted| step <= accepted) {
            return None;
        }
        let step = step as u64;
        bool::from(totp(secret, step).as_bytes().ct_eq(code.as_bytes())).then_some(step)
    })
}

pub(crate) fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher.encrypt(Nonce::from_slice(&nonce), plaintext).ok()?;
    Some((ciphertext, nonce.to_vec()))
}

pub(crate) fn decrypt(key: &[u8; 32], ciphertext: &[u8], nonce: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    if nonce.len() != 12 {
        return None;
    }
    cipher.decrypt(Nonce::from_slice(nonce), ciphertext).ok()
}

pub(crate) fn recovery_code() -> String {
    let mut value = [0u8; 16];
    OsRng.fill_bytes(&mut value);
    URL_SAFE_NO_PAD.encode(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_6238_sha1_vector_and_step_replay_guard() {
        let secret = b"12345678901234567890";
        assert_eq!(totp(secret, 1), "287082");
        assert_eq!(matching_step(secret, "287082", 1, None), Some(1));
        assert_eq!(matching_step(secret, "287082", 1, Some(1)), None);
        assert_eq!(matching_step(secret, "12ab56", 1, None), None);
    }

    #[test]
    fn secret_encryption_authenticates_ciphertext_and_base32_roundtrips() {
        let secret = new_secret();
        assert_eq!(base32_decode(&base32_encode(&secret)).unwrap(), secret);
        let key = [7u8; 32];
        let (mut ciphertext, nonce) = encrypt(&key, &secret).unwrap();
        assert_eq!(decrypt(&key, &ciphertext, &nonce).unwrap(), secret);
        ciphertext[0] ^= 1;
        assert!(decrypt(&key, &ciphertext, &nonce).is_none());
    }
}
