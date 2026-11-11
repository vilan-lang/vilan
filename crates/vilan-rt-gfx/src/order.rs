//! Paint order by an overlap test over bounds (ruling R14, GPUI's design).
//!
//! The scene is a list in paint order, and a primitive must be drawn after
//! every EARLIER primitive it overlaps. Nothing else constrains the order.
//! Each primitive therefore gets an ORDER number: one more than the largest
//! order among the earlier primitives whose bounds intersect its own, or 1
//! when it overlaps nothing. Two primitives with the same order never overlap,
//! so they may be drawn in either sequence, in one batch or in two; the
//! batcher (`encode.rs`) uses that freedom to merge primitives of one kind
//! across the whole frame.
//!
//! The test is exact (rectangle intersection, on the quads the GPU will
//! actually rasterise, clipped), and a uniform grid of cells keeps it from
//! being quadratic: each cell lists the primitives touching it, and a new
//! primitive is tested only against the lists of the cells it touches.
//! Touching edges do not overlap: a quad covers the pixels strictly inside its
//! bounds, so two snapped rectangles that share an edge share no pixel.

/// A rectangle in device pixels: `[left, top, right, bottom]`.
pub(crate) type DeviceBounds = [f32; 4];

/// The side of a grid cell, in device pixels. 64 keeps a typical row of text
/// or a button in a handful of cells, and a 1920 x 1080 frame at 510 cells.
const CELL_SIZE: f32 = 64.0;

/// The grid is never wider or taller than this many cells: a frame larger
/// than 16,384 device pixels still works, its far cells just collect more.
const MAXIMUM_CELLS_PER_SIDE: usize = 256;

pub(crate) fn intersects(first: &DeviceBounds, second: &DeviceBounds) -> bool {
    first[0] < second[2] && second[0] < first[2] && first[1] < second[3] && second[1] < first[3]
}

pub(crate) fn intersection(first: &DeviceBounds, second: &DeviceBounds) -> DeviceBounds {
    [
        first[0].max(second[0]),
        first[1].max(second[1]),
        first[2].min(second[2]),
        first[3].min(second[3]),
    ]
}

pub(crate) fn is_empty(bounds: &DeviceBounds) -> bool {
    !(bounds[2] > bounds[0] && bounds[3] > bounds[1])
}

pub(crate) struct OrderAssigner {
    columns: usize,
    rows: usize,
    cells: Vec<Vec<u32>>,
    placed: Vec<(DeviceBounds, u32)>,
}

impl OrderAssigner {
    /// An assigner for a frame of `width` x `height` device pixels. Bounds
    /// outside the frame are still compared exactly; they fall in the edge
    /// cells.
    pub(crate) fn new(width: u32, height: u32) -> OrderAssigner {
        let side = |pixels: u32| {
            ((pixels as f32 / CELL_SIZE).ceil() as usize).clamp(1, MAXIMUM_CELLS_PER_SIDE)
        };
        let (columns, rows) = (side(width), side(height));
        OrderAssigner {
            columns,
            rows,
            cells: vec![Vec::new(); columns * rows],
            placed: Vec::new(),
        }
    }

    /// Empties the assigner for a new frame, keeping its allocations.
    pub(crate) fn reset(&mut self, width: u32, height: u32) {
        let fresh = OrderAssigner::new(width, height);
        if fresh.columns == self.columns && fresh.rows == self.rows {
            for cell in &mut self.cells {
                cell.clear();
            }
            self.placed.clear();
        } else {
            *self = fresh;
        }
    }

    /// The first and last cell, per axis, that the bounds reach into. The far
    /// edge is exclusive: bounds ending exactly on a cell boundary do not
    /// reach the next cell, and bounds ending anywhere inside a cell do.
    fn cell_range(&self, bounds: &DeviceBounds) -> (usize, usize, usize, usize) {
        let first = |start: f32, limit: usize| {
            ((start / CELL_SIZE).floor().max(0.0) as usize).min(limit - 1)
        };
        let last = |end: f32, limit: usize| {
            (((end / CELL_SIZE).ceil().max(1.0) as usize) - 1).min(limit - 1)
        };
        let (first_column, first_row) =
            (first(bounds[0], self.columns), first(bounds[1], self.rows));
        (
            first_column,
            first_row,
            last(bounds[2], self.columns).max(first_column),
            last(bounds[3], self.rows).max(first_row),
        )
    }

