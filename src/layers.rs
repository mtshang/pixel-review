use crate::{Image, Point, Result, View};
use serde_json::{json, Value};
use std::path::Path;
use tempfile::NamedTempFile;

pub struct Layer {
    pub name: String,
    pub at: [i32; 2],
    pub visible: bool,
    pub image: Image,
    // None until this exact pixel buffer is committed to the current project.
    pub file: Option<String>,
}

impl Layer {
    pub fn new(name: String, image: Image, at: [i32; 2]) -> Result<Self> {
        if name.trim().is_empty() || name.chars().any(char::is_control) || name.len() > 240 {
            return Err("layer name must be 1..240 UTF-8 bytes without control characters".into());
        }
        Ok(Self {
            name,
            image,
            at,
            visible: true,
            file: None,
        })
    }

    pub fn description(&self) -> Value {
        let view = self.image.view();
        json!({"name":self.name,"at":self.at,"visible":self.visible,"width":view.w,"height":view.h})
    }
}

pub struct Stack {
    pub palette: crate::palette::Palette,
    pub layers: Vec<Layer>, // bottom first
    pub active: String,
}

impl Stack {
    pub fn index(&self, name: Option<&str>) -> Result<usize> {
        let name = name.unwrap_or(&self.active);
        self.layers
            .iter()
            .position(|l| l.name == name)
            .ok_or_else(|| format!("unknown layer: {name}").into())
    }

    pub fn add(&mut self, layer: Layer) -> Result<()> {
        if self.layers.iter().any(|l| l.name == layer.name) {
            return Err("layer name already exists".into());
        }
        self.active = layer.name.clone();
        self.layers.push(layer);
        Ok(())
    }

    pub fn is_plain(&self, size: Point) -> bool {
        self.palette.is_empty() && self.layers.len() == 1
            && self.layers[0].name == "Base"
            && self.layers[0].visible
            && self.layers[0].at == [0, 0]
            && self.layers[0].image.view().area() == [0, 0, size[0], size[1]]
    }

    pub fn render(&self, dest: &Path, view: View) -> Result<NamedTempFile> {
        let base = &self.layers[0].image;
        let visible: Vec<_> = self.layers.iter().filter(|l| l.visible).collect();
        let mut row = vec![0; view.w as usize * 4];
        base.stage_rows(dest, view, |local_y, expanded| {
            row.fill(0);
            let y = i64::from(view.y + local_y);
            for (index, layer) in visible.iter().enumerate() {
                let [x0, y0] = layer.at.map(i64::from);
                let native = layer.image.view();
                if y < y0 || y >= y0 + i64::from(native.h) {
                    continue;
                }
                let left = x0.max(i64::from(view.x));
                let right = (x0 + i64::from(native.w)).min(i64::from(view.x + view.w));
                if right <= left {
                    continue;
                }
                let src = layer.image.row_span(
                    (left - x0) as u32,
                    (y - y0) as u32,
                    (right - left) as u32,
                );
                let offset = (left - i64::from(view.x)) as usize * 4;
                let dst = &mut row[offset..offset + src.len()];
                if index == 0 {
                    dst.copy_from_slice(src);
                } else {
                    for (top, bottom) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
                        over(top, bottom);
                    }
                }
            }
            Image::expand(&row, view.scale, expanded)
        })
    }
}

// Straight RGBA8, source-over, rounding once per channel per visible layer.
pub(crate) fn over(src: &[u8], dst: &mut [u8]) {
    let a = u32::from(src[3]);
    if a == 0 {
        return;
    }
    if a == 255 || dst[3] == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let bottom = u32::from(dst[3]) * (255 - a);
    let total = a * 255 + bottom;
    for i in 0..3 {
        dst[i] =
            ((u32::from(src[i]) * a * 255 + u32::from(dst[i]) * bottom + total / 2) / total) as u8;
    }
    dst[3] = ((total + 127) / 255) as u8;
}
