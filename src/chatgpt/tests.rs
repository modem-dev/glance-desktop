use super::*;
use std::{io::Cursor, os::unix::fs::PermissionsExt};
fn attempt(client: Option<&str>) -> oauth::Attempt {
    oauth::Attempt {
        state: "state".into(),
        nonce: "nonce".into(),
        verifier: "verifier".into(),
        redirect: "http://127.0.0.1:54321/auth/callback".into(),
        client_id: client.map(str::to_owned),
    }
}
#[test]
fn authorization_uses_stable_host_pkce_and_exact_loopback_callback() {
    let attempt = attempt(None);
    let url = reqwest::Url::parse(
        &attempt.authorize_url("urn:uuid:67c104e2-f917-4ee1-b8f0-e5b8c24896f4", None),
    )
    .unwrap();
    let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(url.host_str(), Some("auth.openai.com"));
    assert_eq!(params["redirect_uri"], attempt.redirect);
    assert_eq!(params["client_id"], "dynamic_agent_client");
    assert_eq!(
        params["ext_agent_host_id"],
        "urn:uuid:67c104e2-f917-4ee1-b8f0-e5b8c24896f4"
    );
    assert_eq!(params["agent_name_hint"], "Glance");
    assert_eq!(
        params["code_challenge"],
        URL_SAFE_NO_PAD.encode(Sha256::digest(b"verifier"))
    );
    assert_eq!(params["resource"], RESOURCE);
    assert_eq!(params["scope"], SCOPES);
    assert_eq!(params["nonce"], "nonce");
    let returning = attempt_client("oaiapp_returning");
    let params: std::collections::HashMap<_, _> = reqwest::Url::parse(
        &returning.authorize_url("urn:uuid:67c104e2-f917-4ee1-b8f0-e5b8c24896f4", None),
    )
    .unwrap()
    .query_pairs()
    .into_owned()
    .collect();
    assert_eq!(params["client_id"], "oaiapp_returning");
    assert!(!params.contains_key("agent_name_hint"));
}
fn attempt_client(client: &str) -> oauth::Attempt {
    attempt(Some(client))
}
#[test]
fn callback_rejects_wrong_state_path_duplicate_parameters_and_client_replacement() {
    let new = attempt(None);
    let good = "GET /auth/callback?state=state&code=synthetic&client_id=oaiapp_new HTTP/1.1";
    let result = new.parse_callback(good).unwrap();
    assert_eq!(result.client_id, "oaiapp_new");
    assert_eq!(result.code, "synthetic");
    for bad in [
        "GET /callback?state=state&code=synthetic&client_id=oaiapp_new HTTP/1.1",
        "GET /auth/callback?state=wrong&code=synthetic&client_id=oaiapp_new HTTP/1.1",
        "GET /auth/callback?state=state&state=state&code=synthetic&client_id=oaiapp_new HTTP/1.1",
        "GET /auth/callback?state=state&code=synthetic HTTP/1.1",
        "GET /auth/callback?state=state&code=synthetic&client_id=dynamic_agent_client HTTP/1.1",
        "GET /auth/callback?state=state&error=access_denied&client_id=oaiapp_new HTTP/1.1",
    ] {
        assert!(new.parse_callback(bad).is_err(), "{bad}");
    }
    let returning = attempt(Some("oaiapp_saved"));
    assert!(returning.parse_callback(good).is_err());
    assert_eq!(
        returning
            .parse_callback("GET /auth/callback?state=state&code=synthetic HTTP/1.1")
            .unwrap()
            .client_id,
        "oaiapp_saved"
    );
}
#[test]
fn signed_identity_checks_signature_issuer_audience_expiry_and_nonce() {
    // Public key and signatures generated solely for these fictional claims; no private key is committed.
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("synthetic-identity.json")).unwrap();
    let jwks: jsonwebtoken::jwk::JwkSet = serde_json::from_value(fixture["jwks"].clone()).unwrap();
    assert!(
        oauth::validate_identity(
            fixture["valid"].as_str().unwrap(),
            &jwks,
            "oaiapp_synthetic",
            "synthetic-nonce"
        )
        .is_ok()
    );
    for key in [
        "expired",
        "wrong_issuer",
        "wrong_audience",
        "wrong_nonce",
        "malformed_nbf",
        "future_nbf",
    ] {
        assert!(
            oauth::validate_identity(
                fixture[key].as_str().unwrap(),
                &jwks,
                "oaiapp_synthetic",
                "synthetic-nonce"
            )
            .is_err(),
            "{key}"
        );
    }
    let valid = fixture["valid"].as_str().unwrap();
    let (claims, _) = valid.rsplit_once('.').unwrap();
    assert!(
        oauth::validate_identity(
            &format!("{claims}.{}", URL_SAFE_NO_PAD.encode([0u8; 256])),
            &jwks,
            "oaiapp_synthetic",
            "synthetic-nonce"
        )
        .is_err()
    );
    assert!(
        oauth::validate_identity(
            valid,
            &jsonwebtoken::jwk::JwkSet { keys: vec![] },
            "oaiapp_synthetic",
            "synthetic-nonce"
        )
        .is_err()
    );
}
fn account() -> Account {
    Account {
        id: "synthetic-account".into(),
        subject: "synthetic-user".into(),
        client_id: "oaiapp_synthetic".into(),
        email: Some("demo@example.invalid".into()),
        tokens: Some(Tokens {
            access_token: "synthetic-access".into(),
            refresh_token: Some("synthetic-refresh".into()),
            id_token: Some("synthetic-id".into()),
            expires_at: now() + 3600,
            scopes: vec!["chatgpt.tokens.use.direct".into()],
        }),
        models: vec![Model {
            slug: "synthetic-model".into(),
            display_name: "Synthetic model".into(),
        }],
        model: Some("synthetic-model".into()),
        welcomed: false,
    }
}
#[test]
fn private_atomic_storage_preserves_host_account_and_choice_and_locks_refreshes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chatgpt");
    let mut first = Service::new(Some(path.clone()));
    first.load().unwrap();
    let host = first.store.as_ref().unwrap().host_id.clone();
    assert!(host.starts_with("urn:uuid:"));
    let uuid = host.strip_prefix("urn:uuid:").unwrap();
    assert_eq!(uuid.len(), 36);
    assert_eq!(&uuid[14..15], "4");
    assert!(matches!(&uuid[19..20], "8" | "9" | "a" | "b"));
    assert_eq!(uuid.chars().filter(|&c| c == '-').count(), 4);
    first.store.as_mut().unwrap().accounts.push(account());
    first.store.as_mut().unwrap().active = Some("synthetic-account".into());
    first.set_engine(OcrEngine::Chatgpt).unwrap();
    first.dismiss_welcome().unwrap();
    assert_eq!(
        std::fs::metadata(path.join("accounts.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(Service::new(Some(path.clone())).load().is_err());
    drop(first);
    let mut second = Service::new(Some(path));
    let snapshot = second.snapshot().unwrap();
    assert_eq!(second.store.as_ref().unwrap().host_id, host);
    assert_eq!(snapshot.ocr_engine, OcrEngine::Chatgpt);
    assert!(!snapshot.welcome_pending);
    assert!(snapshot.can_infer());
    let json = serde_json::to_string(&snapshot).unwrap();
    for secret in [
        "synthetic-access",
        "synthetic-refresh",
        "synthetic-id",
        "access_token",
        "refresh_token",
        "id_token",
        "subject",
        "client_id",
    ] {
        assert!(!json.contains(secret));
    }
}
#[test]
fn storage_rejects_symlinks_and_permissive_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chatgpt");
    let mut service = Service::new(Some(path.clone()));
    service.load().unwrap();
    drop(service);
    std::fs::set_permissions(
        path.join("accounts.json"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(Service::new(Some(path.clone())).load().is_err());
    std::fs::remove_file(path.join("accounts.json")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("missing"), path.join("accounts.json")).unwrap();
    assert!(Service::new(Some(path)).load().is_err());
}
#[test]
fn refreshed_tokens_rotate_credentials_together_and_require_plan_scope() {
    let previous = account().tokens.unwrap();
    let token = TokenResponse {
        access_token: "new-access".into(),
        refresh_token: Some("new-refresh".into()),
        id_token: None,
        token_type: "Bearer".into(),
        expires_in: 3600,
        scope: None,
    }
    .into_tokens(Some(&previous))
    .unwrap();
    assert_eq!(token.refresh_token.as_deref(), Some("new-refresh"));
    assert_eq!(token.id_token, previous.id_token);
    assert!(plan_enabled(&token));
    let no_plan = TokenResponse {
        access_token: "access".into(),
        refresh_token: None,
        id_token: None,
        token_type: "Bearer".into(),
        expires_in: 3600,
        scope: Some("openid email".into()),
    }
    .into_tokens(None)
    .unwrap();
    assert!(!plan_enabled(&no_plan));
    let mut service = Service::new(None);
    service.load().unwrap();
    assert!(service.set_engine(OcrEngine::Chatgpt).is_err());
    assert!(service.set_model("not-in-catalog").is_err());
}
fn event(value: serde_json::Value) -> String {
    format!("data: {value}\n\n")
}
#[test]
fn streaming_requires_completion_and_preserves_unicode_lines() {
    let stream =
        event(serde_json::json!({"type":"response.output_text.delta","delta":"First line\n"}))
            + &event(serde_json::json!({"type":"response.output_text.delta","delta":"第二行 👋"}))
            + &event(
                serde_json::json!({"type":"response.completed","response":{"status":"completed"}}),
            );
    assert_eq!(
        inference::stream_text(Cursor::new(stream)).unwrap(),
        "First line\n第二行 👋"
    );
    let partial = event(serde_json::json!({"type":"response.output_text.delta","delta":"partial"}));
    assert!(inference::stream_text(Cursor::new(partial.clone())).is_err());
    for ending in ["response.failed", "response.incomplete", "error"] {
        let stream = partial.clone()
            + &event(
                serde_json::json!({"type":ending,"response":{"error":{"code":"subscription_sharing_usage_limit_exceeded"}}}),
            );
        assert!(
            inference::stream_text(Cursor::new(stream))
                .unwrap_err()
                .contains("Manage usage")
        );
    }
    let empty = event(serde_json::json!({"type":"response.completed"}));
    assert_eq!(inference::stream_text(Cursor::new(empty)).unwrap(), "");
    let too_big =
        event(serde_json::json!({"type":"response.output_text.delta","delta":"中".repeat(30_000)}));
    assert!(inference::stream_text(Cursor::new(too_big)).is_err());
    assert!(
        inference::stream_text(Cursor::new(format!("data: {}", "x".repeat(MAX_JSON + 1)))).is_err()
    );
}
#[test]
fn image_request_uses_subscription_contract_without_unsupported_parameters() {
    let body = inference::request_body("synthetic-model", "data:image/png;base64,synthetic".into());
    assert_eq!(body["store"], false);
    assert_eq!(body["stream"], true);
    assert!(body["input"].is_array());
    assert_eq!(body["input"][0]["content"][0]["type"], "input_image");
    assert!(body.get("max_output_tokens").is_none());
    assert!(body.get("previous_response_id").is_none());
}

#[test]
fn canceled_stream_cannot_return_text_even_when_completion_is_queued() {
    let stream =
        event(serde_json::json!({"type":"response.output_text.delta","delta":"Must not copy"}))
            + &event(serde_json::json!({"type":"response.completed"}));
    assert!(
        inference::consume_stream(Cursor::new(stream), &AtomicBool::new(true))
            .unwrap_err()
            .contains("canceled")
    );
}

#[test]
fn account_without_plan_permission_can_be_selected_for_reconnection() {
    let mut service = Service::new(None);
    service.load().unwrap();
    let mut identity_only = account();
    identity_only.tokens.as_mut().unwrap().scopes = vec!["openid".into()];
    identity_only.models.clear();
    identity_only.model = None;
    service.store.as_mut().unwrap().accounts.push(identity_only);
    service.select_account("synthetic-account").unwrap();
    let snapshot = service.snapshot().unwrap();
    assert_eq!(
        snapshot.active_account.as_deref(),
        Some("synthetic-account")
    );
    assert!(snapshot.accounts[0].signed_in);
    assert!(!snapshot.can_infer());
    assert!(service.set_engine(OcrEngine::Chatgpt).is_err());
}
