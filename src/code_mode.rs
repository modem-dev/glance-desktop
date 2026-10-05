//! Explicit, private local Code Mode service shared by CLI and MCP clients.
use incurs::cli::Cli;
use incurs::command::{CommandDef, McpCommandOptions, TypedContext, TypedResult};
use incurs_codemode::{
    CodeMode, CodeModeRunOptions, CodeModeService, ExecutionState, IncurConnector, SearchOutput,
};
use incurs_codemode_local::{LocalCodeModeService, LocalExecutor};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};

mod store;

const MAX_REQUEST: usize = 24 * 1024 * 1024;

fn directory() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("GLANCE_CODE_DIR") {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err("GLANCE_CODE_DIR must be absolute".into());
        }
        return Ok(path);
    }
    Ok(crate::automation::directory()?.join("code"))
}

fn private_directory(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "snake_case")]
enum Request {
    Search { query: String },
    Execute { code: String },
    Execution { id: String },
    Artifact { id: String, artifact_id: String },
    Decide { id: String, seq: u64, approve: bool },
    Cancel { id: String },
}

pub(crate) struct Client;

impl Client {
    async fn call(&self, request: Request) -> Result<Value, String> {
        let stream = UnixStream::connect(directory()?.join("service.sock"))
            .await
            .map_err(|_| {
                "Code Mode service unavailable. Run Glance --cli code serve.".to_string()
            })?;
        let (read, mut write) = stream.into_split();
        let mut bytes = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_REQUEST {
            return Err("Code Mode request too large".into());
        }
        bytes.push(b'\n');
        write.write_all(&bytes).await.map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(read);
        let line = read_message(&mut reader).await?;
        let response: Value = serde_json::from_slice(&line).map_err(|e| e.to_string())?;
        if let Some(error) = response["error"].as_str() {
            Err(error.into())
        } else {
            Ok(response["result"].clone())
        }
    }
}

async fn read_message<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Vec<u8>, String> {
    // Bound the actual read; read_until alone can allocate without limit.
    let mut bytes = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(|e| e.to_string())?;
        if available.is_empty() {
            return Err("Code Mode service disconnected".into());
        }
        let count = available
            .iter()
            .position(|b| *b == b'\n')
            .map_or(available.len(), |i| i + 1);
        if bytes.len() + count > MAX_REQUEST {
            return Err("Code Mode message too large".into());
        }
        let complete = available[count - 1] == b'\n';
        bytes.extend_from_slice(&available[..count]);
        reader.consume(count);
        if complete {
            return Ok(bytes);
        }
    }
}

fn encode<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

#[async_trait::async_trait]
impl CodeModeService for Client {
    async fn search(&self, query: String) -> Result<SearchOutput, String> {
        serde_json::from_value(self.call(Request::Search { query }).await?)
            .map_err(|e| e.to_string())
    }
    async fn execute(&self, code: String, _: CodeModeRunOptions) -> Result<ExecutionState, String> {
        serde_json::from_value(self.call(Request::Execute { code }).await?)
            .map_err(|e| e.to_string())
    }
    async fn execution(&self, id: String) -> Result<ExecutionState, String> {
        serde_json::from_value(self.call(Request::Execution { id }).await?)
            .map_err(|e| e.to_string())
    }
    async fn artifact(&self, id: String, artifact_id: String) -> Result<Value, String> {
        self.call(Request::Artifact { id, artifact_id }).await
    }
    async fn approve(
        &self,
        id: String,
        seq: u64,
        _: CodeModeRunOptions,
    ) -> Result<ExecutionState, String> {
        serde_json::from_value(
            self.call(Request::Decide {
                id,
                seq,
                approve: true,
            })
            .await?,
        )
        .map_err(|e| e.to_string())
    }
    async fn reject(&self, id: String, seq: u64) -> Result<ExecutionState, String> {
        serde_json::from_value(
            self.call(Request::Decide {
                id,
                seq,
                approve: false,
            })
            .await?,
        )
        .map_err(|e| e.to_string())
    }
    async fn cancel(&self, id: String) -> Result<ExecutionState, String> {
        serde_json::from_value(self.call(Request::Cancel { id }).await?).map_err(|e| e.to_string())
    }
}

