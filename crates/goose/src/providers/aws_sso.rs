//! AWS SSO (IAM Identity Center) device-authorization flow.
//!
//! Mirrors what `aws sso configure` + `aws sso login` do, so that the existing
//! Bedrock provider can pick up short-lived SSO credentials via
//! `aws_config::defaults()` with zero extra plumbing.
//!
//! Uses public AWS SSO OIDC + Portal HTTP APIs directly via `reqwest` — the
//! OIDC endpoints (`register_client`, `start_device_authorization`, `token`)
//! and the Portal endpoints (`/federation/accounts`, `/federation/roles`)
//! accept anonymous JSON and don't need SigV4, so we skip pulling in the
//! `aws-sdk-ssooidc` / `aws-sdk-sso` crates.
//!
//! The Okta login happens transparently in the browser during the AWS-hosted
//! sign-in step; nothing in this module knows about Okta.
//!
//! POC scope (marked with `ponytail:` comments where deliberate):
//! - single account / single role auto-pick; if the user has multiple, we
//!   pick the first and log the alternatives.
//! - no refresh_token support; the 8-hour SSO session expires and the user
//!   re-runs the flow.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::sleep;

pub const DEFAULT_PROFILE_NAME: &str = "goose";
pub const DEFAULT_SESSION_NAME: &str = "goose";
const CLIENT_NAME: &str = "goose";
const CLIENT_TYPE: &str = "public";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const SCOPES: &[&str] = &["sso:account:access"];

fn oidc_endpoint(region: &str) -> String {
    format!("https://oidc.{}.amazonaws.com", region)
}

fn portal_endpoint(region: &str) -> String {
    format!("https://portal.sso.{}.amazonaws.com", region)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredClient {
    #[serde(rename = "clientId")]
    pub client_id: String,
    #[serde(rename = "clientSecret")]
    pub client_secret: String,
    #[serde(rename = "clientSecretExpiresAt")]
    pub client_secret_expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAuth {
    #[serde(rename = "deviceCode")]
    pub device_code: String,
    #[serde(rename = "userCode")]
    pub user_code: String,
    #[serde(rename = "verificationUri")]
    pub verification_uri: String,
    #[serde(rename = "verificationUriComplete")]
    pub verification_uri_complete: String,
    #[serde(rename = "expiresIn")]
    pub expires_in: i32,
    pub interval: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessToken {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "expiresIn")]
    pub expires_in: i32,
    #[serde(rename = "tokenType")]
    pub token_type: String,
}

#[derive(Debug, Deserialize)]
struct OidcError {
    error: String,
    #[serde(default, rename = "error_description")]
    _error_description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SsoSummary {
    pub profile_name: String,
    pub session_name: String,
    pub start_url: String,
    pub sso_region: String,
    pub bedrock_region: String,
    pub account_id: String,
    pub account_name: Option<String>,
    pub role_name: String,
    pub other_accounts: Vec<String>,
    pub other_roles: Vec<String>,
    pub expires_at: DateTime<Utc>,
}

pub async fn register_client(region: &str) -> Result<RegisteredClient> {
    let url = format!("{}/client/register", oidc_endpoint(region));
    let body = serde_json::json!({
        "clientName": CLIENT_NAME,
        "clientType": CLIENT_TYPE,
        "scopes": SCOPES,
    });
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await
        .context("register_client request failed")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("register_client HTTP {}: {}", status, text));
    }
    resp.json::<RegisteredClient>()
        .await
        .context("register_client response parse failed")
}

pub async fn start_device_authorization(
    region: &str,
    start_url: &str,
    client: &RegisteredClient,
) -> Result<DeviceAuth> {
    let url = format!("{}/device_authorization", oidc_endpoint(region));
    let body = serde_json::json!({
        "clientId": client.client_id,
        "clientSecret": client.client_secret,
        "startUrl": start_url,
    });
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await
        .context("start_device_authorization request failed")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "start_device_authorization HTTP {}: {}",
            status,
            text
        ));
    }
    resp.json::<DeviceAuth>()
        .await
        .context("start_device_authorization response parse failed")
}

