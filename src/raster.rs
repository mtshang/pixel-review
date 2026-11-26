use crate::{Result, View};
use png::{BitDepth, ColorType, Decoder, Encoder, Info, Transformations};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use tempfile::NamedTempFile;

pub struct Rows {
    reader: png::Reader<BufReader<File>>,
    interlaced: Option<Vec<u8>>,
    next_y: usize,
    channels: usize,
    pub width: u32,
    pub height: u32,
}

impl Rows {
    pub fn open(path: &Path) -> Result<Self> {
        Self::from_file(File::open(path)?)
    }

    pub fn from_file(file: File) -> Result<Self> {
        let mut decoder = Decoder::new(BufReader::new(file));
        decoder.set_transformations(Transformations::EXPAND);
        decoder.set_ignore_text_chunk(true);
        let reader = decoder.read_info()?;
        let info = reader.info();
        if info.animation_control.is_some() || info.bit_depth == BitDepth::Sixteen {
            return Err("expected a static PNG with at most 8 bits per channel".into());
        }
        let (width, height) = (info.width, info.height);
        let channels = reader.output_color_type().0.samples();
        Ok(Self {
            reader,
            interlaced: None,
            next_y: 0,
            channels,
            width,
            height,
        })
    }

    fn color_info(&self) -> Info<'static> {
        let src = self.reader.info();
        let mut dest = Info::default();
        dest.srgb = src.srgb;
        dest.source_gamma = src.source_gamma;
        dest.source_chromaticities = src.source_chromaticities;
        // A grayscale ICC profile cannot be attached unchanged to an RGB PNG.
        if !matches!(
            src.color_type,
            ColorType::Grayscale | ColorType::GrayscaleAlpha
        ) {
            dest.icc_profile = src.icc_profile.clone();
        }
        dest
    }

    pub fn read_span(&mut self, x: u32, width: u32, rgba: &mut Vec<u8>) -> Result<()> {
        if self.reader.info().interlaced && self.interlaced.is_none() {
            let size = self
                .reader
                .output_buffer_size()
                .ok_or("PNG dimensions overflow")?;
            let mut frame = Vec::new();
            frame.try_reserve_exact(size)?;
            frame.resize(size, 0);
            self.reader.next_frame(&mut frame)?;
            self.interlaced = Some(frame);
        }
        let row;
        let bytes = if let Some(frame) = &self.interlaced {
            let line = self.width as usize * self.channels;
            &frame[self.next_y * line..(self.next_y + 1) * line]
        } else {
            row = self.reader.next_row()?.ok_or("missing PNG row")?;
            row.data()
        };
        self.next_y += 1;
        let range = x as usize * self.channels..(x + width) as usize * self.channels;
        let span = &bytes[range];
        rgba.clear();
        rgba.try_reserve(width as usize * 4)?;
        if self.channels == 4 {
            rgba.extend_from_slice(span);
        } else {
            for p in span.chunks_exact(self.channels) {
                let color = match *p {
                    [g] => [g, g, g, 255],
                    [g, a] => [g, g, g, a],
                    [r, g, b] => [r, g, b, 255],
                    _ => return Err("unsupported decoded color format".into()),
                };
                rgba.extend_from_slice(&color);
            }
        }
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        Ok(self.reader.finish()?)
    }

    /// Sample validated absolute points in caller order, decoding each PNG row once.
    pub fn sample_points(mut self, points: &[[u32; 2]]) -> Result<Vec<[u8; 4]>> {
        let mut order: Vec<usize> = (0..points.len()).collect();
        order.sort_unstable_by_key(|&i| (points[i][1], points[i][0]));
        let mut colors = vec![[0; 4]; points.len()];
        let mut buf = Vec::new();
        for group in order.chunk_by(|&a, &b| points[a][1] == points[b][1]) {
            let [left, y] = points[group[0]];
            let right = points[*group.last().unwrap()][0];
            while self.next_y < y as usize {
                self.read_span(0, 0, &mut buf)?;
            }
            self.read_span(left, right - left + 1, &mut buf)?;
            for &i in group {
                let offset = (points[i][0] - left) as usize * 4;
                colors[i].copy_from_slice(&buf[offset..offset + 4]);
            }
        }
        self.finish()?;
        Ok(colors)
    }

    /// Read a validated rectangle from a fresh decoder. Row indices are global.
    pub fn visit_region(
        &mut self,
        area: [u32; 4],
        mut visit: impl FnMut(u32, &[u8]) -> Result<()>,
    ) -> Result<()> {
        let [x, y, w, h] = area;
        let mut row = Vec::new();
        for _ in 0..y {
            self.read_span(0, 0, &mut row)?;
        }
        for iy in y..y + h {
            self.read_span(x, w, &mut row)?;
            visit(iy, &row)?;
        }
        self.finish()
    }
}

