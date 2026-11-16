//! OAuth2 authorization code with PKCE, and the refresh that keeps a token
//! alive. The person's own client id and secret arrive as stored fields;
//! nothing here ships one. The browser's redirect lands on the node; this
//! module only builds the URL it is sent to and exchanges what it brings back.
//!
//! The client-credentials grant lives here too: no browser and no refresh
//! token — the client's id and secret are traded for an access token, and
//! traded again when it lapses, under the same one-at-a-time rule.

use crate::client::Client;
use crate::creds::{AccountRef, Entropy, Secret, Stored, TokenSet};
use crate::error::ConnectorError;
use crate::hosts::HostJudge;
use crate::http::Request;
use crate::spec::{AuthSpec, ChallengeEncoding, Method, ScopeJoin, TokenAuth};
use crate::template::{self, Encode, Values};
use base64::Engine as _;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::Duration;
use url::Url;

/// How soon before its expiry an access token is refreshed.
pub const REFRESH_MARGIN_SECS: u64 = 60;

/// Where the browser is sent, and what the node must remember to finish.
#[derive(Clone, Debug)]
pub struct Authorize {
    pub url: String,
    pub state: String,
    pub verifier: Secret,
}

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// The OAuth2 scheme's parts, borrowed; any other scheme is a bad definition.
struct OAuthParts<'a> {
    authorization_url: &'a str,
    token_url: &'a str,
    scopes: &'a [String],
    pkce: bool,
    extra: &'a std::collections::BTreeMap<String, String>,
    client_id_param: &'a str,
    scope_join: ScopeJoin,
    code_challenge: ChallengeEncoding,
}

fn oauth_parts(auth: &AuthSpec) -> Result<OAuthParts<'_>, ConnectorError> {
    match auth {
        AuthSpec::OAuth2 {
            authorization_url,
            token_url,
            scopes,
            pkce,
            extra,
            client_id_param,
            scope_join,
            code_challenge,
        } => Ok(OAuthParts {
            authorization_url,
            token_url,
            scopes,
            pkce: *pkce,
            extra,
            client_id_param,
            scope_join: *scope_join,
            code_challenge: *code_challenge,
        }),
        _ => Err(ConnectorError::BadDefinition(
            "this connector does not authenticate with OAuth2".into(),
        )),
    }
}

/// An OAuth endpoint must be `https`, or `http` on loopback for a test —
/// and a host the workspace's policy denies is refused here as anywhere:
/// the deny list holds for the consent page and the token endpoint too.
fn endpoint(url: &str, judge: &dyn HostJudge) -> Result<Url, ConnectorError> {
    let parsed = scheme_checked(url)?;
    let host = crate::hosts::host_of(&parsed).unwrap_or_default();
    judge
        .judge(&host)
        .map_err(|reason| ConnectorError::HostRefused {
            host,
            allowed: reason,
        })?;
    Ok(parsed)
}

fn scheme_checked(url: &str) -> Result<Url, ConnectorError> {
    let parsed = Url::parse(url).map_err(|e| {
        ConnectorError::BadDefinition(format!("OAuth URL {url:?} does not parse: {e}"))
    })?;
    let host = parsed.host_str().unwrap_or_default();
    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" if crate::hosts::is_loopback(host) => Ok(parsed),
        _ => Err(ConnectorError::BadDefinition(format!(
            "OAuth URL {url:?} must be https"
        ))),
    }
}

