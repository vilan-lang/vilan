//! Where a path's mask lives: in the glyph atlas's mask pages (R1), or, over
//! [`LARGEST_ATLAS_MASK`] device pixels a side, in a texture of its own (R8).
//!
//! A mask in the atlas is a glyph with another key: the atlas's shelf
//! packing, its least-recently-used eviction that never takes an entry the
//! current frame uses, its gutter. A mask of its own is made the first frame
//! that draws it and freed after a frame that does not, as the paper says of
//! an `Image`; while frames keep drawing it, it is made once. A mask larger
//! than the device's largest texture is refused before any pixel is made and
//! counted, rather than failing the frame.

use std::collections::HashMap;

use super::{PathMaskKey, Rasterized, rasterize};
use crate::atlas::{AtlasGlyph, AtlasKey, GlyphAtlas, PageStorage};
use crate::text::{GlyphContent, RasterizedGlyph};

/// Masks up to this many device pixels a side go in the atlas; larger ones
/// get a texture of their own (R8). A 1024-pixel page holds sixteen masks of
/// this size, and a 24 dp icon at 100% takes under 600 bytes of one.
pub(crate) const LARGEST_ATLAS_MASK: u32 = 256;

/// A mask in a texture of its own: the texture's id, and the mask's size and
/// offset from the path's origin as a glyph's are (`left` right, `top` UP).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OwnMask {
    pub(crate) id: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) left: i32,
    pub(crate) top: i32,
}

/// Where the encoder finds a path's mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlacedMask {
    Atlas(AtlasGlyph),
    Own(OwnMask),
}

/// The atlas's pages and the textures of masks of their own. The renderer
/// implements it over `wgpu`; the unit tests over a record of the calls.
pub(crate) trait MaskStorage: PageStorage {
    /// Makes texture `id`: one coverage byte per pixel, rows of `width`.
    fn create_mask_texture(&mut self, id: u64, width: u32, height: u32, pixels: &[u8]);
    fn free_mask_texture(&mut self, id: u64);
}

struct OwnEntry {
    /// `None` for a mask that came to nothing or was too large: remembered,
    /// so it is not rasterised again every frame that draws it.
    mask: Option<OwnMask>,
    last_used: u64,
}

/// The renderer's path masks: the atlas holds the small ones; this holds the
/// large ones and the scratch memory `zeno` rasterises with.
pub(crate) struct PathMasks {
    own: HashMap<PathMaskKey, OwnEntry>,
    frame: u64,
    next_id: u64,
    /// Masks refused for being larger than the device's largest texture.
    too_large: u64,
    scratch: zeno::Scratch,
}

impl Default for PathMasks {
    fn default() -> Self {
        PathMasks {
            own: HashMap::new(),
            frame: 0,
            next_id: 1,
            too_large: 0,
            scratch: zeno::Scratch::new(),
        }
    }
}

impl PathMasks {
    /// Starts a frame: masks of their own used from here on are the new
    /// frame's.
    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// Frees the textures of masks the frame just encoded did not draw.
    pub(crate) fn end_frame(&mut self, storage: &mut dyn MaskStorage) {
        let frame = self.frame;
        self.own.retain(|_, entry| {
            let keep = entry.last_used == frame;
            if !keep && let Some(mask) = entry.mask {
                storage.free_mask_texture(mask.id);
            }
            keep
        });
    }

    /// Masks with a texture of their own now.
    pub(crate) fn own_textures(&self) -> usize {
        self.own
            .values()
            .filter(|entry| entry.mask.is_some())
            .count()
    }

    #[cfg(test)]
    pub(crate) fn too_large(&self) -> u64 {
        self.too_large
    }

    /// The mask for `key`, rasterising it on a miss: from the atlas, or from
    /// a texture of its own.
    pub(crate) fn resolve(
        &mut self,
        key: PathMaskKey,
        atlas: &mut GlyphAtlas,
        storage: &mut dyn MaskStorage,
        maximum_side: u32,
    ) -> Option<PlacedMask> {
        if let Some(found) = atlas.lookup(&AtlasKey::Path(key.clone())) {
            return found.map(PlacedMask::Atlas);
        }
        if let Some(entry) = self.own.get_mut(&key) {
            entry.last_used = self.frame;
            return entry.mask.map(PlacedMask::Own);
        }
        match rasterize(&key, &mut self.scratch, maximum_side) {
            Rasterized::Mask(mask)
                if mask.width <= LARGEST_ATLAS_MASK && mask.height <= LARGEST_ATLAS_MASK =>
            {
                let glyph = RasterizedGlyph {
                    left: mask.left,
                    top: -mask.top,
                    width: mask.width,
                    height: mask.height,
                    content: GlyphContent::Mask(mask.pixels),
                };
                atlas
                    .resolve(AtlasKey::Path(key), || Some(glyph), storage)
                    .map(PlacedMask::Atlas)
            }
            Rasterized::Mask(mask) => {
                let id = self.next_id;
                self.next_id += 1;
                storage.create_mask_texture(id, mask.width, mask.height, &mask.pixels);
                let own = OwnMask {
                    id,
                    width: mask.width,
                    height: mask.height,
                    left: mask.left,
                    top: -mask.top,
                };
                self.own.insert(
                    key,
                    OwnEntry {
                        mask: Some(own),
                        last_used: self.frame,
                    },
                );
                Some(PlacedMask::Own(own))
            }
            // Remembered by the atlas as a mask of no pixels.
            Rasterized::Empty => {
                atlas.resolve(AtlasKey::Path(key), || None, storage);
                None
            }
            Rasterized::TooLarge => {
                self.too_large += 1;
                self.own.insert(
                    key,
                    OwnEntry {
                        mask: None,
                        last_used: self.frame,
                    },
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests;
