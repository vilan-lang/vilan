//! The glyph atlas: rasterised glyphs on GPU texture pages.
//!
//! Two kinds of page (R3's text decisions): MASK pages, one byte per pixel,
//! for the monochrome glyphs that are nearly all text, and COLOUR pages, four
//! bytes per pixel, for colour glyphs (emoji). A mask glyph on an RGBA page
//! would spend four times the memory for nothing.
//!
//! Pages are made on demand up to [`AtlasLimits`], and each is packed by a
//! [`ShelfAllocator`] that can free. A glyph's pixels go straight to its page
//! on the GPU and are then dropped: the atlas keeps NO CPU copy (the spike's
//! did, 16 MB per page, plus 16 MB of texture).
//!
//! # Eviction
//!
//! Every lookup stamps its glyph with the current frame. When a glyph does not
//! fit and no page may be added, glyphs NOT used in the current frame are
//! evicted, least recently used first, until it fits. A glyph used in the
//! current frame is never evicted, so a frame's instance data can never point
//! at a slot that was reused under it. A frame that needs more glyphs than the
//! pages hold draws what fits and counts the rest in
//! [`AtlasStats::dropped_glyphs`], rather than failing the frame.
//!
//! Lookups resolve against the atlas every frame (a scene holds glyph keys,
//! never atlas positions), so a retained scene stays correct across any
//! amount of eviction between the frames that draw it.
//!
//! # Path masks
//!
//! A path's coverage mask (gfx-path.md R1) is a mask glyph with another key,
//! [`AtlasKey::Path`]: the same pages, packing and eviction, so an icon and
//! the text beside it share a page and a draw call. `path/masks.rs` decides
//! which masks come here (up to 256 pixels a side) and which get a texture of
//! their own.

pub(crate) mod packer;

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::path::PathMaskKey;
use crate::text::{GlyphContent, GlyphRasterKey, RasterizedGlyph};
use packer::{Allocation, ShelfAllocator};

/// What an atlas entry is: a glyph at one size and subpixel position, or a
/// path's mask at one paint and device scale.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AtlasKey {
    Glyph(GlyphRasterKey),
    Path(PathMaskKey),
}

impl AtlasKey {
    /// A total order for eviction's tie break, so the order is the same on
    /// every run (the map's iteration order is not): glyphs before paths,
    /// each by its fields.
    fn eviction_order(&self, other: &AtlasKey) -> Ordering {
        match (self, other) {
            (AtlasKey::Glyph(first), AtlasKey::Glyph(second)) => (
                first.system,
                first.font,
                first.glyph,
                first.size_bits,
                first.subpixel_bin,
            )
                .cmp(&(
                    second.system,
                    second.font,
                    second.glyph,
                    second.size_bits,
                    second.subpixel_bin,
                )),
            (AtlasKey::Path(first), AtlasKey::Path(second)) => {
                first.order_fields().cmp(&second.order_fields())
            }
            (AtlasKey::Glyph(_), AtlasKey::Path(_)) => Ordering::Less,
            (AtlasKey::Path(_), AtlasKey::Glyph(_)) => Ordering::Greater,
        }
    }
}

/// How large the atlas may grow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasLimits {
    /// Each page is `page_size` x `page_size` pixels.
    pub page_size: u32,
    pub maximum_mask_pages: u32,
    pub maximum_color_pages: u32,
}

impl Default for AtlasLimits {
    /// 1024-pixel pages: a mask page is 1 MiB and holds several thousand UI
    /// glyphs, a colour page 4 MiB. At most 4 MiB of mask glyphs and 8 MiB of
    /// colour glyphs before eviction starts.
    fn default() -> Self {
        AtlasLimits {
            page_size: 1024,
            maximum_mask_pages: 4,
            maximum_color_pages: 2,
        }
    }
}

/// Which kind of page a glyph lives on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum PageKind {
    Mask,
    Color,
}

impl PageKind {
    pub(crate) fn bytes_per_pixel(self) -> u32 {
        match self {
            PageKind::Mask => 1,
            PageKind::Color => 4,
        }
    }
}