/// The authorization URL for `client_id`, with a fresh `state` and, when the
/// scheme asks for PKCE, a fresh verifier whose S256 challenge rides along —
/// the client's id under the name the scheme gives it, the scopes joined
/// as it says, the challenge written as it reads it.
pub fn authorize_url(
    auth: &AuthSpec,
    client_id: &str,
    redirect_uri: &str,
    entropy: &dyn Entropy,
    judge: &dyn HostJudge,
) -> Result<Authorize, ConnectorError> {
    let OAuthParts {
        authorization_url,
        scopes,
        pkce,
        extra,
        client_id_param,
        scope_join,
        code_challenge,
        ..
    } = oauth_parts(auth)?;
    let mut url = endpoint(authorization_url, judge)?;
    let mut state_bytes = [0u8; 32];
    entropy.fill(&mut state_bytes);
    let state = hex::encode(state_bytes);
    let mut verifier_bytes = [0u8; 32];
    entropy.fill(&mut verifier_bytes);
    let verifier = b64url(&verifier_bytes);
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code");
        q.append_pair(client_id_param, client_id);
        q.append_pair("redirect_uri", redirect_uri);
        if !scopes.is_empty() {
            q.append_pair("scope", &scopes.join(scope_join.separator()));
        }
        q.append_pair("state", &state);
        if pkce {
            let digest = Sha256::digest(verifier.as_bytes());
            let challenge = match code_challenge {
                ChallengeEncoding::Base64url => b64url(&digest),
                ChallengeEncoding::Hex => hex::encode(digest),
            };
            q.append_pair("code_challenge", &challenge);
            q.append_pair("code_challenge_method", "S256");
        }
        for (k, v) in extra {
            if k != "token_auth" {
                q.append_pair(k, v);
            }
        }
    }
    Ok(Authorize {
        url: url.to_string(),
        state,
        verifier: Secret::new(verifier),
    })
}

/// A token-endpoint request: a form body, and the client's own credentials
/// either in the form — the id under `client_id_param` — or, when the
/// definition says so (`basic`), in an `Authorization` header.
fn token_request(
    token_url: &str,
    judge: &dyn HostJudge,
    basic: bool,
    client_id_param: &str,
    stored: &Stored,
    pairs: Vec<(&str, String)>,
    timeout: Duration,
) -> Result<(Request, Vec<String>), ConnectorError> {
    let url = endpoint(token_url, judge)?;
    let client_id = stored
        .field(crate::creds::Field::ClientId)
        .ok_or_else(|| {
            // LCOV_EXCL_START a sign-in refusal; out by decision
            ConnectorError::NotAuthenticated("client_id is not set for this account".into())
            // LCOV_EXCL_STOP
        })?
        .expose()
        .to_string();
    let client_secret = stored
        .field(crate::creds::Field::ClientSecret)
        .map(|s| s.expose().to_string());
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    let mut secrets = Vec::new();
    for (k, v) in &pairs {
        form.append_pair(k, v);
    }
    if !basic {
        form.append_pair(client_id_param, &client_id);
        if let Some(secret) = &client_secret {
            form.append_pair("client_secret", secret);
        }
    }
    let mut req = Request::new(Method::Post, url, timeout);
    req.headers.push((
        "content-type".into(),
        "application/x-www-form-urlencoded".into(),
    ));
    req.headers
        .push(("accept".into(), "application/json".into()));
    req.headers.push(("user-agent".into(), "bisa".into()));
    if basic {
        let pair = format!("{client_id}:{}", client_secret.clone().unwrap_or_default());
        let encoded = base64::engine::general_purpose::STANDARD.encode(pair.as_bytes());
        req.headers
            .push(("authorization".into(), format!("Basic {encoded}")));
        secrets.push(pair);
    }
    req.body = Some(form.finish().into_bytes());
    if let Some(secret) = client_secret {
        secrets.push(secret);
    }
    for (k, v) in pairs {
        if matches!(k, "code" | "code_verifier" | "refresh_token") {
            secrets.push(v);
        }
    }
    Ok((req, secrets))
}

