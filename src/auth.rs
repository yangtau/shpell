//! OAuth (PKCE) login for subscription-backed providers.
//!
//! - `openai-chatgpt`: OpenAI's public Codex CLI client (`auth.openai.com`,
//!   callback on localhost:1455). Same mechanism as openclaw/opencode.
//! - `xai-grok`: xAI's public Grok CLI client (`auth.x.ai`). The accounts
//!   page delivers the code via a CORS fetch to a loopback callback, matching
//!   Grok Build; inference then uses the OAuth token against `api.x.ai`.
//!
//! Tokens live under `~/.local/share/shpell/<provider>.json` (0600) and
//! auto-refresh. XDG-style on every platform, same policy as config.

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

use crate::config::Config;

pub const OPENAI_CHATGPT: &str = "openai-chatgpt";
pub const XAI_GROK: &str = "xai-grok";

const OPENAI_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OPENAI_ISSUER: &str = "https://auth.openai.com";
const OPENAI_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const OPENAI_SCOPE: &str = "openid profile email offline_access";

const XAI_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const XAI_ISSUER: &str = "https://auth.x.ai";
const XAI_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access conversations:read conversations:write workspaces:read workspaces:write";
const XAI_ACCOUNTS_ORIGIN: &str = "https://accounts.x.ai";

#[derive(Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(default)]
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
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn auth_path(provider: &str) -> Result<PathBuf> {
    // ~/.local/share (XDG style) on every platform; dirs::data_dir() would
    // resolve to ~/Library/Application Support on macOS.
    let dir = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("share")))
        .context("cannot locate data directory")?
        .join("shpell");
    Ok(dir.join(format!("{provider}.json")))
}

fn save(provider: &str, tokens: &Tokens) -> Result<()> {
    let path = auth_path(provider)?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_vec_pretty(tokens)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn load(provider: &str) -> Result<Tokens> {
    let path = auth_path(provider)?;
    let raw = std::fs::read(&path)
        .map_err(|_| anyhow!("not logged in, run `shpell auth login {provider}` first"))?;
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
            if let Some(id) = claims["https://api.openai.com/auth"]["chatgpt_account_id"].as_str() {
                return Ok(id.to_string());
            }
        }
    }
    bail!("token response does not contain a ChatGPT account id; does this account have an active ChatGPT subscription?")
}

pub fn login(provider: &str) -> Result<()> {
    match provider {
        OPENAI_CHATGPT => login_openai()?,
        XAI_GROK => login_xai()?,
        other => bail!("unknown provider {other:?} (supported: {OPENAI_CHATGPT}, {XAI_GROK})"),
    }
    eprintln!("Logged in.");
    let cfg = Config::load().unwrap_or_default();
    if cfg.provider != provider {
        if let Ok(path) = Config::path() {
            eprintln!(
                "Set provider = \"{provider}\" in {} to use this account.",
                path.display()
            );
        }
    }
    Ok(())
}

fn login_openai() -> Result<()> {
    let verifier = random_b64(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_b64(32);

    let mut url = Url::parse(&format!("{OPENAI_ISSUER}/oauth/authorize"))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", OPENAI_CLIENT_ID)
        .append_pair("redirect_uri", OPENAI_REDIRECT_URI)
        .append_pair("scope", OPENAI_SCOPE)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("originator", "codex_cli_rs")
        .append_pair("state", &state);

    let listener = TcpListener::bind("127.0.0.1:1455")
        .context("port 1455 is busy (another login running?)")?;

    eprintln!("Open this URL in your browser to log in:\n\n  {url}\n");
    open_browser(url.as_str());

    let code = wait_for_code(&listener, &state, "/auth/callback", None)?;

    let resp: TokenResponse = reqwest::blocking::Client::new()
        .post(format!("{OPENAI_ISSUER}/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", OPENAI_REDIRECT_URI),
            ("client_id", OPENAI_CLIENT_ID),
            ("code_verifier", &verifier),
        ])
        .send()
        .context("token exchange request failed")?
        .error_for_status()
        .context("token exchange rejected")?
        .json()?;

    let account_id = extract_account_id(&resp)?;
    save(
        OPENAI_CHATGPT,
        &Tokens {
            account_id,
            expires_at: now() + resp.expires_in.unwrap_or(3600),
            refresh_token: resp.refresh_token.context("no refresh token in response")?,
            access_token: resp.access_token,
        },
    )
}

fn login_xai() -> Result<()> {
    let verifier = random_b64(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_b64(32);
    let nonce = random_b64(32);

    let listener = TcpListener::bind("127.0.0.1:0")
        .context("cannot bind a loopback port for the OAuth callback")?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");

    let mut url = Url::parse(&format!("{XAI_ISSUER}/oauth2/authorize"))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", XAI_CLIENT_ID)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", XAI_SCOPE)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .append_pair("nonce", &nonce)
        .append_pair("referrer", "shpell");

    eprintln!("Open this URL in your browser to log in with SuperGrok / X Premium:\n\n  {url}\n");
    open_browser(url.as_str());

    let code = wait_for_code(&listener, &state, "/callback", Some(XAI_ACCOUNTS_ORIGIN))?;

    let resp: TokenResponse = reqwest::blocking::Client::new()
        .post(format!("{XAI_ISSUER}/oauth2/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", redirect_uri.as_str()),
            ("client_id", XAI_CLIENT_ID),
            ("code_verifier", &verifier),
        ])
        .send()
        .context("token exchange request failed")?
        .error_for_status()
        .context("token exchange rejected")?
        .json()?;

    save(
        XAI_GROK,
        &Tokens {
            account_id: String::new(),
            expires_at: now() + resp.expires_in.unwrap_or(3600),
            refresh_token: resp.refresh_token.context("no refresh token in response")?,
            access_token: resp.access_token,
        },
    )
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

