use crate::layers::{Layer, Stack};
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

struct Placement {
    size: Point,
    anchor: Point,
    offset: Point,
}

fn foot_anchor(path: &Path, size: Point) -> Result<Point> {
    let mut rows = Rows::open(path)?;
    let mut anchor = [size[0] / 2, size[1] - 1];
    rows.visit_region([0, 0, size[0], size[1]], |y, row| {
        let mut occupied = row.chunks_exact(4);
        if let Some(left) = occupied.position(|p| p[3] > 0) {
            let right = occupied
                .rposition(|p| p[3] > 0)
                .map_or(left, |i| left + 1 + i);
            anchor = [((left + right) / 2) as u32, y];
        }
        Ok(())
    })?;
    Ok(anchor)
}

fn align(paths: &[PathBuf], anchors: &[Point]) -> Result<(Vec<Placement>, Point, Point)> {
    if ![0, 1, paths.len()].contains(&anchors.len()) {
        return Err("use one shared --anchor X,Y or one per column (before first)".into());
    }
    let sizes = paths
        .iter()
        .map(|p| dimensions(p))
        .collect::<Result<Vec<_>>>()?;
    let mixed = sizes.iter().any(|s| s != &sizes[0]);
    let mut placements = Vec::new();
    let mut common = [0; 2];
    let mut after = [0; 2];
    for (i, size) in sizes.into_iter().enumerate() {
        let anchor = if anchors.len() == 1 {
            anchors[0]
        } else if let Some(a) = anchors.get(i) {
            *a
        } else if mixed {
            foot_anchor(&paths[i], size)?
        } else {
            [0, 0]
        };
        View::new([anchor[0], anchor[1], 1, 1], 1, size)?;
        for a in 0..2 {
            common[a] = common[a].max(anchor[a]);
            after[a] = after[a].max(size[a] - anchor[a]);
        }
        placements.push(Placement {
            size,
            anchor,
            offset: [0, 0],
        });
    }
    let size = [
        common[0]
            .checked_add(after[0])
            .ok_or("aligned width overflow")?,
        common[1]
            .checked_add(after[1])
            .ok_or("aligned height overflow")?,
    ];
    for p in &mut placements {
        p.offset = [common[0] - p.anchor[0], common[1] - p.anchor[1]];
    }
    Ok((placements, size, common))
}

struct Scene {
    stack: Stack,
    slot: usize,
    view: View,
}

