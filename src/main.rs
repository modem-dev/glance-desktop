mod accessibility;
mod animation;
mod arrow;
mod automation;
mod backdrop;
mod cli;
mod code_mode;
mod color_picker;
mod document;
mod drawing;
mod effects;
mod enhance;
mod gestures;
mod gif_export;
mod glance;
mod icons;
mod mcp;
mod menus;
mod motion_shader;
mod navigation;
#[cfg(test)]
mod performance;
mod platform;
mod selection;
mod startup;
#[cfg(test)]
mod stress_tests;
mod style;
mod text;
mod video;
actions!(glance, [Quit]);
mod editor;
use editor::Editor;
pub(crate) use editor::{Layout, Message};
use gpui::*;
fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let bundled = std::env::current_exe()
        .ok()
        .is_some_and(|executable| startup::desktop_bundle(&executable));
    let argv = match startup::mode(argv, bundled) {
        startup::Mode::Desktop(argv) => argv,
        startup::Mode::Cli(argv) => {
            if let Err(error) = cli::run(argv) {
                eprintln!("Glance CLI: {error}");
                std::process::exit(1);
            }
            return;
        }
        startup::Mode::CodeMcp => {
            if let Err(error) = cli::run_code_mcp() {
                eprintln!("Glance Code Mode: {error}");
                std::process::exit(1);
            }
            return;
        }
        startup::Mode::NativeMcp => {
            if let Err(error) = mcp::run() {
                eprintln!("Glance MCP: {error}");
                std::process::exit(1);
            }
            return;
        }
    };
    let initial = match platform::startup_image(argv.into_iter()) {
        Ok(platform::Startup::Image(image)) => Some(image),
        Ok(platform::Startup::Demo) => None,
        Ok(platform::Startup::Exit) => return,
        Err(error) => {
            eprintln!("Glance: {error}");
            std::process::exit(1);
        }
    };
    let application = Application::new().with_assets(icons::Icons);
    application.on_reopen(|cx| cx.activate(true));
    application.run(move |cx: &mut App| {
        cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
        cx.bind_keys([KeyBinding::new(&platform::key_binding("cmd-q"), Quit, None)]);
        menus::install(cx);
        let bounds = Bounds::centered(None, size(px(1220.), px(860.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(1050.), px(600.))),
                #[cfg(target_os = "linux")]
                app_id: Some("glance".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some("Glance".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| {
                cx.new(|cx| {
                    window.on_window_should_close(cx, |_, cx| {
                        #[cfg(target_os = "macos")]
                        {
                            cx.hide();
                            false
                        }
                        #[cfg(target_os = "linux")]
                        {
                            cx.quit();
                            true
                        }
                    });
                    let editor = Editor::new(cx, initial);
                    editor.focus.focus(window);
                    editor
                })
            },
        )
        .expect("Unable to open the editor");
        cx.activate(true);
    });
}
