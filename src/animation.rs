use crate::{
    finish_args, info, inspect::AlphaBounds, layers::over, output_dest, Image, Point, PreviewScale,
    Result, Rows, View,
};
use pico_args::Arguments;
use serde_json::{json, Value};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

fn frame(path: &Path, size: Point) -> Result<Image> {
    let rows = Rows::open(path)?;
    if [rows.width, rows.height] != size {
        return Err(format!(
            "frame {} is {}x{}, expected {}x{}",
            path.display(),
            rows.width,
            rows.height,
            size[0],
            size[1]
        )
        .into());
    }
    Image::load(rows, [0, 0, size[0], size[1]])
}

struct Ghost {
    image: Image,
    color: [u8; 3],
}

// Composite changed pixels only. Compare both neighbours against the original
// current row so a previous ghost cannot influence the next comparison.
fn render(
    image: &Image,
    v: View,
    ghosts: &[Ghost],
    opacity: u32,
    baseline: Option<u32>,
    output: &mut Vec<u8>,
) -> Result<AlphaBounds> {
    output.clear();
    let bytes = (v.width() as usize)
        .checked_mul(v.height() as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or("frame too large")?;
    output.try_reserve_exact(bytes)?;
    let mut bounds = AlphaBounds::default();
    let mut mixed = Vec::new();
    let mut expanded = Vec::new();
    for y in 0..v.h {
        let src = image.row_span(0, y, v.w);
        bounds.row(y, src);
        mixed.clear();
        mixed.extend_from_slice(src);
        for ghost in ghosts {
            let other = ghost.image.row_span(0, y, v.w);
            for ((a, b), dst) in src
                .chunks_exact(4)
                .zip(other.chunks_exact(4))
                .zip(mixed.chunks_exact_mut(4))
            {
                if a != b && b[3] > 0 {
                    let [r, g, blue] = ghost.color;
                    over(
                        &[r, g, blue, ((u32::from(b[3]) * opacity + 50) / 100) as u8],
                        dst,
                    );
                }
            }
        }
        Image::expand(&mixed, v.scale, &mut expanded)?;
        for line in 0..v.scale {
            if baseline == Some(y) && line == v.scale - 1 {
                for (i, p) in expanded.chunks_exact_mut(4).enumerate() {
                    if (i / v.scale as usize) % 4 < 2 {
                        p.copy_from_slice(&[255, 200, 50, 255]);
                    }
                }
            }
            output.extend_from_slice(&expanded);
        }
    }
    Ok(bounds)
}

pub fn preview(cmd: &str, mut args: Arguments) -> Result<Value> {
    let onion = cmd == "onion";
    let sequence = crate::sequence::Sequence::read(&mut args, !onion)?;
    let paths = sequence.paths;
    let fps = sequence.fps;
    let baseline = sequence.baseline;
    let scaling = PreviewScale::parse(&mut args)?;
    let out = output_dest(
        args.value_from_str("--out")?,
        true,
        if onion { &["png"] } else { &["png", "apng"] },
    )?;
    let mut strip_path = None;
    let mut opacity = 40;
    let mut index = 0;
    if onion {
        let number: usize = args.value_from_str("--index")?;
        index = number
            .checked_sub(1)
            .filter(|&i| i < paths.len())
            .ok_or("--index outside 1..frame count")?;
        opacity = args.opt_value_from_str("--opacity")?.unwrap_or(40);
        if opacity > 100 {
            return Err("--opacity must be 0..100".into());
        }
    } else {
        let custom = args.opt_value_from_str::<_, PathBuf>("--strip-out")?;
        if args.contains("--no-strip") {
            if custom.is_some() {
                return Err("choose --strip-out or --no-strip".into());
            }
        } else {
            let path = custom
                .unwrap_or_else(|| out.with_file_name(format!("{}.frames.png", crate::stem(&out))));
            let path = crate::png_dest(path, true)?;
            crate::distinct(&out, &path)?;
            strip_path = Some(path);
        }
    }
    finish_args(args)?;
    let _out_lock = crate::write_lock(&out)?;
    let _strip_lock = strip_path.as_deref().map(crate::write_lock).transpose()?;
    if out.exists() || strip_path.as_ref().is_some_and(|p| p.exists()) {
        return Err("preview output already exists".into());
    }
    let dims = |path: &Path| -> Result<Point> {
        let rows = Rows::open(path)?;
        Ok([rows.width, rows.height])
    };
    let size = dims(&paths[0])?;
    let v = scaling.view([0, 0, size[0], size[1]], size)?;
    if baseline.is_some_and(|y| y >= v.h) {
        return Err("--baseline is a zero-based source row".into());
    }
    let mut ghosts = Vec::new();
    if onion {
        for path in &paths[1..] {
            if dims(path)? != size {
                return Err(format!("frame size mismatch: {}", path.display()).into());
            }
        }
        if index > 0 {
            ghosts.push(Ghost {
                image: frame(&paths[index - 1], size)?,
                color: [0, 210, 255],
            });
        }
        if index + 1 < paths.len() {
            ghosts.push(Ghost {
                image: frame(&paths[index + 1], size)?,
                color: [255, 64, 170],
            });
        }
    }
    let mut tmp = NamedTempFile::new_in(out.parent().ok_or("missing preview folder")?)?;
    let mut reports = Vec::new();
    let mut strip = strip_path
        .as_ref()
        .map(|_| crate::strip::Strip::new(paths.len(), size, scaling))
        .transpose()?;
    {
        let mut sink = BufWriter::new(tmp.as_file_mut());
        let mut encoder = png::Encoder::new(&mut sink, v.width(), v.height());
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        if !onion {
            encoder.set_animated(u32::try_from(paths.len())?, 0)?;
            encoder.set_frame_delay(1, fps)?;
            encoder.set_blend_op(png::BlendOp::Source)?;
        }
        let mut writer = encoder.write_header()?;
        let mut pixels = Vec::new();
        let mut strip_pixels = Vec::new();
        for (i, path) in paths.iter().enumerate() {
            if onion && i != index {
                continue;
            }
            let image = frame(path, size)?;
            let bounds = render(&image, v, &ghosts, opacity, baseline, &mut pixels)?;
            writer.write_image_data(&pixels)?;
            let bottom = bounds.0.map(|b| b[3]);
            reports.push(json!({"index":i+1,"source":path,"bounds":bounds.0.map(|[l,t,r,b]|json!({
                "left":l,"top":t,"right":r,"bottom":b,"width":r-l+1,"height":b-t+1})),
                "bottom_y":bottom,"bottom_delta":bottom.zip(baseline).map(|(b,y)|i64::from(b)-i64::from(y))}));
            if let Some(s) = &mut strip {
                let panel = if s.view.scale == v.scale {
                    s.put(i, &pixels)
                } else {
                    render(&image, s.view, &[], opacity, baseline, &mut strip_pixels)?;
                    s.put(i, &strip_pixels)
                };
                reports.last_mut().unwrap()["strip_region"] = crate::rect(panel);
            }
        }
        writer.finish()?;
        sink.flush()?;
    }
    let mut strip_info = None;
    if let (Some(s), Some(path)) = (strip, strip_path.as_ref()) {
        let v = s.image.view();
        s.image.stage(path, v)?.persist_noclobber(path)?;
        let mut desc = info(path, v.w, v.h);
        desc["scale"] = json!(s.view.scale);
        strip_info = Some(desc);
    }
    if let Err(err) = tmp.persist_noclobber(&out) {
        if let Some(path) = &strip_path {
            std::fs::remove_file(path)?;
        }
        return Err(err.into());
    }
    let mut result = json!({"command":cmd,"frame_count":paths.len(),"native_size":size,"scale":v.scale,
        "baseline":baseline,"frames":reports,"preview":info(&out,v.width(),v.height())});
    if onion {
        result["opacity"] = json!(opacity);
        result["previous"] = json!(index.checked_sub(1).map(|i| &paths[i]));
        result["next"] = json!(paths.get(index + 1));
    } else {
        result["fps"] = json!(fps);
        result["strip"] = json!(strip_info);
    }
    result["config"] = json!(sequence.config);
    Ok(result)
}
