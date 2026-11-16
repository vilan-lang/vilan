//! A shelf allocator for one atlas page, with deallocation.
//!
//! Glyphs at UI sizes are many small rectangles of a few distinct heights, so
//! the page is cut into horizontal SHELVES, each as tall as the glyphs it
//! holds (rounded up to [`SHELF_HEIGHT_STEP`] pixels so near-equal heights
//! share a shelf). A shelf keeps its free horizontal spans, sorted and
//! coalesced, so a freed glyph's space is reused by the next glyph that fits
//! it. A shelf left with nothing on it becomes an EMPTY shelf: neighbouring
//! empty shelves merge, an empty shelf at the bottom gives its height back,
//! and an empty shelf can be split to the height a new allocation needs. That
//! is what lets the atlas above evict glyphs and keep the page usable, where a
//! packer that only appends (the spike's) can only be thrown away whole.

/// Shelf heights are multiples of this many pixels.
pub(crate) const SHELF_HEIGHT_STEP: u32 = 4;

/// A request goes on an existing shelf only when the shelf is at most 3/2 the
/// height it needs, so a 10-pixel glyph never takes a slot on a 40-pixel shelf.
const MAXIMUM_SHELF_WASTE_NUMERATOR: u32 = 3;
const MAXIMUM_SHELF_WASTE_DENOMINATOR: u32 = 2;

/// A rectangle handed out by [`ShelfAllocator::allocate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Allocation {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    start: u32,
    end: u32,
}

#[derive(Debug)]
struct Shelf {
    y: u32,
    height: u32,
    /// Free spans, sorted by start, never adjacent (coalesced).
    free: Vec<Span>,
    /// Live allocations on this shelf; zero makes it an empty shelf.
    allocations: u32,
}

#[derive(Debug)]
pub(crate) struct ShelfAllocator {
    width: u32,
    height: u32,
    /// Sorted by `y`, contiguous from 0 to `shelved_height`.
    shelves: Vec<Shelf>,
    shelved_height: u32,
}

impl ShelfAllocator {
    pub(crate) fn new(width: u32, height: u32) -> ShelfAllocator {
        ShelfAllocator {
            width,
            height,
            shelves: Vec::new(),
            shelved_height: 0,
        }
    }

    fn whole_width(&self) -> Vec<Span> {
        vec![Span {
            start: 0,
            end: self.width,
        }]
    }

    /// A `width` x `height` rectangle, or `None` when the page has no room for
    /// it (or it is larger than the page, or empty).
    pub(crate) fn allocate(&mut self, width: u32, height: u32) -> Option<Allocation> {
        if width == 0 || height == 0 || width > self.width || height > self.height {
            return None;
        }
        let shelf_height = height
            .div_ceil(SHELF_HEIGHT_STEP)
            .saturating_mul(SHELF_HEIGHT_STEP)
            .min(self.height);
        let tallest_acceptable =
            shelf_height * MAXIMUM_SHELF_WASTE_NUMERATOR / MAXIMUM_SHELF_WASTE_DENOMINATOR;

        // 1. An occupied shelf of a fitting height: the shortest one with a
        //    free span wide enough (best fit by height, first fit by x).
        let mut best: Option<(usize, usize)> = None;
        for (shelf_index, shelf) in self.shelves.iter().enumerate() {
            if shelf.allocations == 0
                || shelf.height < shelf_height
                || shelf.height > tallest_acceptable
            {
                continue;
            }
            if best.is_some_and(|(best_shelf, _)| self.shelves[best_shelf].height <= shelf.height) {
                continue;
            }
            if let Some(span_index) = shelf
                .free
                .iter()
                .position(|span| span.end - span.start >= width)
            {
                best = Some((shelf_index, span_index));
            }
        }
        if let Some((shelf_index, span_index)) = best {
            return Some(self.take(shelf_index, span_index, width, height));
        }

        // 2. An empty shelf tall enough, cut down to the height needed.
        if let Some(shelf_index) = self
            .shelves
            .iter()
            .position(|shelf| shelf.allocations == 0 && shelf.height >= shelf_height)
        {
            let remainder = self.shelves[shelf_index].height - shelf_height;
            if remainder > 0 {
                let below = Shelf {
                    y: self.shelves[shelf_index].y + shelf_height,
                    height: remainder,
                    free: self.whole_width(),
                    allocations: 0,
                };
                self.shelves[shelf_index].height = shelf_height;
                self.shelves.insert(shelf_index + 1, below);
            }
            return Some(self.take(shelf_index, 0, width, height));
        }

        // 3. A new shelf below the last one.
        if self.height - self.shelved_height >= shelf_height {
            self.shelves.push(Shelf {
                y: self.shelved_height,
                height: shelf_height,
                free: self.whole_width(),
                allocations: 0,
            });
            self.shelved_height += shelf_height;
            let shelf_index = self.shelves.len() - 1;
            return Some(self.take(shelf_index, 0, width, height));
        }
        None
    }

