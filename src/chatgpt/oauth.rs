use super::*;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::atomic::Ordering,
    time::Instant,
};
#[derive(Deserialize)]
pub(super) struct Identity {
    sub: String,
    nonce: String,
    email: Option<String>,
}
pub(super) struct Attempt {
    pub(super) state: String,
    pub(super) nonce: String,
    pub(super) verifier: String,
    pub(super) redirect: String,
    pub(super) client_id: Option<String>,
}
pub(super) struct Callback {
    pub(super) code: String,
    pub(super) client_id: String,
}
impl Attempt {
    fn new(listener: &TcpListener, client_id: Option<String>) -> Result<Self, String> {
        Ok(Self {
            state: random(),
            nonce: random(),
            verifier: random(),
            redirect: format!(
                "http://127.0.0.1:{}/auth/callback",
                listener
                    .local_addr()
                    .map_err(|_| "Callback unavailable")?
                    .port()
            ),
            client_id,
        })
    }
    pub(super) fn authorize_url(&self, host_id: &str, account: Option<&Account>) -> String {
        let mut url = reqwest::Url::parse(AUTHORIZE).unwrap();
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes()));
        let mut query = url.query_pairs_mut();
        query.extend_pairs([
            (
                "client_id",
                self.client_id.as_deref().unwrap_or("dynamic_agent_client"),
            ),
            ("ext_agent_host_id", host_id),
            ("response_type", "code"),
            ("redirect_uri", &self.redirect),
            ("scope", SCOPES),
            ("resource", RESOURCE),
            ("state", &self.state),
            ("nonce", &self.nonce),
            ("code_challenge_method", "S256"),
            ("code_challenge", &challenge),
        ]);
        if self.client_id.is_none() {
            query.append_pair("agent_name_hint", "Glance");
        }
        if let Some(account) = account {
            if let Some(token) = account.tokens.as_ref().and_then(|t| t.id_token.as_ref()) {
                query.append_pair("id_token_hint", token);
            }
            if let Some(email) = &account.email {
                query.append_pair("login_hint", email);
            }
            if account.tokens.as_ref().is_some_and(|t| !plan_enabled(t)) {
                query.append_pair("prompt", "consent");
            }
        }
        drop(query);
        url.into()
    }
    pub(super) fn parse_callback(&self, request: &str) -> Result<Callback, String> {
        let mut parts = request.split_whitespace();
        if parts.next() != Some("GET") {
            return Err("Invalid callback method".into());
        }
        let target = parts.next().ok_or("Invalid callback")?;
        if !target.starts_with("/auth/callback?") {
            return Err("Invalid callback path".into());
        }
        let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}"))
            .map_err(|_| "Invalid callback")?;
        let mut parameters = std::collections::HashMap::new();
        for (key, value) in url.query_pairs() {
            if parameters
                .insert(key.into_owned(), value.into_owned())
                .is_some()
            {
                return Err("Duplicate callback parameter".into());
            }
        }
        if parameters.get("state") != Some(&self.state) {
            return Err("Invalid sign-in state".into());
        }
        if parameters.contains_key("error") {
            return Err("ChatGPT sign-in was declined".into());
        }
        let client_id = parameters
            .get("client_id")
            .cloned()
            .or_else(|| self.client_id.clone())
            .ok_or("OpenAI did not issue a client ID")?;
        if client_id == "dynamic_agent_client"
            || client_id.is_empty()
            || client_id.len() > 256
            || self
                .client_id
                .as_ref()
                .is_some_and(|expected| expected != &client_id)
        {
            return Err("Invalid sign-in client ID".into());
        }
        let code = parameters
            .remove("code")
            .filter(|c| !c.is_empty() && c.len() <= 8192)
            .ok_or("Missing authorization code")?;
        Ok(Callback { code, client_id })
    }
}
pub(super) fn sign_in(
    service: &mut Service,
    account_id: Option<&str>,
    cancel: &AtomicBool,
    open: impl FnOnce(String),
) -> Result<(), String> {
    service.load()?;
    let store = service.store.as_ref().unwrap();
    let previous = account_id
        .map(|id| {
            store
                .accounts
                .iter()
                .find(|a| a.id == id)
                .cloned()
                .ok_or("Unknown ChatGPT account")
        })
        .transpose()?;
    if previous.is_none() && store.accounts.len() >= 32 {
        return Err("Maximum 32 saved ChatGPT accounts".into());
    }
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|_| "Could not start the local ChatGPT sign-in callback")?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "Could not start ChatGPT sign-in")?;
    let attempt = Attempt::new(
        &listener,
        previous
            .as_ref()
            .map(|a| a.client_id.clone())
            .or_else(|| store.pending_client_id.clone()),
    )?;
    check_canceled(cancel)?;
    open(attempt.authorize_url(&store.host_id, previous.as_ref()));
    let started = Instant::now();
    let callback = loop {
        check_canceled(cancel)?;
        if started.elapsed() > Duration::from_secs(300) {
            return Err("ChatGPT sign-in timed out. Continue with ChatGPT to retry.".into());
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let mut line = String::new();
                let read = BufReader::new((&mut stream).take(16_385)).read_line(&mut line);
                if read.is_err() || line.len() > 16_384 {
                    continue;
                }
                match attempt.parse_callback(&line) {
                    Ok(callback) => {
                        reply(
                            &mut stream,
                            "200 OK",
                            "You can return to Glance to finish connecting your ChatGPT account.",
                        );
                        break callback;
                    }
                    Err(error) => {
                        reply(
                            &mut stream,
                            "400 Bad Request",
                            "Sign-in could not be completed. Return to Glance and try again.",
                        );
                        if error == "ChatGPT sign-in was declined" {
                            return Err(error);
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(30))
            }
            Err(_) => return Err("ChatGPT callback stopped unexpectedly".into()),
        }
    };
    check_canceled(cancel)?;
    if previous.is_none() {
        service.store.as_mut().unwrap().pending_client_id = Some(callback.client_id.clone());
        service.save()?;
    }
    let http = http_with_timeout(Duration::from_secs(20))?;
    let response = http
        .post(TOKEN)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", callback.client_id.as_str()),
            ("code", callback.code.as_str()),
            ("code_verifier", attempt.verifier.as_str()),
            ("redirect_uri", attempt.redirect.as_str()),
            ("resource", RESOURCE),
        ])
        .send()
        .map_err(|_| "ChatGPT sign-in exchange failed. Continue with ChatGPT to retry.")?;
    let tokens = read_json::<TokenResponse>(response)?.into_tokens(None)?;
    let id_token = tokens
        .id_token
        .as_deref()
        .ok_or("OpenAI did not return an identity token")?;
    let jwks = read_json::<JwkSet>(
        http.get(format!("{ISSUER}/.well-known/jwks.json"))
            .send()
            .map_err(|_| "Could not verify ChatGPT identity")?,
    )?;
    let identity = validate_identity(id_token, &jwks, &callback.client_id, &attempt.nonce)?;
    if previous.as_ref().is_some_and(|a| a.subject != identity.sub) {
        return Err("The signed-in ChatGPT account differs from the selected registration".into());
    }
    check_canceled(cancel)?;
    let id = URL_SAFE_NO_PAD.encode(Sha256::digest(
        format!("{ISSUER}\0{}\0{}", callback.client_id, identity.sub).as_bytes(),
    ));
    let models = if plan_enabled(&tokens) {
        inference::list_models(&http, &tokens.access_token)?
    } else {
        Vec::new()
    };
    check_canceled(cancel)?;
    let selected_model = previous
        .as_ref()
        .and_then(|a| a.model.clone())
        .filter(|slug| models.iter().any(|m| &m.slug == slug))
        .or_else(|| models.first().map(|m| m.slug.clone()));
    let account = Account {
        id: id.clone(),
        subject: identity.sub,
        client_id: callback.client_id,
        email: identity.email.filter(|s| s.len() <= 320),
        tokens: Some(tokens),
        models,
        model: selected_model,
        welcomed: previous.as_ref().is_some_and(|a| a.welcomed),
    };
    let store = service.store.as_mut().unwrap();
    if let Some(index) = store.accounts.iter().position(|a| a.id == id) {
        let welcomed = store.accounts[index].welcomed;
        store.accounts[index] = account;
        store.accounts[index].welcomed = welcomed;
    } else {
        store.accounts.push(account);
    }
    store.active = Some(id);
    store.pending_client_id = None;
    service.save()?;
    Ok(())
}
fn reply(stream: &mut std::net::TcpStream, status: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}
fn check_canceled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("ChatGPT sign-in canceled".into())
    } else {
        Ok(())
    }
}
pub(super) fn validate_identity(
    token: &str,
    jwks: &JwkSet,
    client_id: &str,
    nonce: &str,
) -> Result<Identity, String> {
    if token.len() > 32_768 {
        return Err("Invalid ChatGPT identity token".into());
    }
    let header = decode_header(token).map_err(|_| "Invalid ChatGPT identity token")?;
    if header.alg != Algorithm::RS256 {
        return Err("Unsupported ChatGPT identity signature".into());
    }
    let kid = header.kid.ok_or("ChatGPT identity has no signing key")?;
    let jwk = jwks
        .find(&kid)
        .ok_or("Unknown ChatGPT identity signing key")?;
    let key = DecodingKey::from_jwk(jwk).map_err(|_| "Invalid ChatGPT signing key")?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.leeway = 5;
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[client_id]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    validation.validate_nbf = true;
    let identity = decode::<Identity>(token, &key, &validation)
        .map_err(|_| "ChatGPT identity verification failed")?
        .claims;
    if identity.nonce != nonce || identity.sub.is_empty() || identity.sub.len() > 512 {
        return Err("ChatGPT identity does not match this sign-in attempt".into());
    }
    Ok(identity)
}
pub(super) fn sign_out(service: &mut Service) -> Result<bool, String> {
    service.load()?;
    let index = service.active_index()?;
    let account = service.store.as_ref().unwrap().accounts[index].clone();
    let confirmed = if let Some(refresh) = account
        .tokens
        .as_ref()
        .and_then(|t| t.refresh_token.as_ref())
    {
        http_with_timeout(Duration::from_secs(10))
            .and_then(|http| revoke(&http, &account.client_id, refresh))
            .is_ok()
    } else {
        account.tokens.is_none()
    };
    service.store.as_mut().unwrap().accounts[index].tokens = None;
    service.store.as_mut().unwrap().engine = OcrEngine::Local;
    service.save()?;
    Ok(confirmed)
}

fn revoke(http: &Client, client_id: &str, refresh: &str) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Discovery {
        revocation_endpoint: String,
    }
    let discovery = read_json::<Discovery>(
        http.get(format!("{ISSUER}/.well-known/openid-configuration"))
            .send()
            .map_err(|_| "Revocation discovery unavailable")?,
    )?;
    let url = reqwest::Url::parse(&discovery.revocation_endpoint)
        .map_err(|_| "Invalid revocation endpoint")?;
    if url.scheme() != "https"
        || url.host_str() != Some("auth.openai.com")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Untrusted revocation endpoint".into());
    }
    for attempt in 0..3 {
        match http
            .post(url.clone())
            .form(&[
                ("token", refresh),
                ("token_type_hint", "refresh_token"),
                ("client_id", client_id),
            ])
            .send()
        {
            Ok(response) if response.status() == reqwest::StatusCode::OK => return Ok(()),
            Ok(response) if !response.status().is_server_error() => {
                return Err("Revocation was not confirmed".into());
            }
            _ => std::thread::sleep(Duration::from_millis(100 * (1 << attempt))),
        }
    }
    Err("Revocation was not confirmed".into())
}
