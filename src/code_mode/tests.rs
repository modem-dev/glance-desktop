use super::*;
use incurs_codemode::{ArtifactStore, ExecutionStatus};
use serde_json::json;

async fn terminal(service: &dyn CodeModeService, id: String) -> ExecutionState {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let state = service.execution(id.clone()).await.unwrap();
            if !matches!(state.status, ExecutionStatus::Running) {
                return state;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn sandbox_executes_async_programs_and_preserves_history_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    let runtime = service(&path).unwrap();
    let started = runtime
        .execute(
            "return await Promise.resolve({answer: 42});".into(),
            Default::default(),
        )
        .await
        .unwrap();
    let state = terminal(&runtime, started.id.clone()).await;
    assert_eq!(state.status, ExecutionStatus::Completed);
    assert_eq!(state.result, Some(json!({"answer":42})));
    let reopened = service(&path).unwrap();
    let saved = reopened.execution(started.id).await.unwrap();
    assert_eq!(saved.result, Some(json!({"answer":42})));
}

#[tokio::test]
async fn search_describes_every_editor_method_and_cancellation_is_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let service = service(&dir.path().join("history.sqlite3")).unwrap();
    for tool in crate::mcp::tools() {
        let name = tool["name"].as_str().unwrap();
        let found = serde_json::to_string(&service.search(name.into()).await.unwrap()).unwrap();
        assert!(found.contains(name), "{name}: {found}");
    }
    let started = service
        .execute("while (true) {}".into(), Default::default())
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(
        service.execution(started.id.clone()).await.unwrap().status,
        ExecutionStatus::Running
    );
    let cancelled = service.cancel(started.id.clone()).await.unwrap();
    assert_eq!(cancelled.status, ExecutionStatus::Cancelled);
    let state = terminal(&service, started.id).await;
    assert_eq!(state.status, ExecutionStatus::Cancelled);
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        service.search("get_document".into()),
    )
    .await
    .expect("CPU-bound sandbox must stop after cancellation")
    .unwrap();
}

#[tokio::test]
async fn artifact_ownership_and_persistence_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    let db = store::Store::open(&path).unwrap();
    let artifact = db
        .put("owner", &json!({"payload":"x".repeat(100_000)}))
        .await
        .unwrap();
    assert!(db.get("intruder", &artifact.id).await.unwrap().is_none());
    drop(db);
    let db = store::Store::open(&path).unwrap();
    assert_eq!(
        db.get("owner", &artifact.id).await.unwrap().unwrap()["payload"]
            .as_str()
            .unwrap()
            .len(),
        100_000
    );
    ArtifactStore::delete_execution(&db, "owner").await.unwrap();
    assert!(db.get("owner", &artifact.id).await.unwrap().is_none());
}

#[tokio::test]
async fn malformed_or_oversized_wire_messages_are_bounded() {
    let mut reader = BufReader::new(&b"{bad}\n"[..]);
    let bytes = read_message(&mut reader).await.unwrap();
    assert!(serde_json::from_slice::<Request>(&bytes).is_err());
    let bytes = vec![b'x'; MAX_REQUEST + 1];
    assert!(
        read_message(&mut BufReader::new(bytes.as_slice()))
            .await
            .unwrap_err()
            .contains("too large")
    );
}

#[test]
fn code_mcp_exposes_all_five_lifecycle_tools() {
    let server = incurs_codemode_mcp::CodeModeMcpServer::new(Arc::new(Client));
    let names: Vec<_> = server.tools().iter().map(|t| t.name.as_ref()).collect();
    assert_eq!(names, incurs_codemode_mcp::TOOL_NAMES);
}

#[tokio::test]
async fn real_code_mcp_transport_negotiates_lists_searches_and_executes() {
    use rmcp::{
        ServiceExt,
        model::{CallToolRequestParams, ClientInfo},
    };
    let dir = tempfile::tempdir().unwrap();
    let runtime = Arc::new(service(&dir.path().join("history.sqlite3")).unwrap());
    let server = incurs_codemode_mcp::CodeModeMcpServer::new(runtime);
    let (server_io, client_io) = tokio::io::duplex(1024 * 1024);
    let task = tokio::spawn(async move {
        server
            .serve(server_io)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let client = ClientInfo::default().serve(client_io).await.unwrap();
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 5);
    let search = client
        .call_tool(
            CallToolRequestParams::new("codemode_search")
                .with_arguments(json!({"query":"resize_image"}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert!(
        serde_json::to_string(&search)
            .unwrap()
            .contains("resize_image")
    );
    let started = client
        .call_tool(
            CallToolRequestParams::new("codemode_execute").with_arguments(
                json!({"code":"return await Promise.resolve({wire: 73});"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    let id = started.structured_content.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let state = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let response = client
                .call_tool(
                    CallToolRequestParams::new("codemode_execution")
                        .with_arguments(json!({"id":id}).as_object().unwrap().clone()),
                )
                .await
                .unwrap();
            let state = response.structured_content.unwrap();
            if state["status"] != "running" {
                break state;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(state["status"], "completed");
    assert_eq!(state["result"], json!({"wire":73}));
    client.cancel().await.unwrap();
    task.await.unwrap();
}

#[tokio::test]
async fn approvals_replay_revision_scoped_edits_without_refreshing_prior_reads() {
    use incurs_codemode::{LogEntryState, MemoryStore};
    use std::sync::atomic::{AtomicU64, Ordering};
    let revision = Arc::new(AtomicU64::new(0));
    let writes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let counter = revision.clone();
    let seen = writes.clone();
    let catalog = crate::cli::editor_cli_with(Arc::new(move |name, args| {
        if name == "get_document" {
            return Ok(json!({"revision":counter.load(Ordering::SeqCst)}));
        }
        if name == "add_annotation" {
            let current = counter.load(Ordering::SeqCst);
            if args["expected_revision"] != current {
                return Err("Stale expected_revision".into());
            }
            seen.lock().unwrap().push(current);
            counter.fetch_add(1, Ordering::SeqCst);
            return Ok(json!({"revision":current+1}));
        }
        Err("Unexpected call".into())
    }))
    .tool_catalog();
    let runtime = LocalCodeModeService::spawn(move || {
        CodeMode::new(
            Arc::new(MemoryStore::default()),
            LocalExecutor::default(),
            vec![Arc::new(connector(catalog))],
        )
    })
    .unwrap();
    let started = runtime.execute(
        "const mark={tool:'rectangle',points:[[1,2],[10,20]],color:[255,0,0,255],width:2,text:''};
         const before=await glance.get_document({});
         await glance.add_annotation({mark,expected_revision:before.revision});
         const after=await glance.get_document({});
         await glance.add_annotation({mark,expected_revision:after.revision});
         return await glance.get_document({});".into(), Default::default()
    ).await.unwrap();
    for _ in 0..2 {
        let state = terminal(&runtime, started.id.clone()).await;
        assert_eq!(state.status, ExecutionStatus::Paused, "{state:?}");
        let seq = state
            .log
            .iter()
            .find(|entry| entry.state == LogEntryState::Pending)
            .unwrap()
            .seq;
        runtime
            .approve(started.id.clone(), seq, Default::default())
            .await
            .unwrap();
    }
    let state = terminal(&runtime, started.id).await;
    assert_eq!(state.status, ExecutionStatus::Completed, "{state:?}");
    assert_eq!(state.result, Some(json!({"revision":2})));
    assert_eq!(*writes.lock().unwrap(), vec![0, 1]);
    assert_eq!(revision.load(Ordering::SeqCst), 2);
}
