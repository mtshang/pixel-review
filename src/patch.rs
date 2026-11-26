//! Batch edits use global source coordinates. Apply in memory before committing any file.
use crate::{clean, color, hex, Point, Result, View};
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
};
use tempfile::NamedTempFile;

pub(crate) fn read_json(path: &Path) -> Result<Value> {
    let bytes = if path == Path::new("-") {
        let mut buf = Vec::new();
        io::stdin().read_to_end(&mut buf)?;
        buf
    } else {
        fs::read(path)?
    };
    Ok(serde_json::from_slice(
        bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes),
    )?)
}

fn uint(v: &Value) -> Result<u32> {
    Ok(u32::try_from(
        v.as_u64().ok_or("expected a nonnegative integer")?,
    )?)
}

fn list(v: &Value) -> Result<&Vec<Value>> {
    v.as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| "expected a nonempty array".into())
}

fn coords<const N: usize>(v: &Value) -> Result<[u32; N]> {
    let a = list(v)?;
    if a.len() != N {
        return Err(format!("expected {N} coordinates").into());
    }
    let mut out = [0; N];
    for (dst, n) in out.iter_mut().zip(a) {
        *dst = uint(n)?;
    }
    Ok(out)
}

pub(crate) fn literal(v: &Value) -> Result<[u8; 4]> {
    let s = v.as_str().ok_or("expected a color string")?;
    if s == "transparent" {
        Ok([0; 4])
    } else {
        color(s)
    }
}

fn fields(v: &Value, allowed: &[&str]) -> Result<()> {
    for key in v.as_object().ok_or("expected a JSON object")?.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unexpected field {key}").into());
        }
    }
    Ok(())
}

fn check_rect(view: View, [x, y, w, h]: [u32; 4]) -> Result<()> {
    if w == 0 || h == 0 {
        return Err("empty rectangle".into());
    }
    view.locate([x, y], false)?;
    let end = [
        x.checked_add(w - 1).ok_or("x overflow")?,
        y.checked_add(h - 1).ok_or("y overflow")?,
    ];
    view.locate(end, false)?;
    Ok(())
}

fn matrix_view(doc: &Value) -> Result<View> {
    if doc["command"] != "matrix" {
        return Err("expected an exported matrix document".into());
    }
    let region = &doc["region"];
    let source = &doc["source"];
    View::new(
        [
            uint(&region["x"])?,
            uint(&region["y"])?,
            uint(&region["width"])?,
            uint(&region["height"])?,
        ],
        1,
        [uint(&source["width"])?, uint(&source["height"])?],
    )
}

use crate::palette::Palette;
type Put<'a> = dyn FnMut(Point, [u8; 4]) + 'a;

fn read_grid(
    view: View,
    at: Point,
    rows: &[Value],
    symbol_mode: bool,
    palette: &Palette,
    expected: Option<[u32; 2]>,
    put: &mut Put,
) -> Result<u64> {
    let width = |row: &Value| -> Result<usize> {
        if symbol_mode {
            Ok(row.as_str().ok_or("expected a string row")?.chars().count())
        } else {
            Ok(list(row)?.len())
        }
    };
    let w = width(&rows[0])?;
    let size = [u32::try_from(w)?, u32::try_from(rows.len())?];
    if expected.is_some_and(|s| s != size) {
        return Err("matrix dimensions differ from its region".into());
    }
    check_rect(view, [at[0], at[1], size[0], size[1]])?;
    let mut writes = 0;
    for (j, row) in rows.iter().enumerate() {
        if width(row)? != w {
            return Err(format!("ragged grid at row {j}").into());
        }
        let mut cell = |i: usize, rgba| {
            put([at[0] + i as u32, at[1] + j as u32], rgba);
            writes += 1;
        };
        if symbol_mode {
            for (i, ch) in row.as_str().unwrap().chars().enumerate() {
                if ch != '.' {
                    cell(
                        i,
                        *palette
                            .get(&ch)
                            .ok_or_else(|| format!("undefined symbol '{ch}'"))?,
                    );
                }
            }
        } else {
            for (i, v) in row.as_array().unwrap().iter().enumerate() {
                if !v.is_null() {
                    cell(i, literal(v)?);
                } else if expected.is_some() {
                    return Err("exported matrix requires an RGBA color in each cell".into());
                }
            }
        }
    }
    Ok(writes)
}

