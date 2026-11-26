use crate::{clean, finish_args, output_dest, patch, Result};
use pico_args::Arguments;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

const FORMAT: &str = "pixel-review.animation/1";

pub struct Sequence {
    pub paths: Vec<PathBuf>,
    pub fps: u16,
    pub baseline: Option<u32>,
    pub config: Option<PathBuf>,
}

impl Sequence {
    pub fn read(args: &mut Arguments, allow_fps: bool) -> Result<Self> {
        let mut paths = args.values_from_str::<_, PathBuf>("--frame")?;
        let config = args
            .opt_value_from_str::<_, PathBuf>("--config")?
            .map(|p| p.canonicalize().map(clean))
            .transpose()?;
        let mut fps = 8;
        let mut baseline = None;
        if let Some(file) = &config {
            if !paths.is_empty() {
                return Err("choose --config or repeated --frame".into());
            }
            let doc = patch::read_json(file)?;
            if doc["format"] != FORMAT {
                return Err("unsupported animation config".into());
            }
            let relative: Vec<PathBuf> = serde_json::from_value(doc["frames"].clone())?;
            paths = relative
                .into_iter()
                .map(|p| file.parent().unwrap().join(p))
                .collect();
            if let Some(v) = doc.get("fps") {
                fps = serde_json::from_value(v.clone())?;
            }
            baseline = serde_json::from_value(doc["baseline"].clone())?;
        }
        if allow_fps {
            fps = args.opt_value_from_str("--fps")?.unwrap_or(fps);
        }
        if fps == 0 {
            return Err("--fps must be positive".into());
        }
        let override_y = args.opt_value_from_str("--baseline")?;
        if args.contains("--no-baseline") {
            if override_y.is_some() {
                return Err("choose --baseline or --no-baseline".into());
            }
            baseline = None;
        } else if override_y.is_some() {
            baseline = override_y;
        }
        if paths.len() < 2 {
            return Err("supply at least two frames in playback order".into());
        }
        let paths = paths
            .into_iter()
            .map(|p| p.canonicalize().map(clean))
            .collect::<std::io::Result<_>>()?;
        Ok(Self {
            paths,
            fps,
            baseline,
            config,
        })
    }
}

fn relative_to(file: &Path, folder: &Path) -> PathBuf {
    let mut common = folder;
    let mut prefix = PathBuf::new();
    loop {
        if let Ok(tail) = file.strip_prefix(common) {
            return prefix.join(tail);
        }
        let Some(parent) = common.parent() else {
            return file.to_owned();
        };
        prefix.push("..");
        common = parent;
    }
}

pub fn save(mut args: Arguments) -> Result<Value> {
    let sequence = Sequence::read(&mut args, true)?;
    let out = output_dest(args.value_from_str("--out")?, true, &["json"])?;
    finish_args(args)?;
    let mut size = None;
    for path in &sequence.paths {
        let r = crate::Rows::open(path)?;
        if size.is_some_and(|s| s != [r.width, r.height]) {
            return Err(format!("frame size mismatch: {}", path.display()).into());
        }
        if sequence.baseline.is_some_and(|y| y >= r.height) {
            return Err("baseline outside frame".into());
        }
        size = Some([r.width, r.height]);
    }
    let parent = out.parent().unwrap();
    let frames: Vec<_> = sequence
        .paths
        .iter()
        .map(|p| relative_to(p, parent))
        .collect();
    let doc =
        json!({"format":FORMAT,"frames":frames,"fps":sequence.fps,"baseline":sequence.baseline});
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut tmp, &doc)?;
    writeln!(tmp)?;
    tmp.persist_noclobber(&out)?;
    Ok(
        json!({"command":"animation-config","config":out,"frame_count":frames.len(),"native_size":size}),
    )
}