impl Scene {
    fn open(
        path: &Path,
        replace: Option<&str>,
        foregrounds: &[PathBuf],
        at: Option<Point>,
        size: Point,
        first_offset: Point,
        scale: u32,
    ) -> Result<Self> {
        let (mut stack, slot, dims, position) = if path.is_dir() {
            if !foregrounds.is_empty() {
                return Err("use project layers for foregrounds in a project scene".into());
            }
            let (_, full, stack) = crate::project::open(path)?;
            let name = replace.ok_or("project scene requires --replace-layer NAME")?;
            let slot = stack.index(Some(name))?;
            let pos = stack.layers[slot].at;
            let position = [
                pos[0]
                    .checked_sub(i32::try_from(first_offset[0])?)
                    .ok_or("scene x overflow")?,
                pos[1]
                    .checked_sub(i32::try_from(first_offset[1])?)
                    .ok_or("scene y overflow")?,
            ];
            (stack, slot, [full.w, full.h], position)
        } else {
            if replace.is_some() {
                return Err("--replace-layer requires a project scene".into());
            }
            let dims = dimensions(path)?;
            let pos = at.unwrap_or([
                dims[0].saturating_sub(size[0]) / 2,
                dims[1].saturating_sub(size[1]) / 2,
            ]);
            View::new([pos[0], pos[1], size[0], size[1]], 1, dims)?;
            let mut layers = vec![
                Layer::new("base".into(), load(path, dims)?, [0, 0])?,
                Layer::new("candidate".into(), Image::blank([1, 1])?, [0, 0])?,
            ];
            for (i, fg) in foregrounds.iter().enumerate() {
                layers.push(Layer::new(format!("front-{i}"), load(fg, dims)?, [0, 0])?);
            }
            (
                Stack {
                    layers,
                    active: "candidate".into(),
                    palette: Default::default(),
                },
                1,
                dims,
                [pos[0] as i32, pos[1] as i32],
            )
        };
        stack.layers[slot].at = match at {
            Some([x, y]) => [i32::try_from(x)?, i32::try_from(y)?],
            None => position,
        };
        Ok(Self {
            stack,
            slot,
            view: View::new([0, 0, dims[0], dims[1]], scale, dims)?,
        })
    }
    fn draw(&mut self, board: &mut Image, at: Point, sprite: Image) -> Result<()> {
        self.stack.layers[self.slot].image = sprite;
        let visible = self
            .stack
            .layers
            .iter()
            .filter(|l| l.visible)
            .collect::<Vec<_>>();
        let v = self.view;
        let mut row = vec![0; v.w as usize * 4];
        let mut expanded = Vec::new();
        for y in 0..v.h {
            Stack::composite_row(visible.iter().copied(), v, y, &mut row);
            Image::expand(&row, v.scale, &mut expanded)?;
            for dy in 0..v.scale {
                board.write_row_span(at[0], at[1] + y * v.scale + dy, &expanded);
            }
        }
        Ok(())
    }
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
    let anchors = args
        .values_from_str::<_, String>("--anchor")?
        .iter()
        .map(|s| numbers(s))
        .collect::<Result<Vec<Point>>>()?;
    let scene_scale = args.opt_value_from_str::<_, u32>("--scene-scale")?;
    let replace = args.opt_value_from_str::<_, String>("--replace-layer")?;
    let foregrounds = args
        .values_from_str::<_, PathBuf>("--foreground")?
        .iter()
        .map(|p| p.canonicalize().map(clean))
        .collect::<std::io::Result<Vec<_>>>()?;
    let out = png_dest(args.value_from_str("--out")?, true)?;
    finish_args(args)?;
    if out.exists() {
        return Err("review sheet output already exists".into());
    }
    let (placements, size, common_anchor) = align(&paths, &anchors)?;
    let full = [0, 0, size[0], size[1]];
    let display = View::new(full, display_scale, size)?;
    let area = region.unwrap_or(full);
    let detail = match detail_scale {
        Some(n) => View::new(area, n, size)?,
        None => crate::PreviewScale::Target(320).view(area, size)?,
    };
    let mut scene = if let Some(path) = &scene_path {
        Some(Scene::open(
            path,
            replace.as_deref(),
            &foregrounds,
            at,
            size,
            placements[0].offset,
            scene_scale.unwrap_or(display_scale),
        )?)
    } else {
        if at.is_some() || scene_scale.is_some() || replace.is_some() || !foregrounds.is_empty() {
            return Err("scene options require --scene".into());
        }
        None
    };
    let cell = display
        .width()
        .max(detail.width())
        .max(scene.as_ref().map_or(0, |s| s.view.width()))
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
                .map_or(0, |s| u64::from(s.view.height()) + u64::from(GAP)),
    )?;
    let mut board = Image::blank([width, height])?;
    let base = [42, 47, 54, 255].repeat(width as usize);
    for y in 0..height {
        board.write_row_span(0, y, &base);
    }
    let mut columns = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        let placement = &placements[i];
        let mut src = load(path, placement.size)?;
        if placement.size != size {
            src = src.reframe(size, placement.offset.map(|n| n as i32))?;
        }
        let left = GAP + i as u32 * (cell + GAP);
        let center = |w| left + (cell - w) / 2;
        crate::strip::draw_number(&mut board, i + 1, left, 4, cell);
        let show = [center(display.width()), HEADER];
        let zoom = [center(detail.width()), detail_y];
        panel(&mut board, &src, display, show, matte)?;
        panel(&mut board, &src, detail, zoom, matte)?;
        let mut column = json!({"number":i+1,"role":if before.is_some()&&i==0{"before"}else{"candidate"},
            "path":path,"source_size":placement.size,"anchor":placement.anchor,"offset":placement.offset,"display":rect([show[0],show[1],display.width(),display.height()]),
            "detail":rect([zoom[0],zoom[1],detail.width(),detail.height()])});
        if let Some(s) = &mut scene {
            let v = s.view;
            let x = center(v.width());
            s.draw(&mut board, [x, scene_y], src)?;
            column["scene"] = rect([x, scene_y, v.width(), v.height()]);
        }
        columns.push(column);
    }
    board.stage(&out, board.view())?.persist_noclobber(&out)?;
    Ok(
        json!({"command":"review-sheet","preview":info(&out,width,height),"native_size":size,
        "display_scale":display.scale,"detail_scale":detail.scale,"detail_region":rect(area),
        "background":crate::hex(&matte),"scene":scene_path,"scene_scale":scene.as_ref().map(|s|s.view.scale),"at":scene.as_ref().map(|s|s.stack.layers[s.slot].at),"common_anchor":common_anchor,"replace_layer":replace,"foregrounds":foregrounds,"columns":columns}),
    )
}
