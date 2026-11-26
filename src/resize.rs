use crate::{finish_args, info, numbers, png_dest, Image, Point, Result, Rows, View};
use pico_args::Arguments;
use serde_json::{json, Value};
use std::path::PathBuf;

pub struct Resize {
    pub size: Point,
    pub shift: [i32; 2],
    padding: [i32; 4],
    baseline: Option<u32>,
}

impl Resize {
    pub fn parse(args: &mut Arguments, old: Point) -> Result<Self> {
        let crop: Option<String> = args.opt_value_from_str("--crop")?;
        let baseline: Option<u32> = args.opt_value_from_str("--baseline")?;
        if baseline.is_some_and(|y| y >= old[1]) {
            return Err("baseline outside input canvas".into());
        }
        let mut edges = [0i32; 4];
        let mut any = false;
        for (edge, key) in edges
            .iter_mut()
            .zip(["--left", "--right", "--top", "--bottom"])
        {
            if let Some(n) = args.opt_value_from_str(key)? {
                *edge = n;
                any = true;
            }
        }
        let (size, shift) = match (crop, any) {
            (Some(raw), false) => {
                let rect = numbers(&raw)?;
                let v = View::new(rect, 1, old)?;
                ([v.w, v.h], [-(v.x as i32), -(v.y as i32)])
            }
            (None, true) => {
                let [l, r, t, b] = edges;
                let dim = |v: u32, a: i32, b: i32| -> Result<u32> {
                    let n = i64::from(v) + i64::from(a) + i64::from(b);
                    if !(1..=i64::from(i32::MAX)).contains(&n) {
                        return Err("invalid resulting canvas size".into());
                    }
                    Ok(n as u32)
                };
                ([dim(old[0], l, r)?, dim(old[1], t, b)?], [l, t])
            }
            _ => return Err("supply --crop X,Y,W,H or one or more signed edge amounts".into()),
        };
        Ok(Self {
            size,
            shift,
            padding: edges.map(|n| n.max(0)),
            baseline,
        })
    }
    pub fn apply(
        &self,
        stack: &mut crate::layers::Stack,
        full: &mut View,
        plain: bool,
        pad: &[String],
    ) -> Result<Option<crate::history::Change>> {
        let ids = pad
            .iter()
            .map(|s| stack.index(Some(s)))
            .collect::<Result<std::collections::HashSet<_>>>()?;
        let old = [full.w, full.h];
        if old == self.size && self.shift == [0, 0] {
            return Ok(None);
        }
        let next = View::new([0, 0, self.size[0], self.size[1]], 1, self.size)?;
        let positions = stack.layers.iter().map(|l| l.at).collect::<Vec<_>>();
        let mut destinations = positions.clone();
        let mut buffers = Vec::new();
        for (i, layer) in stack.layers.iter().enumerate() {
            if plain {
                buffers.push((i, layer.image.reframe(self.size, self.shift)?));
                continue;
            }
            let mut inset = [0, 0];
            if ids.contains(&i) && self.padding != [0; 4] {
                let [l, r, t, b] = self.padding;
                let v = layer.image.view();
                let size = [
                    u32::try_from(i64::from(v.w) + i64::from(l) + i64::from(r))?,
                    u32::try_from(i64::from(v.h) + i64::from(t) + i64::from(b))?,
                ];
                inset = [l, t];
                buffers.push((i, layer.image.reframe(size, inset)?));
            }
            for axis in 0..2 {
                destinations[i][axis] = i32::try_from(
                    i64::from(layer.at[axis]) + i64::from(self.shift[axis])
                        - i64::from(inset[axis]),
                )?;
            }
        }
        // Commit only after every allocation and offset check has succeeded.
        for (i, img) in &mut buffers {
            std::mem::swap(img, &mut stack.layers[*i].image);
            stack.layers[*i].file = None;
        }
        for (layer, at) in stack.layers.iter_mut().zip(destinations) {
            layer.at = at;
        }
        let view = std::mem::replace(full, next);
        Ok(Some(crate::history::Change::Resize {
            view,
            positions,
            buffers,
        }))
    }
    pub fn report(&self, old: Point) -> Value {
        let baseline = self
            .baseline
            .map(|y| i64::from(y) + i64::from(self.shift[1]));
        json!({"old_size":old,"new_size":self.size,"origin_shift":self.shift,"old_baseline":self.baseline,
          "new_baseline":baseline,"baseline_visible":baseline.map(|y| (0..i64::from(self.size[1])).contains(&y))})
    }
}

pub fn file(mut args: Arguments) -> Result<Value> {
    let source = crate::clean(
        args.value_from_str::<_, PathBuf>("--source")?
            .canonicalize()?,
    );
    let out = png_dest(args.value_from_str("--out")?, true)?;
    let rows = Rows::open(&source)?;
    let old = [rows.width, rows.height];
    let resize = Resize::parse(&mut args, old)?;
    finish_args(args)?;
    let image = Image::load(rows, [0, 0, old[0], old[1]])?.reframe(resize.size, resize.shift)?;
    image.stage(&out, image.view())?.persist_noclobber(&out)?;
    Ok(
        json!({"command":"resize","source":info(&source,old[0],old[1]),"out":info(&out,resize.size[0],resize.size[1]),"resize":resize.report(old)}),
    )
}
