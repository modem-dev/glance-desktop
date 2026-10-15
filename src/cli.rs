//! Terminal commands over the existing, opt-in native editor bridge.
//! Names, flags, help and validation come from the published MCP contract.
use crate::{automation, mcp};
use serde_json::{Map, Value};

#[derive(Debug, PartialEq)]
pub(crate) enum Mode {
    Desktop(Vec<String>),
    Mcp,
    Cli(Vec<String>),
}

pub(crate) fn mode(mut args: Vec<String>) -> Mode {
    match args.first().map(String::as_str) {
        Some("--mcp") if args.len() == 1 => Mode::Mcp,
        Some("--cli") => {
            args.remove(0);
            Mode::Cli(args)
        }
        Some("desktop") => {
            args.remove(0);
            Mode::Desktop(args)
        }
        None => Mode::Desktop(args),
        Some("--help" | "-h" | "--version") => Mode::Cli(args),
        Some(arg) if arg.starts_with('-') => Mode::Desktop(args),
        _ => Mode::Cli(args),
    }
}

fn tool(command: &str) -> Result<Value, String> {
    mcp::tools()
        .into_iter()
        .find(|tool| tool["name"].as_str().unwrap().replace('_', "-") == command)
        .ok_or_else(|| format!("Unknown command: {command}. Run glance --help."))
}

fn help() -> String {
    let mut text = format!(
        "Glance {}\n\nUsage: glance [desktop] [--automation] [--open PATH | --capture-area | --capture-screen]\n       glance COMMAND [--flag VALUE | --flag=VALUE]\n       glance COMMAND --help | --schema\n       glance --mcp\n\nPlain glance opens the editor. Commands require a running editor started with --automation.\nResults are JSON; errors are JSON on stderr with exit status 1.\nUse JSON for objects and arrays. Boolean flags accept --flag, --flag=true or --flag=false.\n\nCommands:\n",
        env!("CARGO_PKG_VERSION")
    );
    for tool in mcp::tools() {
        text.push_str(&format!(
            "  {}\n",
            tool["name"].as_str().unwrap().replace('_', "-")
        ));
    }
    text
}

fn command_help(tool: &Value) -> String {
    let command = tool["name"].as_str().unwrap().replace('_', "-");
    let mut text = format!(
        "Usage: glance {command} [OPTIONS]\n\n{}\n\nOptions:\n",
        tool["description"].as_str().unwrap()
    );
    let schema = &tool["inputSchema"];
    for (name, property) in schema["properties"].as_object().unwrap() {
        let required = schema["required"]
            .as_array()
            .is_some_and(|keys| keys.iter().any(|key| key == name));
        text.push_str(&format!(
            "  --{} {}{}\n",
            name.replace('_', "-"),
            property["type"],
            if required { " (required)" } else { "" }
        ));
        if let Some(description) = property["description"].as_str() {
            text.push_str(&format!("    {description}\n"));
        }
    }
    text.push_str("  --help    Show this help\n  --schema  Print the complete input JSON schema\n");
    text
}

fn arguments(tool: &Value, flags: &[String]) -> Result<Value, String> {
    let properties = tool["inputSchema"]["properties"].as_object().unwrap();
    let mut args = Map::new();
    let mut flags = flags.iter().peekable();
    while let Some(flag) = flags.next() {
        let flag = flag.strip_prefix("--").ok_or("Expected a --flag")?;
        let (name, inline) = flag
            .split_once('=')
            .map_or((flag, None), |(name, value)| (name, Some(value)));
        let name = name.replace('-', "_");
        let property = properties
            .get(&name)
            .ok_or_else(|| format!("Unknown flag: --{}", name.replace('_', "-")))?;
        if args.contains_key(&name) {
            return Err(format!("Duplicate flag: --{}", name.replace('_', "-")));
        }
        let boolean = property["type"] == "boolean";
        let text = if let Some(value) = inline {
            value
        } else if boolean && flags.peek().is_none_or(|next| next.starts_with("--")) {
            "true"
        } else {
            let next = flags
                .next()
                .filter(|next| !next.starts_with("--"))
                .ok_or_else(|| format!("Missing value for --{}", name.replace('_', "-")))?;
            next.as_str()
        };
        let value = if property["type"] == "string" {
            Value::String(text.into())
        } else {
            serde_json::from_str(text)
                .map_err(|error| format!("{name}: invalid JSON value: {error}"))?
        };
        args.insert(name, value);
    }
    let args = Value::Object(args);
    mcp::validate_tool(tool["name"].as_str().unwrap(), &args)?;
    Ok(args)
}

// Kept separate from process I/O so tests can exercise parsing and the real dispatcher.
pub(super) fn execute(
    argv: &[String],
    call: impl FnOnce(&str, Value) -> Result<Value, String>,
) -> Result<String, String> {
    match argv {
        [] => return Ok(help()),
        [flag] if flag == "--help" || flag == "-h" => return Ok(help()),
        [flag] if flag == "--version" => return Ok(env!("CARGO_PKG_VERSION").into()),
        _ => {}
    }
    let tool = tool(&argv[0])?;
    let flags = &argv[1..];
    if flags.len() == 1 {
        match flags[0].as_str() {
            "--help" | "-h" => return Ok(command_help(&tool)),
            "--schema" => return Ok(tool["inputSchema"].to_string()),
            _ => {}
        }
    }
    let args = arguments(&tool, flags)?;
    call(tool["name"].as_str().unwrap(), args).map(|value| value.to_string())
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    use std::io::Write;
    let output = execute(args, automation::call)?;
    writeln!(std::io::stdout().lock(), "{output}").map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