fn read_tokens(
    body: &Value,
    now: u64,
    keep_refresh: Option<&Secret>,
) -> Result<TokenSet, ConnectorError> {
    let access = body
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ConnectorError::OAuth("the token endpoint answered no access_token".into())
        })?;
    if let Some(kind) = body.get("token_type").and_then(Value::as_str) {
        if !kind.eq_ignore_ascii_case("bearer") {
            return Err(ConnectorError::OAuth(format!(
                "token_type {kind:?} is not bearer"
            )));
        }
    }
    let refresh = body
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(Secret::new)
        .or_else(|| keep_refresh.cloned());
    let expires_at = body
        .get("expires_in")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        // A platform that answers a number no clock holds does not overflow ours.
        .map(|secs: u64| now.saturating_add(secs));
    Ok(TokenSet {
        access_token: Secret::new(access),
        refresh_token: refresh,
        expires_at,
        scope: body
            .get("scope")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// Exchange the code the redirect brought back for tokens, and keep them.
pub async fn exchange(
    client: &Client,
    auth: &AuthSpec,
    account: &AccountRef,
    code: &str,
    verifier: &Secret,
    redirect_uri: &str,
    judge: &dyn HostJudge,
) -> Result<TokenSet, ConnectorError> {
    let OAuthParts {
        token_url,
        pkce,
        extra,
        client_id_param,
        ..
    } = oauth_parts(auth)?;
    let stored = client.creds().load(account).await?;
    let mut pairs = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
    ];
    if pkce {
        pairs.push(("code_verifier", verifier.expose().to_string()));
    }
    let (req, secrets) = token_request(
        token_url,
        judge,
        basic_in(extra),
        client_id_param,
        &stored,
        pairs,
        client.oauth_timeout(),
    )?;
    let body = token_body(client, &req, &secrets).await?;
    let tokens = read_tokens(&body, client.now(), None).map_err(|e| e.scrubbed(&refs(&secrets)))?;
    client.creds().save_tokens(account, &tokens).await?;
    Ok(tokens)
}

/// Trade the refresh token for a new access token, and keep the pair —
/// under the token endpoint's own deadline.
pub async fn refresh(
    client: &Client,
    auth: &AuthSpec,
    account: &AccountRef,
    stored: &Stored,
    judge: &dyn HostJudge,
) -> Result<TokenSet, ConnectorError> {
    renew_within(
        client,
        auth,
        account,
        stored,
        &BTreeMap::new(),
        judge,
        client.oauth_timeout(),
        None,
    )
    .await
}

/// Whether an OAuth2 scheme's `extra` sends the client's credentials in a
/// header.
fn basic_in(extra: &BTreeMap<String, String>) -> bool {
    extra.get("token_auth").is_some_and(|v| v == "basic")
}

/// The request that gets an account a new access token, by its scheme, and
/// what must be known to read the answer.
struct Renewal {
    req: Request,
    /// What the request carries that no message may repeat.
    secrets: Vec<String>,
    /// The refresh token to keep when the platform answers none — an
    /// OAuth2 refresh's; a client-credentials grant has none.
    kept: Option<Secret>,
    /// What the platform's refusal of this request means to the person.
    refused: Refusal,
}

/// The two ways a token endpoint says *no* for good.
#[derive(Clone, Copy)]
enum Refusal {
    /// `invalid_grant`: the refresh token no longer works.
    Grant,
    /// `invalid_client`, or a 401: the client's id or secret is wrong.
    Client,
}

impl Refusal {
    /// The sentence for a refusal that is the account's to mend, or the
    /// error as it came.
    fn read(self, e: ConnectorError) -> ConnectorError {
        match (self, e) {
            (
                Refusal::Grant,
                ConnectorError::Refused {
                    status: 400,
                    reason,
                },
            ) if reason.contains("invalid_grant") => ConnectorError::NotAuthenticated(
                "the refresh token was revoked; connect the account again".into(),
            ),
            (
                Refusal::Client,
                ConnectorError::Refused {
                    status: 400,
                    reason,
                },
            ) if reason.contains("invalid_client") => ConnectorError::NotAuthenticated(
                "the platform refused the client's id and secret; set them again".into(),
            ),
            (Refusal::Client, ConnectorError::NotAuthenticated(_)) => {
                ConnectorError::NotAuthenticated(
                    "the platform refused the client's id and secret; set them again".into(),
                )
            }
            (_, e) => e,
        }
    }
}

/// Build the renewal a scheme names: a refresh for OAuth2, the grant itself
/// for client credentials — its extra pairs rendered against the account.
fn renewal(
    client: &Client,
    auth: &AuthSpec,
    stored: &Stored,
    account_params: &BTreeMap<String, Value>,
    judge: &dyn HostJudge,
) -> Result<Renewal, ConnectorError> {
    if let AuthSpec::ClientCredentials {
        token_url,
        scopes,
        scope_join,
        token_auth,
        grant_type,
        params,
    } = auth
    {
        // The signer asked for the client's id and secret before the call
        // got here (`ClientCredentialsSigner::credential_for`); a secret
        // taken away since is the platform's to refuse, as `invalid_client`.
        let none = BTreeMap::new();
        let values = Values {
            account: account_params,
            params: &none,
        };
        let mut pairs = vec![("grant_type", grant_type.clone())];
        if !scopes.is_empty() {
            pairs.push(("scope", scopes.join(scope_join.separator())));
        }
        for (k, tmpl) in params {
            pairs.push((k.as_str(), template::render(tmpl, &values, Encode::Text)?));
        }
        let (req, secrets) = token_request(
            token_url,
            judge,
            *token_auth == TokenAuth::Basic,
            crate::spec::DEFAULT_CLIENT_ID_PARAM,
            stored,
            pairs,
            client.oauth_timeout(),
        )?;
        return Ok(Renewal {
            req,
            secrets,
            kept: None,
            refused: Refusal::Client,
        });
    }
    let OAuthParts {
        token_url,
        extra,
        client_id_param,
        ..
    } = oauth_parts(auth)?;
    let refresh_token = stored
        .field(crate::creds::Field::RefreshToken)
        .ok_or_else(|| {
            // LCOV_EXCL_START a sign-in refusal; out by decision
            ConnectorError::NotAuthenticated(
                "the access token expired and there is no refresh token; connect the account again"
                    .into(),
            )
            // LCOV_EXCL_STOP
        })?;
    let pairs = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.expose().to_string()),
    ];
    let (req, secrets) = token_request(
        token_url,
        judge,
        basic_in(extra),
        client_id_param,
        stored,
        pairs,
        client.oauth_timeout(),
    )?;
    Ok(Renewal {
        req,
        secrets,
        kept: Some(refresh_token.clone()),
        refused: Refusal::Grant,
    })
}

