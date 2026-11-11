//! `vilan-rt-gfx`: the GPU graphics layer of native vilan programs, the Rust
//! half of `std::gfx` (native-ui.md, part P1; its section 9 is RULED).
//!
//! A program builds a [`Scene`] of primitives in display pixels (dp) and draws
//! it into a window ([`WindowRenderer`]) or a texture it can read back
//! ([`OffscreenRenderer`]). The primitives are the set a user interface is
//! made of (native-ui.md 3.1, less Path, which is a later part):
//!
//! | Primitive | Here |
//! |---|---|
//! | Rectangle | [`Rectangle`]: per-corner radius, solid or linear-gradient [`Fill`], per-side [`Border`] |
//! | Shadow | [`Shadow`]: a blurred rounded rectangle, analytic, no offscreen pass |
//! | Text run | [`TextLayout`] from a [`TextSystem`], drawn with [`SceneBuilder::text`] |
//! | Underline | [`Underline`] |
//! | Image | [`Image`] (RGBA pixels), drawn with a rounded clip |
//! | Clip | [`SceneBuilder::push_clip`]: a rectangle with corner radii |
//! | Transform | [`SceneBuilder::push_transform`]: translate and uniform scale |
//! | Layer | [`SceneBuilder::push_layer`]: opacity |
//!
//! Nothing in this crate is reached by a vilan program yet: no `std::gfx`
//! surface exists, and no generated project names the crate. The surface
//! follows (after the executor slice of native-ui.md 11.4), and maps onto
//! this API as described below.
//!
//! # How a frame is drawn
//!
//! Every primitive is ONE instanced quad, its shape computed in the fragment
//! shader from a signed distance (rounded corners, borders, the clip's
//! corners, antialiasing) or sampled from a texture (glyphs, images). A frame
//! is two pipelines and as few draw calls as paint order allows:
//!
//! - **Units** ([`Units`], `units.rs`): dp times the display's scale factor
//!   (and, for everything but line widths, the user's text scale), snapped to
//!   the device pixel grid; a border under 2 dp is one device pixel at 150%
//!   (R16), and never thinner than one.
//! - **Paint order** (R14, `order.rs`): each primitive's order is one more
//!   than the highest order of the earlier primitives it overlaps. Batches
//!   then merge by order and kind, so a later rectangle over earlier text
//!   draws after it, while primitives that overlap nothing of another kind
//!   share one draw call however the scene interleaves them.
//! - **Text** (R3, R15, `text/`): shaped once in dp by `parley` behind a small
//!   internal interface, rasterised by `swash` into a glyph atlas with
//!   single-channel pages for monochrome glyphs and RGBA pages for colour
//!   glyphs, packed with a shelf allocator that frees, evicted least recently
//!   used first, with no CPU copy kept. Quarter-pixel subpixel positioning;
//!   no LCD subpixel text.
//! - **On demand**: a frame is drawn when the caller asks for one. Nothing
//!   draws on a timer.
//!
//! # Retained scenes, and repainting one element
//!
//! The framework above (`std::ui`, native-ui.md 4.4) keeps a retained element
//! tree whose properties are bound to signals; a signal write marks ONE
//! element for repaint. Three properties of this API are what make that cheap:
//!
//! 1. **A [`Scene`] is an immutable, shared value, and scenes nest.** An
//!    element paints into a scene of its own, in its own coordinates; its
//!    parent appends that scene ([`SceneBuilder::scene`]) under the
//!    transform, clip and opacity that place it. Repainting one element is
//!    rebuilding that one element's scene and the (short) chain of parent
//!    scenes that hold it; every sibling's scene is appended again as the
//!    same shared value, copying nothing. [`Scene::same_as`] tells a cache
//!    that a scene is unchanged.
//! 2. **A [`TextLayout`] is shaped once and independent of the display.**
//!    Positions are fractional dp; the renderer places glyphs on the device
//!    grid each frame. A repaint that only changes a colour, a scale-factor
//!    change, or a scroll (a transform) reshapes nothing.
//! 3. **Nothing in a scene points into the atlas.** Glyphs are resolved by key
//!    every frame, so a retained scene stays valid however the atlas evicts.
//!
//! What a frame still costs is linear in the primitives it draws: the
//! encoder walks every retained scene, scales and snaps each primitive and
//! runs the overlap test. Measured at P1 (`examples/measure.rs`, release, one
//! core of a loaded machine): building a 1,000-primitive scene takes about
//! 13 us and encoding, uploading and submitting it about 225 us, under a
//! quarter of a microsecond per primitive (with a software driver's submit
//! included). The next step, when a
//! measurement asks for it, caches each nested scene's ENCODED instances keyed
//! by the scene's identity and the transform and units it was drawn at, and
//! re-encodes only scenes that changed; the API above already carries what
//! that cache needs (identity, immutability, no atlas positions). A layer that
//! scrolls is the natural unit for it, which is why native-ui.md 3.1 calls a
//! layer "the unit of caching for scrolling".
//!
//! # How `std::gfx` maps onto it
//!
//! A vilan `Scene` value is a [`SceneBuilder`] while it is filled and a
//! [`Scene`] once finished; each primitive is a struct the emitted Rust builds
//! and passes by value; `push_*`/`pop` are plain calls (no closures cross the
//! boundary), and misuse is one error at [`SceneBuilder::finish`], not a check
//! per call. A vilan program compiled to Rust calls this crate directly: no
//! interpreter, no foreign-function boundary on the hot path.
//!
//! # Known limits of the first version
//!
//! - A layer's opacity is applied per primitive (as GPUI does): overlapping
//!   primitives inside one translucent layer show through each other. Group
//!   opacity needs an offscreen pass per layer.
//! - Two rounded clips that cross near a corner are approximated
//!   (`encode.rs`, `clip_within`); a clip inside another, the case scrolling
//!   makes, is exact.
//! - Glyph coverage is blended in linear light with no gamma or contrast
//!   adjustment, so dark text on a light background reads slightly lighter
//!   than a platform's own rendering.