struct HttpRequest {
    method: String,
    path: String,
    origin: Option<String>,
}

fn read_http_request(stream: &TcpStream) -> Result<HttpRequest> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut origin = None;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("origin") {
                origin = Some(v.trim().to_string());
            }
        }
    }
    Ok(HttpRequest {
        method,
        path,
        origin,
    })
}

fn write_http(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    allow_origin: Option<&str>,
    content_type: &str,
    body: &str,
) {
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\n",
        body.len()
    );
    if let Some(origin) = allow_origin {
        head.push_str(&format!(
            "access-control-allow-origin: {origin}\r\n\
             access-control-allow-methods: GET, OPTIONS\r\n\
             access-control-allow-headers: *\r\n\
             access-control-allow-private-network: true\r\n\
             vary: Origin\r\n"
        ));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

fn cors_allow<'a>(expected: Option<&'a str>, origin: Option<&str>) -> Option<&'a str> {
    match (expected, origin) {
        (Some(exp), Some(got)) if got == exp => Some(exp),
        _ => None,
    }
}

fn wait_for_code(
    listener: &TcpListener,
    state: &str,
    expected_path: &str,
    cors_origin: Option<&str>,
) -> Result<String> {
    for stream in listener.incoming() {
        let mut stream = stream?;
        let req = match read_http_request(&stream) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let allow = cors_allow(cors_origin, req.origin.as_deref());
        if req.method == "OPTIONS" {
            write_http(&mut stream, 204, "No Content", allow, "text/plain", "");
            continue;
        }
        if req.method != "GET" || !req.path.starts_with(expected_path) {
            write_http(&mut stream, 404, "Not Found", allow, "text/plain", "");
            continue;
        }
        let url = Url::parse(&format!("http://localhost{}", req.path))?;
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
        write_http(&mut stream, 200, "OK", allow, "text/html", body);
        if got_state.as_deref() != Some(state) {
            bail!("OAuth state mismatch");
        }
        return code.context("callback did not contain an authorization code");
    }
    bail!("callback listener closed unexpectedly")
}

/// Valid access token for `provider`, refreshing if it expires within 5 minutes.
pub fn access(provider: &str) -> Result<Tokens> {
    let mut tokens = load(provider)?;
    if tokens.expires_at > now() + 300 {
        return Ok(tokens);
    }
    match provider {
        OPENAI_CHATGPT => refresh_openai(&mut tokens)?,
        XAI_GROK => refresh_xai(&mut tokens)?,
        other => bail!("unknown provider {other:?}"),
    }
    save(provider, &tokens)?;
    Ok(tokens)
}

fn refresh_openai(tokens: &mut Tokens) -> Result<()> {
    let resp: TokenResponse = reqwest::blocking::Client::new()
        .post(format!("{OPENAI_ISSUER}/oauth/token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &tokens.refresh_token),
            ("client_id", OPENAI_CLIENT_ID),
            ("scope", OPENAI_SCOPE),
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
    Ok(())
}

fn refresh_xai(tokens: &mut Tokens) -> Result<()> {
    let resp: TokenResponse = reqwest::blocking::Client::new()
        .post(format!("{XAI_ISSUER}/oauth2/token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &tokens.refresh_token),
            ("client_id", XAI_CLIENT_ID),
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
    Ok(())
}

pub fn logout(provider: &str) -> Result<()> {
    match provider {
        OPENAI_CHATGPT | XAI_GROK => {}
        other => bail!("unknown provider {other:?} (supported: {OPENAI_CHATGPT}, {XAI_GROK})"),
    }
    let path = auth_path(provider)?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    eprintln!("Logged out ({provider}).");
    Ok(())
}

pub fn status() -> Result<()> {
    let current = Config::load()
        .map(|c| c.provider)
        .unwrap_or_else(|_| OPENAI_CHATGPT.into());
    let mut any = false;
    for provider in [OPENAI_CHATGPT, XAI_GROK] {
        match load(provider) {
            Ok(t) => {
                any = true;
                let left = t.expires_at.saturating_sub(now());
                let mark = if provider == current { " (active)" } else { "" };
                let validity = if left == 0 {
                    "expired (will refresh on next use)".to_string()
                } else {
                    format!("valid for {}m", left / 60)
                };
                if provider == OPENAI_CHATGPT {
                    eprintln!(
                        "{provider}{mark}: logged in (account {}), access token {validity}",
                        t.account_id
                    );
                } else {
                    eprintln!("{provider}{mark}: logged in, access token {validity}");
                }
            }
            Err(_) => eprintln!("{provider}: not logged in"),
        }
    }
    if !any {
        let hint = if current == XAI_GROK {
            format!("shpell auth login {XAI_GROK}")
        } else {
            "shpell auth login".into()
        };
        eprintln!("Run `{hint}`.");
    }
    Ok(())
}