/// The account's refresh lock, held by whoever is exchanging its token.
type RefreshTurn = tokio::sync::OwnedMutexGuard<()>;

/// The renewal: a refresh, or a client-credentials grant asked again. The
/// caller waits `timeout` for it — what is left of the call that needs the
/// token, never more than the endpoint's own deadline — but the exchange is
/// **not the caller's to cut short**: a platform that rotates its refresh
/// token spent the old one the moment it answered, so the request and the
/// save run on a task of their own, under the endpoint's deadline and
/// holding the account's `turn`, and what the platform answered is kept
/// whether or not anybody still waits for it.
#[allow(clippy::too_many_arguments)]
async fn renew_within(
    client: &Client,
    auth: &AuthSpec,
    account: &AccountRef,
    stored: &Stored,
    account_params: &BTreeMap<String, Value>,
    judge: &dyn HostJudge,
    timeout: Duration,
    turn: Option<RefreshTurn>,
) -> Result<TokenSet, ConnectorError> {
    let Renewal {
        req,
        secrets,
        kept,
        refused,
    } = renewal(client, auth, stored, account_params, judge)?;
    let wait = timeout.min(client.oauth_timeout());
    let flight = {
        let (client, account) = (client.clone(), account.clone());
        tokio::spawn(async move {
            let _turn = turn;
            let body = token_body(&client, &req, &secrets)
                .await
                .map_err(|e| refused.read(e))?;
            let mut tokens = read_tokens(&body, client.now(), kept.as_ref())
                .map_err(|e| e.scrubbed(&refs(&secrets)))?;
            if kept.is_none() {
                // A grant asked again needs no refresh token, and the
                // account has no field for one: what a platform adds is let go.
                tokens.refresh_token = None;
            }
            client.creds().save_tokens(&account, &tokens).await?;
            Ok(tokens)
        })
    };
    match tokio::time::timeout(wait, flight).await {
        Ok(Ok(settled)) => settled,
        // LCOV_EXCL_START the exchange's task unwound: a panic in the transport, which no fake raises
        Ok(Err(lost)) => Err(ConnectorError::Transport(format!(
            "the token exchange did not finish ({lost})"
        ))),
        // LCOV_EXCL_STOP
        Err(_) => Err(ConnectorError::Timeout(wait)),
    }
}