/// Where the atlas's pages live. The renderer implements it over `wgpu`
/// textures; the unit tests implement it over a record of the calls, which is
/// how the bookkeeping below is pinned without a GPU.
pub(crate) trait PageStorage {
    /// Makes page number `page` of `kind` (pages are made in order, from 0).
    fn create_page(&mut self, kind: PageKind, page: u32, size: u32);
    /// Writes `pixels` (rows of `width * bytes_per_pixel` bytes) at `x`, `y`.
    fn upload(
        &mut self,
        kind: PageKind,
        page: u32,
        origin: (u32, u32),
        extent: (u32, u32),
        pixels: &[u8],
    );
}

/// A glyph as the encoder needs it: its page, its texels on the page, and its
/// bitmap's offset from the pen position (`left` right, `top` up).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AtlasGlyph {
    pub(crate) kind: PageKind,
    pub(crate) page: u32,
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) left: i32,
    pub(crate) top: i32,
}

/// Counts that say what the atlas costs and how it is coping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AtlasStats {
    pub mask_pages: u32,
    pub color_pages: u32,
    /// GPU memory of every page, in bytes.
    pub texture_bytes: u64,
    /// Glyphs (and path masks) on the pages now.
    pub resident_glyphs: u32,
    /// Glyphs (and path masks) evicted since the atlas was made.
    pub evictions: u64,
    /// Glyph (and path mask) draws skipped because the pages were full of the
    /// current frame's own entries (or the glyph was larger than a page),
    /// since the atlas was made.
    pub dropped_glyphs: u64,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    /// `None` for a glyph with no pixels (a space) or that would not
    /// rasterise: remembered, so it is not asked for again every frame.
    placed: Option<Placed>,
    last_used: u64,
}

#[derive(Clone, Copy, Debug)]
struct Placed {
    kind: PageKind,
    page: u32,
    allocation: Allocation,
    width: u32,
    height: u32,
    left: i32,
    top: i32,
}

impl Placed {
    fn glyph(&self) -> AtlasGlyph {
        AtlasGlyph {
            kind: self.kind,
            page: self.page,
            x: self.allocation.x,
            y: self.allocation.y,
            width: self.width,
            height: self.height,
            left: self.left,
            top: self.top,
        }
    }
}

pub(crate) struct GlyphAtlas {
    limits: AtlasLimits,
    mask_pages: Vec<ShelfAllocator>,
    color_pages: Vec<ShelfAllocator>,
    entries: HashMap<AtlasKey, Entry>,
    frame: u64,
    evictions: u64,
    dropped_glyphs: u64,
}

/// One pixel of empty gutter right of and below every glyph, so a sampler
/// that strays past a glyph's edge (a future linear filter) reads nothing.
const GUTTER: u32 = 1;

impl GlyphAtlas {
    pub(crate) fn new(limits: AtlasLimits) -> GlyphAtlas {
        GlyphAtlas {
            limits,
            mask_pages: Vec::new(),
            color_pages: Vec::new(),
            entries: HashMap::new(),
            frame: 0,
            evictions: 0,
            dropped_glyphs: 0,
        }
    }

