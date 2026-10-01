//! A token signed here, at each request, with the account's private key —
//! the signed-assertion scheme behind service accounts and developer APIs.
//! The header carries `alg` and `typ` and whatever the definition adds (a
//! `kid`); the claims are the definition's, rendered against the account,
//! plus `iat` and `exp` from the clock. The key is PEM: PKCS#8 for both
//! algorithms, PKCS#1 for RSA as well; anything else is refused by name.

use super::{bearer::write_bearer, mismatch, need, Applied, AuthSigner, SignContext};
use crate::creds::{Credential, Field, Stored};
use crate::error::ConnectorError;
use crate::http::Request;
use crate::spec::JwtAlg;
use crate::template::{self, Encode, Values};
use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{
    EcdsaKeyPair, RsaKeyPair, ECDSA_P256_SHA256_FIXED_SIGNING, RSA_PKCS1_SHA256,
};
use base64::Engine as _;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// How far behind the clock `iat` is written: a platform whose clock runs a
/// little ahead of this machine's would otherwise refuse a token minted just
/// now as one from the future.
pub const IAT_LEEWAY_SECS: u64 = 30;

pub struct JwtSigner<'a> {
    pub alg: JwtAlg,
    pub claims: &'a BTreeMap<String, String>,
    pub header: &'a BTreeMap<String, String>,
    pub ttl_secs: u64,
}

impl AuthSigner for JwtSigner<'_> {
    fn credential_for(&self, stored: &Stored) -> Result<Credential, ConnectorError> {
        Ok(Credential::Jwt(need(stored, Field::PrivateKey)?))
    }

    fn apply(
        &self,
        credential: &Credential,
        req: &mut Request,
        cx: &SignContext<'_>,
    ) -> Result<Applied, ConnectorError> {
        let Credential::Jwt(key) = credential else {
            return Err(mismatch("jwt"));
        };
        let token = self.mint(key.expose(), cx)?;
        let mut applied = write_bearer(req, &crate::creds::Secret::new(token));
        applied.secrets.push(key.expose().to_string());
        Ok(applied)
    }
}

impl JwtSigner<'_> {
    /// The compact serialisation: `base64url(header).base64url(claims).base64url(signature)`.
    fn mint(&self, pem: &str, cx: &SignContext<'_>) -> Result<String, ConnectorError> {
        let none = BTreeMap::new();
        let values = Values {
            account: cx.account,
            params: &none,
        };
        let mut header = Map::new();
        header.insert("alg".into(), Value::String(self.alg.as_str().into()));
        header.insert("typ".into(), Value::String("JWT".into()));
        for (k, tmpl) in self.header {
            header.insert(
                k.clone(),
                Value::String(template::render(tmpl, &values, Encode::Text)?),
            );
        }
        let mut claims = Map::new();
        for (k, tmpl) in self.claims {
            claims.insert(
                k.clone(),
                Value::String(template::render(tmpl, &values, Encode::Text)?),
            );
        }
        claims.insert(
            "iat".into(),
            Value::from(cx.now.saturating_sub(IAT_LEEWAY_SECS)),
        );
        claims.insert(
            "exp".into(),
            Value::from(cx.now.saturating_add(self.ttl_secs)),
        );
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let signing_input = format!(
            "{}.{}",
            b64.encode(Value::Object(header).to_string()),
            b64.encode(Value::Object(claims).to_string())
        );
        let signature = sign(self.alg, &parse_pem(pem)?, signing_input.as_bytes())?;
        Ok(format!("{signing_input}.{}", b64.encode(signature)))
    }
}

/// A private key as its PEM labels it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pem {
    /// `BEGIN PRIVATE KEY` — PKCS#8, either algorithm.
    Pkcs8(Vec<u8>),
    /// `BEGIN RSA PRIVATE KEY` — PKCS#1, RSA only.
    Pkcs1Rsa(Vec<u8>),
}

fn bad_key(why: &str) -> ConnectorError {
    ConnectorError::NotAuthenticated(format!("private_key: {why}"))
}

