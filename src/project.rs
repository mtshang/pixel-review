use crate::canvas::{digest, read_snapshot};
use crate::layers::{Layer, Stack};
use crate::{write_lock, Image, Point, Result, Rows, View};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

const MANIFEST: &str = "project.json";
const FORMAT: &str = "pixel-review.project/1";

pub struct Revision {
    manifest: u64,
    assets: BTreeMap<String, u64>,
}

fn asset_path(dir: &Path, name: &str) -> Result<PathBuf> {
    let id = name
        .strip_prefix("layer-")
        .and_then(|s| s.strip_suffix(".png"))
        .ok_or("invalid layer filename")?;
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err("invalid layer filename".into());
    }
    let path = dir.join(name);
    if !fs::symlink_metadata(&path)?.is_file() {
        return Err("expected a regular layer PNG".into());
    }
    Ok(path)
}

pub fn open(dir: &Path) -> Result<(Revision, View, Stack)> {
    // Keep this read handle until every referenced PNG is loaded. Writers
    // cannot replace the manifest and retire those files during the read.
    let mut file = read_snapshot(&dir.join(MANIFEST))?;
    let manifest = digest(&mut file)?;
    let doc: Value = serde_json::from_reader(&file)?;
    if doc["format"] != FORMAT && doc["format"] != "pixel-review.project/2" {
        return Err("unsupported project format".into());
    }
    let size: Point = serde_json::from_value(doc["size"].clone())?;
    let full = View::new([0, 0, size[0], size[1]], 1, size)?;
    let mut stack = Stack {
        palette: doc.get("palette").map(crate::palette::parse).transpose()?.unwrap_or_default(),
        layers: Vec::new(),
        active: String::new(),
    };
    let mut assets = BTreeMap::new();
    for item in doc["layers"].as_array().ok_or("missing project layers")? {
        let name = item["name"]
            .as_str()
            .ok_or("missing layer name")?
            .to_owned();
        let at = serde_json::from_value(item["at"].clone())?;
        let leaf = item["file"].as_str().ok_or("missing layer filename")?;
        let mut png = read_snapshot(&asset_path(dir, leaf)?)?;
        if assets.insert(leaf.to_owned(), digest(&mut png)?).is_some() {
            return Err("duplicate layer file".into());
        }
        let rows = Rows::from_file(png)?;
        let area = [0, 0, rows.width, rows.height];
        let mut layer = Layer::new(name, Image::load(rows, area)?, at)?;
        layer.visible = item["visible"]
            .as_bool()
            .ok_or("missing layer visibility")?;
        layer.file = Some(leaf.to_owned());
        stack.add(layer)?;
    }
    let active = doc["active"].as_str().ok_or("missing active layer")?;
    stack.index(Some(active))?;
    stack.active = active.to_owned();
    Ok((Revision { manifest, assets }, full, stack))
}

impl Revision {
    fn check(&self, dir: &Path) -> Result<()> {
        let conflict = "project changed on disk; use save --project NEW_DIR";
        if digest(&mut read_snapshot(&dir.join(MANIFEST))?)? != self.manifest {
            return Err(conflict.into());
        }
        for (name, expected) in &self.assets {
            if digest(&mut read_snapshot(&asset_path(dir, name)?)?)? != *expected {
                return Err(conflict.into());
            }
        }
        Ok(())
    }
}

// Unchanged layer files are shared by successive saves. Publish the manifest
// last, then retire superseded files while still holding the writer lock.
fn commit(
    dir: &Path,
    size: Point,
    stack: &Stack,
    old: Option<&Revision>,
) -> Result<(Revision, Vec<String>)> {
    let path = dir.join(MANIFEST);
    let _lock = write_lock(&path)?;
    let mut assets = BTreeMap::new();
    let mut pending = Vec::new();
    let mut files = Vec::new();
    let mut entries = Vec::new();
    for layer in &stack.layers {
        let existing = layer
            .file
            .as_ref()
            .and_then(|name| Some((name, old?.assets.get(name)?)));
        let (name, stamp) = if let Some((name, stamp)) = existing {
            (name.clone(), *stamp)
        } else {
            let mut png = layer.image.stage(&path, layer.image.view())?;
            let stamp = digest(png.as_file_mut())?;
            png.as_file().sync_all()?;
            let name = png
                .path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            pending.push(png);
            (name, stamp)
        };
        entries.push(json!({"name":layer.name,"at":layer.at,"visible":layer.visible,"file":name}));
        assets.insert(name.clone(), stamp);
        files.push(name);
    }
    let mut output = NamedTempFile::new_in(dir)?;
    let mut doc = json!({"format":FORMAT,"size":size,"active":stack.active,"layers":entries});
    if !stack.palette.is_empty() {
        doc["format"]=json!("pixel-review.project/2");
        doc["palette"]=crate::palette::description(&stack.palette);
    }
    serde_json::to_writer_pretty(&mut output,&doc)?;
    writeln!(output)?;
    let manifest = digest(output.as_file_mut())?;
    output.as_file().sync_all()?;
    if let Some(old) = old {
        old.check(dir)?;
        output.persist(&path)?;
    } else {
        output.persist_noclobber(&path)?;
    }
    for file in &mut pending {
        file.disable_cleanup(true);
    }
    if let Some(old) = old {
        for name in old.assets.keys().filter(|s| !assets.contains_key(*s)) {
            // A reader may have the obsolete PNG open; the committed project
            // is complete even if Windows denies this optional cleanup.
            let _ = fs::remove_file(dir.join(name));
        }
    }
    Ok((Revision { manifest, assets }, files))
}

pub fn save(
    dir: &Path,
    size: Point,
    stack: &mut Stack,
    old: Option<&Revision>,
) -> Result<Revision> {
    let (revision, files) = if old.is_some() {
        commit(dir, size, stack, old)?
    } else {
        if dir.exists() {
            return Err("--project requires a new directory".into());
        }
        let parent = dir.parent().ok_or("missing project parent")?;
        fs::create_dir_all(parent)?;
        let stage = tempfile::tempdir_in(parent)?;
        let result = commit(stage.path(), size, stack, None)?;
        fs::rename(stage.path(), dir)?;
        let _ = stage.keep();
        result
    };
    for (layer, file) in stack.layers.iter_mut().zip(files) {
        layer.file = Some(file);
    }
    Ok(revision)
}
