//! Lens's settings: a typed `Settings`, kept in `prefs.json` under `dev.attn.lens`
//! (`~/Library/Application Support/dev.attn.lens` on the Mac, the folder that holds
//! the speech models). `RUNE_PREFS_DIR` moves it, so headless runs and tests stay
//! off the real file. The rest of Lens reads `Settings` from `Lens` state and never
//! touches `Prefs`; the Settings window changes them through `Lens::set_settings`.
//!
//! One Lens runs at a time, so it is the only writer. Rune's `Prefs` writes through
//! a temporary file and a rename and moves a file that doesn't parse aside, so
//! nothing here recovers on its own.
//!
//! A new setting is one line in `settings!` below: its field, its key in
//! `prefs.json` and its default. A value that is missing, or of another type, is
//! the default.
use rune::prefs::Prefs;
use serde::Serialize;

/// The folder under the config directory that holds Lens's `prefs.json`.
pub const APP_ID: &str = "dev.attn.lens";

macro_rules! settings {
    ($($(#[$doc:meta])* $field:ident: $ty:ty = $default:expr, $key:literal;)*) => {
        #[derive(Clone, Debug, PartialEq, Serialize)]
        pub struct Settings {
            $($(#[$doc])* pub $field: $ty,)*
        }

        impl Default for Settings {
            fn default() -> Self {
                Self { $($field: $default,)* }
            }
        }

        impl Settings {
            pub fn load(prefs: &Prefs) -> Self {
                let default = Self::default();
                Self { $($field: prefs.get($key).unwrap_or(default.$field),)* }
            }

            /// Writes each setting that differs from `was`. On an error the values
            /// stay set in `prefs` for this run.
            pub fn save(&self, was: &Self, prefs: &mut Prefs) -> Result<(), String> {
                $(if self.$field != was.$field {
                    prefs.set($key, &self.$field).map_err(|e| e.to_string())?;
                })*
                Ok(())
            }
        }
    };
}

settings! {
    /// When you dictate, read the window in front and send its names and
    /// identifiers with your words to attn's refiner.
    screen_words: bool = true, "dictation.screen_words";
}
