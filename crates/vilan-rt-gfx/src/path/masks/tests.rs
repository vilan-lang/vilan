use super::*;
use crate::atlas::{AtlasLimits, PageKind};
use crate::path::{FillRule, Path, PathPaint};

/// What the storage was asked to do.
#[derive(Default)]
struct RecordedStorage {
    pages: Vec<(PageKind, u32)>,
    page_uploads: Vec<((u32, u32), (u32, u32))>,
    created: Vec<(u64, u32, u32)>,
    freed: Vec<u64>,
}

impl PageStorage for RecordedStorage {
    fn create_page(&mut self, kind: PageKind, page: u32, _size: u32) {
        self.pages.push((kind, page));
    }

    fn upload(
        &mut self,
        _kind: PageKind,
        _page: u32,
        origin: (u32, u32),
        extent: (u32, u32),
        pixels: &[u8],
    ) {
        assert_eq!(pixels.len() as u32, extent.0 * extent.1);
        self.page_uploads.push((origin, extent));
    }
}

impl MaskStorage for RecordedStorage {
    fn create_mask_texture(&mut self, id: u64, width: u32, height: u32, pixels: &[u8]) {
        assert_eq!(pixels.len() as u32, width * height);
        self.created.push((id, width, height));
    }

    fn free_mask_texture(&mut self, id: u64) {
        self.freed.push(id);
    }
}

/// A filled `width` x 15 rectangle whose last corner moves by `variant`
/// tenths: distinct paths with masks of one size.
fn block(width: u32, variant: u32) -> Path {
    Path::from_svg(&format!(
        "M0 0 H{width} V15 H{} Z",
        f64::from(variant) / 10.0
    ))
    .expect("valid")
}

fn fill(path: &Path) -> PathMaskKey {
    PathMaskKey::new(path, PathPaint::Fill(FillRule::NonZero), 1.0)
}

/// One 32-pixel mask page: four 15 x 15 masks (16 x 16 slots with their
/// gutter) fill it.
fn small_atlas() -> GlyphAtlas {
    GlyphAtlas::new(AtlasLimits {
        page_size: 32,
        maximum_mask_pages: 1,
        maximum_color_pages: 1,
    })
}

struct Fixture {
    masks: PathMasks,
    atlas: GlyphAtlas,
    storage: RecordedStorage,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture {
            masks: PathMasks::default(),
            atlas: small_atlas(),
            storage: RecordedStorage::default(),
        }
    }

    fn begin(&mut self) {
        self.atlas.begin_frame();
        self.masks.begin_frame();
    }

    fn end(&mut self) {
        self.masks.end_frame(&mut self.storage);
    }

    fn resolve(&mut self, key: PathMaskKey) -> Option<PlacedMask> {
        self.masks
            .resolve(key, &mut self.atlas, &mut self.storage, 4096)
    }
}

/// Two `Path` values parsed from the same data (two allocations) share one
/// mask: one rasterise, one upload, the same slot.
#[test]
fn two_equal_paths_share_one_mask() {
    let mut fixture = Fixture::new();
    fixture.atlas = GlyphAtlas::new(AtlasLimits::default());
    fixture.begin();
    let first = Path::from_svg("M20 6 9 17l-5-5").expect("valid");
    let second = Path::from_svg("M 20 6 L 9 17 l -5 -5").expect("valid");
    let paint = PathPaint::Stroke(crate::path::Stroke::new(2.0));
    let one = fixture.resolve(PathMaskKey::new(&first, paint, 1.0));
    let other = fixture.resolve(PathMaskKey::new(&second, paint, 1.0));
    assert!(one.is_some());
    assert_eq!(one, other);
    assert_eq!(fixture.storage.page_uploads.len(), 1);
    // Another scale is another mask.
    fixture.resolve(PathMaskKey::new(&second, paint, 1.5));
    assert_eq!(fixture.storage.page_uploads.len(), 2);
    assert_eq!(fixture.atlas.stats().resident_glyphs, 2);
}

/// A mask the current frame uses is never evicted: a frame that needs more
/// than the page holds draws what fits and drops the rest, the masks it
/// placed stay where they are, and the next frame evicts the least recently
/// used to make room.
#[test]
fn eviction_never_takes_a_mask_the_current_frame_uses() {
    let mut fixture = Fixture::new();
    fixture.begin();
    let paths: Vec<Path> = (0..5).map(|variant| block(15, variant)).collect();
    let placed: Vec<Option<PlacedMask>> = paths[..4]
        .iter()
        .map(|path| fixture.resolve(fill(path)))
        .collect();
    assert!(placed.iter().all(Option::is_some));
    assert_eq!(fixture.resolve(fill(&paths[4])), None, "the page is full");
    let stats = fixture.atlas.stats();
    assert_eq!((stats.evictions, stats.dropped_glyphs), (0, 1));
    // The frame's masks are all still where they were.
    for (path, before) in paths[..4].iter().zip(&placed) {
        assert_eq!(fixture.resolve(fill(path)), *before);
    }
    assert_eq!(fixture.storage.page_uploads.len(), 4, "nothing re-made");
    fixture.end();

    // Next frame: the first three are drawn again, then the fifth asks for
    // room, and the fourth (the least recently used) gives it.
    fixture.begin();
    for path in &paths[..3] {
        fixture.resolve(fill(path));
    }
    let fifth = fixture.resolve(fill(&paths[4])).expect("room by eviction");
    assert_eq!(fixture.atlas.stats().evictions, 1);
    let PlacedMask::Atlas(fifth) = fifth else {
        panic!("a small mask is in the atlas")
    };
    let Some(PlacedMask::Atlas(fourth)) = placed[3] else {
        panic!("a small mask is in the atlas")
    };
    assert_eq!(
        (fifth.x, fifth.y),
        (fourth.x, fourth.y),
        "the fourth's slot"
    );
    for (path, before) in paths[..3].iter().zip(&placed) {
        assert_eq!(fixture.resolve(fill(path)), *before, "kept");
    }
}