mod atlas;
mod color;
mod encode;
mod geometry;
mod gpu;
mod offscreen;
mod order;
mod renderer;
mod scene;
mod text;
mod units;
mod window;

pub use atlas::{AtlasLimits, AtlasStats};
pub use color::{Color, Fill, LinearGradient};
pub use geometry::{Corners, Edges, Point, Rect, Size, Transform};
pub use gpu::{AdapterPreference, AdapterSummary, GpuContext};
pub use offscreen::OffscreenRenderer;
pub use renderer::FrameStats;
pub use scene::{Border, Image, Rectangle, Scene, SceneBuilder, SceneError, Shadow, Underline};
pub use text::{FontFamily, SUBPIXEL_BINS, TextLayout, TextStyle, TextSystem};
pub use units::Units;
pub use window::{FrameOutcome, WindowRenderer};
/// The window layer, re-exported so a caller names the same `winit` this
/// crate was built against.
pub use winit;

/// Why a graphics operation failed.
#[derive(Clone, Debug, PartialEq)]
pub enum GfxError {
    /// No GPU adapter (or no software one, when that was asked for).
    NoAdapter,
    /// The adapter refused to make a device.
    Device(String),
    /// The window's surface could not be made or used.
    Surface(String),
    /// A scale factor or text scale that is not a finite, positive number.
    InvalidUnits { scale_factor: f32, text_scale: f32 },
    /// An offscreen target with a zero side, or larger than the device allows.
    InvalidTargetSize {
        width: u32,
        height: u32,
        maximum_side: u32,
    },
    /// Pixels that are not `width * height * 4` bytes, or a zero side.
    InvalidImage {
        width: u32,
        height: u32,
        bytes: usize,
    },
    /// An image larger than the device's largest texture.
    ImageTooLarge {
        width: u32,
        height: u32,
        maximum_side: u32,
    },
    /// Font data holding no face the text engine can read.
    UnreadableFont,
    /// Reading an offscreen target back failed.
    Readback(String),
}

impl std::fmt::Display for GfxError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GfxError::NoAdapter => write!(formatter, "no GPU adapter is available"),
            GfxError::Device(reason) => write!(formatter, "the GPU refused a device: {reason}"),
            GfxError::Surface(reason) => write!(formatter, "the window's surface failed: {reason}"),
            GfxError::InvalidUnits {
                scale_factor,
                text_scale,
            } => write!(
                formatter,
                "a scale factor and a text scale must be finite and positive; got {scale_factor} and {text_scale}"
            ),
            GfxError::InvalidTargetSize {
                width,
                height,
                maximum_side,
            } => write!(
                formatter,
                "an offscreen target of {width} x {height} pixels; each side must be 1 to {maximum_side}"
            ),
            GfxError::InvalidImage {
                width,
                height,
                bytes,
            } => write!(
                formatter,
                "an image of {width} x {height} pixels needs {} bytes of RGBA; got {bytes}",
                u64::from(*width) * u64::from(*height) * 4
            ),
            GfxError::ImageTooLarge {
                width,
                height,
                maximum_side,
            } => write!(
                formatter,
                "an image of {width} x {height} pixels is larger than this GPU's {maximum_side}-pixel texture limit"
            ),
            GfxError::UnreadableFont => {
                write!(formatter, "the font data holds no face that can be read")
            }
            GfxError::Readback(reason) => {
                write!(formatter, "reading the frame back failed: {reason}")
            }
        }
    }
}

impl std::error::Error for GfxError {}
