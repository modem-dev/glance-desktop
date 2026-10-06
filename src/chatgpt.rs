//! Opt-in ChatGPT plan integration. Secrets stay in the worker-owned service.
mod inference;
mod oauth;
mod storage;
#[cfg(test)]
mod tests;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::PathBuf,
    sync::atomic::AtomicBool,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const ISSUER: &str = "https://auth.openai.com";
const AUTHORIZE: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN: &str = "https://auth.openai.com/api/accounts/oauth/token";
const RESOURCE: &str = "https://api.openai.com/v1";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const MAX_JSON: usize = 1_048_576;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OcrEngine {
    #[default]
    Local,
    Chatgpt,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct Model {
    pub slug: String,
    pub display_name: String,
}
#[derive(Clone, Serialize)]
pub(crate) struct AccountInfo {
    pub id: String,
    pub label: String,
    pub signed_in: bool,
    pub plan_enabled: bool,
}
#[derive(Clone, Default, Serialize)]
pub(crate) struct Snapshot {
    pub accounts: Vec<AccountInfo>,
    pub active_account: Option<String>,
    pub models: Vec<Model>,
    pub model: Option<String>,
    pub ocr_engine: OcrEngine,
    pub welcome_pending: bool,
}
impl Snapshot {
    pub fn can_infer(&self) -> bool {
        self.accounts
            .iter()
            .any(|a| Some(&a.id) == self.active_account.as_ref() && a.signed_in && a.plan_enabled)
            && self.model.is_some()
    }
}
// Deliberately no Debug implementations: a worker panic must not print credentials.
#[derive(Default, Serialize, Deserialize)]
struct Store {
    host_id: String,
    #[serde(default)]
    pending_client_id: Option<String>,
    accounts: Vec<Account>,
    active: Option<String>,
    #[serde(default)]
    engine: OcrEngine,
}
#[derive(Clone, Serialize, Deserialize)]
struct Account {
    id: String,
    subject: String,
    client_id: String,
    email: Option<String>,
    #[serde(default)]
    tokens: Option<Tokens>,
    #[serde(default)]
    models: Vec<Model>,
    model: Option<String>,
    #[serde(default)]
    welcomed: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_at: u64,
    scopes: Vec<String>,
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    token_type: String,
    expires_in: u64,
    #[serde(default)]
    scope: Option<String>,
}
impl TokenResponse {
    fn into_tokens(self, previous: Option<&Tokens>) -> Result<Tokens, String> {
        if !self.token_type.eq_ignore_ascii_case("bearer")
            || self.access_token.is_empty()
            || self.access_token.len() > 32_768
            || self.expires_in == 0
            || self.expires_in > 604_800
        {
            return Err("OpenAI returned an invalid token response".into());
        }
        let scopes = match self.scope {
            Some(scope) => scope.split_whitespace().map(str::to_owned).collect(),
            None => previous.map_or_else(Vec::new, |p| p.scopes.clone()),
        };
        Ok(Tokens {
            access_token: self.access_token,
            refresh_token: self
                .refresh_token
                .or_else(|| previous.and_then(|p| p.refresh_token.clone())),
            id_token: self
                .id_token
                .or_else(|| previous.and_then(|p| p.id_token.clone())),
            expires_at: now().saturating_add(self.expires_in),
            scopes,
        })
    }
}
pub(crate) struct Service {
    directory: Option<PathBuf>,
    store: Option<Store>,
    lock: Option<std::fs::File>,
}
impl Service {
    pub fn new(directory: Option<PathBuf>) -> Self {
        Self {
            directory,
            store: None,
            lock: None,
        }
    }
    pub fn default_directory() -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            std::env::var_os("HOME")
                .map(|p| PathBuf::from(p).join("Library/Application Support/Glance/chatgpt"))
        }
        #[cfg(target_os = "linux")]
        {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
                .map(|p| p.join("glance/chatgpt"))
        }
    }
    pub fn snapshot(&mut self) -> Result<Snapshot, String> {
        self.load()?;
        let store = self.store.as_ref().unwrap();
        let active = store
            .accounts
            .iter()
            .find(|a| Some(&a.id) == store.active.as_ref());
        Ok(Snapshot {
            accounts: store
                .accounts
                .iter()
                .map(|a| AccountInfo {
                    id: a.id.clone(),
                    label: format!(
                        "{} · {}",
                        a.email.as_deref().unwrap_or("ChatGPT account"),
                        &a.id[..8.min(a.id.len())]
                    ),
                    signed_in: a.tokens.is_some(),
                    plan_enabled: a.tokens.as_ref().is_some_and(plan_enabled),
                })
                .collect(),
            active_account: store.active.clone(),
            models: active.map_or_else(Vec::new, |a| a.models.clone()),
            model: active.and_then(|a| a.model.clone()),
            ocr_engine: store.engine,
            welcome_pending: active
                .is_some_and(|a| !a.welcomed && a.tokens.as_ref().is_some_and(plan_enabled)),
        })
    }
    pub fn select_account(&mut self, id: &str) -> Result<(), String> {
        self.load()?;
        let index = self
            .store
            .as_ref()
            .unwrap()
            .accounts
            .iter()
            .position(|a| a.id == id)
            .ok_or("Unknown ChatGPT account")?;
        // Refresh the catalog for the selected registration before making it active.
        if self.store.as_ref().unwrap().accounts[index]
            .tokens
            .as_ref()
            .is_some_and(plan_enabled)
        {
            let http = http_client()?;
            let token = self.access_token(index, &http)?;
            let models = inference::list_models(&http, &token)?;
            let account = &mut self.store.as_mut().unwrap().accounts[index];
            if !models
                .iter()
                .any(|m| Some(&m.slug) == account.model.as_ref())
            {
                account.model = models.first().map(|m| m.slug.clone());
            }
            account.models = models;
        }
        self.store.as_mut().unwrap().active = Some(id.to_owned());
        self.save()
    }
    pub fn set_engine(&mut self, engine: OcrEngine) -> Result<(), String> {
        if engine == OcrEngine::Chatgpt && !self.snapshot()?.can_infer() {
            return Err("Sign in with ChatGPT and choose an available model first".into());
        }
        self.load()?;
        self.store.as_mut().unwrap().engine = engine;
        self.save()
    }
    pub fn set_model(&mut self, model: &str) -> Result<(), String> {
        self.load()?;
        let index = self.active_index()?;
        let account = &mut self.store.as_mut().unwrap().accounts[index];
        if !account.models.iter().any(|m| m.slug == model) {
            return Err("Choose a model available to this ChatGPT account".into());
        }
        account.model = Some(model.into());
        self.save()
    }
    pub fn dismiss_welcome(&mut self) -> Result<(), String> {
        self.load()?;
        let index = self.active_index()?;
        self.store.as_mut().unwrap().accounts[index].welcomed = true;
        self.save()
    }
    fn active_index(&self) -> Result<usize, String> {
        let store = self
            .store
            .as_ref()
            .ok_or("ChatGPT settings have not loaded")?;
        store
            .accounts
            .iter()
            .position(|a| Some(&a.id) == store.active.as_ref())
            .ok_or_else(|| "Continue with ChatGPT first".into())
    }
    fn access_token(&mut self, index: usize, http: &Client) -> Result<String, String> {
        let account = self.store.as_ref().unwrap().accounts[index].clone();
        let previous = account
            .tokens
            .as_ref()
            .ok_or("Continue with ChatGPT to reconnect this account")?;
        if !plan_enabled(previous) {
            return Err("ChatGPT plan use is not enabled. Reconnect this account and grant plan permission.".into());
        }
        if previous.expires_at <= now().saturating_add(60) {
            let refresh = previous
                .refresh_token
                .as_deref()
                .ok_or("ChatGPT sign-in expired. Reconnect this account.")?;
            let response = http
                .post(TOKEN)
                .form(&[
                    ("grant_type", "refresh_token"),
                    ("client_id", account.client_id.as_str()),
                    ("refresh_token", refresh),
                    ("resource", RESOURCE),
                ])
                .send()
                .map_err(|_| "Could not renew ChatGPT sign-in. Check your connection.")?;
            if !response.status().is_success() {
                let status = response.status();
                let mut body = Vec::new();
                response
                    .take(MAX_JSON as u64 + 1)
                    .read_to_end(&mut body)
                    .map_err(|_| "Could not renew ChatGPT sign-in. Try again.")?;
                let invalid_grant = body.len() <= MAX_JSON
                    && serde_json::from_slice::<serde_json::Value>(&body)
                        .ok()
                        .and_then(|v| v.get("error").and_then(|v| v.as_str()).map(str::to_owned))
                        .as_deref()
                        == Some("invalid_grant");
                if invalid_grant || status == reqwest::StatusCode::UNAUTHORIZED {
                    self.store.as_mut().unwrap().accounts[index].tokens = None;
                    self.save()?;
                    return Err("ChatGPT sign-in expired. Reconnect this account.".into());
                }
                return Err(format!(
                    "Could not renew ChatGPT sign-in (HTTP {}). Try again.",
                    status.as_u16()
                ));
            }
            let tokens = read_json::<TokenResponse>(response)?.into_tokens(Some(previous))?;
            self.store.as_mut().unwrap().accounts[index].tokens = Some(tokens);
            self.save()?;
        }
        let tokens = self.store.as_ref().unwrap().accounts[index]
            .tokens
            .as_ref()
            .unwrap();
        if !plan_enabled(tokens) {
            return Err(
                "ChatGPT plan permission is no longer enabled. Reconnect this account.".into(),
            );
        }
        Ok(tokens.access_token.clone())
    }
    pub fn recognize(
        &mut self,
        image: &image::RgbaImage,
        rectangle: [u32; 4],
        account_id: &str,
        model: &str,
        cancel: &AtomicBool,
    ) -> Result<String, String> {
        self.load()?;
        let index = self.active_index()?;
        let account = &self.store.as_ref().unwrap().accounts[index];
        if account.id != account_id || account.model.as_deref() != Some(model) {
            return Err("ChatGPT account or model changed. Copy as OCR again.".into());
        }
        let http = http_client()?;
        let token = self.access_token(index, &http)?;
        inference::recognize(&http, &token, model, image, rectangle, cancel)
    }
    pub fn sign_in(
        &mut self,
        account_id: Option<&str>,
        cancel: &AtomicBool,
        open: impl FnOnce(String),
    ) -> Result<(), String> {
        oauth::sign_in(self, account_id, cancel, open)
    }
    pub fn sign_out(&mut self) -> Result<bool, String> {
        oauth::sign_out(self)
    }
}
fn plan_enabled(tokens: &Tokens) -> bool {
    tokens
        .scopes
        .iter()
        .any(|s| s == "chatgpt.tokens.use.direct")
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn random() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
fn http_client() -> Result<Client, String> {
    http_with_timeout(Duration::from_secs(90))
}
fn http_with_timeout(timeout: Duration) -> Result<Client, String> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .user_agent("Glance/0.3 Sign-in-with-ChatGPT")
        .build()
        .map_err(|_| "Could not initialize ChatGPT connection".into())
}
fn read_json<T: serde::de::DeserializeOwned>(
    response: reqwest::blocking::Response,
) -> Result<T, String> {
    if !response.status().is_success() {
        return Err(format!(
            "OpenAI request failed (HTTP {}). Try again or reconnect your account.",
            response.status().as_u16()
        ));
    }
    let mut data = Vec::new();
    response
        .take(MAX_JSON as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|_| "OpenAI response was interrupted")?;
    if data.len() > MAX_JSON {
        return Err("OpenAI response was too large".into());
    }
    serde_json::from_slice(&data).map_err(|_| "OpenAI returned an invalid response".into())
}