/// A usable access token: the stored one while it is not about to expire,
/// else a new one — asked for once even when several calls ask at the same
/// moment, and within `budget`, what the call that needs it has left.
pub(crate) async fn ensure_fresh(
    client: &Client,
    auth: &AuthSpec,
    account: &AccountRef,
    stored: &Stored,
    account_params: &BTreeMap<String, Value>,
    judge: &dyn HostJudge,
    budget: Duration,
) -> Result<Secret, ConnectorError> {
    let fresh = |s: &Stored| -> Option<Secret> {
        let token = s.field(crate::creds::Field::AccessToken)?.clone();
        match s.expires_at {
            None => Some(token),
            Some(at) if at.saturating_sub(client.now()) >= REFRESH_MARGIN_SECS => Some(token),
            Some(_) => None,
        }
    };
    if let Some(token) = fresh(stored) {
        return Ok(token);
    }
    let turn = client.refresh_lock(account).lock_owned().await;
    // Another flight may have refreshed while this one waited for the lock.
    let again = client.creds().load(account).await?;
    if let Some(token) = fresh(&again) {
        return Ok(token);
    }
    let tokens = renew_within(
        client,
        auth,
        account,
        &again,
        account_params,
        judge,
        budget,
        Some(turn),
    )
    .await?;
    Ok(tokens.access_token)
}

/// Force a refresh — a 401 said the token no longer works whatever its
/// expiry. Once under contention: `used` is the token the refused request
/// carried, and a flight that finds another one already stored under the
/// lock takes that one rather than refreshing again.
pub(crate) async fn force_refresh(
    client: &Client,
    auth: &AuthSpec,
    account: &AccountRef,
    used: Option<&Secret>,
    account_params: &BTreeMap<String, Value>,
    judge: &dyn HostJudge,
    budget: Duration,
) -> Result<Secret, ConnectorError> {
    let turn = client.refresh_lock(account).lock_owned().await;
    let stored = client.creds().load(account).await?;
    let replaced = stored
        .field(crate::creds::Field::AccessToken)
        .filter(|now| used.is_some_and(|u| u != *now));
    if let Some(now) = replaced {
        return Ok(now.clone());
    }
    let tokens = renew_within(
        client,
        auth,
        account,
        &stored,
        account_params,
        judge,
        budget,
        Some(turn),
    )
    .await?;
    Ok(tokens.access_token)
}

async fn token_body(
    client: &Client,
    req: &Request,
    secrets: &[String],
) -> Result<Value, ConnectorError> {
    let host = crate::hosts::host_of(&req.url).unwrap_or_default();
    let resp = client
        // A token request is a write nobody can deduplicate: a refresh sent
        // twice spends a one-time refresh token twice.
        .send_judged(req, crate::retry::Resend::Write, false, &host)
        .await
        .map_err(|e| e.scrubbed(&refs(secrets)))?;
    match crate::outcome::parse(&resp, None, None, client.now()) {
        Ok(outcome) => Ok(outcome.body),
        Err(e) => Err(e.scrubbed(&refs(secrets))),
    }
}

fn refs(secrets: &[String]) -> Vec<&str> {
    secrets.iter().map(String::as_str).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_read_with_their_expiry_and_a_kept_refresh_token() {
        let body = serde_json::json!({"access_token": "a", "expires_in": 3600, "token_type": "Bearer", "scope": "x"});
        let kept = Secret::new("r0");
        let t = read_tokens(&body, 100, Some(&kept)).unwrap();
        assert_eq!(t.access_token.expose(), "a");
        assert_eq!(t.refresh_token.as_ref().map(Secret::expose), Some("r0"));
        assert_eq!(t.expires_at, Some(3700));
        assert_eq!(t.scope.as_deref(), Some("x"));
        let bad = serde_json::json!({"access_token": "a", "token_type": "mac"});
        assert!(matches!(
            read_tokens(&bad, 0, None),
            Err(ConnectorError::OAuth(_))
        ));
        assert!(matches!(
            read_tokens(&serde_json::json!({}), 0, None),
            Err(ConnectorError::OAuth(_))
        ));
    }
}