/// Poll `create_token` until the user completes the browser sign-in.
/// Handles RFC 8628 `authorization_pending` / `slow_down` semantics.
pub async fn poll_for_token(
    region: &str,
    client: &RegisteredClient,
    device: &DeviceAuth,
) -> Result<AccessToken> {
    let url = format!("{}/token", oidc_endpoint(region));
    let mut interval = Duration::from_secs(device.interval.max(1) as u64);
    let deadline = std::time::Instant::now() + Duration::from_secs(device.expires_in.max(1) as u64);
    let http = reqwest::Client::new();

    loop {
        if std::time::Instant::now() >= deadline {
            return Err(anyhow!("AWS SSO device code expired before authorization"));
        }
        sleep(interval).await;

        let body = serde_json::json!({
            "clientId": client.client_id,
            "clientSecret": client.client_secret,
            "grantType": DEVICE_GRANT,
            "deviceCode": device.device_code,
        });
        let resp = http.post(&url).json(&body).send().await?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp.json::<AccessToken>().await?);
        }
        let text = resp.text().await.unwrap_or_default();
        let err: OidcError = serde_json::from_str(&text).unwrap_or(OidcError {
            error: "unknown_error".into(),
            _error_description: Some(text.clone()),
        });
        match err.error.as_str() {
            "authorization_pending" => continue,
            "slow_down" => interval += Duration::from_secs(5),
            other => return Err(anyhow!("token endpoint error: {} ({})", other, text)),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct AccountListResponse {
    #[serde(default, rename = "accountList")]
    account_list: Vec<Account>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Account {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(default, rename = "accountName")]
    pub account_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RoleListResponse {
    #[serde(default, rename = "roleList")]
    role_list: Vec<Role>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Role {
    #[serde(rename = "roleName")]
    pub role_name: String,
}

pub async fn list_accounts(region: &str, access_token: &str) -> Result<Vec<Account>> {
    let url = format!("{}/assignment/accounts", portal_endpoint(region));
    let resp = reqwest::Client::new()
        .get(&url)
        .header("x-amz-sso_bearer_token", access_token)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("list_accounts HTTP {}: {}", status, text));
    }
    Ok(resp.json::<AccountListResponse>().await?.account_list)
}

pub async fn list_roles(region: &str, access_token: &str, account_id: &str) -> Result<Vec<Role>> {
    let url = format!(
        "{}/assignment/roles?account_id={}",
        portal_endpoint(region),
        urlencoding::encode(account_id)
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .header("x-amz-sso_bearer_token", access_token)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("list_roles HTTP {}: {}", status, text));
    }
    Ok(resp.json::<RoleListResponse>().await?.role_list)
}

fn aws_dir() -> Result<PathBuf> {
    let mut p = dirs::home_dir().ok_or_else(|| anyhow!("no home directory"))?;
    p.push(".aws");
    Ok(p)
}

/// AWS SDK cache filename convention: sha1(session_name) hex + ".json".
pub fn sso_cache_filename(session_name: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(session_name.as_bytes());
    format!("{:x}.json", hasher.finalize())
}

/// Cache file the AWS SDKs read to vend short-lived credentials.
#[derive(Debug, Serialize)]
struct SsoTokenCache<'a> {
    #[serde(rename = "startUrl")]
    start_url: &'a str,
    region: &'a str,
    #[serde(rename = "accessToken")]
    access_token: &'a str,
    #[serde(rename = "expiresAt")]
    expires_at: String,
    #[serde(rename = "clientId")]
    client_id: &'a str,
    #[serde(rename = "clientSecret")]
    client_secret: &'a str,
}

pub fn write_sso_token_cache(
    session_name: &str,
    start_url: &str,
    sso_region: &str,
    token: &AccessToken,
    client: &RegisteredClient,
) -> Result<(PathBuf, DateTime<Utc>)> {
    write_sso_token_cache_in(
        &aws_dir()?,
        session_name,
        start_url,
        sso_region,
        token,
        client,
    )
}

