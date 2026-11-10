use crate::{Upload, ValidationError};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};
pub struct UrdfStorage(pub PathBuf);

fn safe_path(value: &str) -> Result<PathBuf> {
    ensure!(
        !value.contains('\\') && !value.contains(':') && !value.is_empty(),
        "Invalid asset path"
    );
    let path = Path::new(value);
    ensure!(
        path.components()
            .all(|part| matches!(part, Component::Normal(_))),
        "Invalid asset path"
    );
    Ok(path.to_path_buf())
}
fn mime(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "urdf" | "xml" => Some("application/xml"),
        "stl" => Some("model/stl"),
        "dae" => Some("model/vnd.collada+xml"),
        "obj" | "mtl" => Some("text/plain"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "glb" => Some("model/gltf-binary"),
        "gltf" => Some("model/gltf+json"),
        "bin" => Some("application/octet-stream"),
        _ => None,
    }
}
impl UrdfStorage {
    pub fn store(&self, upload: &Upload, profile: i64, configuration: i64) -> Result<String> {
        let root_relative = format!("robots/{profile}/{configuration}");
        let root = self.0.join(&root_relative);
        let main = safe_path(&upload.filename)?;
        let files = std::iter::once((&upload.filename, &upload.bytes))
            .chain(upload.assets.iter().map(|a| (&a.filename, &a.bytes)));
        let mut paths = Vec::new();
        let mut names = std::collections::HashSet::new();
        for (name, bytes) in files {
            let relative = safe_path(name)?;
            ensure!(mime(&relative).is_some(), "Unsupported asset type");
            ensure!(names.insert(relative.clone()), "Duplicate asset path");
            paths.push((root.join(relative), bytes));
        }
        fs::create_dir_all(&self.0)?;
        let storage_root = self.0.canonicalize()?;
        let mut stored = Vec::new();
        let result = (|| -> Result<()> {
            for (path, bytes) in paths {
                fs::create_dir_all(path.parent().context("Missing parent")?)?;
                ensure!(
                    path.parent()
                        .unwrap()
                        .canonicalize()?
                        .starts_with(&storage_root),
                    "Asset escaped storage"
                );
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)?;
                stored.push(path);
                file.write_all(bytes)?;
                file.sync_all()?;
            }
            Ok(())
        })();
        if result.is_err() {
            for path in stored {
                let _ = fs::remove_file(path);
            }
        }
        result.map_err(|_| ValidationError(-100100018, "Cannot store URDF package"))?;
        Ok(format!(
            "{root_relative}/{}",
            main.to_string_lossy().replace('\\', "/")
        ))
    }
    pub fn manifest(&self, urdf: &str) -> Result<Vec<String>> {
        let relative = safe_path(urdf)?;
        let root = self
            .0
            .join(relative.components().take(3).collect::<PathBuf>())
            .canonicalize()?;
        ensure!(
            root.starts_with(self.0.canonicalize()?),
            "Invalid storage root"
        );
        let mut stack = vec![root.clone()];
        let mut files = Vec::new();
        while let Some(directory) = stack.pop() {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                if entry.file_type()?.is_symlink() {
                    continue;
                }
                let path = entry.path();
                if entry.file_type()?.is_dir() {
                    stack.push(path);
                } else if mime(&path).is_some() {
                    files.push(
                        path.strip_prefix(&root)?
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                    ensure!(files.len() <= 500, "Too many package files");
                }
            }
        }
        files.sort();
        Ok(files)
    }
    pub fn read_asset(&self, urdf: &str, name: &str) -> Result<(Vec<u8>, &'static str)> {
        let relative = safe_path(urdf)?;
        let root = self
            .0
            .join(relative.components().take(3).collect::<PathBuf>())
            .canonicalize()?;
        ensure!(
            root.starts_with(self.0.canonicalize()?),
            "Invalid storage root"
        );
        let name = safe_path(name)?;
        let content_type = mime(&name).context("Unsupported asset")?;
        let file = root.join(name).canonicalize()?;
        ensure!(file.starts_with(&root), "Asset escaped configuration");
        Ok((fs::read(file)?, content_type))
    }
    pub fn remove_stored(&self, relative: &str) {
        // Only a freshly stored package path is passed after a failed database commit.
        if let Ok(path) = safe_path(relative) {
            let root = self.0.join(path.components().take(3).collect::<PathBuf>());
            if let (Ok(root), Ok(storage)) = (root.canonicalize(), self.0.canonicalize())
                && root.starts_with(&storage)
                && root != storage
            {
                let _ = fs::remove_dir_all(root);
            }
        }
    }
}
