use crate::urdf_storage::safe_path;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fmt::Write as _,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};
use zip::ZipArchive;

pub const VLA_PACKAGE_FORMAT_VERSION: u32 = 1;
pub const MAX_PACKAGE_ENTRIES: usize = 32;
const ALLOWED_EXTENSIONS: &[&str] = &["json", "onnx", "data", "txt", "md"];
const SUPPORTED_INPUT_KINDS: &[&str] = &[
    "joint_position",
    "image",
    "task_tokens",
    "task_attention_mask",
];
const SUPPORTED_OUTPUT_KINDS: &[&str] = &["joint_position_chunk", "done_flag"];
const SUPPORTED_IMAGE_ENCODINGS: &[&str] = &["rgb8", "bgr8", "mono8"];
const SUPPORTED_IMAGE_LAYOUTS: &[&str] = &["NCHW", "NHWC"];
const SUPPORTED_IMAGE_SCALES: &[&str] = &["0_1", "neg1_1", "imagenet", "raw"];
const SUPPORTED_DTYPES: &[&str] = &["float32", "float64", "int64", "int32", "uint8", "bool"];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VlaManifest {
    pub format_version: u32,
    pub code: String,
    pub name: String,
    pub model_type: String,
    pub version: String,
    pub robot: VlaManifestRobot,
    pub onnx: VlaManifestOnnx,
    pub inputs: Vec<VlaManifestInput>,
    pub outputs: Vec<VlaManifestOutput>,
    #[serde(default)]
    pub normalization: Option<String>,
    #[serde(default)]
    pub control: Option<VlaManifestControl>,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VlaManifestRobot {
    pub joint_names: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VlaManifestOnnx {
    pub file: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VlaManifestInput {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub dtype: Option<String>,
    #[serde(default)]
    pub camera: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub encoding: Option<String>,
    #[serde(default)]
    pub layout: Option<String>,
    #[serde(default)]
    pub scale: Option<String>,
    #[serde(default)]
    pub max_length: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VlaManifestOutput {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub dtype: Option<String>,
    #[serde(default)]
    pub optional: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VlaManifestControl {
    #[serde(default)]
    pub chunk_size: Option<usize>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VlaNormalization {
    #[serde(default)]
    pub state_mean: Option<Vec<f32>>,
    #[serde(default)]
    pub state_std: Option<Vec<f32>>,
    #[serde(default)]
    pub action_mean: Option<Vec<f32>>,
    #[serde(default)]
    pub action_std: Option<Vec<f32>>,
}

#[derive(Clone, Debug)]
pub struct ExtractedVlaPackage {
    pub manifest: VlaManifest,
    pub manifest_json: String,
    pub package_sha256: String,
    pub package_bytes: i64,
}

impl VlaManifest {
    pub fn required_cameras(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut cameras = Vec::new();
        for input in &self.inputs {
            if input.kind == "image"
                && let Some(camera) = input.camera.as_deref()
                && seen.insert(camera.to_owned())
            {
                cameras.push(camera.to_owned());
            }
        }
        cameras
    }

    pub fn requires_tokenizer(&self) -> bool {
        self.inputs.iter().any(|input| input.kind == "task_tokens")
    }
}

pub fn validate_manifest(manifest: &VlaManifest) -> Result<()> {
    ensure!(
        manifest.format_version == VLA_PACKAGE_FORMAT_VERSION,
        "Unsupported manifest formatVersion: {}",
        manifest.format_version
    );
    ensure!(
        !manifest.code.is_empty()
            && manifest.code.len() <= 64
            && manifest
                .code
                .bytes()
                .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_')),
        "Manifest code must match [a-z0-9-_]{{1,64}}"
    );
    ensure!(!manifest.name.trim().is_empty(), "Manifest name is required");
    ensure!(
        !manifest.model_type.trim().is_empty(),
        "Manifest modelType is required"
    );
    ensure!(
        !manifest.version.trim().is_empty(),
        "Manifest version is required"
    );

    let onnx_path = safe_path(&manifest.onnx.file).context("Invalid onnx.file path")?;
    ensure!(
        onnx_path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("onnx")),
        "onnx.file must have .onnx extension"
    );

    ensure!(
        !manifest.robot.joint_names.is_empty(),
        "Manifest robot.jointNames must not be empty"
    );
    let mut joint_set = HashSet::new();
    for joint in &manifest.robot.joint_names {
        ensure!(!joint.trim().is_empty(), "Joint name must not be empty");
        ensure!(
            joint_set.insert(joint.as_str()),
            "Duplicate joint name in manifest: {joint}"
        );
    }

    ensure!(
        !manifest.inputs.is_empty(),
        "Manifest inputs must not be empty"
    );
    let mut input_names = HashSet::new();
    for input in &manifest.inputs {
        ensure!(!input.name.trim().is_empty(), "Input name is required");
        ensure!(
            input_names.insert(input.name.as_str()),
            "Duplicate input name in manifest: {}",
            input.name
        );
        ensure!(
            SUPPORTED_INPUT_KINDS.contains(&input.kind.as_str()),
            "Unsupported input kind: {}",
            input.kind
        );
        if let Some(dtype) = &input.dtype {
            ensure!(
                SUPPORTED_DTYPES.contains(&dtype.as_str()),
                "Unsupported input dtype: {dtype}"
            );
        }
        if input.kind == "image" {
            let camera = input
                .camera
                .as_deref()
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .with_context(|| format!("Image input '{}' is missing camera", input.name))?;
            let _ = camera;
            let width = input
                .width
                .filter(|&w| w > 0)
                .with_context(|| format!("Image input '{}' requires width > 0", input.name))?;
            let height = input
                .height
                .filter(|&h| h > 0)
                .with_context(|| format!("Image input '{}' requires height > 0", input.name))?;
            let _ = (width, height);
            let encoding = input
                .encoding
                .as_deref()
                .with_context(|| format!("Image input '{}' is missing encoding", input.name))?;
            ensure!(
                SUPPORTED_IMAGE_ENCODINGS.contains(&encoding),
                "Unsupported image encoding '{encoding}' on input '{}'",
                input.name
            );
            let layout = input
                .layout
                .as_deref()
                .with_context(|| format!("Image input '{}' is missing layout", input.name))?;
            ensure!(
                SUPPORTED_IMAGE_LAYOUTS.contains(&layout),
                "Unsupported image layout '{layout}' on input '{}'",
                input.name
            );
            if let Some(scale) = input.scale.as_deref() {
                ensure!(
                    SUPPORTED_IMAGE_SCALES.contains(&scale),
                    "Unsupported image scale '{scale}' on input '{}'",
                    input.name
                );
            }
        }
        if let Some(max_length) = input.max_length {
            ensure!(max_length > 0, "Input '{}' maxLength must be > 0", input.name);
        }
    }

    ensure!(
        !manifest.outputs.is_empty(),
        "Manifest outputs must not be empty"
    );
    let mut output_names = HashSet::new();
    let mut has_action_output = false;
    for output in &manifest.outputs {
        ensure!(!output.name.trim().is_empty(), "Output name is required");
        ensure!(
            output_names.insert(output.name.as_str()),
            "Duplicate output name in manifest: {}",
            output.name
        );
        ensure!(
            SUPPORTED_OUTPUT_KINDS.contains(&output.kind.as_str()),
            "Unsupported output kind: {}",
            output.kind
        );
        if output.kind == "joint_position_chunk" {
            has_action_output = true;
        }
        if let Some(dtype) = &output.dtype {
            ensure!(
                SUPPORTED_DTYPES.contains(&dtype.as_str()),
                "Unsupported output dtype: {dtype}"
            );
        }
    }
    ensure!(
        has_action_output,
        "Manifest outputs must include a joint_position_chunk output"
    );

    if let Some(control) = &manifest.control
        && let Some(chunk_size) = control.chunk_size
    {
        ensure!(chunk_size > 0, "Manifest control.chunkSize must be > 0");
    }

    if let Some(norm) = &manifest.normalization {
        let norm_path = safe_path(norm).context("Invalid normalization path")?;
        ensure!(
            norm_path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("json")),
            "normalization file must have .json extension"
        );
    }

    for (file, expected_sha) in &manifest.files {
        safe_path(file).with_context(|| format!("Invalid path in manifest.files: {file}"))?;
        ensure!(
            expected_sha.len() == 64 && expected_sha.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid sha256 digest for '{file}' in manifest.files"
        );
    }

    Ok(())
}

pub fn sha256_file(path: &Path) -> Result<(String, u64)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        total = total.saturating_add(n as u64);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(&mut hex, "{byte:02x}");
    }
    Ok((hex, total))
}

fn allowed_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            let lower = ext.to_ascii_lowercase();
            ALLOWED_EXTENSIONS.contains(&lower.as_str())
        })
}