fn operation(op: &Value, view: View, palette: &Palette, put: &mut Put) -> Result<u64> {
    if op.get("points").is_some() || op.get("rects").is_some() {
        let points = op.get("points").is_some();
        let key = if points { "points" } else { "rects" };
        fields(op, &[key, "color"])?;
        let rgba = literal(&op["color"])?;
        let mut count = 0;
        for entry in list(&op[key])? {
            let rect = if points {
                let [x, y] = coords(entry)?;
                [x, y, 1, 1]
            } else {
                coords(entry)?
            };
            check_rect(view, rect)?;
            let [x, y, w, h] = rect;
            for iy in y..y + h {
                for ix in x..x + w {
                    put([ix, iy], rgba);
                }
            }
            count += u64::from(w) * u64::from(h);
        }
        return Ok(count);
    }
    let symbols = op.get("rows").is_some();
    let key = if symbols { "rows" } else { "pixels" };
    fields(op, &["at", key])?;
    read_grid(
        view,
        coords(&op["at"])?,
        list(&op[key])?,
        symbols,
        palette,
        None,
        put,
    )
}

fn replay_matrix(doc: &Value, view: View, put: &mut Put) -> Result<u64> {
    let src = matrix_view(doc)?;
    read_grid(
        view,
        [src.x, src.y],
        list(&doc["pixels"])?,
        false,
        &Palette::new(),
        Some([src.w, src.h]),
        put,
    )
}

pub fn apply(
    path: &Path,
    view: View,
    shared: &Palette,
    mut put: impl FnMut(Point, [u8; 4]),
) -> Result<Value> {
    let doc = read_json(path)?;
    if doc["command"] == "matrix" {
        let matrices = match doc.get("regions") {
            Some(items) => list(items)?.as_slice(),
            None => std::slice::from_ref(&doc),
        };
        let mut writes = 0;
        for item in matrices {
            writes += replay_matrix(item, view, &mut put)?;
        }
        return Ok(json!({"operations":matrices.len(),"pixel_writes":writes}));
    }
    let mut palette = shared.clone();
    fields(&doc, &["palette", "ops"])?;
    if let Some(p) = doc.get("palette") {
        palette.extend(crate::palette::parse(p)?);
    }
    let ops = list(&doc["ops"])?;
    let mut writes = 0;
    for (i, op) in ops.iter().enumerate() {
        writes += operation(op, view, &palette, &mut put).map_err(|e| format!("ops[{i}]: {e}"))?;
    }
    Ok(json!({"operations":ops.len(),"pixel_writes":writes}))
}

pub fn edit_matrix(src: &Path, patch: &Path, dest: &Path, palette: &Palette) -> Result<Value> {
    let mut doc = read_json(src)?;
    let view = matrix_view(&doc)?;
    // Validate even the cells outside this patch before publishing a new matrix.
    read_grid(
        view,
        [view.x, view.y],
        list(&doc["pixels"])?,
        false,
        &Palette::new(),
        Some([view.w, view.h]),
        &mut |_, _| {},
    )?;
    let cells = doc["pixels"].as_array_mut().unwrap();
    let summary = apply(patch, view, palette, |[x, y], rgba| {
        cells[(y - view.y) as usize][(x - view.x) as usize] = Value::String(hex(&rgba));
    })?;
    let dest = std::path::absolute(dest)?;
    let mut tmp = NamedTempFile::new_in(dest.parent().ok_or("missing output folder")?)?;
    {
        let mut buffer = io::BufWriter::new(&mut tmp);
        serde_json::to_writer(&mut buffer, &doc)?;
        buffer.write_all(b"\n")?;
        buffer.flush()?;
    }
    tmp.persist_noclobber(&dest)?;
    Ok(
        json!({"command":"edit-matrix","out":clean(dest.canonicalize()?),"patch":summary,"region":doc["region"]}),
    )
}
