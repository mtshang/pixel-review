mod canvas;
mod animation;
mod tiles;
mod review;
mod canvas_pipe;
mod inspect;
mod history;
mod palette;
mod layers;
mod patch;
mod project;
mod raster;
mod resize;
mod sequence;
mod strip;
mod selection;

use pico_args::Arguments;
use raster::{ChangeTracker, Image, Rows};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Point = [u32; 2];

#[derive(Clone, Copy)]
enum PreviewScale {
    Fixed(u32),
    Target(u32),
}

impl PreviewScale {
    fn parse(args: &mut Arguments) -> Result<Self> {
        let fixed = args.opt_value_from_str("--scale")?;
        let target = args.opt_value_from_str("--target-long-edge")?;
        match (fixed, target) {
            (Some(_), Some(_)) => Err("choose --scale or --target-long-edge".into()),
            (Some(n), None) => Ok(Self::Fixed(n)),
            (None, Some(0)) => Err("--target-long-edge must be positive".into()),
            (None, n) => Ok(Self::Target(n.unwrap_or(1600))),
        }
    }

    fn view(self, area: [u32; 4], size: Point) -> Result<View> {
        let scale = match self {
            Self::Fixed(n) => n,
            Self::Target(target) => {
                let edge = area[2].max(area[3]);
                if edge == 0 { return Err("empty source region".into()); }
                let remainder = target % edge;
                // Prefer the smaller candidate at the midpoint.
                (target / edge + u32::from(remainder > edge - remainder)).max(1)
            }
        };
        View::new(area, scale, size)
    }
}

#[derive(Clone, Copy)]
struct View {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    scale: u32,
}

impl View {
    fn new(area: [u32; 4], scale: u32, size: Point) -> Result<Self> {
        let [x, y, w, h] = area;
        if scale == 0
            || w == 0
            || h == 0
            || x > size[0]
            || w > size[0] - x
            || y > size[1]
            || h > size[1] - y
        {
            return Err("invalid scale or source region".into());
        }
        for dim in [w, h] {
            if u64::from(dim) * u64::from(scale) > i32::MAX as u64 {
                return Err("scaled dimension exceeds PNG limit".into());
            }
        }
        Ok(Self { x, y, w, h, scale })
    }
    fn width(self) -> u32 {
        self.w * self.scale
    }
    fn height(self) -> u32 {
        self.h * self.scale
    }
    fn area(self) -> [u32; 4] {
        [self.x, self.y, self.w, self.h]
    }

    fn locate(self, point: Point, from_preview: bool) -> Result<(Point, Point)> {
        let [x, y] = point;
        if from_preview {
            if x >= self.width() || y >= self.height() {
                return Err("preview point outside view".into());
            }
            Ok(([self.x + x / self.scale, self.y + y / self.scale], point))
        } else {
            if x < self.x || x - self.x >= self.w || y < self.y || y - self.y >= self.h {
                return Err("source point outside selected region".into());
            }
            Ok((
                point,
                [(x - self.x) * self.scale, (y - self.y) * self.scale],
            ))
        }
    }
}

fn numbers<const N: usize>(s: &str) -> Result<[u32; N]> {
    let fields = s
        .split(',')
        .map(str::parse)
        .collect::<std::result::Result<Vec<u32>, _>>()?;
    fields
        .try_into()
        .map_err(|_| format!("expected {N} comma-separated integers").into())
}

fn color(s: &str) -> Result<[u8; 4]> {
    let s = s.trim_start_matches('#');
    if !(s.len() == 6 || s.len() == 8) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("expected RRGGBB or RRGGBBAA".into());
    }
    let mut result = [255; 4];
    for (i, value) in result.iter_mut().enumerate().take(s.len() / 2) {
        *value = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)?;
    }
    Ok(result)
}

fn hex(p: &[u8]) -> String {
    format!("#{:02X}{:02X}{:02X}{:02X}", p[0], p[1], p[2], p[3])
}

fn clean(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(p) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{p}"))
    } else if let Some(p) = s.strip_prefix(r"\\?\") {
        PathBuf::from(p)
    } else {
        path
    }
}

