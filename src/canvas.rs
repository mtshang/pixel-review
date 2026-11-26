use super::{
    canvas_pipe as pipe, clean, finish_args, info, numbers, png_dest, rect, selection, write_lock,
    Image, Point, PreviewScale, Result, Rows, View,
};
use crate::history::{Change, History};
use crate::layers::{Layer, Stack};
use crate::palette;
use crate::{project, ChangeTracker};
use pico_args::Arguments;
use serde_json::{json, Value};
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::hash::{DefaultHasher, Hasher};
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::os::windows::io::AsRawHandle;
use std::os::windows::{fs::OpenOptionsExt, process::CommandExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use windows_sys::Win32::Foundation::{
    SetHandleInformation, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

// Keep the digest of the compressed file so two canvases opened on the same
// PNG cannot silently overwrite each other's saved changes.
pub(crate) fn digest(file: &mut File) -> Result<u64> {
    file.rewind()?;
    let mut remaining = file.metadata()?.len();
    let mut hash = DefaultHasher::new();
    let mut buf = [0; 65536];
    while remaining != 0 {
        let n = remaining.min(buf.len() as u64) as usize;
        file.read_exact(&mut buf[..n])?;
        hash.write(&buf[..n]);
        remaining -= n as u64;
    }
    file.rewind()?;
    Ok(hash.finish())
}

pub(crate) fn read_snapshot(path: &Path) -> Result<File> {
    Ok(OpenOptions::new().read(true).share_mode(1).open(path)?)
}

struct Canvas {
    name: String,
    source: PathBuf,
    saved_digest: u64,
    full: View,
    stack: Stack,
    project: Option<project::Revision>,
    history: History,
}

impl Canvas {
    fn open(name: String, source: PathBuf, create: Option<Point>) -> Result<Self> {
        if create.is_none() && source.is_dir() {
            let (revision, full, stack) = project::open(&source)?;
            return Ok(Self {
                name,
                source,
                saved_digest: 0,
                full,
                stack,
                project: Some(revision),
                history: History::default(),
            });
        }
        let (image, saved_digest) = if let Some(size) = create {
            let image = Image::blank(size)?;
            let mut tmp = image.stage(&source, image.view())?;
            let stamp = digest(tmp.as_file_mut())?;
            tmp.persist_noclobber(&source)?;
            (image, stamp)
        } else {
            let mut file = read_snapshot(&source)?;
            let stamp = digest(&mut file)?;
            let reader = Rows::from_file(file)?;
            let area = [0, 0, reader.width, reader.height];
            (Image::load(reader, area)?, stamp)
        };
        let full = image.view();
        Ok(Self {
            name,
            source,
            saved_digest,
            full,
            stack: Stack {
                palette: Default::default(),
                layers: vec![Layer::new("Base".into(), image, [0, 0])?],
                active: "Base".into(),
            },
            project: None,
            history: History::default(),
        })
    }

    fn status(&self) -> Value {
        let [undo, redo] = self.history.counts();
        json!({"canvas":self.name,"pid":std::process::id(),"source":info(&self.source,self.full.w,self.full.h),
            "storage":if self.project.is_some() {"project"} else {"png"}, "active_layer":self.stack.active,
            "layers":self.stack.layers.iter().map(Layer::description).collect::<Vec<_>>(),
            "palette":palette::description(&self.stack.palette),"dirty":self.history.dirty(),"undo_steps":undo,"redo_steps":redo})
    }

    fn save(&mut self, out: Option<PathBuf>) -> Result<()> {
        if self.project.is_some() || !self.stack.is_plain([self.full.w, self.full.h]) {
            return Err(
                "preserve layers with canvas save --project NEW_DIR; use canvas export for a PNG"
                    .into(),
            );
        }
        let save_as = out.is_some();
        let dest = match out {
            Some(p) => png_dest(p, true)?,
            None => self.source.clone(),
        };
        let _lock = write_lock(&dest)?;
        let check_current = || -> Result<()> {
            if digest(&mut read_snapshot(&dest)?)? != self.saved_digest {
                return Err("PNG changed on disk; save with --out NEW.png".into());
            }
            Ok(())
        };
        if save_as {
            if dest.exists() {
                return Err("--out requires a new PNG path".into());
            }
        } else if !self.history.dirty() {
            return check_current();
        }
        let mut stage = self.stack.layers[0].image.stage(&dest, self.full)?;
        let saved_digest = digest(stage.as_file_mut())?;
        if save_as {
            stage.persist_noclobber(&dest)?;
        } else {
            check_current()?;
            stage.persist(&dest)?;
        }
        self.source = dest;
        self.saved_digest = saved_digest;
        self.history.saved();
        Ok(())
    }

    fn execute(&mut self, action: &str, cwd: &Path, mut args: Arguments) -> Result<Value> {
        let mut details = json!({});
        let mut change = None;
        let named: Option<String> = args.opt_value_from_str("--layer")?;
        match action {
            "status" if named.is_none() => finish_args(args)?,
            "undo" | "redo" if named.is_none() => {
                finish_args(args)?;
                self.history
                    .step(&mut self.stack, &mut self.full, action == "redo")?;
            }
            "resize" if named.is_none() => {
                let old = [self.full.w, self.full.h];
                let resize = crate::resize::Resize::parse(&mut args, old)?;
                let pad = args.values_from_str::<_, String>("--pad-layer")?;
                finish_args(args)?;
                details["resize"] = resize.report(old);
                let plain = self.project.is_none() && self.stack.is_plain(old);
                change = resize.apply(&mut self.stack, &mut self.full, plain, &pad)?;
            }
            "palette-use" if named.is_none() => {
                let path: PathBuf = args.value_from_str("--palette")?;
                finish_args(args)?;
                let pal = palette::load(&cwd.join(path))?;
                if pal != self.stack.palette {
                    change = Some(Change::Palette(std::mem::replace(
                        &mut self.stack.palette,
                        pal,
                    )));
                }
            }
            "palette-check" | "recolor" => {
                let source: Option<PathBuf> = args.opt_value_from_str("--palette")?;
                let external = source.map(|p| palette::load(&cwd.join(p))).transpose()?;
                let pal = external.as_ref().unwrap_or(&self.stack.palette);
                let indices: Vec<_> = match named.as_deref() {
                    Some(s) => vec![self.stack.index(Some(s))?],
                    None => (0..self.stack.layers.len()).collect(),
                };
                let mut reports = Vec::new();
                if action == "palette-check" {
                    let allowed = palette::allowed(&mut args, pal)?;
                    finish_args(args)?;
                    for i in indices {
                        let layer = &self.stack.layers[i];
                        let view = layer.image.view();
                        let mut audit = palette::Audit::default();
                        for y in 0..view.h {
                            audit.row(y, layer.image.row_span(0, y, view.w), &allowed);
                        }
                        let mut report = audit.report();
                        report["layer"] = json!(layer.name);
                        reports.push(report);
                    }
                    details["ok"] = json!(reports.iter().all(|r| r["ok"] == true));
                } else {
                    let map = palette::mappings(&mut args, pal)?;
                    finish_args(args)?;
                    let mut edits = Vec::new();
                    for i in indices {
                        let layer = &mut self.stack.layers[i];
                        let mut tracker = ChangeTracker::default();
                        let count = palette::replace(&mut layer.image, &map, Some(&mut tracker));
                        if count > 0 {
                            edits.push((i, tracker));
                            layer.file = None;
                        }
                        reports.push(json!({"layer":layer.name,"changed_pixels":count}));
                    }
                    if !edits.is_empty() {
                        change = Some(Change::Pixels(edits));
                    }
                }
                details["reports"] = json!(reports);
            }
            "layer-add" => {
                let name = named.ok_or("supply --layer NAME")?;
                let source: Option<PathBuf> = args.opt_value_from_str("--source")?;
                let size: Option<String> = args.opt_value_from_str("--size")?;
                let at = layer_at(&mut args)?.unwrap_or([0, 0]);
                finish_args(args)?;
                let image = match (source, size) {
                    (Some(path), None) => {
                        let reader = Rows::open(&cwd.join(path))?;
                        let area = [0, 0, reader.width, reader.height];
                        Image::load(reader, area)?
                    }
                    (None, size) => Image::blank(
                        size.as_deref()
                            .map(numbers)
                            .transpose()?
                            .unwrap_or([self.full.w, self.full.h]),
                    )?,
                    _ => return Err("choose --source PNG or --size W,H".into()),
                };
                let active = self.stack.active.clone();
                self.stack.add(Layer::new(name, image, at)?)?;
                change = Some(Change::Slot {
                    index: self.stack.layers.len() - 1,
                    absent: None,
                    active,
                });
            }
            "layer-select" | "layer-remove" | "layer-set" => {
                let name = named.ok_or("supply --layer NAME")?;
                let i = self.stack.index(Some(&name))?;
                match action {
                    "layer-select" => {
                        finish_args(args)?;
                        if self.stack.active != name {
                            change = Some(Change::Select(std::mem::replace(
                                &mut self.stack.active,
                                name,
                            )));
                        }
                    }
                    "layer-remove" => {
                        finish_args(args)?;
                        if self.stack.layers.len() == 1 {
                            return Err("cannot remove last layer".into());
                        }
                        let active = self.stack.active.clone();
                        let layer = self.stack.layers.remove(i);
                        if self.stack.active == name {
                            self.stack.active = self.stack.layers[i.saturating_sub(1)].name.clone();
                        }
                        change = Some(Change::Slot {
                            index: i,
                            absent: Some(Box::new(layer)),
                            active,
                        });
                    }
                    _ => {
                        let at = layer_at(&mut args)?;
                        let visible: Option<bool> = args.opt_value_from_str("--visible")?;
                        let index: Option<usize> = args.opt_value_from_str("--index")?;
                        finish_args(args)?;
                        if at.is_none() && visible.is_none() && index.is_none() {
                            return Err("supply --at X,Y, --visible true|false or --index N".into());
                        }
                        let index = index.unwrap_or(i);
                        if index >= self.stack.layers.len() {
                            return Err("layer index out of range".into());
                        }
                        let layer = &mut self.stack.layers[i];
                        let at = at.unwrap_or(layer.at);
                        let visible = visible.unwrap_or(layer.visible);
                        if at != layer.at || visible != layer.visible || index != i {
                            change = Some(Change::Layout {
                                current: index,
                                previous: i,
                                at: layer.at,
                                visible: layer.visible,
                            });
                        }
                        layer.at = at;
                        layer.visible = visible;
                        if i != index {
                            let layer = self.stack.layers.remove(i);
                            self.stack.layers.insert(index, layer);
                        }
                    }
                }
            }
            "edit" => {
                let path: PathBuf = args.value_from_str("--patch")?;
                let external = args
                    .opt_value_from_str::<_, PathBuf>("--palette")?
                    .map(|p| palette::load(&cwd.join(p)))
                    .transpose()?;
                finish_args(args)?;
                let i = self.stack.index(named.as_deref())?;
                let layer = &mut self.stack.layers[i];
                let native = layer.image.view();
                let mut tracker = ChangeTracker::default();
                match crate::patch::apply(
                    &cwd.join(path),
                    native,
                    external.as_ref().unwrap_or(&self.stack.palette),
                    |[x, y], rgba| {
                        layer.image.set(x, y, rgba, Some(&mut tracker));
                    },
                ) {
                    Ok(report) => details["patch"] = report,
                    Err(e) => {
                        tracker.rollback(&mut layer.image);
                        return Err(e);
                    }
                }
                if tracker.changed(&layer.image) {
                    change = Some(Change::Pixels(vec![(i, tracker)]));
                    layer.file = None;
                }
                details["layer"] = json!(layer.name);
            }
            "copy" => {
                let areas = selection::areas(&mut args)?;
                let clip_path = cwd.join(args.value_from_str::<_, PathBuf>("--clip")?);
                finish_args(args)?;
                let layer = &self.stack.layers[self.stack.index(named.as_deref())?];
                let clip = selection::Clip::from_image(areas, &layer.image, layer.image.view())?;
                details["layer"] = json!(layer.name);
                clip.save(&clip_path)?;
                details["clip"] = json!(clean(clip_path.canonicalize()?));
                details["selection"] = clip.report();
            }
            "paste" => {
                let clip_path = cwd.join(args.value_from_str::<_, PathBuf>("--clip")?);
                let skip = args.contains("--skip-transparent");
                let at = args
                    .values_from_str::<_, String>("--at")?
                    .iter()
                    .map(|s| numbers(s))
                    .collect::<Result<Vec<Point>>>()?;
                finish_args(args)?;
                let clip = selection::Clip::read(&clip_path)?;
                let i = self.stack.index(named.as_deref())?;
                let layer = &mut self.stack.layers[i];
                let native = layer.image.view();
                let mut tracker = ChangeTracker::default();
                clip.paste_many(&mut layer.image, native, &at, skip, &mut tracker)?;
                if tracker.changed(&layer.image) {
                    layer.file = None;
                    change = Some(Change::Pixels(vec![(i, tracker)]));
                }
                details["layer"] = json!(layer.name);
                details["targets"] = json!(at);
                details["selection"] = clip.report();
            }
            "preview" | "export" => {
                let (scale, region) = if action == "preview" {
                    (
                        PreviewScale::parse(&mut args)?,
                        args.opt_value_from_str::<_, String>("--region")?,
                    )
                } else {
                    (PreviewScale::Fixed(1), None)
                };
                let out = cwd.join(args.value_from_str::<_, PathBuf>("--out")?);
                finish_args(args)?;
                let dest = png_dest(out, true)?;
                let layer = named
                    .as_deref()
                    .map(|s| self.stack.index(Some(s)))
                    .transpose()?
                    .map(|i| &self.stack.layers[i]);
                let native = layer.map(|l| l.image.view()).unwrap_or(self.full);
                let view = scale.view(
                    region
                        .as_deref()
                        .map(numbers)
                        .transpose()?
                        .unwrap_or(native.area()),
                    [native.w, native.h],
                )?;
                let stage = match layer {
                    Some(l) => l.image.stage(&dest, view)?,
                    None => self.stack.render(&dest, view)?,
                };
                stage.persist_noclobber(&dest)?;
                details[action] = info(&dest, view.width(), view.height());
                details["layer"] = json!(named);
                details["region"] = rect(view.area());
                details["scale"] = json!(view.scale);
            }
            "save" if named.is_none() => {
                let out: Option<PathBuf> = args.opt_value_from_str("--out")?;
                let folder: Option<PathBuf> = args.opt_value_from_str("--project")?;
                finish_args(args)?;
                let size = [self.full.w, self.full.h];
                match (folder, out) {
                    (Some(_), Some(_)) => return Err("choose --out or --project".into()),
                    (Some(folder), None) => {
                        let dir = cwd.join(folder);
                        let revision = project::save(&dir, size, &mut self.stack, None)?;
                        self.project = Some(revision);
                        self.source = clean(dir);
                        self.history.saved();
                    }
                    (None, None) if self.project.is_some() => {
                        self.project = Some(project::save(
                            &self.source,
                            size,
                            &mut self.stack,
                            self.project.as_ref(),
                        )?);
                        self.history.saved();
                    }
                    (None, out) => self.save(out.map(|p| cwd.join(p)))?,
                }
            }
            "close" if named.is_none() => {
                let discard = args.contains("--discard");
                finish_args(args)?;
                if self.history.dirty() && !discard {
                    return Err("unsaved canvas: save first or close --discard".into());
                }
            }
            _ => return Err("unknown canvas command".into()),
        }
        if let Some(entry) = change {
            self.history.record(entry);
        }
        let mut reply = self.status();
        reply["command"] = json!(action);
        reply
            .as_object_mut()
            .unwrap()
            .append(details.as_object_mut().unwrap());
        Ok(reply)
    }
}

fn layer_at(args: &mut Arguments) -> Result<Option<[i32; 2]>> {
    let Some(raw) = args.opt_value_from_str::<_, String>("--at")? else {
        return Ok(None);
    };
    let (x, y) = raw.split_once(',').ok_or("expected --at X,Y")?;
    Ok(Some([x.parse()?, y.parse()?]))
}

fn start(name: &str, source: &Path, create: Option<Point>) -> Result<Value> {
    // The CLI may itself have inherited redirected handles (e.g. Python's
    // capture_output). A resident child must not keep the caller's pipes open.
    for handle in [
        std::io::stdin().as_raw_handle(),
        std::io::stdout().as_raw_handle(),
        std::io::stderr().as_raw_handle(),
    ] {
        if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
            if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
    }
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["__canvas-worker", "--canvas", name, "--source"])
        .arg(source);
    if let Some([w, h]) = create {
        command.arg("--size").arg(format!("{w},{h}"));
    }
    let mut child = command
        .current_dir(std::env::temp_dir())
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut line = String::new();
    BufReader::new(
        child
            .stdout
            .take()
            .ok_or("missing worker startup channel")?,
    )
    .read_line(&mut line)?;
    if line.is_empty() {
        return Err(format!("canvas worker exited during open: {}", child.wait()?).into());
    }
    Ok(serde_json::from_str(&line)?)
}

pub fn client(mut args: Arguments) -> Result<(Option<Value>, i32)> {
    let action = args.subcommand()?.ok_or("supply a canvas command")?;
    let name: String = args.value_from_str("--canvas")?;
    let reply = if action == "create" {
        let size = numbers(&args.value_from_str::<_, String>("--size")?)?;
        let out = args.value_from_str::<_, PathBuf>("--out")?;
        finish_args(args)?;
        start(&name, &png_dest(out, true)?, Some(size))?
    } else if action == "open" {
        let source: PathBuf = args.value_from_str("--source")?;
        finish_args(args)?;
        start(&name, &clean(source.canonicalize()?), None)?
    } else {
        let rest = args
            .finish()
            .into_iter()
            .map(|s| s.into_string().map_err(|_| "expected Unicode arguments"))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut channel = pipe::connect(&name)?;
        pipe::send(
            &mut channel,
            &serde_json::to_vec(&(name, action, std::env::current_dir()?, rest))?,
        )?;
        serde_json::from_slice(&pipe::receive(&mut channel)?)?
    };
    let code = if reply.get("error").is_some() { 2 } else { 0 };
    Ok((Some(reply), code))
}

fn dispatch(canvas: &mut Canvas, bytes: &[u8]) -> Result<(Value, bool)> {
    let (name, action, cwd, args): (String, String, PathBuf, Vec<String>) =
        serde_json::from_slice(bytes)?;
    if name != canvas.name || !cwd.is_absolute() {
        return Err("invalid canvas request".into());
    }
    let args = Arguments::from_vec(args.into_iter().map(OsString::from).collect());
    Ok((canvas.execute(&action, &cwd, args)?, action == "close"))
}

pub fn worker(mut args: Arguments) -> Result<()> {
    let name: String = args.value_from_str("--canvas")?;
    let source: PathBuf = args.value_from_str("--source")?;
    let size = args
        .opt_value_from_str::<_, String>("--size")?
        .as_deref()
        .map(numbers)
        .transpose()?;
    finish_args(args)?;
    let mut server = pipe::bind(&name)?;
    let mut canvas = Canvas::open(name, source, size)?;
    let mut ready = canvas.status();
    ready["command"] = json!(if size.is_some() { "create" } else { "open" });
    writeln!(std::io::stdout().lock(), "{ready}")?;
    loop {
        pipe::accept(&server)?;
        let (reply, close) = pipe::receive(&mut server)
            .and_then(|data| dispatch(&mut canvas, &data))
            .unwrap_or_else(|e| (json!({"error":e.to_string()}), false));
        // A client may exit before reading its reply. Keep other clients' canvas alive.
        let _ = pipe::send(&mut server, &serde_json::to_vec(&reply)?);
        pipe::disconnect(&server);
        if close {
            return Ok(());
        }
    }
}