    /// Starts a frame: glyphs looked up from here on are the new frame's, and
    /// the previous frame's become evictable.
    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
    }

    pub(crate) fn page_size(&self) -> u32 {
        self.limits.page_size
    }

    pub(crate) fn stats(&self) -> AtlasStats {
        let page_bytes = u64::from(self.limits.page_size) * u64::from(self.limits.page_size);
        let mask_pages = self.mask_pages.len() as u32;
        let color_pages = self.color_pages.len() as u32;
        AtlasStats {
            mask_pages,
            color_pages,
            texture_bytes: page_bytes * (u64::from(mask_pages) + 4 * u64::from(color_pages)),
            resident_glyphs: self
                .entries
                .values()
                .filter(|entry| entry.placed.is_some())
                .count() as u32,
            evictions: self.evictions,
            dropped_glyphs: self.dropped_glyphs,
        }
    }

    /// The entry for `key` when the atlas has one (stamped with the current
    /// frame): `Some(None)` for one remembered with no pixels, `None` for a
    /// miss.
    pub(crate) fn lookup(&mut self, key: &AtlasKey) -> Option<Option<AtlasGlyph>> {
        let entry = self.entries.get_mut(key)?;
        entry.last_used = self.frame;
        Some(entry.placed.map(|placed| placed.glyph()))
    }

    /// The glyph (or path mask) for `key`, rasterising and uploading it on a
    /// miss.
    pub(crate) fn resolve(
        &mut self,
        key: AtlasKey,
        rasterize: impl FnOnce() -> Option<RasterizedGlyph>,
        storage: &mut dyn PageStorage,
    ) -> Option<AtlasGlyph> {
        if let Some(found) = self.lookup(&key) {
            return found;
        }
        let placed = match rasterize() {
            Some(glyph) if glyph.width > 0 && glyph.height > 0 => {
                let placed = self.place(&glyph, storage);
                if placed.is_none() {
                    // Not remembered: next frame may have room.
                    self.dropped_glyphs += 1;
                    return None;
                }
                placed
            }
            _ => None,
        };
        self.entries.insert(
            key,
            Entry {
                placed,
                last_used: self.frame,
            },
        );
        placed.map(|placed| placed.glyph())
    }

    fn pages(&mut self, kind: PageKind) -> &mut Vec<ShelfAllocator> {
        match kind {
            PageKind::Mask => &mut self.mask_pages,
            PageKind::Color => &mut self.color_pages,
        }
    }

    fn maximum_pages(&self, kind: PageKind) -> u32 {
        match kind {
            PageKind::Mask => self.limits.maximum_mask_pages,
            PageKind::Color => self.limits.maximum_color_pages,
        }
    }

    fn place(&mut self, glyph: &RasterizedGlyph, storage: &mut dyn PageStorage) -> Option<Placed> {
        let (kind, pixels) = match &glyph.content {
            GlyphContent::Mask(pixels) => (PageKind::Mask, pixels),
            GlyphContent::Color(pixels) => (PageKind::Color, pixels),
        };
        let expected =
            glyph.width as usize * glyph.height as usize * kind.bytes_per_pixel() as usize;
        if pixels.len() != expected {
            return None;
        }
        let (slot_width, slot_height) = (glyph.width + GUTTER, glyph.height + GUTTER);
        if slot_width > self.limits.page_size || slot_height > self.limits.page_size {
            return None;
        }
        let (page, allocation) = self.allocate(kind, slot_width, slot_height, storage)?;
        // The glyph and its gutter in one write: the gutter is zeroed, since a
        // reused slot still holds whatever the evicted glyph left there.
        let bytes_per_pixel = kind.bytes_per_pixel() as usize;
        let row_bytes = slot_width as usize * bytes_per_pixel;
        let mut slot = vec![0u8; row_bytes * slot_height as usize];
        let glyph_row_bytes = glyph.width as usize * bytes_per_pixel;
        for row in 0..glyph.height as usize {
            slot[row * row_bytes..row * row_bytes + glyph_row_bytes]
                .copy_from_slice(&pixels[row * glyph_row_bytes..(row + 1) * glyph_row_bytes]);
        }
        storage.upload(
            kind,
            page,
            (allocation.x, allocation.y),
            (slot_width, slot_height),
            &slot,
        );
        Some(Placed {
            kind,
            page,
            allocation,
            width: glyph.width,
            height: glyph.height,
            left: glyph.left,
            top: glyph.top,
        })
    }

    fn allocate(
        &mut self,
        kind: PageKind,
        width: u32,
        height: u32,
        storage: &mut dyn PageStorage,
    ) -> Option<(u32, Allocation)> {
        for (page, allocator) in self.pages(kind).iter_mut().enumerate() {
            if let Some(allocation) = allocator.allocate(width, height) {
                return Some((page as u32, allocation));
            }
        }
        let page_size = self.limits.page_size;
        if (self.pages(kind).len() as u32) < self.maximum_pages(kind) {
            let page = self.pages(kind).len() as u32;
            storage.create_page(kind, page, page_size);
            let mut allocator = ShelfAllocator::new(page_size, page_size);
            let allocation = allocator.allocate(width, height);
            self.pages(kind).push(allocator);
            return allocation.map(|allocation| (page, allocation));
        }
        self.allocate_by_evicting(kind, width, height)
    }

    /// Evicts glyphs of `kind` not used this frame, least recently used first,
    /// until a `width` x `height` slot frees up on the page an eviction just
    /// made room on.
    fn allocate_by_evicting(
        &mut self,
        kind: PageKind,
        width: u32,
        height: u32,
    ) -> Option<(u32, Allocation)> {
        let current_frame = self.frame;
        let mut candidates: Vec<(u64, AtlasKey)> = self
            .entries
            .iter()
            .filter(|(_, entry)| {
                entry.last_used < current_frame
                    && entry.placed.is_some_and(|placed| placed.kind == kind)
            })
            .map(|(key, entry)| (entry.last_used, key.clone()))
            .collect();
        // Oldest first; the key breaks ties so the order is the same on every
        // run (the map's iteration order is not).
        candidates.sort_by(|(first_used, first), (second_used, second)| {
            first_used
                .cmp(second_used)
                .then_with(|| first.eviction_order(second))
        });
        for (_, key) in candidates {
            let entry = self.entries.remove(&key).expect("a candidate is an entry");
            let placed = entry.placed.expect("a candidate has a slot");
            self.evictions += 1;
            let allocator = &mut self.pages(kind)[placed.page as usize];
            allocator.deallocate(placed.allocation);
            if let Some(allocation) = allocator.allocate(width, height) {
                return Some((placed.page, allocation));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An upload's kind, page, origin, extent and byte count.
    type Upload = (PageKind, u32, (u32, u32), (u32, u32), usize);

    #[derive(Default)]
    struct RecordedStorage {
        pages: Vec<(PageKind, u32, u32)>,
        uploads: Vec<Upload>,
    }

    impl PageStorage for RecordedStorage {
        fn create_page(&mut self, kind: PageKind, page: u32, size: u32) {
            self.pages.push((kind, page, size));
        }

        fn upload(
            &mut self,
            kind: PageKind,
            page: u32,
            origin: (u32, u32),
            extent: (u32, u32),
            pixels: &[u8],
        ) {
            assert_eq!(
                pixels.len() as u32,
                extent.0 * extent.1 * kind.bytes_per_pixel(),
                "an upload is exactly its extent"
            );
            self.uploads
                .push((kind, page, origin, extent, pixels.len()));
        }
    }

    fn key(glyph: u32) -> AtlasKey {
        AtlasKey::Glyph(GlyphRasterKey {
            system: 1,
            font: 0,
            glyph,
            size_bits: 14.0f32.to_bits(),
            subpixel_bin: 0,
        })
    }

    fn mask(width: u32, height: u32) -> Option<RasterizedGlyph> {
        Some(RasterizedGlyph {
            left: 1,
            top: height as i32,
            width,
            height,
            content: GlyphContent::Mask(vec![255; (width * height) as usize]),
        })
    }

    fn color(width: u32, height: u32) -> Option<RasterizedGlyph> {
        Some(RasterizedGlyph {
            left: 0,
            top: height as i32,
            width,
            height,
            content: GlyphContent::Color(vec![200; (width * height * 4) as usize]),
        })
    }

    fn small_atlas(maximum_mask_pages: u32) -> GlyphAtlas {
        GlyphAtlas::new(AtlasLimits {
            page_size: 32,
            maximum_mask_pages,
            maximum_color_pages: 1,
        })
    }

    #[test]
    fn a_glyph_is_rasterised_once_and_then_found() {
        let mut atlas = small_atlas(1);
        let mut storage = RecordedStorage::default();
        atlas.begin_frame();
        let first = atlas
            .resolve(key(1), || mask(7, 7), &mut storage)
            .expect("placed");
        let again = atlas
            .resolve(key(1), || panic!("a hit must not rasterise"), &mut storage)
            .expect("found");
        assert_eq!(first, again);
        assert_eq!(storage.uploads.len(), 1);
        // The slot carries its gutter: 8 x 8 for a 7 x 7 glyph.
        assert_eq!(storage.uploads[0].3, (8, 8));
    }

    #[test]
    fn mask_and_colour_glyphs_go_to_pages_of_their_own_kind() {
        let mut atlas = small_atlas(1);
        let mut storage = RecordedStorage::default();
        atlas.begin_frame();
        let mono = atlas
            .resolve(key(1), || mask(7, 7), &mut storage)
            .expect("placed");
        let emoji = atlas
            .resolve(key(2), || color(7, 7), &mut storage)
            .expect("placed");
        assert_eq!((mono.kind, emoji.kind), (PageKind::Mask, PageKind::Color));
        assert_eq!(
            storage.pages,
            vec![(PageKind::Mask, 0, 32), (PageKind::Color, 0, 32)]
        );
        let stats = atlas.stats();
        assert_eq!(stats.texture_bytes, 32 * 32 + 32 * 32 * 4);
    }

    #[test]
    fn a_full_page_opens_another_up_to_the_limit() {
        let mut atlas = small_atlas(2);
        let mut storage = RecordedStorage::default();
        atlas.begin_frame();
        // 15 x 15 glyphs take 16 x 16 slots: four to a 32-pixel page.
        let pages: Vec<u32> = (0..8)
            .map(|glyph| {
                atlas
                    .resolve(key(glyph), || mask(15, 15), &mut storage)
                    .expect("placed")
                    .page
            })
            .collect();
        assert_eq!(pages, vec![0, 0, 0, 0, 1, 1, 1, 1]);
        assert_eq!(atlas.stats().mask_pages, 2);
    }

    #[test]
    fn the_least_recently_used_glyph_is_evicted_first() {
        let mut atlas = small_atlas(1);
        let mut storage = RecordedStorage::default();
        atlas.begin_frame();
        for glyph in 0..4 {
            atlas
                .resolve(key(glyph), || mask(15, 15), &mut storage)
                .expect("placed");
        }
        atlas.begin_frame();
        // Glyphs 1..4 are used again; 0 is now the oldest.
        for glyph in 1..4 {
            atlas
                .resolve(key(glyph), || panic!("resident"), &mut storage)
                .expect("found");
        }
        atlas.begin_frame();
        let newcomer = atlas
            .resolve(key(9), || mask(15, 15), &mut storage)
            .expect("room by eviction");
        assert_eq!((newcomer.x, newcomer.y), (0, 0), "glyph 0's slot");
        assert_eq!(atlas.stats().evictions, 1);
        // Glyph 0 is gone (asks to rasterise again); glyph 1 is still there.
        let mut rasterised_again = false;
        atlas.begin_frame();
        atlas.resolve(
            key(1),
            || {
                rasterised_again = true;
                None
            },
            &mut storage,
        );
        assert!(!rasterised_again, "glyph 1 was not evicted");
    }

    #[test]
    fn a_glyph_of_the_current_frame_is_never_evicted() {
        let mut atlas = small_atlas(1);
        let mut storage = RecordedStorage::default();
        atlas.begin_frame();
        for glyph in 0..4 {
            atlas
                .resolve(key(glyph), || mask(15, 15), &mut storage)
                .expect("placed");
        }
        // Same frame: the page is full of this frame's glyphs.
        assert_eq!(atlas.resolve(key(9), || mask(15, 15), &mut storage), None);
        let stats = atlas.stats();
        assert_eq!((stats.evictions, stats.dropped_glyphs), (0, 1));
        // Next frame, the dropped glyph gets its room.
        atlas.begin_frame();
        assert!(
            atlas
                .resolve(key(9), || mask(15, 15), &mut storage)
                .is_some()
        );
    }

    #[test]
    fn a_glyph_with_no_pixels_is_remembered_and_takes_no_room() {
        let mut atlas = small_atlas(1);
        let mut storage = RecordedStorage::default();
        atlas.begin_frame();
        assert_eq!(atlas.resolve(key(1), || mask(0, 0), &mut storage), None);
        assert_eq!(
            atlas.resolve(key(1), || panic!("remembered"), &mut storage),
            None
        );
        assert!(storage.pages.is_empty());
    }

    #[test]
    fn a_glyph_larger_than_a_page_is_dropped_not_fatal() {
        let mut atlas = small_atlas(1);
        let mut storage = RecordedStorage::default();
        atlas.begin_frame();
        assert_eq!(atlas.resolve(key(1), || mask(40, 8), &mut storage), None);
        assert_eq!(atlas.stats().dropped_glyphs, 1);
    }
}