pub fn extract(
    zip_path: &Path,
    dest: &Path,
    max_unpacked_bytes: u64,
) -> Result<ExtractedVlaPackage> {
    let (package_sha256, package_bytes_u64) =
        sha256_file(zip_path).context("Cannot read VLA zip package")?;
    let package_bytes = i64::try_from(package_bytes_u64).context("Package size overflow")?;

    let file = File::open(zip_path).context("Cannot open VLA zip package")?;
    let mut archive = ZipArchive::new(file).context("Invalid zip archive")?;
    ensure!(
        archive.len() <= MAX_PACKAGE_ENTRIES,
        "VLA package exceeds maximum of {MAX_PACKAGE_ENTRIES} entries"
    );

    fs::create_dir_all(dest).context("Cannot create staging directory")?;
    let dest_canonical = dest.canonicalize()?;

    let mut total_written = 0u64;
    let mut buffer = [0u8; 64 * 1024];

    for index in 0..archive.len() {
        let mut entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(zip::result::ZipError::UnsupportedArchive(msg)) => {
                bail!("Unsupported or encrypted zip entry: {msg}");
            }
            Err(err) => return Err(err.into()),
        };
        ensure!(
            !entry.encrypted(),
            "Encrypted entries are not allowed in VLA package"
        );
        let is_unix_symlink = entry
            .unix_mode()
            .is_some_and(|mode| (mode & 0o170000) == 0o120000);
        ensure!(
            !is_unix_symlink,
            "Symlinks are not allowed in VLA package"
        );

        let raw_name = entry.name().to_owned();
        if entry.is_dir() {
            let trimmed = raw_name.trim_end_matches('/');
            if !trimmed.is_empty() {
                safe_path(trimmed).context("Invalid directory path in VLA package")?;
            }
            continue;
        }

        let relative = safe_path(&raw_name)
            .with_context(|| format!("Invalid entry path in VLA package: {raw_name}"))?;
        ensure!(
            allowed_extension(&relative),
            "Unsupported file extension in VLA package: {raw_name}"
        );

        let target = dest.join(&relative);
        let parent = target.parent().context("Missing parent directory")?;
        fs::create_dir_all(parent)?;
        ensure!(
            parent.canonicalize()?.starts_with(&dest_canonical),
            "Entry path escaped extraction directory: {raw_name}"
        );

        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .with_context(|| format!("Duplicate or unwritable entry in VLA package: {raw_name}"))?;

        loop {
            let n = entry.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            total_written = total_written.saturating_add(n as u64);
            ensure!(
                total_written <= max_unpacked_bytes,
                "VLA package unpacked size exceeds limit of {max_unpacked_bytes} bytes"
            );
            output.write_all(&buffer[..n])?;
        }
        output.sync_all()?;
    }

    let manifest_path = dest.join("manifest.json");
    ensure!(
        manifest_path.is_file(),
        "Missing required manifest.json in VLA package"
    );
    let manifest_json =
        fs::read_to_string(&manifest_path).context("Cannot read manifest.json")?;
    let manifest: VlaManifest =
        serde_json::from_str(&manifest_json).context("Invalid manifest.json format")?;
    validate_manifest(&manifest)?;

    let onnx_file = dest.join(safe_path(&manifest.onnx.file)?);
    ensure!(
        onnx_file.is_file(),
        "Missing ONNX model file '{}' declared in manifest",
        manifest.onnx.file
    );

    if manifest.requires_tokenizer() {
        ensure!(
            dest.join("tokenizer.json").is_file(),
            "Missing required tokenizer.json for task_tokens input"
        );
    }

    if let Some(norm_name) = &manifest.normalization {
        let norm_path = dest.join(safe_path(norm_name)?);
        ensure!(
            norm_path.is_file(),
            "Missing normalization file '{norm_name}' declared in manifest"
        );
        let norm_text = fs::read_to_string(&norm_path)
            .with_context(|| format!("Cannot read normalization file '{norm_name}'"))?;
        let _: VlaNormalization = serde_json::from_str(&norm_text)
            .with_context(|| format!("Invalid normalization JSON '{norm_name}'"))?;
    }

    for (rel_name, expected_sha) in &manifest.files {
        let rel = safe_path(rel_name)?;
        let path = dest.join(&rel);
        ensure!(
            path.is_file(),
            "Missing file '{rel_name}' declared in manifest.files"
        );
        let (actual_sha, _) = sha256_file(&path)?;
        ensure!(
            actual_sha.eq_ignore_ascii_case(expected_sha),
            "SHA-256 mismatch for '{rel_name}'"
        );
    }

    Ok(ExtractedVlaPackage {
        manifest,
        manifest_json,
        package_sha256,
        package_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use zip::write::SimpleFileOptions;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vla-pkg-test-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn sha256_bytes(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        let mut hex = String::with_capacity(64);
        for b in digest {
            let _ = write!(&mut hex, "{b:02x}");
        }
        hex
    }

    fn sample_manifest(onnx_sha: &str) -> String {
        serde_json::json!({
            "formatVersion": 1,
            "code": "yellowarm-smolvla",
            "name": "YellowArm pick cube",
            "modelType": "smolvla",
            "version": "2026-11-20",
            "robot": { "jointNames": ["joint1", "joint2"] },
            "onnx": { "file": "model.onnx" },
            "inputs": [
                { "name": "state", "kind": "joint_position", "dtype": "float32" },
                {
                    "name": "image_front",
                    "kind": "image",
                    "camera": "front",
                    "width": 8,
                    "height": 8,
                    "encoding": "rgb8",
                    "layout": "NCHW",
                    "scale": "0_1"
                },
                { "name": "input_ids", "kind": "task_tokens", "dtype": "int64", "maxLength": 16 }
            ],
            "outputs": [
                { "name": "actions", "kind": "joint_position_chunk", "dtype": "float32" },
                { "name": "done", "kind": "done_flag", "optional": true }
            ],
            "control": { "chunkSize": 4 },
            "files": { "model.onnx": onnx_sha }
        })
        .to_string()
    }

    #[test]
    fn extracts_valid_package_and_verifies_sha256() {
        let root = temp_dir("valid");
        let zip_path = root.join("pkg.zip");
        let dest = root.join("out");
        let model_bytes = b"fake-onnx-model-bytes";
        let manifest = sample_manifest(&sha256_bytes(model_bytes));
        write_zip(
            &zip_path,
            &[
                ("manifest.json", manifest.as_bytes()),
                ("model.onnx", model_bytes),
                ("tokenizer.json", b"{}"),
            ],
        );

        let extracted = extract(&zip_path, &dest, 1024 * 1024).unwrap();
        assert_eq!(extracted.manifest.code, "yellowarm-smolvla");
        assert_eq!(extracted.manifest.required_cameras(), vec!["front"]);
        assert_eq!(extracted.package_sha256.len(), 64);
        assert!(extracted.package_bytes > 0);
        assert!(dest.join("model.onnx").is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_missing_manifest() {
        let root = temp_dir("missing-manifest");
        let zip_path = root.join("pkg.zip");
        write_zip(&zip_path, &[("model.onnx", b"abc")]);
        let err = extract(&zip_path, &root.join("out"), 1024 * 1024).unwrap_err();
        assert!(err.to_string().contains("manifest.json"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_path_traversal_entry() {
        let root = temp_dir("traversal");
        let zip_path = root.join("pkg.zip");
        write_zip(&zip_path, &[("../escape.txt", b"bad")]);
        assert!(extract(&zip_path, &root.join("out"), 1024 * 1024).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_zip_bomb_exceeding_unpacked_limit() {
        let root = temp_dir("bomb");
        let zip_path = root.join("pkg.zip");
        let big = vec![b'A'; 4096];
        let manifest = sample_manifest(&sha256_bytes(&big));
        write_zip(
            &zip_path,
            &[
                ("manifest.json", manifest.as_bytes()),
                ("model.onnx", &big),
                ("tokenizer.json", b"{}"),
            ],
        );
        let err = extract(&zip_path, &root.join("out"), 1024).unwrap_err();
        assert!(err.to_string().contains("unpacked size exceeds"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_wrong_file_sha256() {
        let root = temp_dir("bad-sha");
        let zip_path = root.join("pkg.zip");
        let manifest = sample_manifest(&"0".repeat(64));
        write_zip(
            &zip_path,
            &[
                ("manifest.json", manifest.as_bytes()),
                ("model.onnx", b"actual-bytes"),
                ("tokenizer.json", b"{}"),
            ],
        );
        let err = extract(&zip_path, &root.join("out"), 1024 * 1024).unwrap_err();
        assert!(err.to_string().contains("SHA-256 mismatch"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_unknown_manifest_field() {
        let root = temp_dir("unknown-field");
        let zip_path = root.join("pkg.zip");
        let mut value: serde_json::Value =
            serde_json::from_str(&sample_manifest(&sha256_bytes(b"m"))).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unexpectedField".into(), serde_json::json!(true));
        write_zip(
            &zip_path,
            &[
                ("manifest.json", value.to_string().as_bytes()),
                ("model.onnx", b"m"),
                ("tokenizer.json", b"{}"),
            ],
        );
        assert!(extract(&zip_path, &root.join("out"), 1024 * 1024).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_image_input_without_camera() {
        let root = temp_dir("missing-cam");
        let zip_path = root.join("pkg.zip");
        let mut value: serde_json::Value =
            serde_json::from_str(&sample_manifest(&sha256_bytes(b"m"))).unwrap();
        value["inputs"][1]
            .as_object_mut()
            .unwrap()
            .remove("camera");
        write_zip(
            &zip_path,
            &[
                ("manifest.json", value.to_string().as_bytes()),
                ("model.onnx", b"m"),
                ("tokenizer.json", b"{}"),
            ],
        );
        let err = extract(&zip_path, &root.join("out"), 1024 * 1024).unwrap_err();
        assert!(err.to_string().contains("camera"));
        let _ = fs::remove_dir_all(root);
    }
}
