//! Workspace: where a session writes its output products
use std::{
    fs::{create_dir_all, File},
    io::BufWriter,
    path::{Path, PathBuf},
};

use log::info;

use crate::error::Error;

/// Session workspace: `$OUTPUT/$SESSION`
#[derive(Debug, Clone)]
pub struct Workspace {
    /// Root of this session
    pub root: PathBuf,
}

impl Workspace {
    /// Creates (if need be) the `output/session` folder
    pub fn new(output: &Path, session: &str) -> Result<Self, Error> {
        let root = output.join(session);
        create_dir_all(&root)?;
        info!("session workspace: \"{}\"", root.display());
        Ok(Self { root })
    }

    /// Full path of a file within this session
    pub fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    /// Creates a subfolder, returns its path
    pub fn subdir(&self, dir: &str) -> Result<PathBuf, Error> {
        let path = self.root.join(dir);
        create_dir_all(&path)?;
        Ok(path)
    }

    /// Creates a (buffered) file within this session
    pub fn create_file(&self, name: &str) -> Result<BufWriter<File>, Error> {
        let path = self.path(name);
        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }
        let fd = File::create(&path)?;
        Ok(BufWriter::new(fd))
    }

    /// Opens a file of this session with the default system application
    pub fn open(&self, name: &str) {
        let path = self.path(name);
        let path = path.to_string_lossy().to_string();

        #[cfg(target_os = "linux")]
        let result = std::process::Command::new("xdg-open").arg(&path).spawn();

        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open").arg(&path).spawn();

        #[cfg(target_os = "windows")]
        let result = std::process::Command::new("cmd")
            .args(["/C", "start", "", &path])
            .spawn();

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        let result: std::io::Result<()> = Ok(());

        if let Err(e) = result {
            log::warn!("failed to open \"{}\": {}", path, e);
        }
    }
}