fn png_dest(path: PathBuf, create_folder: bool) -> Result<PathBuf> {
    output_dest(path,create_folder,&["png"])
}

fn output_dest(path: PathBuf, create_folder: bool, extensions: &[&str]) -> Result<PathBuf> {
    if !extensions.iter().any(|ext| path.extension().is_some_and(|s| s.eq_ignore_ascii_case(ext))) {
        return Err(format!("expected output extension: {}", extensions.join(" or ")).into());
    }
    let absolute = std::path::absolute(path)?;
    let folder = absolute.parent().ok_or("missing output folder")?;
    if create_folder {
        fs::create_dir_all(folder)?;
    }
    if absolute.exists() {
        if !absolute.is_file() {
            return Err("output points to a directory".into());
        }
        Ok(clean(absolute.canonicalize()?))
    } else {
        Ok(clean(folder.canonicalize()?).join(absolute.file_name().ok_or("missing filename")?))
    }
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn write_lock(path: &Path) -> Result<File> {
    let mut name = path.as_os_str().to_os_string();
    name.push(".pixel-review.lock");
    // The OS removes the sidecar even if a writer is interrupted.
    Ok(OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(0)
        .custom_flags(0x04000000)
        .open(&name)
        .map_err(|e| format!("cannot reserve {} for writing: {e}", path.display()))?)
}

fn distinct(a: &Path, b: &Path) -> Result<()> {
    if a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase() {
        return Err(format!("source and output paths collide: {}", a.display()).into());
    }
    Ok(())
}

fn finish_args(args: Arguments) -> Result<()> {
    let rest = args.finish();
    if rest.is_empty() {
        Ok(())
    } else {
        Err(format!("unexpected arguments: {rest:?}").into())
    }
}

fn info(path: &Path, width: u32, height: u32) -> Value {
    json!({"path":path,"width":width,"height":height})
}

fn rect([x, y, width, height]: [u32; 4]) -> Value {
    json!({"x":x,"y":y,"width":width,"height":height})
}

fn padded_view([x, y, w, h]: [u32;4], padding: u32, size: Point, scale: PreviewScale) -> Result<View> {
    let left = x.saturating_sub(padding);
    let top = y.saturating_sub(padding);
    let end_x = (x+w).saturating_add(padding).min(size[0]);
    let end_y = (y+h).saturating_add(padding).min(size[1]);
    scale.view([left, top, end_x-left, end_y-top], size)
}

fn compare(src: &Image, v: View, mut actual: Rows, limit: usize) -> Result<Value> {
    if actual.width != v.width() || actual.height != v.height() {
        return Ok(
            json!({"match":false,"reason":"dimensions","expected_width":v.width(),
            "expected_height":v.height(),"mismatch_pixels":null,"samples":[]}),
        );
    }
    let mut expected = Vec::new();
    let mut row = Vec::new();
    let mut count = 0u64;
    let mut samples = Vec::new();
    for y in 0..v.height() {
        if y % v.scale == 0 {
            src.expand_row(v, y / v.scale, &mut expected)?;
        }
        actual.read_span(0, v.width(), &mut row)?;
        if row == expected {
            continue;
        }
        for (x, (a, e)) in row
            .chunks_exact(4)
            .zip(expected.chunks_exact(4))
            .enumerate()
        {
            if a == e {
                continue;
            }
            count += 1;
            if samples.len() < limit {
                let p = [x as u32, y];
                samples.push(json!({"preview_xy":p,"source_xy":v.locate(p,true)?.0,
                    "expected_rgba":hex(e),"actual_rgba":hex(a)}));
            }
        }
    }
    actual.finish()?;
    Ok(json!({"match":count==0,"mismatch_pixels":count,"samples":samples}))
}

fn run() -> Result<(Option<Value>, i32)> {
    let mut args = Arguments::from_env();
    if args.contains(["-h", "--help"]) {
        return Ok((Some(json!({"usage":HELP})), 0));
    }
    let cmd = args.subcommand()?.ok_or("missing command; use --help")?;
    if cmd == "review-sheet" { return Ok((Some(review::sheet(args)?),0)); }
    if cmd=="resize" { return Ok((Some(resize::file(args)?),0)); }
    if cmd=="animation-config" { return Ok((Some(sequence::save(args)?),0)); }
    if cmd == "tile-preview" { return Ok((Some(tiles::preview(args)?),0)); }
    if cmd == "animation" || cmd == "onion" { return Ok((Some(animation::preview(&cmd,args)?),0)); }
    if cmd == "palette-check" || cmd == "recolor" {
        return Ok((Some(palette::batch(&cmd,args)?),0));
    }
    if cmd == "create" {
        let size = numbers(&args.value_from_str::<_,String>("--size")?)?;
        let path = args.value_from_str::<_,PathBuf>("--out")?;
        finish_args(args)?;
        let dest=png_dest(path,true)?;
        let image=Image::blank(size)?;
        image.stage(&dest,image.view())?.persist_noclobber(&dest)?;
        return Ok((Some(json!({"command":cmd,"source":info(&dest,size[0],size[1])})),0));
    }
    if cmd == "canvas" {
        return canvas::client(args);
    }
    if cmd == "__canvas-worker" {
        canvas::worker(args)?;
        return Ok((None, 0));
    }
    if cmd == "licenses" {
        let path: PathBuf = args.value_from_str("--out")?;
        finish_args(args)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        file.write_all(include_bytes!("../references/third-party.zip"))?;
        return Ok((Some(json!({"licenses_zip":clean(path.canonicalize()?)})), 0));
    }
    if ![
        "zoom",
        "compare",
        "pick",
        "edit",
        "bounds",
        "matrix",
        "edit-matrix",
        "copy",
    ]
    .contains(&cmd.as_str())
    {
        return Err("unknown command; use --help".into());
    }
    let source: PathBuf = args.value_from_str("--source")?;
    let source = clean(source.canonicalize()?);
    if cmd == "copy" {
        return Ok((Some(selection::copy(&source, args)?), 0));
    }
    if cmd == "edit-matrix" {
        let patch: PathBuf = args.value_from_str("--patch")?;
        let dest: PathBuf = args.value_from_str("--out")?;
        let pal=args.opt_value_from_str::<_,PathBuf>("--palette")?.as_deref().map(palette::load).transpose()?.unwrap_or_default();
        finish_args(args)?;
        return Ok((Some(patch::edit_matrix(&source, &patch, &dest,&pal)?), 0));
    }
    if cmd == "bounds" {
        finish_args(args)?;
        return Ok((Some(inspect::bounds(&source)?), 0));
    }
    if cmd == "matrix" {
        let mut regions = Vec::new();
        for spec in args.values_from_str::<_, String>("--point")? {
            let [x, y] = numbers(&spec)?;
            regions.push([x, y, 1, 1]);
        }
        for spec in args.values_from_str::<_, String>("--region")? {
            regions.push(numbers(&spec)?);
        }
        let out: Option<PathBuf> = args.opt_value_from_str("--out")?;
        finish_args(args)?;
        return Ok((inspect::matrix(&source, &regions, out)?, 0));
    }
    let scaling = PreviewScale::parse(&mut args)?;
    let region: Option<String> = args.opt_value_from_str("--region")?;
    let editing = cmd == "edit";
    let writing = editing || cmd == "zoom";
    let in_place = editing && args.contains("--in-place");
    let edited = if editing {
        let out: Option<PathBuf> = args.opt_value_from_str("--edited-source")?;
        if in_place && out.is_some() {
            return Err("choose --in-place or --edited-source".into());
        }
        if in_place {
            source.clone()
        } else {
            let path = png_dest(
                out.unwrap_or_else(|| {
                    source.with_file_name(format!("{}.edited.png", stem(&source)))
                }),
                true,
            )?;
            distinct(&source, &path)?;
            if path.exists() {
                return Err("edited output exists; choose a fresh --edited-source or use --in-place on that file".into());
            }
            path
        }
    } else {
        source.clone()
    };
    let mut locks = Vec::new();
    if editing {
        locks.push(write_lock(&edited)?);
    }
    let reader = Rows::open(&source)?;
    let size = [reader.width, reader.height];
    let full = [0, 0, size[0], size[1]];
    let v = scaling.view(
        region.as_deref().map(numbers).transpose()?.unwrap_or(full),
        size,
    )?;
    let scale = v.scale;
    let suffix = if region.is_some() {
        format!(".{}-{}-{}-{}", v.x, v.y, v.w, v.h)
    } else {
        String::new()
    };
    let default_name = format!("{}{suffix}@{scale}x.png", stem(&edited));
    let preview = if writing {
        let dir: Option<PathBuf> = args.opt_value_from_str("--out-dir")?;
        let name: String = args
            .opt_value_from_str("--out-name")?
            .unwrap_or(default_name);
        if Path::new(&name).file_name() != Some(name.as_ref()) || name.contains(':') {
            return Err("--out-name takes a filename only".into());
        }
        png_dest(
            dir.unwrap_or_else(|| edited.parent().unwrap().into())
                .join(name),
            true,
        )?
    } else {
        let p: Option<PathBuf> = args.opt_value_from_str("--preview")?;
        clean(
            p.unwrap_or_else(|| source.with_file_name(default_name))
                .canonicalize()?,
        )
    };
    if writing {
        distinct(&source, &preview)?;
        distinct(&edited, &preview)?;
        locks.push(write_lock(&preview)?);
    }
    let mut out = json!({"command":cmd,"source":info(&edited,size[0],size[1]),
        "preview":info(&preview,v.width(),v.height()),"out_dir":preview.parent(),
        "out_name":preview.file_name().unwrap().to_string_lossy(),"scale":scale,
        "region":{"x":v.x,"y":v.y,"width":v.w,"height":v.h}});
    if let PreviewScale::Target(target) = scaling {
        out["target_long_edge"] = json!(target);
    }
    match cmd.as_str() {
        "zoom" => {
            finish_args(args)?;
            Image::load(reader, v.area())?
                .stage(&preview, v)?
                .persist(&preview)?;
        }
        "compare" => {
            let limit = args.opt_value_from_str("--sample-limit")?.unwrap_or(8);
            finish_args(args)?;
            let actual = Rows::open(&preview)?;
            out["preview"] = info(&preview, actual.width, actual.height);
            let result = compare(&Image::load(reader, v.area())?, v, actual, limit)?;
            let code = if result["match"] == true { 0 } else { 1 };
            out["comparison"] = result;
            return Ok((Some(out), code));
        }
        "pick" => {
            let mut requests = Vec::new();
            for (key, from_preview) in [("--pick-source", false), ("--pick-preview", true)] {
                for spec in args.values_from_str::<_, String>(key)? {
                    let (source_point, preview_point) = v.locate(numbers(&spec)?, from_preview)?;
                    requests.push((source_point, preview_point, from_preview));
                }
            }
            finish_args(args)?;
            if requests.is_empty() { return Err("supply --pick-source or --pick-preview".into()); }
            let actual = Rows::open(&preview)?;
            if [actual.width, actual.height] != [v.width(), v.height()] {
                return Err("preview dimensions differ; run compare".into());
            }
            let sources: Vec<_> = requests.iter().map(|r| r.0).collect();
            let previews: Vec<_> = requests.iter().map(|r| r.1).collect();
            let original = reader.sample_points(&sources)?;
            let enlarged = actual.sample_points(&previews)?;
            let mut selections = Vec::with_capacity(requests.len());
            for (i, (source_xy, preview_xy, from_preview)) in requests.into_iter().enumerate() {
                let left = (source_xy[0] - v.x) * scale;
                let top = (source_xy[1] - v.y) * scale;
                selections.push(json!({"selected_on":if from_preview {"preview"}else{"source"},
                    "source_xy":source_xy,"preview_xy":preview_xy,"source_rgba":hex(&original[i]),
                    "preview_rgba":hex(&enlarged[i]),"preview_block_inclusive":{
                        "x":[left,left+scale-1],"y":[top,top+scale-1]}}));
            }
            if selections.len()==1 { out["selection"]=selections.pop().unwrap(); }
            else { out["selections"]=Value::Array(selections); }
        }
        "edit" => {
            let enable_crop = args.contains("--preview-changes");
            let disable_crop = args.contains("--no-preview-changes");
            if enable_crop && disable_crop {
                return Err("choose one change-preview mode".into());
            }
            let padding: u32 = args.opt_value_from_str("--padding")?.unwrap_or(2);
            let mut tracker = (!disable_crop).then(ChangeTracker::default);
            let patch_file: Option<PathBuf> = args.opt_value_from_str("--patch")?;
            let pal=args.opt_value_from_str::<_,PathBuf>("--palette")?.as_deref().map(palette::load).transpose()?.unwrap_or_default();
            if patch_file.is_none() && !pal.is_empty() { return Err("--palette requires --patch for edit".into()); }
            let mut changes = Vec::new();
            let mut seen = HashSet::new();
            for (key, from_preview) in [("--set-source", false), ("--set-preview", true)] {
                for spec in args.values_from_str::<_, String>(key)? {
                    let (coords, c) = spec.split_once('=').ok_or("use X,Y=RRGGBBAA")?;
                    let p = numbers(coords)?;
                    let (base, pv) = v.locate(p, from_preview)?;
                    if !seen.insert(base) {
                        return Err("duplicate edit of the same source pixel".into());
                    }
                    changes.push((base, pv, color(c)?));
                }
            }
            let transform = selection::Transform::parse(&mut args)?;
            finish_args(args)?;
            let mode_count = usize::from(!changes.is_empty())
                + usize::from(patch_file.is_some())
                + usize::from(transform.is_some());
            if mode_count != 1 {
                return Err("choose one edit mode: point colors, --patch, --move-by or --paste".into());
            }
            let mut image = Image::load(reader, full)?;
            if let Some(transform) = transform {
                out["selection"] = transform.apply(&mut image, v, &mut tracker)?;
            }
            if let Some(path) = patch_file {
                out["patch"] = patch::apply(&path, v,&pal, |[x, y], rgba| image.set(x, y, rgba, tracker.as_mut()))?;
            }
            let mut report = Vec::new();
            for ([x, y], pv, rgba) in changes {
                report.push(json!({"source_xy":[x,y],"preview_xy":pv,
                    "before_rgba":hex(image.pixel(x,y)),"after_rgba":hex(&rgba)}));
                image.set(x, y, rgba, tracker.as_mut());
            }
            let mut crop_output = None;
            out["change_preview"] = match tracker {
                None => json!({"status":"disabled"}),
                Some(tracker) => match tracker.finish(&image) {
                    None => json!({"status":"unchanged","changed_pixels":0}),
                    Some((count, bounds)) => {
                        let crop = padded_view(bounds, padding, size, scaling)?;
                        let path = png_dest(preview.with_file_name(format!("{}.changes.png", stem(&preview))), false)?;
                        for other in [&source, &edited, &preview] { distinct(other, &path)?; }
                        locks.push(write_lock(&path)?);
                        let mut result = info(&path, crop.width(), crop.height());
                        result["status"] = json!("generated");
                        result["changed_pixels"] = json!(count);
                        result["bounds"] = rect(bounds);
                        result["region"] = rect(crop.area());
                        result["scale"] = json!(crop.scale);
                        result["padding"] = json!(padding);
                        crop_output = Some((image.stage(&path, crop)?, path, crop));
                        result
                    }
                }
            };
            let new_source = image.stage(&edited, View::new(full, 1, size)?)?;
            let new_preview = image.stage(&preview, v)?;
            if in_place {
                new_source.persist(&edited)?;
            } else {
                new_source.persist_noclobber(&edited)?;
            }
            new_preview.persist(&preview).map_err(|e| {
                format!(
                    "edited source saved at {}; preview commit failed: {e}; regenerate with zoom",
                    edited.display()
                )
            })?;
            if let Some((file, path, crop)) = crop_output {
                file.persist(&path).map_err(|e| format!(
                    "source and main preview saved; change preview {} failed: {e}; regenerate with zoom --source \"{}\" --scale {} --region {},{},{},{}",
                    path.display(), edited.display(), crop.scale, crop.x, crop.y, crop.w, crop.h))?;
            }
            out["input_source"] = info(&source, size[0], size[1]);
            if out.get("patch").is_none() && out.get("selection").is_none() {
                out["changes"] = json!(report);
            }
        }
        _ => unreachable!(),
    }
    Ok((Some(out), 0))
}

const HELP: &str = "review-sheet --source PNG [--source PNG]... [--before PNG] [--display-scale N] [--detail-scale N] [--region X,Y,W,H] [--background RRGGBB] [--scene PNG [--at X,Y]] --out NEW.png; tile-preview (--source TILE [--region X,Y,W,H] [--repeat COLS,ROWS] | --layout FILE.json) [--overlay PNG --at X,Y] --out NEW.png; animation-config --frame PNG... [--fps N] [--baseline Y] --out NEW.json; animation (--config FILE | --frame PNG...) [--fps N] [--strip-out NEW.png | --no-strip] --out NEW.apng; onion (--config FILE | --frame PNG...) --index N [--opacity PERCENT] --out NEW.png; animation/onion [--baseline Y | --no-baseline]; all three previews [--scale N | --target-long-edge PX]; resize --source PNG --out NEW.png or canvas resize --canvas NAME [--pad-layer NAME]...: (--left N / --right N / --top N / --bottom N in any combination, signed | --crop X,Y,W,H) [--baseline Y]; create [or canvas create --canvas NAME] --size W,H --out NEW.png; canvas undo|redo --canvas NAME; canvas palette-use --canvas NAME --palette FILE; palette-check --source PNG... --palette FILE [--allow COLOR]...; recolor --source PNG... --map OLD=NEW... --out-dir NEW_DIR [--palette FILE]; canvas palette-check|recolor --canvas NAME [--layer NAME]; edit/edit-matrix/canvas edit --patch FILE [--palette FILE]; canvas layer-add --layer NAME [--source PNG | --size W,H] [--at X,Y]; layer-set --layer NAME [--at X,Y] [--visible true|false] [--index N]; layer-select|layer-remove --layer NAME; canvas edit [--layer NAME] --patch FILE; canvas export [--layer NAME] --out NEW.png; canvas save --project NEW_DIR; canvas open|status|copy|paste|preview|save|close --canvas NAME (open --source FILE; copy --clip NEW.json + selections; paste --clip FILE --at X,Y... [--skip-transparent]; preview --out NEW.png [--region X,Y,W,H] [--scale N | --target-long-edge PX]; save [--out NEW.png]; close [--discard]); copy --source FILE (--point X,Y | --rect X,Y,W,H)... --out NEW.json; edit --move-by DX,DY (--point X,Y | --rect X,Y,W,H)... or --paste CLIP.json --at X,Y [--flip-x] [--flip-y]; bounds --source FILE; matrix --source FILE [--point X,Y]... [--region X,Y,W,H]... [--out NEW.json]; zoom|compare|pick|edit --source FILE [--scale N | --target-long-edge PX] (default target 1600, minimum 1x) [--region X,Y,W,H]; zoom/edit [--out-dir DIR] [--out-name FILE.png]; compare/pick [--preview FILE]; compare [--sample-limit N]; pick (--pick-source X,Y | --pick-preview X,Y)...; edit --patch FILE|- or (--set-source X,Y=RRGGBBAA | --set-preview X,Y=RRGGBBAA)... [--edited-source NEW.png | --in-place] [--preview-changes | --no-preview-changes] [--padding N]; edit-matrix --source MATRIX.json --patch FILE|- --out NEW.json; licenses --out NEW.zip. See cli.md or cliWiki.md.";

fn main() {
    let (output, code) = run().unwrap_or_else(|e| (Some(json!({"error":e.to_string()})), 2));
    if let Some(output) = output {
        println!("{output}");
    }
    std::process::exit(code);
}
