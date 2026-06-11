//! OAuth (PKCE) login against the ChatGPT subscription, using OpenAI's
//! public Codex CLI client. Same mechanism as openclaw/opencode: the
//! browser flow redirects to a local callback server on port 1455.

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const ISSUER: &str = "https://auth.openai.com";
const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const SCOPE: &str = "openid profile email offline_access";

#[derive(Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub account_id: String,
    pub expires_at: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: Option<u64>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn auth_path() -> Result<PathBuf> {
    let dir = dirs::data_dir().context("cannot locate data directory")?;
    Ok(dir.join("shpell").join("auth.json"))
}

fn save(tokens: &Tokens) -> Result<()> {
    let path = auth_path()?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_vec_pretty(tokens)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn load() -> Result<Tokens> {
    let path = auth_path()?;
    let raw = std::fs::read(&path)
        .map_err(|_| anyhow!("not logged in, run `shpell auth login` first"))?;
    Ok(serde_json::from_slice(&raw)?)
}

fn random_b64(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decode a JWT payload without verifying the signature; we only need to
/// read our own token's claims locally.
fn jwt_claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn extract_account_id(resp: &TokenResponse) -> Result<String> {
    for token in [resp.id_token.as_deref(), Some(resp.access_token.as_str())]
        .into_iter()
        .flatten()
    {
        if let Some(claims) = jwt_claims(token) {
            if let Some(id) = claims["https://api.openai.com/auth"]["chatgpt_account_id"].as_str()
            {
                return Ok(id.to_string());
            }
        }
    }
    bail!("token response does not contain a ChatGPT account id; does this account have an active ChatGPT subscription?")
}

pub fn login() -> Result<()> {
    let verifier = random_b64(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_b64(32);

    let mut url = Url::parse(&format!("{ISSUER}/oauth/authorize"))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("scope", SCOPE)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("originator", "codex_cli_rs")
        .append_pair("state", &state);

    let listener =
        TcpListener::bind("127.0.0.1:1455").context("port 1455 is busy (another login running?)")?;

    eprintln!("Open this URL in your browser to log in:\n\n  {url}\n");
    open_browser(url.as_str());

    let code = wait_for_code(&listener, &state)?;

    let resp: TokenResponse = reqwest::blocking::Client::new()
        .post(format!("{ISSUER}/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT_URI),
            ("client_id", CLIENT_ID),
            ("code_verifier", &verifier),
        ])
        .send()
        .context("token exchange request failed")?
        .error_for_status()
        .context("token exchange rejected")?
        .json()?;

    let account_id = extract_account_id(&resp)?;
    save(&Tokens {
        account_id,
        expires_at: now() + resp.expires_in.unwrap_or(3600),
        refresh_token: resp
            .refresh_token
            .context("no refresh token in response")?,
        access_token: resp.access_token,
    })?;
    eprintln!("Logged in.");
    Ok(())
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(not(target_os = "macos"))]
    let cmd = "xdg-open";
    let _ = std::process::Command::new(cmd)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String> {
    for stream in listener.incoming() {
        let mut stream = stream?;
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line)?;
        // request line: GET /auth/callback?code=...&state=... HTTP/1.1
        let path = line.split_whitespace().nth(1).unwrap_or("");
        if !path.starts_with("/auth/callback") {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n");
            continue;
        }
        let url = Url::parse(&format!("http://localhost{path}"))?;
        let mut code = None;
        let mut got_state = None;
        for (k, v) in url.query_pairs() {
            match k.as_ref() {
                "code" => code = Some(v.into_owned()),
                "state" => got_state = Some(v.into_owned()),
                _ => {}
            }
        }
        let body = "<html><body>Login successful. You can close this tab.</body></html>";
        let _ = stream.write_all(
            format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        );
        if got_state.as_deref() != Some(state) {
            bail!("OAuth state mismatch");
        }
        return code.context("callback did not contain an authorization code");
    }
    bail!("callback listener closed unexpectedly")
}

/// Returns a valid access token and the ChatGPT account id, refreshing the
/// token if it expires within 5 minutes.
pub fn access() -> Result<(String, String)> {
    let mut tokens = load()?;
    if tokens.expires_at <= now() + 300 {
        let resp: TokenResponse = reqwest::blocking::Client::new()
            .post(format!("{ISSUER}/oauth/token"))
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", &tokens.refresh_token),
                ("client_id", CLIENT_ID),
                ("scope", SCOPE),
            ])
            .send()
            .context("token refresh request failed")?
            .error_for_status()
            .context("token refresh rejected, run `shpell auth login` again")?
            .json()?;
        tokens.expires_at = now() + resp.expires_in.unwrap_or(3600);
        tokens.access_token = resp.access_token;
        if let Some(rt) = resp.refresh_token {
            tokens.refresh_token = rt;
        }
        save(&tokens)?;
    }
    Ok((tokens.access_token, tokens.account_id))
}

pub fn logout() -> Result<()> {
    let path = auth_path()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    eprintln!("Logged out.");
    Ok(())
}

pub fn status() -> Result<()> {
    match load() {
        Ok(t) => {
            let left = t.expires_at.saturating_sub(now());
            eprintln!(
                "Logged in (account {}), access token {}",
                t.account_id,
                if left == 0 {
                    "expired (will refresh on next use)".to_string()
                } else {
                    format!("valid for {}m", left / 60)
                }
            );
        }
        Err(_) => eprintln!("Not logged in. Run `shpell auth login`."),
    }
    Ok(())
}