    fn take(
        &mut self,
        shelf_index: usize,
        span_index: usize,
        width: u32,
        height: u32,
    ) -> Allocation {
        let shelf = &mut self.shelves[shelf_index];
        let span = &mut shelf.free[span_index];
        let x = span.start;
        span.start += width;
        if span.start == span.end {
            shelf.free.remove(span_index);
        }
        shelf.allocations += 1;
        Allocation {
            x,
            y: shelf.y,
            width,
            height,
        }
    }

    /// Gives an allocation's space back. The allocation must be one this
    /// allocator handed out and not yet freed.
    pub(crate) fn deallocate(&mut self, allocation: Allocation) {
        let shelf_index = self
            .shelves
            .binary_search_by(|shelf| shelf.y.cmp(&allocation.y))
            .expect("an allocation's y is the top of one of the page's shelves");
        let whole_width = self.whole_width();
        let shelf = &mut self.shelves[shelf_index];
        debug_assert!(
            shelf.allocations > 0,
            "freeing on a shelf with nothing allocated"
        );
        shelf.allocations -= 1;
        if shelf.allocations == 0 {
            shelf.free = whole_width;
            self.merge_empty_shelves_around(shelf_index);
            return;
        }
        let freed = Span {
            start: allocation.x,
            end: allocation.x + allocation.width,
        };
        let insert_at = shelf.free.partition_point(|span| span.start < freed.start);
        shelf.free.insert(insert_at, freed);
        // Coalesce with the following span, then with the preceding one.
        if insert_at + 1 < shelf.free.len()
            && shelf.free[insert_at].end == shelf.free[insert_at + 1].start
        {
            shelf.free[insert_at].end = shelf.free[insert_at + 1].end;
            shelf.free.remove(insert_at + 1);
        }
        if insert_at > 0 && shelf.free[insert_at - 1].end == shelf.free[insert_at].start {
            shelf.free[insert_at - 1].end = shelf.free[insert_at].end;
            shelf.free.remove(insert_at);
        }
    }

    fn merge_empty_shelves_around(&mut self, mut shelf_index: usize) {
        // Absorb the empty shelf below, then fold into the empty shelf above.
        if shelf_index + 1 < self.shelves.len() && self.shelves[shelf_index + 1].allocations == 0 {
            let below = self.shelves.remove(shelf_index + 1);
            self.shelves[shelf_index].height += below.height;
        }
        if shelf_index > 0 && self.shelves[shelf_index - 1].allocations == 0 {
            let this = self.shelves.remove(shelf_index);
            shelf_index -= 1;
            self.shelves[shelf_index].height += this.height;
        }
        // An empty shelf at the bottom gives its height back to the page.
        if shelf_index + 1 == self.shelves.len() {
            let last = self.shelves.pop().expect("the shelf just merged");
            self.shelved_height = last.y;
        }
    }