async fn dispatch(service: &dyn CodeModeService, request: Request) -> Result<Value, String> {
    match request {
        Request::Search { query } => encode(service.search(query).await?),
        Request::Execute { code } => encode(service.execute(code, Default::default()).await?),
        Request::Execution { id } => encode(service.execution(id).await?),
        Request::Artifact { id, artifact_id } => service.artifact(id, artifact_id).await,
        Request::Decide {
            id,
            seq,
            approve: true,
        } => encode(service.approve(id, seq, Default::default()).await?),
        Request::Decide {
            id,
            seq,
            approve: false,
        } => encode(service.reject(id, seq).await?),
        Request::Cancel { id } => encode(service.cancel(id).await?),
    }
}

struct EditorPolicy;

impl incurs_codemode::ToolPolicyResolver for EditorPolicy {
    fn resolve(
        &self,
        origin: incurs_codemode::ToolOrigin,
        annotations: &incurs_codemode::ToolAnnotations,
    ) -> incurs_codemode::ToolPolicy {
        let mut policy = incurs_codemode::ToolPolicyResolver::resolve(
            &incurs_codemode::DefaultToolPolicyResolver,
            origin,
            annotations,
        );
        policy.replay = incurs_codemode::ReplayPolicy::Log;
        policy
    }
}

fn connector(catalog: incurs::tool::ToolCatalog) -> IncurConnector {
    IncurConnector::new(catalog).with_policy_resolver(Arc::new(EditorPolicy)).with_name("glance").with_instructions(
        "Control the native editor. Read get_document before revision-scoped edits. Import replaces the document; other edits support undo. Read worker status after dispatch_action."
    )
}

pub(crate) struct Service {
    inner: LocalCodeModeService,
    runtime: Arc<incurs_codemode::CodeModeRuntime>,
}