/// Read a PEM private key into its DER bytes, or say what is wrong with it —
/// never quoting it.
pub fn parse_pem(text: &str) -> Result<Pem, ConnectorError> {
    let text = text.trim();
    let begin = text
        .strip_prefix("-----BEGIN ")
        .ok_or_else(|| bad_key("not PEM; expected a -----BEGIN PRIVATE KEY----- block"))?;
    let (label, rest) = begin
        .split_once("-----")
        .ok_or_else(|| bad_key("the BEGIN line is not closed by -----"))?;
    let end_marker = format!("-----END {label}-----");
    let body = rest
        .strip_suffix(&end_marker)
        .ok_or_else(|| bad_key(&format!("the block does not end with {end_marker}")))?;
    let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    let der = base64::engine::general_purpose::STANDARD
        .decode(compact.as_bytes())
        .map_err(|_| bad_key("the body is not base64"))?;
    match label {
        "PRIVATE KEY" => Ok(Pem::Pkcs8(der)),
        "RSA PRIVATE KEY" => Ok(Pem::Pkcs1Rsa(der)),
        "EC PRIVATE KEY" => Err(bad_key(
            "the key is SEC1 (BEGIN EC PRIVATE KEY); convert it to PKCS#8 (BEGIN PRIVATE KEY)",
        )),
        "ENCRYPTED PRIVATE KEY" => Err(bad_key(
            "the key is encrypted; store it decrypted — the keystore protects it",
        )),
        other => Err(bad_key(&format!(
            "a {other} block is not a private key this scheme signs with"
        ))),
    }
}

/// Sign `input` as `alg` says, with the key as its PEM labelled it.
fn sign(alg: JwtAlg, key: &Pem, input: &[u8]) -> Result<Vec<u8>, ConnectorError> {
    let rng = SystemRandom::new();
    match (alg, key) {
        (JwtAlg::Es256, Pem::Pkcs8(der)) => {
            let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, der)
                .map_err(|e| bad_key(&format!("not a P-256 key ES256 can sign with ({e})")))?;
            let sig = pair
                .sign(&rng, input)
                .map_err(|_| bad_key("signing failed"))?;
            Ok(sig.as_ref().to_vec())
        }
        (JwtAlg::Es256, Pem::Pkcs1Rsa(_)) => Err(bad_key("an RSA key cannot sign ES256")),
        (JwtAlg::Rs256, key) => {
            let pair = match key {
                Pem::Pkcs8(der) => RsaKeyPair::from_pkcs8(der),
                Pem::Pkcs1Rsa(der) => RsaKeyPair::from_der(der),
            }
            .map_err(|e| bad_key(&format!("not an RSA key RS256 can sign with ({e})")))?;
            let mut sig = vec![0u8; pair.public_modulus_len()];
            pair.sign(&RSA_PKCS1_SHA256, &rng, input, &mut sig)
                .map_err(|_| bad_key("signing failed"))?;
            Ok(sig)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pem_is_read_by_its_label_and_the_rest_is_refused_by_name() {
        let pkcs8 = "-----BEGIN PRIVATE KEY-----\nAQID\nBAU=\n-----END PRIVATE KEY-----\n";
        assert_eq!(parse_pem(pkcs8).unwrap(), Pem::Pkcs8(vec![1, 2, 3, 4, 5]));
        let pkcs1 = "-----BEGIN RSA PRIVATE KEY-----\nAQID\n-----END RSA PRIVATE KEY-----";
        assert_eq!(parse_pem(pkcs1).unwrap(), Pem::Pkcs1Rsa(vec![1, 2, 3]));
        for (text, says) in [
            (
                "-----BEGIN EC PRIVATE KEY-----\nAQID\n-----END EC PRIVATE KEY-----",
                "SEC1",
            ),
            (
                "-----BEGIN ENCRYPTED PRIVATE KEY-----\nAQID\n-----END ENCRYPTED PRIVATE KEY-----",
                "encrypted",
            ),
            (
                "-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----",
                "not a private key",
            ),
            ("AQID", "not PEM"),
            (
                "-----BEGIN PRIVATE KEY-----\n!!!\n-----END PRIVATE KEY-----",
                "not base64",
            ),
            (
                "-----BEGIN PRIVATE KEY-----\nAQID\n-----END RSA PRIVATE KEY-----",
                "does not end with",
            ),
        ] {
            let err = parse_pem(text).unwrap_err();
            assert!(err.to_string().contains(says), "{text:?} → {err}");
            assert!(!err.to_string().contains("AQID"), "the key is never quoted");
        }
    }
}