pub struct Image {
    data: Vec<u8>,
    area: [u32; 4],
    info: Info<'static>,
}

// Snapshot only blocks containing a real write. Comparing against these original
// bytes after the entire batch also handles overlapping operations and reversions.
#[derive(Default)]
pub struct ChangeTracker {
    blocks: HashMap<usize, Vec<u8>>,
}

impl ChangeTracker {
    pub fn changed(&mut self, image: &Image) -> bool {
        self.blocks.retain(|start, old| image.data[*start..*start+old.len()] != old[..]);
        !self.blocks.is_empty()
    }

    pub fn swap(&mut self, image: &mut Image) {
        for (&start, block) in &mut self.blocks {
            let end = start + block.len();
            image.data[start..end].swap_with_slice(block);
        }
    }
    pub fn rollback(self, image: &mut Image) {
        for (start, before) in self.blocks {
            image.data[start..start + before.len()].copy_from_slice(&before);
        }
    }
    fn remember(&mut self, data: &[u8], offset: usize) {
        const BLOCK_BYTES: usize = 256 * 4;
        let start = offset / BLOCK_BYTES * BLOCK_BYTES;
        self.blocks.entry(start).or_insert_with(|| {
            data[start..data.len().min(start + BLOCK_BYTES)].to_vec()
        });
    }

    pub fn finish(self, image: &Image) -> Option<(u64, [u32; 4])> {
        let mut count = 0;
        let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
        for (start, before) in self.blocks {
            let after = &image.data[start..start + before.len()];
            for (i, (a, b)) in before.chunks_exact(4).zip(after.chunks_exact(4)).enumerate() {
                if a != b {
                    let index = start / 4 + i;
                    let x = image.area[0] + (index % image.area[2] as usize) as u32;
                    let y = image.area[1] + (index / image.area[2] as usize) as u32;
                    left = left.min(x);
                    right = right.max(x);
                    top = top.min(y);
                    bottom = bottom.max(y);
                    count += 1;
                }
            }
        }
        (count > 0).then(|| (count, [left, top, right-left+1, bottom-top+1]))
    }
}

impl Image {
    pub fn blank(size: [u32; 2]) -> Result<Self> {
        let area = View::new([0, 0, size[0], size[1]], 1, size)?.area();
        let length = (size[0] as usize).checked_mul(size[1] as usize)
            .and_then(|n| n.checked_mul(4)).ok_or("layer too large")?;
        let mut data = Vec::new();
        data.try_reserve_exact(length)?;
        data.resize(length, 0);
        Ok(Self { area, data, info: Info::default() })
    }

    pub fn view(&self) -> View {
        let [x,y,w,h] = self.area;
        View {x,y,w,h,scale:1}
    }

    pub fn write_row_span(&mut self,x:u32,y:u32,bytes:&[u8]) {
        let offset=self.offset(x,y);
        self.data[offset..offset+bytes.len()].copy_from_slice(bytes);
    }

