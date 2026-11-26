use crate::{
    clean, color, finish_args, info, layers::over, numbers, png_dest, rect, Image, Point, Result,
    Rows, View,
};
use pico_args::Arguments;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const GAP: u32 = 8;
const HEADER: u32 = 24;

fn dimensions(path: &Path) -> Result<Point> {
    let rows = Rows::open(path)?;
    Ok([rows.width, rows.height])
}
fn load(path: &Path, expected: Point) -> Result<Image> {
    let rows = Rows::open(path)?;
    if [rows.width, rows.height] != expected {
        return Err(format!("unexpected canvas size: {}", path.display()).into());
    }
    Image::load(rows, [0, 0, expected[0], expected[1]])
}

// Apply alpha once per source pixel, then replicate the resulting row.
fn panel(board: &mut Image, src: &Image, view: View, at: Point, matte: [u8; 4]) -> Result<()> {
    let mut row = Vec::new();
    let mut expanded = Vec::new();
    for y in 0..view.h {
        row.clear();
        for p in src.row_span(view.x, view.y + y, view.w).chunks_exact(4) {
            let mut dst = matte;
            over(p, &mut dst);
            row.extend_from_slice(&dst);
        }
        Image::expand(&row, view.scale, &mut expanded)?;
        for dy in 0..view.scale {
            board.write_row_span(at[0], at[1] + y * view.scale + dy, &expanded);
        }
    }
    Ok(())
}

pub fn sheet(mut args: Arguments) -> Result<Value> {
    let before = args.opt_value_from_str::<_, PathBuf>("--before")?;
    let candidates = args.values_from_str::<_, PathBuf>("--source")?;
    if candidates.is_empty() {
        return Err("supply at least one --source PNG".into());
    }
    let paths = before
        .iter()
        .chain(&candidates)
        .map(|p| p.canonicalize().map(clean))
        .collect::<std::io::Result<Vec<_>>>()?;
    let display_scale = args.opt_value_from_str("--display-scale")?.unwrap_or(1);
    let detail_scale = args.opt_value_from_str("--detail-scale")?;
    let region = args
        .opt_value_from_str::<_, String>("--region")?
        .as_deref()
        .map(numbers)
        .transpose()?;
    let background = args
        .opt_value_from_str::<_, String>("--background")?
        .unwrap_or_else(|| "80858C".into());
    let matte = color(&background)?;
    if matte[3] != 255 {
        return Err("review background must be opaque".into());
    }
    let scene_path = args
        .opt_value_from_str::<_, PathBuf>("--scene")?
        .map(|p| p.canonicalize().map(clean))
        .transpose()?;
    let at: Option<Point> = args
        .opt_value_from_str::<_, String>("--at")?
        .as_deref()
        .map(numbers)
        .transpose()?;
    let out = png_dest(args.value_from_str("--out")?, true)?;
    finish_args(args)?;
    if out.exists() {
        return Err("review sheet output already exists".into());
    }
    let size = dimensions(&paths[0])?;
    for path in &paths[1..] {
        if dimensions(path)? != size {
            return Err(format!(
                "all versions must share native canvas dimensions: {}",
                path.display()
            )
            .into());
        }
    }
    let full = [0, 0, size[0], size[1]];
    let display = View::new(full, display_scale, size)?;
    let area = region.unwrap_or(full);
    let detail = match detail_scale {
        Some(n) => View::new(area, n, size)?,
        None => crate::PreviewScale::Target(320).view(area, size)?,
    };
    let scene = if let Some(path) = &scene_path {
        let dims = dimensions(path)?;
        let pos = at.unwrap_or([
            dims[0].saturating_sub(size[0]) / 2,
            dims[1].saturating_sub(size[1]) / 2,
        ]);
        View::new([pos[0], pos[1], size[0], size[1]], 1, dims)?;
        Some((load(path, dims)?, pos))
    } else {
        if at.is_some() {
            return Err("--at requires --scene".into());
        }
        None
    };
    let cell = display
        .width()
        .max(detail.width())
        .max(scene.as_ref().map_or(0, |(img, _)| img.view().w))
        .max(12 * paths.len().to_string().len() as u32);
    let width = u32::try_from(
        (u64::from(cell) + u64::from(GAP)) * u64::try_from(paths.len())? + u64::from(GAP),
    )?;
    let detail_y = HEADER + display.height() + GAP;
    let scene_y = u32::try_from(u64::from(detail_y) + u64::from(detail.height()) + u64::from(GAP))?;
    let height = u32::try_from(
        u64::from(scene_y)
            + scene
                .as_ref()
                .map_or(0, |(img, _)| u64::from(img.view().h) + u64::from(GAP)),
    )?;
    let mut board = Image::blank([width, height])?;
    let base = [42, 47, 54, 255].repeat(width as usize);
    for y in 0..height {
        board.write_row_span(0, y, &base);
    }
    let mut columns = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        let src = load(path, size)?;
        let left = GAP + i as u32 * (cell + GAP);
        let center = |w| left + (cell - w) / 2;
        crate::strip::draw_number(&mut board, i + 1, left, 4, cell);
        let show = [center(display.width()), HEADER];
        let zoom = [center(detail.width()), detail_y];
        panel(&mut board, &src, display, show, matte)?;
        panel(&mut board, &src, detail, zoom, matte)?;
        let mut column = json!({"number":i+1,"role":if before.is_some()&&i==0{"before"}else{"candidate"},
            "path":path,"display":rect([show[0],show[1],display.width(),display.height()]),
            "detail":rect([zoom[0],zoom[1],detail.width(),detail.height()])});
        if let Some((bg, pos)) = &scene {
            let v = bg.view();
            let x = center(v.w);
            for y in 0..v.h {
                board.write_row_span(x, scene_y + y, bg.row_span(0, y, v.w));
            }
            for sy in 0..size[1] {
                let mut row = bg.row_span(pos[0], pos[1] + sy, size[0]).to_vec();
                for (p, dst) in src
                    .row_span(0, sy, size[0])
                    .chunks_exact(4)
                    .zip(row.chunks_exact_mut(4))
                {
                    over(p, dst);
                }
                board.write_row_span(x + pos[0], scene_y + pos[1] + sy, &row);
            }
            column["scene"] = rect([x, scene_y, v.w, v.h]);
        }
        columns.push(column);
    }
    board.stage(&out, board.view())?.persist_noclobber(&out)?;
    Ok(
        json!({"command":"review-sheet","preview":info(&out,width,height),"native_size":size,
        "display_scale":display.scale,"detail_scale":detail.scale,"detail_region":rect(area),
        "background":crate::hex(&matte),"scene":scene_path,"at":scene.map(|(_,pos)|pos),"columns":columns}),
    )
}
