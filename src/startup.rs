//! Choose process mode before initializing GPUI or a CLI runtime.
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Cli(Vec<String>),
    Desktop(Vec<String>),
    NativeMcp,
    CodeMcp,
}

pub(crate) fn desktop_bundle(executable: &Path) -> bool {
    executable.file_name().is_some_and(|name| name == "Glance")
        && executable
            .parent()
            .is_some_and(|parent| parent.ends_with("Contents/MacOS"))
        && executable
            .ancestors()
            .nth(3)
            .and_then(Path::extension)
            .is_some_and(|extension| extension == "app")
}

pub(crate) fn mode(mut argv: Vec<String>, bundled: bool) -> Mode {
    match argv.first().map(String::as_str) {
        Some("--cli") => {
            argv.remove(0);
            Mode::Cli(argv)
        }
        Some("--codemode-mcp") => Mode::CodeMcp,
        Some("--native-mcp") => Mode::NativeMcp,
        Some("--mcp") if bundled => Mode::NativeMcp,
        Some("desktop") => {
            argv.remove(0);
            Mode::Desktop(argv)
        }
        Some("--automation" | "--open" | "--capture-area" | "--capture-screen") => {
            Mode::Desktop(argv)
        }
        Some(arg) if arg.starts_with("-psn_") => Mode::Desktop(argv),
        None | Some("--help" | "-h") if bundled => Mode::Desktop(argv),
        _ => Mode::Cli(argv),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn shell_command_defaults_to_cli() {
        for values in [
            vec![],
            vec!["--help"],
            vec!["--version"],
            vec!["--mcp"],
            vec!["--llms"],
            vec!["get-document"],
            vec!["export-png", "--path", "/tmp/new.png"],
            vec!["code", "search", "--query", "annotation"],
            vec!["not-a-command"],
            vec!["dispatch-action", "--action", "{\"text\":\"--mcp\"}"],
        ] {
            assert_eq!(mode(args(&values), false), Mode::Cli(args(&values)));
        }
    }

    #[test]
    fn explicit_desktop_preserves_startup_arguments() {
        assert_eq!(mode(args(&["desktop"]), false), Mode::Desktop(vec![]));
        assert_eq!(
            mode(
                args(&["desktop", "--automation", "--open", "/tmp/demo.png"]),
                false
            ),
            Mode::Desktop(args(&["--automation", "--open", "/tmp/demo.png"]))
        );
        for flag in [
            "--automation",
            "--open",
            "--capture-area",
            "--capture-screen",
            "-psn_0_123",
        ] {
            assert_eq!(mode(args(&[flag]), false), Mode::Desktop(args(&[flag])));
        }
    }

    #[test]
    fn bundle_launch_preserves_desktop_and_native_mcp() {
        for values in [vec![], vec!["--help"], vec!["-h"]] {
            assert_eq!(mode(args(&values), true), Mode::Desktop(args(&values)));
        }
        assert_eq!(mode(args(&["--mcp"]), true), Mode::NativeMcp);
        assert_eq!(
            mode(args(&["get-document"]), true),
            Mode::Cli(args(&["get-document"]))
        );
    }

    #[test]
    fn legacy_cli_and_code_mode_still_work() {
        assert_eq!(
            mode(args(&["--cli", "--mcp"]), true),
            Mode::Cli(args(&["--mcp"]))
        );
        assert_eq!(mode(args(&["--codemode-mcp"]), false), Mode::CodeMcp);
        assert_eq!(mode(args(&["--native-mcp"]), false), Mode::NativeMcp);
    }

    #[test]
    fn only_packaged_executable_defaults_to_desktop() {
        assert!(desktop_bundle(Path::new(
            "/Applications/Glance.app/Contents/MacOS/Glance"
        )));
        for path in [
            "/usr/local/bin/glance",
            "/tmp/Glance",
            "/tmp/Contents/MacOS/Glance",
        ] {
            assert!(!desktop_bundle(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn linux_launcher_requests_desktop_explicitly() {
        let entry = include_str!("../packaging/linux/glance.desktop");
        let launches: Vec<_> = entry
            .lines()
            .filter(|line| line.starts_with("Exec="))
            .collect();
        assert_eq!(
            launches,
            [
                "Exec=glance desktop",
                "Exec=glance desktop --capture-area",
                "Exec=glance desktop --capture-screen"
            ]
        );
    }
}
