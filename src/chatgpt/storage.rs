use super::*;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
};
impl Service {
    pub(super) fn load(&mut self) -> Result<(), String> {
        let result = self.load_inner();
        if result.is_err() {
            self.store = None;
            self.lock = None;
        }
        result
    }
    fn load_inner(&mut self) -> Result<(), String> {
        if self.store.is_some() {
            return Ok(());
        }
        if let Some(directory) = &self.directory {
            if !directory.is_absolute() {
                return Err("ChatGPT settings directory must be absolute".into());
            }
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(directory)
                .map_err(|_| "Could not create private ChatGPT settings")?;
            check_private(directory, true)?;
            let lock_path = directory.join("session.lock");
            if lock_path.symlink_metadata().is_ok() {
                check_private(&lock_path, false)?;
            }
            let lock = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .mode(0o600)
                .open(&lock_path)
                .map_err(|_| "Could not open ChatGPT session lock")?;
            fs2::FileExt::try_lock_exclusive(&lock)
                .map_err(|_| "ChatGPT settings are in use by another Glance process")?;
            self.lock = Some(lock);
            let path = directory.join("accounts.json");
            if path.symlink_metadata().is_ok() {
                check_private(&path, false)?;
                let mut data = Vec::new();
                fs::File::open(path)
                    .map_err(|_| "Could not read ChatGPT settings")?
                    .take(MAX_JSON as u64 + 1)
                    .read_to_end(&mut data)
                    .map_err(|_| "Could not read ChatGPT settings")?;
                if data.len() > MAX_JSON {
                    return Err("ChatGPT settings are too large".into());
                }
                self.store = Some(
                    serde_json::from_slice(&data).map_err(|_| "ChatGPT settings are invalid")?,
                );
            }
        }
        if self.store.is_none() {
            self.store = Some(Store {
                host_id: new_host_id(),
                ..Default::default()
            });
            self.save()?;
        }
        let store = self.store.as_ref().unwrap();
        if store.host_id.is_empty()
            || store.host_id.len() > 256
            || store.accounts.len() > 32
            || store.accounts.iter().any(|a| {
                a.id.len() > 128
                    || !a.id.is_ascii()
                    || a.client_id == "dynamic_agent_client"
                    || a.client_id.is_empty()
            })
        {
            return Err("ChatGPT settings are invalid".into());
        }
        Ok(())
    }
    pub(super) fn save(&self) -> Result<(), String> {
        let Some(directory) = &self.directory else {
            return Ok(());
        };
        check_private(directory, true)?;
        let path = directory.join("accounts.json");
        if path.symlink_metadata().is_ok() {
            check_private(&path, false)?;
        }
        let bytes = serde_json::to_vec(self.store.as_ref().ok_or("ChatGPT settings not loaded")?)
            .map_err(|_| "Could not save ChatGPT settings")?;
        if bytes.len() > MAX_JSON {
            return Err("ChatGPT settings are too large".into());
        }
        let mut file = tempfile::NamedTempFile::new_in(directory)
            .map_err(|_| "Could not reserve private ChatGPT settings file")?;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| "Could not protect ChatGPT settings")?;
        file.write_all(&bytes)
            .and_then(|_| file.as_file().sync_all())
            .map_err(|_| "Could not save ChatGPT settings")?;
        file.persist(path)
            .map_err(|_| "Could not replace ChatGPT settings")?;
        Ok(())
    }
}
fn check_private(path: &std::path::Path, directory: bool) -> Result<(), String> {
    let metadata = path
        .symlink_metadata()
        .map_err(|_| "Could not inspect ChatGPT settings permissions")?;
    if metadata.file_type().is_symlink()
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(
            "ChatGPT settings must be regular, owner-only files in a private directory".into(),
        );
    }
    Ok(())
}

fn new_host_id() -> String {
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}