/// Glyphs and path masks share the pages and the eviction: a page full of
/// the current frame's glyphs evicts none of them for a path.
#[test]
fn a_frames_glyphs_are_never_evicted_for_a_path_nor_its_paths_for_a_glyph() {
    use crate::text::{GlyphContent, GlyphRasterKey, RasterizedGlyph};
    let mut fixture = Fixture::new();
    fixture.begin();
    for glyph in 0..3 {
        let key = AtlasKey::Glyph(GlyphRasterKey {
            system: 1,
            font: 0,
            glyph,
            size_bits: 14.0f32.to_bits(),
            subpixel_bin: 0,
        });
        let placed = fixture.atlas.resolve(
            key,
            || {
                Some(RasterizedGlyph {
                    left: 0,
                    top: 15,
                    width: 15,
                    height: 15,
                    content: GlyphContent::Mask(vec![255; 225]),
                })
            },
            &mut fixture.storage,
        );
        assert!(placed.is_some());
    }
    assert!(fixture.resolve(fill(&block(15, 0))).is_some());
    assert_eq!(fixture.resolve(fill(&block(15, 1))), None);
    let glyph = AtlasKey::Glyph(GlyphRasterKey {
        system: 1,
        font: 0,
        glyph: 9,
        size_bits: 14.0f32.to_bits(),
        subpixel_bin: 0,
    });
    assert_eq!(
        fixture.atlas.resolve(
            glyph,
            || Some(RasterizedGlyph {
                left: 0,
                top: 15,
                width: 15,
                height: 15,
                content: GlyphContent::Mask(vec![255; 225]),
            }),
            &mut fixture.storage,
        ),
        None
    );
    assert_eq!(fixture.atlas.stats().evictions, 0);
}

/// R8: a mask over 256 device pixels a side gets a texture of its own and
/// never touches the atlas; it is made once while frames draw it, and freed
/// after a frame that does not. 256 exactly still goes in the atlas.
#[test]
fn a_large_mask_gets_a_texture_of_its_own_off_the_atlas() {
    let mut fixture = Fixture::new();
    fixture.atlas = GlyphAtlas::new(AtlasLimits::default());
    let large = Path::from_svg("M0 0 H300 V10 H0 Z").expect("valid");
    let tall = Path::from_svg("M0 0 H10 V257 H0 Z").expect("valid");
    let edge = Path::from_svg("M0 0 H256 V10 H0 Z").expect("valid");
    fixture.begin();
    let placed = fixture.resolve(fill(&large));
    assert_eq!(
        placed,
        Some(PlacedMask::Own(OwnMask {
            id: 1,
            width: 300,
            height: 10,
            left: 0,
            top: 0,
        }))
    );
    assert!(matches!(
        fixture.resolve(fill(&tall)),
        Some(PlacedMask::Own(OwnMask { height: 257, .. }))
    ));
    assert!(matches!(
        fixture.resolve(fill(&edge)),
        Some(PlacedMask::Atlas(_))
    ));
    assert_eq!(fixture.storage.created, vec![(1, 300, 10), (2, 10, 257)]);
    assert_eq!(fixture.storage.page_uploads.len(), 1, "only the 256 mask");
    assert_eq!(fixture.masks.own_textures(), 2);
    fixture.end();
    assert!(fixture.storage.freed.is_empty(), "drawn this frame");

    // Drawn again: not made again. The tall one is not drawn this frame.
    fixture.begin();
    assert_eq!(fixture.resolve(fill(&large)), placed);
    fixture.end();
    assert_eq!(fixture.storage.created.len(), 2);
    assert_eq!(fixture.storage.freed, vec![2]);
    assert_eq!(fixture.masks.own_textures(), 1);

    // A frame without it frees it; drawn after that, it is made anew.
    fixture.begin();
    fixture.end();
    assert_eq!(fixture.storage.freed, vec![2, 1]);
    fixture.begin();
    assert!(matches!(
        fixture.resolve(fill(&large)),
        Some(PlacedMask::Own(OwnMask { id: 3, .. }))
    ));
}

/// A mask larger than the device's largest texture draws nothing, is
/// counted, and is not rasterised again while frames keep drawing it.
#[test]
fn a_mask_larger_than_the_device_allows_is_counted_and_remembered() {
    let mut fixture = Fixture::new();
    let huge = Path::from_svg("M0 0 H5000 V10 H0 Z").expect("valid");
    for _ in 0..3 {
        fixture.begin();
        assert_eq!(fixture.resolve(fill(&huge)), None);
        fixture.end();
    }
    assert_eq!(fixture.masks.too_large(), 1);
    assert!(fixture.storage.created.is_empty());
    assert!(fixture.storage.page_uploads.is_empty());
}

/// A mask of no pixels (a zero-width stroke) is remembered by the atlas
/// and takes no room.
#[test]
fn a_mask_of_no_pixels_takes_no_room() {
    let mut fixture = Fixture::new();
    fixture.begin();
    let path = block(15, 0);
    let key = PathMaskKey::new(&path, PathPaint::Stroke(crate::path::Stroke::new(0.0)), 1.0);
    assert_eq!(fixture.resolve(key.clone()), None);
    assert_eq!(fixture.resolve(key), None);
    assert!(fixture.storage.pages.is_empty());
}