    pub fn reframe(&self, size:[u32;2], shift:[i32;2]) -> Result<Self> {
        let mut result=Self::blank(size)?;
        result.info=self.info.clone();
        let [dx,dy]=shift.map(i64::from);
        let [_,_,w,h]=self.area;
        let left=dx.max(0);
        let right=(dx+i64::from(w)).min(i64::from(size[0]));
        if right>left {
            for y in dy.max(0)..(dy+i64::from(h)).min(i64::from(size[1])) {
                let row=self.row_span((left-dx) as u32,(y-dy) as u32,(right-left) as u32);
                result.write_row_span(left as u32,y as u32,row);
            }
        }
        Ok(result)
    }
    pub fn load(mut src: Rows, area: [u32; 4]) -> Result<Self> {
        let [_, _, w, h] = area;
        let mut data = Vec::new();
        let size = (w as usize)
            .checked_mul(h as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or("source too large")?;
        data.try_reserve_exact(size)?;
        src.visit_region(area, |_, row| {
            data.extend_from_slice(row);
            Ok(())
        })?;
        Ok(Self {
            data,
            area,
            info: src.color_info(),
        })
    }

    fn offset(&self, x: u32, y: u32) -> usize {
        (((y - self.area[1]) as usize) * self.area[2] as usize + (x - self.area[0]) as usize) * 4
    }

    pub fn row_span(&self, x: u32, y: u32, width: u32) -> &[u8] {
        let i = self.offset(x, y);
        &self.data[i..i + width as usize * 4]
    }

    pub fn pixel(&self, x: u32, y: u32) -> &[u8] {
        let i = self.offset(x, y);
        &self.data[i..i + 4]
    }

    pub fn set(&mut self, x: u32, y: u32, rgba: [u8; 4], tracker: Option<&mut ChangeTracker>) {
        let i = self.offset(x, y);
        if self.data[i..i+4] == rgba { return; }
        if let Some(tracker) = tracker { tracker.remember(&self.data, i); }
        self.data[i..i + 4].copy_from_slice(&rgba);
    }

    pub fn expand_row(&self, v: View, local_y: u32, expanded: &mut Vec<u8>) -> Result<()> {
        let i = self.offset(v.x, v.y + local_y);
        let row = &self.data[i..i + v.w as usize * 4];
        Self::expand(row, v.scale, expanded)
    }

    pub fn expand(row: &[u8], scale: u32, expanded: &mut Vec<u8>) -> Result<()> {
        expanded.clear();
        expanded.try_reserve(row.len() * scale as usize)?;
        if scale == 1 {
            expanded.extend_from_slice(row);
        } else {
            for p in row.chunks_exact(4) {
                for _ in 0..scale {
                    expanded.extend_from_slice(p);
                }
            }
        }
        Ok(())
    }

    pub fn stage(&self, dest: &Path, v: View) -> Result<NamedTempFile> {
        self.stage_rows(dest, v, |y, out| self.expand_row(v, y, out))
    }

    pub fn stage_rows(&self, dest: &Path, v: View, mut row: impl FnMut(u32, &mut Vec<u8>) -> Result<()>) -> Result<NamedTempFile> {
        let mut tmp = tempfile::Builder::new().prefix("layer-").suffix(".png")
            .tempfile_in(dest.parent().ok_or("missing output folder")?)?;
        let mut info = self.info.clone();
        info.width = v.width();
        info.height = v.height();
        info.color_type = ColorType::Rgba;
        info.bit_depth = BitDepth::Eight;
        {
            let mut buf = BufWriter::new(tmp.as_file_mut());
            let mut encoder = Encoder::with_info(&mut buf, info)?;
            encoder.set_compression(png::Compression::Fast);
            let mut writer = encoder.write_header()?;
            let mut stream = writer.stream_writer_with_size(64 * 1024)?;
            let mut expanded = Vec::new();
            for y in 0..v.h {
                row(y, &mut expanded)?;
                for _ in 0..v.scale {
                    stream.write_all(&expanded)?;
                }
            }
            stream.finish()?;
            writer.finish()?;
            buf.flush()?;
        }
        Ok(tmp)
    }
}
