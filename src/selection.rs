use super::{clean, finish_args, numbers, ChangeTracker, Image, Point, Result, Rows, View};
use pico_args::Arguments;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub fn areas(args: &mut Arguments) -> Result<Vec<[u32; 4]>> {
    let mut result = Vec::new();
    for spec in args.values_from_str::<_, String>("--point")? {
        let [x, y] = numbers(&spec)?;
        result.push([x, y, 1, 1]);
    }
    for spec in args.values_from_str::<_, String>("--rect")? {
        result.push(numbers(&spec)?);
    }
    Ok(result)
}

// Exclusive row ends. Merge intersections before sampling or writing.
struct Mask {
    area: [u32; 4],
    rows: BTreeMap<u32, Vec<(u32, u32)>>,
}

impl Mask {
    fn new(areas: Vec<[u32; 4]>, view: View) -> Result<Self> {
        if areas.is_empty() {
            return Err("select at least one --point or --rect".into());
        }
        let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
        let mut rows: BTreeMap<u32, Vec<(u32, u32)>> = BTreeMap::new();
        for [x, y, w, h] in areas {
            let (end_x, end_y) = check_extent([x, y], [w, h], view)?;
            left = left.min(x);
            top = top.min(y);
            right = right.max(end_x);
            bottom = bottom.max(end_y);
            for row in y..end_y {
                rows.entry(row).or_default().push((x, end_x));
            }
        }
        for spans in rows.values_mut() {
            spans.sort_unstable();
            let mut merged: Vec<(u32, u32)> = Vec::new();
            for (start, end) in spans.drain(..) {
                if let Some(last) = merged.last_mut().filter(|last| start <= last.1) {
                    last.1 = last.1.max(end);
                } else {
                    merged.push((start, end));
                }
            }
            *spans = merged;
        }
        Ok(Self {
            area: [left, top, right - left, bottom - top],
            rows,
        })
    }
}

fn check_extent([x, y]: Point, [w, h]: Point, view: View) -> Result<(u32, u32)> {
    let right = x.checked_add(w).ok_or("x extent overflow")?;
    let bottom = y.checked_add(h).ok_or("y extent overflow")?;
    if w == 0 || h == 0 {
        return Err("empty selection rectangle".into());
    }
    view.locate([x, y], false)?;
    view.locate([right - 1, bottom - 1], false)?;
    Ok((right, bottom))
}

type Run = (u32, u32, Vec<[u8; 4]>);
const FORMAT: &str = "pixel-review.clip/1";

pub struct Clip {
    origin: Point,
    size: Point,
    runs: Vec<Run>,
}

impl Clip {
    fn empty(mask: &Mask) -> Self {
        let [x, y, w, h] = mask.area;
        Self {
            origin: [x, y],
            size: [w, h],
            runs: Vec::new(),
        }
    }

    fn sample_row(&mut self, y: u32, spans: &[(u32, u32)], row: &[u8]) {
        for &(start, end) in spans {
            let x = start - self.origin[0];
            let bytes = &row[x as usize * 4..(end - self.origin[0]) as usize * 4];
            let pixels = bytes
                .chunks_exact(4)
                .map(|c| [c[0], c[1], c[2], c[3]])
                .collect();
            self.runs.push((x, y - self.origin[1], pixels));
        }
    }

    pub fn read(path: &Path) -> Result<Self> {
        let (tag, origin, size, runs): (String, Point, Point, Vec<Run>) =
            serde_json::from_reader(BufReader::new(File::open(path)?))?;
        if tag != FORMAT {
            return Err("unknown clipboard format".into());
        }
        let mut bounds = [u32::MAX, u32::MAX, 0, 0];
        let mut prev = None;
        for (x, y, pixels) in &runs {
            let end = x
                .checked_add(u32::try_from(pixels.len())?)
                .ok_or("clipboard row overflow")?;
            if pixels.is_empty()
                || end > size[0]
                || *y >= size[1]
                || prev.is_some_and(|p| (*y, *x) < p)
            {
                return Err("invalid clipboard spans".into());
            }
            bounds = [
                bounds[0].min(*x),
                bounds[1].min(*y),
                bounds[2].max(end),
                bounds[3].max(y + 1),
            ];
            prev = Some((*y, end));
        }
        if bounds != [0, 0, size[0], size[1]] {
            return Err("invalid clipboard bounds".into());
        }
        Ok(Self { origin, size, runs })
    }

    pub fn report(&self) -> Value {
        let count: usize = self.runs.iter().map(|(_, _, pixels)| pixels.len()).sum();
        json!({"source_anchor":self.origin,"size":self.size,"selected_pixels":count})
    }

    fn write_pixels(
        &self,
        anchor: Point,
        flip: [bool; 2],
        clear: bool,
        image: &mut Image,
        tracker: &mut Option<ChangeTracker>,
    ) {
        for (start, row, pixels) in &self.runs {
            let y = if flip[1] {
                self.size[1] - 1 - row
            } else {
                *row
            };
            for (i, &rgba) in pixels.iter().enumerate() {
                let x = start + i as u32;
                let x = if flip[0] { self.size[0] - 1 - x } else { x };
                image.set(
                    anchor[0] + x,
                    anchor[1] + y,
                    if clear { [0; 4] } else { rgba },
                    tracker.as_mut(),
                );
            }
        }
    }

