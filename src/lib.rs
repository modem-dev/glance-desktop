//! Attn Lens: capture part of the live screen, draw on it and talk, then send
//! it to an attn agent session.
pub mod animation;
pub mod app;
pub mod arrow;
pub mod backdrop;
pub mod document;
pub mod editor;
pub mod effects;
pub mod enhance;
pub mod gif_export;
pub mod keys;
mod kit;
pub mod lens;
#[cfg(target_os = "linux")]
pub mod linux_compute;
pub mod motion_shader;
pub mod navigation;
pub mod ocr;
#[cfg(target_os = "macos")]
pub mod paste;
#[cfg(test)]
mod performance;
#[cfg(target_os = "macos")]
pub mod permission_guide;
pub mod platform;
pub mod platform_port;
pub mod selection;
pub mod settings;
pub mod settings_window;
pub mod stand_in;
#[cfg(test)]
mod stress_tests;
pub mod style;
pub mod video;