    /// The order of a primitive with these (non-empty) bounds, painted after
    /// every primitive inserted before it.
    pub(crate) fn insert(&mut self, bounds: DeviceBounds) -> u32 {
        let (first_column, first_row, last_column, last_row) = self.cell_range(&bounds);
        let mut largest_below = 0;
        for row in first_row..=last_row {
            for column in first_column..=last_column {
                for &index in &self.cells[row * self.columns + column] {
                    let (other, order) = &self.placed[index as usize];
                    if *order > largest_below && intersects(&bounds, other) {
                        largest_below = *order;
                    }
                }
            }
        }
        let order = largest_below + 1;
        let index =
            u32::try_from(self.placed.len()).expect("fewer than 2^32 primitives in a frame");
        self.placed.push((bounds, order));
        for row in first_row..=last_row {
            for column in first_column..=last_column {
                self.cells[row * self.columns + column].push(index);
            }
        }
        order
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_that_overlap_nothing_share_order_one() {
        let mut assigner = OrderAssigner::new(400, 400);
        assert_eq!(assigner.insert([0.0, 0.0, 50.0, 50.0]), 1);
        assert_eq!(assigner.insert([100.0, 0.0, 150.0, 50.0]), 1);
        assert_eq!(assigner.insert([300.0, 300.0, 390.0, 390.0]), 1);
    }

    #[test]
    fn a_primitive_overlapping_an_earlier_one_comes_after_it() {
        let mut assigner = OrderAssigner::new(400, 400);
        let background = assigner.insert([0.0, 0.0, 400.0, 400.0]);
        let label = assigner.insert([10.0, 10.0, 60.0, 30.0]);
        let badge = assigner.insert([50.0, 20.0, 70.0, 40.0]);
        let elsewhere = assigner.insert([200.0, 200.0, 220.0, 220.0]);
        assert_eq!((background, label, badge, elsewhere), (1, 2, 3, 2));
    }

    #[test]
    fn shared_edges_are_not_an_overlap() {
        let mut assigner = OrderAssigner::new(400, 400);
        assert_eq!(assigner.insert([0.0, 0.0, 64.0, 64.0]), 1);
        assert_eq!(assigner.insert([64.0, 0.0, 128.0, 64.0]), 1);
        assert_eq!(assigner.insert([0.0, 64.0, 64.0, 128.0]), 1);
    }

    #[test]
    fn bounds_off_the_frame_are_still_compared_exactly() {
        let mut assigner = OrderAssigner::new(100, 100);
        assert_eq!(assigner.insert([-500.0, -500.0, -400.0, -400.0]), 1);
        assert_eq!(assigner.insert([-450.0, -450.0, -420.0, -420.0]), 2);
        assert_eq!(assigner.insert([-10.0, -10.0, -5.0, -5.0]), 1);
        assert_eq!(assigner.insert([5000.0, 5000.0, 5001.0, 5001.0]), 1);
    }

    /// The grid is an acceleration only: it must answer exactly what the
    /// quadratic definition answers.
    #[test]
    fn the_grid_answers_what_the_definition_answers() {
        let mut state: u32 = 0x9e37_79b9;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state % 1000) as f32
        };
        let mut assigner = OrderAssigner::new(900, 700);
        let mut seen: Vec<(DeviceBounds, u32)> = Vec::new();
        for _ in 0..600 {
            // Fractional starts and ends, so bounds end inside cells too.
            let (x, y) = (
                next() - 50.0 + next() / 1000.0,
                next() - 150.0 + next() / 1000.0,
            );
            let (width, height) = (1.0 + next() / 8.0, 1.0 + next() / 8.0);
            let bounds = [x, y, x + width, y + height];
            let expected = 1 + seen
                .iter()
                .filter(|(other, _)| intersects(&bounds, other))
                .map(|(_, order)| *order)
                .max()
                .unwrap_or(0);
            assert_eq!(assigner.insert(bounds), expected);
            seen.push((bounds, expected));
        }
    }
}
