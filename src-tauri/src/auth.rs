// ──────────────────────────────────────────────────────────────
// auth.rs — Google OAuth2 flow with refresh token persistence
// ──────────────────────────────────────────────────────────────
//
// Flow:
//   1. First launch → open browser for consent → localhost:8080 callback
//      → exchange code → store refresh_token in auth.json
//   2. Subsequent launches → read auth.json → refresh access_token silently

use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointSet,
    RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
    basic::{BasicClient, BasicErrorResponseType, BasicTokenType},
    StandardErrorResponse, StandardRevocableToken, StandardTokenIntrospectionResponse,
    StandardTokenResponse, EmptyExtraTokenFields, EndpointNotSet,
    RevocationErrorResponseType,
};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;

// Concrete type for our configured client (auth_uri=Set, token_uri=Set)
type ConfiguredClient = oauth2::Client<
    StandardErrorResponse<BasicErrorResponseType>,
    StandardTokenResponse<EmptyExtraTokenFields, BasicTokenType>,
    StandardTokenIntrospectionResponse<EmptyExtraTokenFields, BasicTokenType>,
    StandardRevocableToken,
    StandardErrorResponse<RevocationErrorResponseType>,
    EndpointSet,    // auth_uri
    EndpointNotSet, // device_auth_uri
    EndpointNotSet, // introspection_uri
    EndpointNotSet, // revocation_uri
    EndpointSet,    // token_uri
>;

// ─── Persisted token file ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct StoredAuth {
    pub refresh_token: String,
    pub access_token: String,
    /// Unix timestamp (seconds) when access_token expires
    pub expires_at: i64,
}

/// Path to the auth.json file inside the AppLocalDataDir.
pub fn auth_file_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("auth.json")
}