    /// Whether nothing is allocated on the page.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.shelves.iter().all(|shelf| shelf.allocations == 0)
    }

    #[cfg(test)]
    fn shelf_count(&self) -> usize {
        self.shelves.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(first: &Allocation, second: &Allocation) -> bool {
        first.x < second.x + second.width
            && second.x < first.x + first.width
            && first.y < second.y + second.height
            && second.y < first.y + first.height
    }

    fn assert_disjoint_and_inside(allocations: &[Allocation], width: u32, height: u32) {
        for (index, allocation) in allocations.iter().enumerate() {
            assert!(allocation.x + allocation.width <= width);
            assert!(allocation.y + allocation.height <= height);
            for other in &allocations[index + 1..] {
                assert!(
                    !overlaps(allocation, other),
                    "{allocation:?} overlaps {other:?}"
                );
            }
        }
    }

    #[test]
    fn glyphs_of_near_equal_height_share_a_shelf() {
        let mut allocator = ShelfAllocator::new(64, 64);
        let first = allocator.allocate(10, 9).expect("room");
        let second = allocator.allocate(10, 12).expect("room");
        assert_eq!((first.y, second.y), (0, 0));
        assert_eq!(second.x, 10);
        assert_eq!(allocator.shelf_count(), 1);
        // A much shorter glyph does not waste a slot on the 12-pixel shelf.
        let short = allocator.allocate(4, 4).expect("room");
        assert_eq!(short.y, 12);
    }

    #[test]
    fn a_freed_slot_is_reused_by_the_next_glyph_that_fits() {
        let mut allocator = ShelfAllocator::new(30, 8);
        let first = allocator.allocate(10, 8).expect("room");
        let _second = allocator.allocate(10, 8).expect("room");
        let _third = allocator.allocate(10, 8).expect("room");
        assert_eq!(allocator.allocate(10, 8), None, "the page is full");
        allocator.deallocate(first);
        let reused = allocator.allocate(10, 8).expect("the freed slot");
        assert_eq!((reused.x, reused.y), (0, 0));
    }

    #[test]
    fn freed_neighbours_coalesce_into_one_span() {
        let mut allocator = ShelfAllocator::new(30, 8);
        let first = allocator.allocate(10, 8).expect("room");
        let second = allocator.allocate(10, 8).expect("room");
        let _third = allocator.allocate(10, 8).expect("room");
        allocator.deallocate(second);
        allocator.deallocate(first);
        let wide = allocator.allocate(20, 8).expect("the two slots coalesced");
        assert_eq!(wide.x, 0);
    }

    #[test]
    fn an_emptied_shelf_is_cut_to_a_new_height() {
        let mut allocator = ShelfAllocator::new(16, 32);
        let tall = allocator.allocate(16, 16).expect("room");
        let _below = allocator.allocate(16, 16).expect("room");
        assert_eq!(allocator.allocate(4, 4), None, "the page is full");
        allocator.deallocate(tall);
        // The emptied 16-pixel shelf takes two 8-pixel rows (4 + 4 rounded).
        let first = allocator.allocate(16, 8).expect("the empty shelf, split");
        let second = allocator.allocate(16, 8).expect("its remainder");
        assert_eq!((first.y, second.y), (0, 8));
    }

    #[test]
    fn emptied_shelves_merge_and_the_bottom_one_returns_its_height() {
        let mut allocator = ShelfAllocator::new(16, 32);
        let top = allocator.allocate(16, 8).expect("room");
        let middle = allocator.allocate(16, 8).expect("room");
        let bottom = allocator.allocate(16, 8).expect("room");
        allocator.deallocate(top);
        allocator.deallocate(middle);
        allocator.deallocate(bottom);
        assert!(allocator.is_empty());
        assert_eq!(
            allocator.shelf_count(),
            0,
            "every shelf merged and returned"
        );
        let whole = allocator.allocate(16, 32).expect("the whole page again");
        assert_eq!((whole.x, whole.y), (0, 0));
    }

    #[test]
    fn a_request_larger_than_the_page_or_empty_is_refused() {
        let mut allocator = ShelfAllocator::new(16, 16);
        assert_eq!(allocator.allocate(17, 1), None);
        assert_eq!(allocator.allocate(1, 17), None);
        assert_eq!(allocator.allocate(0, 4), None);
    }

    /// A deterministic churn of allocations and frees: whatever is live is
    /// always disjoint and inside the page, and once everything is freed the
    /// page is whole again.
    #[test]
    fn churn_never_overlaps_and_ends_whole() {
        let (width, height) = (128, 128);
        let mut allocator = ShelfAllocator::new(width, height);
        let mut live: Vec<Allocation> = Vec::new();
        let mut state: u32 = 0x1234_5678;
        let mut next = || {
            // xorshift32: reproducible, no dependency.
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..4000 {
            if next() % 3 == 0 && !live.is_empty() {
                let index = (next() as usize) % live.len();
                allocator.deallocate(live.swap_remove(index));
            } else {
                let glyph_width = 1 + next() % 24;
                let glyph_height = 1 + next() % 24;
                if let Some(allocation) = allocator.allocate(glyph_width, glyph_height) {
                    live.push(allocation);
                }
            }
            assert_disjoint_and_inside(&live, width, height);
        }
        for allocation in live.drain(..) {
            allocator.deallocate(allocation);
        }
        assert!(allocator.is_empty());
        assert_eq!(allocator.shelf_count(), 0);
        assert!(allocator.allocate(width, height).is_some());
    }
}
