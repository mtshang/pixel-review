use crate::{
    clean, finish_args, info, layers::over, numbers, patch, png_dest, Image, Point, PreviewScale,
    Result, Rows, View,
};
use pico_args::Arguments;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn load(path: &Path, region: Option<[u32; 4]>) -> Result<Image> {
    let rows = Rows::open(path)?;
    let size = [rows.width, rows.height];
    let area = region.unwrap_or([0, 0, size[0], size[1]]);
    View::new(area, 1, size)?;
    Image::load(rows, area)
}

struct Pattern {
    tiles: Vec<Image>,
    cells: Vec<Vec<usize>>,
    grid: Point,
}

impl Pattern {
    fn layout(path: &Path) -> Result<Self> {
        let doc = patch::read_json(path)?;
        let parent = path.parent().unwrap_or(Path::new("."));
        let mut keys = HashMap::new();
        let mut tiles: Vec<Image> = Vec::new();
        for (key, spec) in doc["tiles"].as_object().ok_or("layout requires tiles")? {
            let mut chars = key.chars();
            let ch = chars.next().ok_or("empty tile symbol")?;
            if chars.next().is_some() {
                return Err("tile symbols must be single characters".into());
            }
            let file = spec["source"].as_str().ok_or("tile requires source")?;
            let region = spec
                .get("region")
                .cloned()
                .map(serde_json::from_value)
                .transpose()?;
            let image = load(&parent.join(file), region)?;
            let v = image.view();
            if tiles
                .first()
                .is_some_and(|first| (v.w, v.h) != (first.view().w, first.view().h))
            {
                return Err(format!("tile {key} has different dimensions").into());
            }
            keys.insert(ch, tiles.len());
            tiles.push(image);
        }
        let mut cells: Vec<Vec<usize>> = Vec::new();
        for row in doc["rows"].as_array().ok_or("layout requires rows")? {
            let mut line = Vec::new();
            for ch in row.as_str().ok_or("layout rows must be strings")?.chars() {
                line.push(*keys.get(&ch).ok_or_else(|| format!("unknown tile: {ch}"))?);
            }
            if line.is_empty() || cells.first().is_some_and(|r| r.len() != line.len()) {
                return Err("layout rows must have equal positive width".into());
            }
            cells.push(line);
        }
        let grid = [
            u32::try_from(cells.first().ok_or("layout is empty")?.len())?,
            u32::try_from(cells.len())?,
        ];
        Ok(Self { tiles, cells, grid })
    }
}

pub fn preview(mut args: Arguments) -> Result<Value> {
    let scale = PreviewScale::parse(&mut args)?;
    let out = png_dest(args.value_from_str("--out")?, true)?;
    let overlay: Option<PathBuf> = args.opt_value_from_str("--overlay")?;
    let at: Option<String> = args.opt_value_from_str("--at")?;
    let layout: Option<PathBuf> = args.opt_value_from_str("--layout")?;
    let pattern = if let Some(path) = layout {
        Pattern::layout(&path)?
    } else {
        let source: PathBuf = args.value_from_str("--source")?;
        let region = args
            .opt_value_from_str::<_, String>("--region")?
            .as_deref()
            .map(numbers)
            .transpose()?;
        let grid = args
            .opt_value_from_str::<_, String>("--repeat")?
            .as_deref()
            .map(numbers)
            .transpose()?
            .unwrap_or([3, 3]);
        Pattern {
            tiles: vec![load(&source, region)?],
            cells: vec![],
            grid,
        }
    };
    finish_args(args)?;
    let tile = pattern.tiles[0].view();
    let size = [
        tile.w
            .checked_mul(pattern.grid[0])
            .ok_or("grid width overflow")?,
        tile.h
            .checked_mul(pattern.grid[1])
            .ok_or("grid height overflow")?,
    ];
    let view = scale.view([0, 0, size[0], size[1]], size)?;
    let sprite = match (overlay, at) {
        (None, None) => None,
        (Some(path), Some(at)) => {
            let image = load(&path, None)?;
            let [x, y] = numbers(&at)?;
            View::new([x, y, image.view().w, image.view().h], 1, size)?;
            Some((image, x, y))
        }
        _ => return Err("use --overlay and --at together".into()),
    };
    let mut row = Vec::new();
    row.try_reserve_exact(size[0] as usize * 4)?;
    let stage = pattern.tiles[0].stage_rows(&out, view, |y, expanded| {
        row.clear();
        let cells = pattern.cells.get((y / tile.h) as usize);
        for x in 0..pattern.grid[0] {
            let img = &pattern.tiles[cells.map_or(0, |c| c[x as usize])];
            let v = img.view();
            row.extend_from_slice(img.row_span(v.x, v.y + y % tile.h, v.w));
        }
        if let Some((img, x, top)) = &sprite {
            let v = img.view();
            if y >= *top && y - *top < v.h {
                let dest = &mut row[*x as usize * 4..];
                for (src, dst) in img
                    .row_span(0, y - *top, v.w)
                    .chunks_exact(4)
                    .zip(dest.chunks_exact_mut(4))
                {
                    over(src, dst);
                }
            }
        }
        Image::expand(&row, view.scale, expanded)
    })?;
    stage.persist_noclobber(&out)?;
    Ok(
        json!({"command":"tile-preview","tile_size":[tile.w,tile.h],"grid":pattern.grid,
        "native_size":size,"scale":view.scale,"preview":info(&clean(out),view.width(),view.height())}),
    )
}