pub fn load_stored_auth(data_dir: &std::path::Path) -> Option<StoredAuth> {
    let path = auth_file_path(data_dir);
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_stored_auth(data_dir: &std::path::Path, auth: &StoredAuth) -> Result<(), String> {
    let path = auth_file_path(data_dir);
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(auth).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

// ─── OAuth client builder ──────────────────────────────────────────────────

/// NOTE: For production, store these in env vars or a config file.
/// These are placeholder values — replace with your GCP credentials.
const GOOGLE_CLIENT_ID: &str = "486140904182-847nauv7ulua9e2ag0p90b9a9uqmj4fu.apps.googleusercontent.com";
const GOOGLE_CLIENT_SECRET: &str = "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";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REDIRECT_URI: &str = "http://localhost:8080";

fn build_oauth_client() -> Result<ConfiguredClient, String> {
    let client = BasicClient::new(ClientId::new(GOOGLE_CLIENT_ID.to_string()))
        .set_client_secret(ClientSecret::new(GOOGLE_CLIENT_SECRET.to_string()))
        .set_auth_uri(AuthUrl::new(AUTH_URL.to_string()).map_err(|e| e.to_string())?)
        .set_token_uri(TokenUrl::new(TOKEN_URL.to_string()).map_err(|e| e.to_string())?)
        .set_redirect_uri(
            RedirectUrl::new(REDIRECT_URI.to_string()).map_err(|e| e.to_string())?,
        );
    Ok(client)
}

/// Build the reqwest HTTP client used for OAuth token requests.
fn build_http_client() -> Result<oauth2::reqwest::Client, String> {
    oauth2::reqwest::ClientBuilder::new()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

// ─── Initial auth (browser-based) ─────────────────────────────────────────

/// Opens the default browser for Google consent.  Blocks until the user
/// completes the flow and the localhost redirect is captured.
pub async fn run_initial_auth_flow(data_dir: &std::path::Path) -> Result<StoredAuth, String> {
    let client = build_oauth_client()?;
    let http_client = build_http_client()?;

    let (auth_url, _csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new(
            "https://www.googleapis.com/auth/spreadsheets.readonly".to_string(),
        ))
        .add_extra_param("access_type", "offline")
        .add_extra_param("prompt", "consent")
        .url();

    // Open the browser
    let url_str: String = auth_url.to_string();
    let _ = open::that(&url_str);
    log::info!("Opened browser for Google sign-in");

    // Spin up a one-shot TCP listener on port 8080 to capture the redirect
    let code = capture_redirect_code().map_err(|e| format!("Redirect capture failed: {}", e))?;

    // Exchange the authorisation code for tokens
    let token_result = client
        .exchange_code(AuthorizationCode::new(code))
        .request_async(&http_client)
        .await
        .map_err(|e| format!("Token exchange failed: {}", e))?;

    let access_token: String = token_result.access_token().secret().to_string();
    let refresh_token: String = token_result
        .refresh_token()
        .ok_or_else(|| "Google did not return a refresh token".to_string())?
        .secret()
        .to_string();

    let expires_in = token_result
        .expires_in()
        .unwrap_or(std::time::Duration::from_secs(3600));

    let stored = StoredAuth {
        refresh_token,
        access_token,
        expires_at: chrono::Utc::now().timestamp() + expires_in.as_secs() as i64,
    };

    save_stored_auth(data_dir, &stored)?;

    Ok(stored)
}

/// Binds to localhost:8080, waits for the redirect, parses the `code` query
/// param, sends a minimal HTML response, then closes the listener.
fn capture_redirect_code() -> Result<String, String> {
    let listener =
        TcpListener::bind("127.0.0.1:8080").map_err(|e| format!("Bind failed: {}", e))?;

    log::info!("Waiting for OAuth redirect on http://127.0.0.1:8080 …");

    let (mut stream, _) = listener.accept().map_err(|e| format!("Accept failed: {}", e))?;

    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|e| format!("Read failed: {}", e))?;

    // Parse code from "GET /?code=XYZ&scope=...  HTTP/1.1"
    let code = request_line
        .split_whitespace()
        .nth(1) // the path part
        .and_then(|path| url::Url::parse(&format!("http://localhost{}", path)).ok())
        .and_then(|url| {
            url.query_pairs()
                .find(|(k, _)| k == "code")
                .map(|(_, v)| v.to_string())
        })
        .ok_or_else(|| "No code parameter in redirect URL".to_string())?;

    // Respond to the browser
    let body = r#"<!DOCTYPE html>
<html><body style="font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#1a1713;color:#e8e4de">
<div style="text-align:center"><h2>✅ Authenticated!</h2><p>You can close this tab and return to ProgressLens.</p></div>
</body></html>"#;

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );

    stream
        .write_all(response.as_bytes())
        .map_err(|e| format!("Write failed: {}", e))?;

    Ok(code)
}

// ─── Refresh token silently ────────────────────────────────────────────────

/// Uses a stored refresh token to get a fresh access token without opening
/// the browser.  Updates auth.json on success.
pub async fn refresh_access_token(data_dir: &std::path::Path) -> Result<StoredAuth, String> {
    let mut stored = load_stored_auth(data_dir)
        .ok_or_else(|| "No stored auth found — run initial auth first".to_string())?;

    // If token hasn't expired yet (with 60 s buffer), reuse it
    let now = chrono::Utc::now().timestamp();
    if stored.expires_at > now + 60 {
        return Ok(stored);
    }

    let client = build_oauth_client()?;
    let http_client = build_http_client()?;

    let token_result = client
        .exchange_refresh_token(&RefreshToken::new(stored.refresh_token.clone()))
        .request_async(&http_client)
        .await
        .map_err(|e| format!("Refresh failed: {}", e))?;

    stored.access_token = token_result.access_token().secret().to_string();
    stored.expires_at = chrono::Utc::now().timestamp()
        + token_result
            .expires_in()
            .unwrap_or(std::time::Duration::from_secs(3600))
            .as_secs() as i64;

    // If Google rotated the refresh token, update it
    if let Some(rt) = token_result.refresh_token() {
        let new_rt: String = rt.secret().to_string();
        stored.refresh_token = new_rt;
    }

    save_stored_auth(data_dir, &stored)?;

    Ok(stored)
}

// ─── Public helper ─────────────────────────────────────────────────────────

/// Returns a valid access token — refreshing or running the initial flow
/// as needed.
pub async fn get_access_token(data_dir: &std::path::Path) -> Result<String, String> {
    if load_stored_auth(data_dir).is_some() {
        let auth = refresh_access_token(data_dir).await?;
        Ok(auth.access_token)
    } else {
        let auth = run_initial_auth_flow(data_dir).await?;
        Ok(auth.access_token)
    }
}
