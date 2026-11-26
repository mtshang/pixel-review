use crate::{Image, PreviewScale, Result, View};

// 3x5 digit glyphs. Labels occupy a separate band above the untouched frames.
const DIGITS: [[u8; 5]; 10] = [
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [7, 1, 7, 4, 7],
    [7, 1, 7, 1, 7],
    [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7],
    [7, 4, 7, 5, 7],
    [7, 1, 1, 1, 1],
    [7, 5, 7, 5, 7],
    [7, 5, 7, 1, 7],
];
const FONT: u32 = 3;
const HEADER: u32 = 23;
const GAP: u32 = 4;

pub struct Strip {
    pub image: Image,
    pub view: View,
    cell: u32,
}
impl Strip {
    pub fn new(count: usize, size: [u32; 2], scaling: PreviewScale) -> Result<Self> {
        let n = u32::try_from(count)?;
        let seq_width = size[0].checked_mul(n).ok_or("frame strip too wide")?;
        let scaled = scaling.view([0, 0, seq_width, size[1]], [seq_width, size[1]])?;
        let view = View::new([0, 0, size[0], size[1]], scaled.scale, size)?;
        let cell = view
            .width()
            .max((4 * count.to_string().len() as u32 - 1) * FONT + 4);
        let width = (u64::from(cell) + u64::from(GAP)) * u64::from(n) - u64::from(GAP);
        let mut image = Image::blank([
            u32::try_from(width)?,
            view.height().checked_add(HEADER).ok_or("strip too tall")?,
        ])?;
        let band = [35, 41, 49, 255].repeat(image.view().w as usize);
        for y in 0..HEADER {
            image.write_row_span(0, y, &band);
        }
        Ok(Self { image, view, cell })
    }

    pub fn put(&mut self, index: usize, pixels: &[u8]) -> [u32; 4] {
        let left = (u64::from(self.cell + GAP) * index as u64) as u32;
        draw_number(&mut self.image, index + 1, left, 4, self.cell);
        let x = left + (self.cell - self.view.width()) / 2;
        for (y, row) in pixels
            .chunks_exact(self.view.width() as usize * 4)
            .enumerate()
        {
            self.image.write_row_span(x, HEADER + y as u32, row);
        }
        [x, HEADER, self.view.width(), self.view.height()]
    }
}

pub(crate) fn draw_number(image: &mut Image, number: usize, left: u32, top: u32, width: u32) {
    let label = number.to_string();
    let label_width = (label.len() as u32 * 4 - 1) * FONT;
    let mut x = left + (width - label_width) / 2;
    for ch in label.bytes() {
        for (y, bits) in DIGITS[(ch - b'0') as usize].iter().enumerate() {
            for col in 0..3 {
                if bits & (4 >> col) != 0 {
                    for dy in 0..FONT {
                        for dx in 0..FONT {
                            image.set(
                                x + col * FONT + dx,
                                top + y as u32 * FONT + dy,
                                [245, 246, 248, 255],
                                None,
                            );
                        }
                    }
                }
            }
        }
        x += 4 * FONT;
    }
}