fn write_sso_token_cache_in(
    aws_base: &Path,
    session_name: &str,
    start_url: &str,
    sso_region: &str,
    token: &AccessToken,
    client: &RegisteredClient,
) -> Result<(PathBuf, DateTime<Utc>)> {
    let mut cache_dir = aws_base.to_path_buf();
    cache_dir.push("sso");
    cache_dir.push("cache");
    fs::create_dir_all(&cache_dir).context("create ~/.aws/sso/cache")?;
    let cache_file = cache_dir.join(sso_cache_filename(session_name));

    let expires_at = Utc::now() + ChronoDuration::seconds(token.expires_in as i64);
    let payload = SsoTokenCache {
        start_url,
        region: sso_region,
        access_token: &token.access_token,
        expires_at: expires_at.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        client_id: &client.client_id,
        client_secret: &client.client_secret,
    };
    atomic_write(&cache_file, &serde_json::to_vec_pretty(&payload)?)
        .with_context(|| format!("write {}", cache_file.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&cache_file, fs::Permissions::from_mode(0o600));
    }
    Ok((cache_file, expires_at))
}

/// Idempotently upsert `[sso-session <name>]` and `[profile <name>]` blocks in
/// `~/.aws/config`. Existing profiles/sessions with the same name are replaced;
/// unrelated content is preserved verbatim.
pub fn write_aws_config_profile(
    profile_name: &str,
    session_name: &str,
    start_url: &str,
    sso_region: &str,
    account_id: &str,
    role_name: &str,
    bedrock_region: &str,
) -> Result<PathBuf> {
    write_aws_config_profile_in(
        &aws_dir()?,
        profile_name,
        session_name,
        start_url,
        sso_region,
        account_id,
        role_name,
        bedrock_region,
    )
}