    fn capture(mask: &Mask, image: &Image) -> Self {
        let mut clip = Self::empty(mask);
        let [x, _, w, _] = mask.area;
        for (&row, spans) in &mask.rows {
            clip.sample_row(row, spans, image.row_span(x, row, w));
        }
        clip
    }

    pub fn from_image(areas: Vec<[u32; 4]>, image: &Image, view: View) -> Result<Self> {
        Ok(Self::capture(&Mask::new(areas, view)?, image))
    }

    pub fn paste_many(
        &self,
        image: &mut Image,
        view: View,
        targets: &[Point],
        skip_transparent: bool,
        tracker: &mut ChangeTracker,
    ) -> Result<()> {
        if targets.is_empty() {
            return Err("supply at least one --at X,Y".into());
        }
        for &target in targets {
            check_extent(target, self.size, view)?;
        }
        for &[left, top] in targets {
            for (start, row, pixels) in &self.runs {
                for (i, &rgba) in pixels.iter().enumerate() {
                    if skip_transparent && rgba[3] == 0 {
                        continue;
                    }
                    let (x, y) = (left + start + i as u32, top + row);
                    image.set(x, y, rgba, Some(tracker));
                }
            }
        }
        Ok(())
    }

    pub fn save(&self, dest: &Path) -> Result<()> {
        let mut file =
            tempfile::NamedTempFile::new_in(dest.parent().ok_or("missing clipboard folder")?)?;
        {
            let mut writer = BufWriter::new(file.as_file_mut());
            serde_json::to_writer(&mut writer, &(FORMAT, self.origin, self.size, &self.runs))?;
            writer.flush()?;
        }
        file.persist_noclobber(dest)?;
        Ok(())
    }
}

pub fn copy(source: &Path, mut args: Arguments) -> Result<Value> {
    let selected = areas(&mut args)?;
    let path: PathBuf = args.value_from_str("--out")?;
    finish_args(args)?;
    let mut reader = Rows::open(source)?;
    let size = [reader.width, reader.height];
    let mask = Mask::new(selected, View::new([0, 0, size[0], size[1]], 1, size)?)?;
    let mut clip = Clip::empty(&mask);
    reader.visit_region(mask.area, |y, row| {
        if let Some(spans) = mask.rows.get(&y) {
            clip.sample_row(y, spans, row);
        }
        Ok(())
    })?;
    let dest = std::path::absolute(path)?;
    clip.save(&dest)?;
    Ok(
        json!({"command":"copy","source":super::info(source,size[0],size[1]),"out":clean(dest.canonicalize()?),"selection":clip.report()}),
    )
}

enum Operation {
    Move(Vec<[u32; 4]>, [i32; 2]),
    Paste(PathBuf, Point),
}

pub struct Transform {
    operation: Operation,
    flip: [bool; 2],
}

impl Transform {
    pub fn parse(args: &mut Arguments) -> Result<Option<Self>> {
        let selected = areas(args)?;
        let movement: Option<String> = args.opt_value_from_str("--move-by")?;
        let paste: Option<PathBuf> = args.opt_value_from_str("--paste")?;
        let at: Option<String> = args.opt_value_from_str("--at")?;
        let flip = [args.contains("--flip-x"), args.contains("--flip-y")];
        let operation = match (movement, paste, at) {
            (Some(delta), None, None) => {
                let (dx, dy) = delta.split_once(',').ok_or("use --move-by DX,DY")?;
                Operation::Move(selected, [dx.parse()?, dy.parse()?])
            }
            (None, Some(path), Some(at)) if selected.is_empty() => {
                Operation::Paste(path, numbers(&at)?)
            }
            (None, None, None) if selected.is_empty() && flip == [false; 2] => return Ok(None),
            _ => {
                return Err("use --move-by with a selection, or --paste FILE with --at X,Y".into())
            }
        };
        Ok(Some(Self { operation, flip }))
    }

    pub fn apply(
        self,
        image: &mut Image,
        view: View,
        tracker: &mut Option<ChangeTracker>,
    ) -> Result<Value> {
        let (clip, target, moving) = match self.operation {
            Operation::Move(selected, [dx, dy]) => {
                let mask = Mask::new(selected, view)?;
                let [x, y, _, _] = mask.area;
                let target = [
                    x.checked_add_signed(dx).ok_or("move outside canvas")?,
                    y.checked_add_signed(dy).ok_or("move outside canvas")?,
                ];
                let clip = Clip::capture(&mask, image);
                (clip, target, true)
            }
            Operation::Paste(path, target) => (Clip::read(&path)?, target, false),
        };
        check_extent(target, clip.size, view)?;
        if moving {
            clip.write_pixels(clip.origin, [false; 2], true, image, tracker);
        }
        clip.write_pixels(target, self.flip, false, image, tracker);
        let mut report = clip.report();
        report["action"] = json!(if moving { "move" } else { "paste" });
        report["target_anchor"] = json!(target);
        report["flip_x"] = json!(self.flip[0]);
        report["flip_y"] = json!(self.flip[1]);
        Ok(report)
    }
}
