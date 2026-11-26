use crate::{
    layers::{Layer, Stack},
    palette::Palette,
    ChangeTracker, Image, Result, View,
};
use std::collections::VecDeque;
use std::mem::swap;

// Each entry contains the inverse. Swapping with the live canvas turns it
// into the forward operation, including the prior save-point identity.
pub enum Change {
    Resize {
        view: View,
        positions: Vec<[i32; 2]>,
        buffers: Vec<(usize, Image)>,
    },
    Pixels(Vec<(usize, ChangeTracker)>),
    Select(String),
    Slot {
        index: usize,
        absent: Option<Box<Layer>>,
        active: String,
    },
    Layout {
        current: usize,
        previous: usize,
        at: [i32; 2],
        visible: bool,
    },
    Palette(Palette),
}

impl Change {
    fn apply(&mut self, stack: &mut Stack, full: &mut View) {
        match self {
            Self::Resize {
                view,
                positions,
                buffers,
            } => {
                swap(view, full);
                for (layer, at) in stack.layers.iter_mut().zip(positions) {
                    swap(&mut layer.at, at);
                }
                for (i, image) in buffers {
                    swap(&mut stack.layers[*i].image, image);
                    stack.layers[*i].file = None;
                }
            }
            Self::Pixels(edits) => {
                for (i, tracker) in edits {
                    let layer = &mut stack.layers[*i];
                    tracker.swap(&mut layer.image);
                    layer.file = None;
                }
            }
            Self::Select(active) => swap(active, &mut stack.active),
            Self::Slot {
                index,
                absent,
                active,
            } => {
                if let Some(mut layer) = absent.take() {
                    // Saves/save-as may have retired this layer's backing file.
                    layer.file = None;
                    stack.layers.insert(*index, *layer);
                } else {
                    *absent = Some(Box::new(stack.layers.remove(*index)));
                }
                swap(active, &mut stack.active);
            }
            Self::Layout {
                current,
                previous,
                at,
                visible,
            } => {
                let mut layer = stack.layers.remove(*current);
                swap(at, &mut layer.at);
                swap(visible, &mut layer.visible);
                stack.layers.insert(*previous, layer);
                swap(current, previous);
            }
            Self::Palette(old) => swap(old, &mut stack.palette),
        }
    }
}

#[derive(Default)]
pub struct History {
    undo: VecDeque<(u64, Change)>,
    redo: Vec<(u64, Change)>,
    sequence: u64,
    current: u64,
    saved: u64,
}

impl History {
    pub fn record(&mut self, change: Change) {
        self.redo.clear();
        if self.undo.len() == 32 {
            self.undo.pop_front();
        }
        self.undo.push_back((self.current, change));
        self.sequence += 1;
        self.current = self.sequence;
    }
    pub fn step(&mut self, stack: &mut Stack, full: &mut View, redo: bool) -> Result<()> {
        let mut entry = if redo {
            self.redo.pop()
        } else {
            self.undo.pop_back()
        }
        .ok_or(if redo {
            "nothing to redo"
        } else {
            "nothing to undo"
        })?;
        entry.1.apply(stack, full);
        swap(&mut self.current, &mut entry.0);
        if redo {
            self.undo.push_back(entry);
        } else {
            self.redo.push(entry);
        }
        Ok(())
    }
    pub fn saved(&mut self) {
        self.saved = self.current;
    }
    pub fn dirty(&self) -> bool {
        self.saved != self.current
    }
    pub fn counts(&self) -> [usize; 2] {
        [self.undo.len(), self.redo.len()]
    }
}