#[allow(clippy::too_many_arguments)]
fn write_aws_config_profile_in(
    aws_base: &Path,
    profile_name: &str,
    session_name: &str,
    start_url: &str,
    sso_region: &str,
    account_id: &str,
    role_name: &str,
    bedrock_region: &str,
) -> Result<PathBuf> {
    let dir = aws_base.to_path_buf();
    fs::create_dir_all(&dir).context("create ~/.aws")?;
    let path = dir.join("config");
    let existing = fs::read_to_string(&path).unwrap_or_default();

    let session_header = format!("[sso-session {}]", session_name);
    let profile_header = if profile_name == "default" {
        "[default]".to_string()
    } else {
        format!("[profile {}]", profile_name)
    };

    let stripped = strip_ini_sections(&existing, &[&session_header, &profile_header]);
    let mut out = stripped.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(&format!(
        "{header}\nsso_start_url = {start_url}\nsso_region = {sso_region}\nsso_registration_scopes = {scopes}\n\n{profile_header}\nsso_session = {session_name}\nsso_account_id = {account_id}\nsso_role_name = {role_name}\nregion = {bedrock_region}\n",
        header = session_header,
        start_url = start_url,
        sso_region = sso_region,
        scopes = SCOPES.join(","),
        profile_header = profile_header,
        session_name = session_name,
        account_id = account_id,
        role_name = role_name,
        bedrock_region = bedrock_region,
    ));
    atomic_write(&path, out.as_bytes()).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

/// Write bytes to `path` atomically by staging to `<path>.tmp` and renaming.
/// `rename` is atomic on the same filesystem, so an interrupted write can
/// never leave a half-updated `~/.aws/config`.
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(
        path.extension()
            .map(|e| format!("{}.tmp", e.to_string_lossy()))
            .unwrap_or_else(|| "tmp".to_string()),
    );
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

/// Remove entire INI sections (header + body until next header) matching any
/// header in `headers`. Blank lines between kept sections are collapsed.
fn strip_ini_sections(input: &str, headers: &[&str]) -> String {
    let mut out = String::with_capacity(input.len());
    let mut skip = false;
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            skip = headers.contains(&trimmed);
        }
        if !skip {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Point Goose at the new profile so `BedrockProvider::from_env` picks it up.
pub fn save_goose_config(
    profile_name: &str,
    bedrock_region: &str,
    default_model: &str,
) -> Result<()> {
    use crate::config::providers::set_active_provider;
    use crate::config::Config;
    use crate::providers::bedrock::BEDROCK_PROVIDER_NAME;

    let config = Config::global();
    config.set_param("AWS_PROFILE", profile_name)?;
    config.set_param("AWS_REGION", bedrock_region)?;
    set_active_provider(config, BEDROCK_PROVIDER_NAME, default_model)?;
    Ok(())
}

/// End-to-end: register client, do device flow, pick account/role, write
/// `~/.aws/*` files and Goose config. `on_prompt` fires once the verification
/// URI is known so the caller can open a browser / display the user code.
pub async fn run_end_to_end<F>(
    start_url: &str,
    sso_region: &str,
    bedrock_region: &str,
    default_model: &str,
    on_prompt: F,
) -> Result<SsoSummary>
where
    F: FnOnce(&DeviceAuth),
{
    let client = register_client(sso_region).await?;
    let device = start_device_authorization(sso_region, start_url, &client).await?;
    on_prompt(&device);
    let token = poll_for_token(sso_region, &client, &device).await?;

    let accounts = list_accounts(sso_region, &token.access_token).await?;
    if accounts.is_empty() {
        return Err(anyhow!(
            "AWS SSO returned no accounts for this user — check permission sets"
        ));
    }
    // ponytail: single-account auto-pick; multi-account picker if users demand it.
    let account = &accounts[0];
    let other_accounts = accounts
        .iter()
        .skip(1)
        .map(|a| a.account_id.clone())
        .collect::<Vec<_>>();

    let roles = list_roles(sso_region, &token.access_token, &account.account_id).await?;
    if roles.is_empty() {
        return Err(anyhow!(
            "AWS SSO returned no roles for account {}",
            account.account_id
        ));
    }
    // ponytail: single-role auto-pick; multi-role picker if users demand it.
    let role = &roles[0];
    let other_roles = roles
        .iter()
        .skip(1)
        .map(|r| r.role_name.clone())
        .collect::<Vec<_>>();

    let (_cache_path, expires_at) =
        write_sso_token_cache(DEFAULT_SESSION_NAME, start_url, sso_region, &token, &client)?;
    write_aws_config_profile(
        DEFAULT_PROFILE_NAME,
        DEFAULT_SESSION_NAME,
        start_url,
        sso_region,
        &account.account_id,
        &role.role_name,
        bedrock_region,
    )?;
    save_goose_config(DEFAULT_PROFILE_NAME, bedrock_region, default_model)?;

    Ok(SsoSummary {
        profile_name: DEFAULT_PROFILE_NAME.into(),
        session_name: DEFAULT_SESSION_NAME.into(),
        start_url: start_url.into(),
        sso_region: sso_region.into(),
        bedrock_region: bedrock_region.into(),
        account_id: account.account_id.clone(),
        account_name: account.account_name.clone(),
        role_name: role.role_name.clone(),
        other_accounts,
        other_roles,
        expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sso_cache_filename_matches_aws_sdk_convention() {
        // sha1("goose") — verified against `printf goose | shasum -a 1`.
        assert_eq!(
            sso_cache_filename("goose"),
            "4f7f358cd341d703b4a502b2d0f53db07823b501.json"
        );
    }

    #[test]
    fn strip_ini_sections_removes_only_named_sections() {
        let input = "[default]\nregion = us-east-1\n\n[profile goose]\nold = value\n\n[profile other]\nkeep = me\n";
        let out = strip_ini_sections(input, &["[profile goose]"]);
        assert!(out.contains("[default]"));
        assert!(out.contains("[profile other]"));
        assert!(out.contains("keep = me"));
        assert!(!out.contains("[profile goose]"));
        assert!(!out.contains("old = value"));
    }

    #[test]
    fn write_aws_config_profile_preserves_unrelated_content() {
        let tmp = tempfile::tempdir().unwrap();
        let aws = tmp.path().join(".aws");
        fs::create_dir_all(&aws).unwrap();
        fs::write(aws.join("config"), "[profile keepme]\nregion = eu-west-1\n").unwrap();

        let path = write_aws_config_profile_in(
            &aws,
            "goose",
            "goose",
            "https://d-x.awsapps.com/start",
            "us-east-1",
            "111122223333",
            "PowerUserAccess",
            "us-east-1",
        )
        .unwrap();
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[profile keepme]"));
        assert!(contents.contains("[sso-session goose]"));
        assert!(contents.contains("[profile goose]"));
        assert!(contents.contains("sso_account_id = 111122223333"));
    }
}
