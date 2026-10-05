//! Incurs transports over the published native-editor contract.
use std::sync::{Arc, LazyLock};

use incurs::cli::Cli;
use incurs::command::{
    CommandContext, CommandDef, CommandHandler, McpAnnotations, McpCommandOptions, McpResultContent,
};
use incurs::output::{CommandResult, Format};
use incurs::schema::{FieldMeta, FieldType};
use serde_json::Value;

use crate::{automation, mcp};

pub(crate) type EditorCall = Arc<dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync>;
struct EditorCommand(String, EditorCall);

fn arguments(name: &str, mut args: Value) -> Result<Value, String> {
    let tool = mcp::tools()
        .into_iter()
        .find(|t| t["name"] == name)
        .ok_or("Unknown editor command")?;
    for (field, schema) in tool["inputSchema"]["properties"].as_object().unwrap() {
        if !matches!(
            schema["type"].as_str(),
            Some("string" | "number" | "integer" | "boolean")
        ) && let Some(text) = args[field].as_str()
        {
            args[field] =
                serde_json::from_str(text).map_err(|e| format!("{field}: invalid JSON: {e}"))?;
        }
    }
    Ok(args)
}

fn result(value: Result<Value, String>) -> CommandResult {
    match value {
        Ok(data) => CommandResult::Ok {
            data,
            cta: None,
            exit_code: None,
        },
        Err(message) => CommandResult::Error {
            code: "GLANCE".into(),
            message,
            retryable: false,
            exit_code: None,
            cta: None,
        },
    }
}

#[async_trait::async_trait]
impl CommandHandler for EditorCommand {
    async fn run(&self, context: CommandContext) -> CommandResult {
        let name = self.0.clone();
        let args = match arguments(&name, context.options) {
            Ok(args) => args,
            Err(error) => return result(Err(error)),
        };
        if let Err(error) = mcp::validate_tool(&name, &args) {
            return result(Err(error));
        }
        let call = self.1.clone();
        result(
            tokio::task::spawn_blocking(move || call(&name, args))
                .await
                .unwrap_or_else(|e| Err(e.to_string())),
        )
    }
}

// Incurs metadata uses static field names. Intern the bounded published contract
// once, rather than leaking names each time a catalog is constructed.
static EDITOR_FIELDS: LazyLock<Vec<Vec<FieldMeta>>> = LazyLock::new(|| {
    mcp::tools()
        .iter()
        .map(|tool| {
            let schema = &tool["inputSchema"];
            schema["properties"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(name, property)| {
                    let field_type = match property["type"].as_str() {
                        Some("string") => FieldType::String,
                        Some("number" | "integer") => FieldType::Number,
                        Some("boolean") => FieldType::Boolean,
                        _ => FieldType::Value,
                    };
                    FieldMeta {
                        name: Box::leak(name.clone().into_boxed_str()),
                        cli_name: name.replace('_', "-"),
                        description: property["description"]
                            .as_str()
                            .map(|s| &*Box::leak(s.to_owned().into_boxed_str())),
                        field_type,
                        required: schema["required"]
                            .as_array()
                            .is_some_and(|names| names.iter().any(|n| n == name)),
                        default: None,
                        alias: None,
                        deprecated: false,
                        env_name: None,
                    }
                })
                .collect()
        })
        .collect()
});

pub(crate) fn editor_cli() -> Cli {
    editor_cli_with(Arc::new(automation::call))
}

pub(crate) fn editor_cli_with(call: EditorCall) -> Cli {
    let mut cli = Cli::create("glance")
        .version(env!("CARGO_PKG_VERSION"))
        .description(
            "Control the native Glance editor. Open the app with 'glance desktop --automation'.",
        )
        .format(Format::Json)
        .mcp(incurs::mcp::McpServeOptions {
            tools: incurs::mcp::McpToolFilter {
                discovery: incurs::mcp::McpDiscovery::Direct,
                ..Default::default()
            },
            ..Default::default()
        });
    for (tool, fields) in mcp::tools().into_iter().zip(EDITOR_FIELDS.iter()) {
        let name = tool["name"].as_str().unwrap();
        let content = if matches!(name, "read_image" | "read_video_frame") {
            vec![McpResultContent::Image {
                data_pointer: "/content/0/data".into(),
                mime_type_pointer: "/content/0/mimeType".into(),
            }]
        } else {
            vec![]
        };
        let mut command = CommandDef::build(
            name.replace('_', "-"),
            EditorCommand(name.into(), call.clone()),
        )
        .description(tool["description"].as_str().unwrap())
        .mcp(McpCommandOptions {
            name: Some(name.into()),
            input_schema: Some(tool["inputSchema"].clone()),
            annotations: Some(McpAnnotations {
                title: tool["annotations"]["title"].as_str().map(str::to_owned),
                read_only_hint: tool["annotations"]["readOnlyHint"].as_bool(),
                destructive_hint: tool["annotations"]["destructiveHint"].as_bool(),
                idempotent_hint: tool["annotations"]["idempotentHint"].as_bool(),
                open_world_hint: tool["annotations"]["openWorldHint"].as_bool(),
            }),
            result_content: content,
            ..Default::default()
        })
        .done();
        command.options_fields = fields.clone();
        cli = cli.command(name.replace('_', "-"), command);
    }
    cli
}

pub(crate) fn cli() -> Cli {
    editor_cli().group(crate::code_mode::cli())
}

pub(crate) fn run(argv: Vec<String>) -> Result<(), String> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?
        .block_on(cli().serve_with(argv))
        .map_err(|e| e.to_string())
}

pub(crate) fn run_code_mcp() -> Result<(), String> {
    use rmcp::ServiceExt;
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?
        .block_on(async {
            incurs_codemode_mcp::CodeModeMcpServer::new(Arc::new(crate::code_mode::Client))
                .with_identity("glance", env!("CARGO_PKG_VERSION"))
                .serve(rmcp::transport::io::stdio())
                .await
                .map_err(|e| e.to_string())?
                .waiting()
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
}

#[cfg(test)]
mod tests;
