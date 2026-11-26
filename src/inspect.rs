use crate::{clean, info, raster::Rows, Result, View};
use serde_json::{json, Value};
use std::fs::File;
use std::io::{self, BufWriter, Seek, Write};
use std::path::{Path, PathBuf};
use tempfile::{spooled_tempfile, NamedTempFile};

#[derive(Default)]
pub struct AlphaBounds(pub Option<[u32; 4]>);

impl AlphaBounds {
    pub fn row(&mut self, y: u32, row: &[u8]) {
        // Only the first and last occupied columns can change the box.
        let mut pixels = row.chunks_exact(4);
        if let Some(first) = pixels.position(|p| p[3] > 0) {
            let last = pixels
                .rposition(|p| p[3] > 0)
                .map_or(first, |i| first + 1 + i);
            let b = self.0.get_or_insert([first as u32,y,last as u32,y]);
            b[0] = b[0].min(first as u32);
            b[2] = b[2].max(last as u32);
            b[3] = y;
        }
    }
}

pub fn bounds(path: &Path) -> Result<Value> {
    let mut rows = Rows::open(path)?;
    let (width, height) = (rows.width, rows.height);
    let mut bounds = AlphaBounds::default();
    rows.visit_region([0, 0, width, height], |y, row| {
        bounds.row(y,row);
        Ok(())
    })?;
    let mut result = json!({"command":"bounds","source":info(path,width,height),
                            "bounds":null,"margins":null});
    if let Some([left,top,right,bottom]) = bounds.0 {
        result["bounds"] = json!({"left":left,"top":top,"right":right,"bottom":bottom,
                                   "width":right-left+1,"height":bottom-top+1});
        result["margins"] =
            json!({"left":left,"top":top,"right":width-right-1,"bottom":height-bottom-1});
    }
    Ok(result)
}

fn write_matrix(rows: &mut Rows, view: View, metadata: &Value, target: impl Write) -> Result<()> {
    let mut out = BufWriter::new(target);
    write!(
        out,
        "{{\"command\":\"matrix\",\"source\":{},\"region\":{},\"pixels\":[",
        metadata["source"], metadata["region"]
    )?;
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut line = Vec::new();
    line.try_reserve_exact(view.w as usize * 12 + 2)?;
    rows.visit_region(view.area(), |y, row| {
        line.clear();
        if y != view.y {
            line.push(b',');
        }
        line.push(b'[');
        for (x, p) in row.chunks_exact(4).enumerate() {
            if x > 0 {
                line.push(b',');
            }
            line.extend_from_slice(b"\"#");
            for &channel in p {
                line.push(HEX[(channel >> 4) as usize]);
                line.push(HEX[(channel & 15) as usize]);
            }
            line.push(b'"');
        }
        line.push(b']');
        out.write_all(&line)?;
        Ok(())
    })?;
    out.write_all(b"]}")?;
    out.flush()?;
    Ok(())
}

pub fn matrix(path: &Path, regions: &[[u32; 4]], dest: Option<PathBuf>) -> Result<Option<Value>> {
    let mut file = File::open(path)?;
    let mut rows = Rows::from_file(file.try_clone()?)?;
    let size = [rows.width, rows.height];
    let full = [[0, 0, size[0], size[1]]];
    let regions = if regions.is_empty() {
        &full[..]
    } else {
        regions
    };
    let views: Vec<View> = regions
        .iter()
        .map(|r| View::new(*r, 1, size))
        .collect::<Result<_>>()?;
    let source = info(path, size[0], size[1]);
    let metadata_for = |v: View| {
        json!({"command":"matrix","source":source,
        "region":{"x":v.x,"y":v.y,"width":v.w,"height":v.h}})
    };
    let batch = views.len() > 1;
    let mut metadata = if batch {
        json!({"command":"matrix","source":source,"count":views.len()})
    } else {
        metadata_for(views[0])
    };
    let mut write = |mut out: &mut dyn Write| -> Result<()> {
        if batch {
            write!(
                out,
                "{{\"command\":\"matrix\",\"source\":{},\"regions\":[",
                source
            )?;
        }
        for (i, &view) in views.iter().enumerate() {
            if i > 0 {
                out.write_all(b",")?;
                file.rewind()?;
                rows = Rows::from_file(file.try_clone()?)?;
            }
            write_matrix(&mut rows, view, &metadata_for(view), &mut out)?;
        }
        if batch {
            out.write_all(b"]}")?;
        }
        out.write_all(b"\n")?;
        Ok(())
    };
    if let Some(dest) = dest {
        let dest = std::path::absolute(dest)?;
        if dest.exists() {
            return Err("--out requires a new JSON file".into());
        }
        let mut tmp = NamedTempFile::new_in(dest.parent().ok_or("missing output folder")?)?;
        write(&mut tmp)?;
        tmp.persist_noclobber(&dest)?;
        metadata["out"] = json!(clean(dest.canonicalize()?));
        return Ok(Some(metadata));
    }
    // Hold small replies in RAM. Do not publish a partial JSON object if the PNG
    // turns out to be truncated, or allocate memory proportional to a huge export.
    let mut staged = spooled_tempfile(1024 * 1024);
    write(&mut staged)?;
    staged.rewind()?;
    let mut stdout = io::stdout().lock();
    io::copy(&mut staged, &mut stdout)?;
    stdout.flush()?;
    Ok(None)
}
