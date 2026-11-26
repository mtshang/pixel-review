use crate::{clean, color, finish_args, hex, ChangeTracker, Image, Point, Result, Rows};
use pico_args::Arguments;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

pub type Rgba = [u8; 4];
pub type Palette = BTreeMap<char, Rgba>;

pub fn parse(doc: &Value) -> Result<Palette> {
    let mut colors = Palette::new();
    for (key, v) in doc.as_object().ok_or("expected a palette object")? {
        let mut chars = key.chars();
        let ch = chars.next().ok_or("empty palette symbol")?;
        if chars.next().is_some() || ch == '.' {
            return Err("palette keys must be single characters other than '.'".into());
        }
        colors.insert(ch, crate::patch::literal(v)?);
    }
    Ok(colors)
}

pub fn load(path: &Path) -> Result<Palette> {
    let doc = crate::patch::read_json(path)?;
    let colors = parse(&doc["palette"])?;
    if colors.is_empty() {
        return Err("empty shared palette".into());
    }
    Ok(colors)
}

pub fn description(pal: &Palette) -> Value {
    json!(pal
        .iter()
        .map(|(k, v)| (k, hex(v)))
        .collect::<BTreeMap<_, _>>())
}

fn resolve(s: &str, pal: &Palette) -> Result<Rgba> {
    if let Some(symbol) = s.strip_prefix('@') {
        let mut chars = symbol.chars();
        let ch = chars.next().ok_or("missing color symbol")?;
        if chars.next().is_some() {
            return Err("expected @ followed by one palette symbol".into());
        }
        return pal
            .get(&ch)
            .copied()
            .ok_or_else(|| format!("unknown palette symbol: {s}").into());
    }
    if s == "transparent" {
        Ok([0; 4])
    } else {
        color(s)
    }
}

pub fn mappings(args: &mut Arguments, pal: &Palette) -> Result<HashMap<Rgba, Rgba>> {
    let mut map = HashMap::new();
    for spec in args.values_from_str::<_, String>("--map")? {
        let (src, dst) = spec.split_once('=').ok_or("expected --map OLD=NEW")?;
        if map.insert(resolve(src, pal)?, resolve(dst, pal)?).is_some() {
            return Err("duplicate source color".into());
        }
    }
    if map.is_empty() {
        return Err("supply at least one --map OLD=NEW".into());
    }
    Ok(map)
}

pub fn replace(
    image: &mut Image,
    map: &HashMap<Rgba, Rgba>,
    mut tracker: Option<&mut ChangeTracker>,
) -> u64 {
    let view = image.view();
    let mut count = 0;
    for y in 0..view.h {
        for x in 0..view.w {
            let src: Rgba = image.pixel(x, y).try_into().unwrap();
            if let Some(&dst) = map.get(&src) {
                if src != dst {
                    image.set(x, y, dst, tracker.as_deref_mut());
                    count += 1;
                }
            }
        }
    }
    count
}

pub fn allowed(args: &mut Arguments, pal: &Palette) -> Result<HashSet<Rgba>> {
    if pal.is_empty() {
        return Err("load a shared palette first".into());
    }
    let mut allowed: HashSet<_> = pal.values().copied().collect();
    for s in args.values_from_str::<_, String>("--allow")? {
        allowed.insert(resolve(&s, pal)?);
    }
    Ok(allowed)
}

#[derive(Default)]
pub struct Audit {
    counts: HashMap<Rgba, (u64, Point)>,
    pixels: u64,
}

impl Audit {
    pub fn row(&mut self, y: u32, row: &[u8], allowed: &HashSet<Rgba>) {
        for (x, p) in row.chunks_exact(4).enumerate() {
            let rgba: Rgba = p.try_into().unwrap();
            if rgba[3] > 0 && !allowed.contains(&rgba) {
                self.counts.entry(rgba).or_insert((0, [x as u32, y])).0 += 1;
                self.pixels += 1;
            }
        }
    }
    pub fn report(self) -> Value {
        let mut all: Vec<_> = self.counts.into_iter().collect();
        all.sort_unstable_by_key(|(rgba, (n, _))| (std::cmp::Reverse(*n), *rgba));
        json!({"ok":self.pixels==0,"outside_pixels":self.pixels,"outside_colors":all.len(),
            "samples":all.iter().take(64).map(|(rgba,(count,first))|json!({"rgba":hex(rgba),"count":count,"first":first})).collect::<Vec<_>>()})
    }
}

pub fn batch(cmd: &str, mut args: Arguments) -> Result<Value> {
    let paths = args.values_from_str::<_, PathBuf>("--source")?;
    if paths.is_empty() {
        return Err("supply --source PNG (repeatable)".into());
    }
    let palette: Option<PathBuf> = args.opt_value_from_str("--palette")?;
    let pal = palette
        .as_deref()
        .map(load)
        .transpose()?
        .unwrap_or_default();
    let mut results = Vec::new();
    if cmd == "palette-check" {
        let allowed = allowed(&mut args, &pal)?;
        finish_args(args)?;
        for path in paths {
            let path = clean(path.canonicalize()?);
            let mut rows = Rows::open(&path)?;
            let mut audit = Audit::default();
            rows.visit_region([0, 0, rows.width, rows.height], |y, row| {
                audit.row(y, row, &allowed);
                Ok(())
            })?;
            let mut report = audit.report();
            report["source"] = json!(path);
            results.push(report);
        }
        return Ok(
            json!({"command":cmd,"ok":results.iter().all(|v|v["ok"]==true),"images":results}),
        );
    }
    let map = mappings(&mut args, &pal)?;
    let dir = std::path::absolute(args.value_from_str::<_, PathBuf>("--out-dir")?)?;
    finish_args(args)?;
    if dir.exists() {
        return Err("--out-dir requires a new directory".into());
    }
    let parent = dir.parent().ok_or("missing output parent")?;
    fs::create_dir_all(parent)?;
    let stage = tempfile::tempdir_in(parent)?;
    for path in paths {
        let path = clean(path.canonicalize()?);
        let leaf = path.file_name().ok_or("missing filename")?;
        let dest = stage.path().join(leaf);
        let rows = Rows::open(&path)?;
        let area = [0, 0, rows.width, rows.height];
        let mut image = Image::load(rows, area)?;
        let count = replace(&mut image, &map, None);
        image.stage(&dest, image.view())?.persist_noclobber(&dest)?;
        results.push(json!({"source":path,"out":dir.join(leaf),"changed_pixels":count}));
    }
    fs::rename(stage.path(), &dir)?;
    let _ = stage.keep();
    Ok(json!({"command":cmd,"images":results}))
}