#[async_trait::async_trait]
impl CodeModeService for Service {
    async fn search(&self, query: String) -> Result<SearchOutput, String> {
        self.inner.search(query).await
    }
    async fn execute(
        &self,
        code: String,
        options: CodeModeRunOptions,
    ) -> Result<ExecutionState, String> {
        self.inner.execute(code, options).await
    }
    async fn execution(&self, id: String) -> Result<ExecutionState, String> {
        self.runtime
            .execution_snapshot(&id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Execution not found".into())
    }
    async fn artifact(&self, id: String, artifact_id: String) -> Result<Value, String> {
        self.runtime
            .artifact(&id, &artifact_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Artifact not found for this execution".into())
    }
    async fn approve(
        &self,
        id: String,
        seq: u64,
        options: CodeModeRunOptions,
    ) -> Result<ExecutionState, String> {
        self.inner.approve(id, seq, options).await
    }
    async fn reject(&self, id: String, seq: u64) -> Result<ExecutionState, String> {
        self.inner.reject(id, seq).await
    }
    async fn cancel(&self, id: String) -> Result<ExecutionState, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis() as u64;
        self.runtime
            .cancel(&id, now)
            .await
            .map_err(|e| e.to_string())?;
        self.execution(id).await
    }
}

pub(crate) fn service(path: &std::path::Path) -> Result<Service, String> {
    let store = Arc::new(store::Store::open(path)?);
    store.recover_interrupted()?;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let inner = LocalCodeModeService::spawn(move || {
        let connector = connector(crate::cli::editor_cli().tool_catalog());
        let mode = CodeMode::with_artifact_store(
            store.clone(),
            store,
            LocalExecutor::new(incurs_codemode_local::LocalExecutorOptions {
                timeout: std::time::Duration::from_secs(1),
                ..Default::default()
            }),
            vec![Arc::new(connector)],
        );
        let _ = send.send(mode.runtime());
        mode
    })?;
    let runtime = receive.recv().map_err(|e| e.to_string())?;
    Ok(Service { inner, runtime })
}

pub(crate) async fn serve() -> Result<(), String> {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
    let dir = directory()?;
    private_directory(&dir)?;
    // Hold the lock for the entire service lifetime. Never unlink a live peer's socket.
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join("service.lock"))
        .map_err(|e| e.to_string())?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|_| "Code Mode service already running".to_string())?;
    let path = dir.join("service.sock");
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_socket() => {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        Ok(_) => return Err("Refusing to replace a non-socket Code Mode path".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    let listener = UnixListener::bind(&path).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    let service = Arc::new(service(&dir.join("history.sqlite3"))?);
    eprintln!("Glance Code Mode service ready");
    loop {
        let (stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let service = service.clone();
        tokio::spawn(async move {
            let (read, mut write) = stream.into_split();
            let request = read_message(&mut BufReader::new(read))
                .await
                .and_then(|bytes| {
                    serde_json::from_slice::<Request>(&bytes).map_err(|e| e.to_string())
                });
            let response = match request {
                Ok(request) => match dispatch(service.as_ref(), request).await {
                    Ok(result) => json!({"result":result}),
                    Err(error) => json!({"error":error}),
                },
                Err(error) => json!({"error":error}),
            };
            if let Ok(mut bytes) = serde_json::to_vec(&response) {
                bytes.push(b'\n');
                let _ = write.write_all(&bytes).await;
            }
        });
    }
}

#[derive(Deserialize, incurs::Options)]
struct Search {
    /// Tool name, description, or parameter to find.
    query: String,
}
#[derive(Deserialize, incurs::Options)]
struct Execute {
    /// JavaScript using the glance namespace and async/await.
    code: String,
}
#[derive(Deserialize, incurs::Options)]
struct Execution {
    /// Execution ID returned by execute.
    id: String,
    /// Fetch an oversized result owned by this execution.
    artifact_id: Option<String>,
}
#[derive(Deserialize, incurs::Options)]
struct Decision {
    /// Execution ID with a pending action.
    id: String,
    /// Pending action sequence number.
    seq: u64,
    /// approve or reject.
    decision: String,
}
#[derive(Deserialize, incurs::Options)]
struct Cancel {
    /// Execution ID to cancel.
    id: String,
}

fn typed(value: Result<Value, String>) -> TypedResult<Value> {
    match value {
        Ok(value) => TypedResult::ok(value),
        Err(error) => TypedResult::error("CODE_MODE", error),
    }
}

pub(crate) fn cli() -> Cli {
    Cli::create("code").description("Code Mode lifecycle. Run code serve before calling it.")
        .command("serve", CommandDef::typed::<(), (), (), Value, _, _>("serve", |_: TypedContext<(), (), ()>| async {
            typed(serve().await.map(|_| Value::Null))
        }).description("Run the private local Code Mode service in the foreground.")
          .mcp(McpCommandOptions { enabled: false, ..Default::default() }).done())
        .command("search", CommandDef::typed::<(), Search, (), Value, _, _>("search", |ctx: TypedContext<(), Search, ()>| async move {
            typed(Client.call(Request::Search { query: ctx.options.query }).await)
        }).description("Search complete editor API with TypeScript declarations.").done())
        .command("execute", CommandDef::typed::<(), Execute, (), Value, _, _>("execute", |ctx: TypedContext<(), Execute, ()>| async move {
            typed(Client.call(Request::Execute { code: ctx.options.code }).await)
        }).description("Start JavaScript; returns an execution ID and running state.").done())
        .command("execution", CommandDef::typed::<(), Execution, (), Value, _, _>("execution", |ctx: TypedContext<(), Execution, ()>| async move {
            let request = match ctx.options.artifact_id {
                Some(artifact_id) => Request::Artifact { id: ctx.options.id, artifact_id },
                None => Request::Execution { id: ctx.options.id },
            };
            typed(Client.call(request).await)
        }).description("Read execution status/result or an owned artifact.").done())
        .command("decide", CommandDef::typed::<(), Decision, (), Value, _, _>("decide", |ctx: TypedContext<(), Decision, ()>| async move {
            let approve = match ctx.options.decision.as_str() {
                "approve" => true, "reject" => false,
                _ => return TypedResult::error("CODE_MODE", "decision must be approve or reject"),
            };
            typed(Client.call(Request::Decide { id: ctx.options.id, seq: ctx.options.seq, approve }).await)
        }).description("Approve or reject one pending action.").done())
        .command("cancel", CommandDef::typed::<(), Cancel, (), Value, _, _>("cancel", |ctx: TypedContext<(), Cancel, ()>| async move {
            typed(Client.call(Request::Cancel { id: ctx.options.id }).await)
        }).description("Cancel a running or paused program. Completed edits remain undoable in the editor.").done())
}

#[cfg(test)]
mod tests;
