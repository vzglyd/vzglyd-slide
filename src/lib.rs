#![deny(missing_docs)]
//! # `vzglyd_slide`
//!
//! ABI contract and shared data types for [VZGLYD](https://github.com/vzglyd/vzglyd) slides.
//!
//! A VZGLYD slide is a `wasm32-wasip1` module that exports `vzglyd_abi_version()` and
//! `vzglyd_update(dt: f32) -> i32`. The engine loads the slide, deserializes its
//! [`SlideSpec`], validates it against device limits, and renders it with the engine's
//! fixed pipeline contract.
//!
//! ## Quick Start
//!
//! Add the crate to your slide:
//!
//! ```toml
//! [dependencies]
//! vzglyd_slide = "0.1"
//! ```
//!
//! Export the required ABI surface:
//!
//! ```no_run
//! use vzglyd_slide::ABI_VERSION;
//!
//! #[unsafe(no_mangle)]
//! pub extern "C" fn vzglyd_abi_version() -> u32 {
//!     ABI_VERSION
//! }
//!
//! #[unsafe(no_mangle)]
//! pub extern "C" fn vzglyd_update(_dt: f32) -> i32 {
//!     0
//! }
//! ```
//!
//! `dt` is the elapsed time since the previous frame, expressed in seconds. Returning `0`
//! tells the engine that geometry is unchanged and can be reused. Returning `1` tells the
//! engine to fetch updated geometry and upload fresh buffers for the next frame.
//!
//! ## Audio
//!
//! Slides can play embedded sound assets (MP3, WAV, Ogg, FLAC). Declare sounds in
//! your `manifest.json` under `assets.sounds`, then use the audio functions to control
//! playback:
//!
//! ```ignore
//! use vzglyd_slide::{play_sound, stop_sound, set_volume, pause_sound, resume_sound};
//!
//! // Play a sound effect (ID 1, key "click", full volume, no loop)
//! play_sound(1, "click", 1.0, false);
//!
//! // Loop background music (ID 2, key "bgm", 30% volume)
//! play_sound(2, "bgm", 0.3, true);
//!
//! // Control playback
//! set_volume(2, 0.5);  // change volume
//! pause_sound(2);      // pause
//! resume_sound(2);     // resume
//! stop_sound(2);       // stop
//! ```
//!
//! See [`AUDIO_GUIDE.md`](../AUDIO_GUIDE.md) for a complete walkthrough.
//!
//! ## Why ABI Version 2?
//!
//! The ABI was bumped from `1` to `2` because audio support introduces **breaking changes**
//! that are not backward-compatible:
//!
//! 1. **`SlideSpec` serialization changed** — A new `sounds: Vec<SoundDesc>` field was added
//!    between `textures` and `static_meshes`. An engine expecting ABI 1 would deserialize
//!    the wrong fields (reading sound data as texture pointers), causing memory corruption or
//!    crashes.
//!
//! 2. **New host FFI imports** — The `vzglyd_host` module now imports `audio_play`,
//!    `audio_stop`, `audio_set_volume`, `audio_pause`, and `audio_resume`. An old host
//!    that doesn't export these functions would trap the WASM module on load.
//!
//! 3. **Manifest structure changed** — `manifest.json` now accepts a `sounds` array under
//!    `assets`. Old engines would ignore it, but the engine must understand it to bundle
//!    the sound data into the `.vzglyd` archive.
//!
//! Because of (1) and (2), the change is **not backward-compatible**. The engine rejects
//! slides whose declared ABI version it doesn't recognise, so old slides (ABI 1) still
//! work and new slides (ABI 2) are only loaded by engines that support audio.
//!
//! See the crate README for a packaging overview, and [`ABI_POLICY.md`](../ABI_POLICY.md) for
//! the versioning and compatibility contract.

use std::fmt;
use std::ops::Range;

use bytemuck::Pod;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

mod audio;
mod trace;

pub use trace::{
    TraceScope, trace_event, trace_event_with_attrs, trace_scope, trace_scope_with_attrs,
};
#[doc(hidden)]
pub use trace::{traced_configure_entrypoint, traced_init_entrypoint, traced_update_entrypoint};

pub use audio::{pause_sound, play_sound, resume_sound, set_volume, stop_sound};

/// Current slide ABI version understood by this crate and the engine.
pub const ABI_VERSION: u32 = 3;

/// Resource and geometry limits a slide is allowed to consume.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Limits {
    /// Maximum total vertex count across static and dynamic meshes.
    pub max_vertices: u32,
    /// Maximum total index count across static and dynamic meshes.
    pub max_indices: u32,
    /// Maximum number of static meshes.
    pub max_static_meshes: u32,
    /// Maximum number of dynamic meshes.
    pub max_dynamic_meshes: u32,
    /// Maximum number of texture slots a slide may occupy.
    pub max_textures: u32,
    /// Maximum aggregate number of texture bytes stored in the package.
    pub max_texture_bytes: u32,
    /// Maximum width or height of any single texture.
    pub max_texture_dim: u32,
}

impl Limits {
    /// Conservative limits chosen to stay within Raspberry Pi 4 budgets.
    pub const fn pi4() -> Self {
        Self {
            max_vertices: 600_000,
            max_indices: 1_500_000,
            max_static_meshes: 10,
            max_dynamic_meshes: 10,
            max_textures: 4,
            max_texture_bytes: 640 * 640 * 4 * 4, // up to four 640² RGBA8 textures
            max_texture_dim: 640,
        }
    }
}

/// Static mesh payload fully provided by the slide.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(
    serialize = "V: Serialize",
    deserialize = "V: Serialize + DeserializeOwned"
))]
pub struct StaticMesh<V: Pod> {
    /// Human-readable mesh label used in diagnostics.
    pub label: String,
    /// Vertex payload uploaded once when the slide is loaded.
    pub vertices: Vec<V>,
    /// Triangle index data for the mesh.
    pub indices: Vec<u32>,
}

/// Dynamic mesh where vertices are rewritten every frame but index order is fixed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DynamicMesh {
    /// Human-readable mesh label used in diagnostics.
    pub label: String,
    /// Maximum number of vertices the slide may upload for this mesh.
    pub max_vertices: u32,
    /// Static index order used for every frame update.
    pub indices: Vec<u32>,
}

/// Fixed render pipeline selection for a draw call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineKind {
    /// Opaque geometry written without alpha blending.
    Opaque,
    /// Transparent geometry rendered with blending enabled.
    Transparent,
}

/// Mesh source referenced by a draw call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawSource {
    /// Read indices from a static mesh.
    Static(usize),
    /// Read indices from a dynamic mesh.
    Dynamic(usize),
}

/// Draw call descriptor for one mesh slice.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawSpec {
    /// Human-readable draw label used in diagnostics.
    pub label: String,
    /// Mesh buffer to draw from.
    pub source: DrawSource,
    /// Pipeline variant used to render the mesh slice.
    pub pipeline: PipelineKind,
    /// Range of indices consumed from the referenced mesh.
    pub index_range: Range<u32>,
}

/// Texture payload embedded in the slide package.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextureDesc {
    /// Human-readable texture label used in diagnostics.
    pub label: String,
    /// Texture width in pixels.
    pub width: u32,
    /// Texture height in pixels.
    pub height: u32,
    /// Pixel format understood by the engine.
    pub format: TextureFormat,
    /// Address mode for the U axis.
    pub wrap_u: WrapMode,
    /// Address mode for the V axis.
    pub wrap_v: WrapMode,
    /// Address mode for the W axis.
    pub wrap_w: WrapMode,
    /// Magnification filter.
    pub mag_filter: FilterMode,
    /// Minification filter.
    pub min_filter: FilterMode,
    /// Mipmap filter.
    pub mip_filter: FilterMode,
    /// Raw texture bytes in the declared format.
    pub data: Vec<u8>,
}

/// Texture formats accepted by the engine.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TextureFormat {
    /// 8-bit normalized RGBA texture data.
    Rgba8Unorm,
}

/// Sampler address mode.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum WrapMode {
    /// Repeat texture coordinates outside the `[0, 1]` range.
    Repeat,
    /// Clamp texture coordinates to the texture edge.
    ClampToEdge,
}

/// Sampler filtering mode.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FilterMode {
    /// Nearest-neighbor sampling.
    Nearest,
    /// Linear interpolation between neighboring texels.
    Linear,
}

/// Coordinate system and renderer contract for the slide.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SceneSpace {
    /// Screen-aligned 2D content rendered in slide pixel space.
    Screen2D,
    /// World-space 3D content rendered with camera and lighting support.
    World3D,
}

/// Optional custom shader source overrides supplied by the slide package.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShaderSources {
    /// Vertex shader WGSL source, if the slide overrides the default.
    pub vertex_wgsl: Option<String>,
    /// Fragment shader WGSL source, if the slide overrides the default.
    pub fragment_wgsl: Option<String>,
}

/// Camera pose at a specific point along an animated path.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CameraKeyframe {
    /// Time, in seconds, measured from the start of the path.
    pub time: f32,
    /// Camera position in world space.
    pub position: [f32; 3],
    /// Camera target point in world space.
    pub target: [f32; 3],
    /// Up vector used to construct the camera basis.
    pub up: [f32; 3],
    /// Vertical field of view in degrees.
    pub fov_y_deg: f32,
}

/// Ordered keyframes that define camera motion for a world-space slide.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CameraPath {
    /// Whether the path should wrap back to the start after the final keyframe.
    pub looped: bool,
    /// Keyframes in strictly increasing time order.
    pub keyframes: Vec<CameraKeyframe>,
}

/// Directional light configuration for world-space slides.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct DirectionalLight {
    /// Direction from the shaded point toward the light source.
    pub direction: [f32; 3],
    /// RGB light color.
    pub color: [f32; 3],
    /// Scalar light intensity multiplier.
    pub intensity: f32,
}

impl DirectionalLight {
    /// Construct a directional light definition.
    pub const fn new(direction: [f32; 3], color: [f32; 3], intensity: f32) -> Self {
        Self {
            direction,
            color,
            intensity,
        }
    }
}

/// Lighting parameters for world-space slides.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WorldLighting {
    /// RGB ambient light color.
    pub ambient_color: [f32; 3],
    /// Scalar ambient intensity multiplier.
    pub ambient_intensity: f32,
    /// Primary directional light, if the slide wants one.
    pub directional_light: Option<DirectionalLight>,
}

impl WorldLighting {
    /// Construct a full lighting description.
    pub const fn new(
        ambient_color: [f32; 3],
        ambient_intensity: f32,
        directional_light: Option<DirectionalLight>,
    ) -> Self {
        Self {
            ambient_color,
            ambient_intensity,
            directional_light,
        }
    }
}

impl Default for WorldLighting {
    fn default() -> Self {
        Self {
            ambient_color: [1.0, 1.0, 1.0],
            ambient_intensity: 0.22,
            directional_light: Some(DirectionalLight::new(
                [0.55, 1.0, 0.38],
                [1.0, 1.0, 1.0],
                1.0,
            )),
        }
    }
}

fn default_slide_lighting() -> Option<WorldLighting> {
    Some(WorldLighting::default())
}

/// Overlay geometry uploaded separately from the main mesh set.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeOverlay<V: Pod> {
    /// Overlay vertices rendered over the main scene.
    pub vertices: Vec<V>,
    /// Triangle indices for the overlay.
    pub indices: Vec<u32>,
}

/// Runtime-updated mesh payload for a specific dynamic mesh slot.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeMesh<V: Pod> {
    /// Index of the target mesh in [`SlideSpec::dynamic_meshes`].
    pub mesh_index: u32,
    /// Replacement vertex payload for the current frame.
    pub vertices: Vec<V>,
    /// Number of indices from the static index buffer to draw.
    pub index_count: u32,
}

/// Batch of dynamic mesh updates produced at runtime.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeMeshSet<V: Pod> {
    /// Per-mesh updates keyed by mesh slot.
    pub meshes: Vec<RuntimeMesh<V>>,
}

/// Vertex data extracted from an imported mesh asset.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MeshAssetVertex {
    /// Vertex position in model space.
    pub position: [f32; 3],
    /// Vertex normal in model space.
    pub normal: [f32; 3],
    /// Primary texture coordinates.
    pub tex_coords: [f32; 2],
    /// Vertex color.
    pub color: [f32; 4],
}

/// Standalone mesh asset containing geometry buffers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshAsset {
    /// Mesh vertices.
    pub vertices: Vec<MeshAssetVertex>,
    /// Mesh triangle indices.
    pub indices: Vec<u32>,
}

/// Runtime font atlas used by text-capable slides.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FontAtlas {
    /// Atlas width in pixels.
    pub width: u32,
    /// Atlas height in pixels.
    pub height: u32,
    /// RGBA8 pixel data for the atlas.
    pub pixels: Vec<u8>, // RGBA8
    /// Glyph metadata packed into the atlas.
    pub glyphs: Vec<GlyphInfo>,
}

/// UV mapping data for one glyph in a [`FontAtlas`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GlyphInfo {
    /// Unicode code point for the glyph.
    pub codepoint: u32,
    /// Minimum U coordinate.
    pub u0: f32,
    /// Minimum V coordinate.
    pub v0: f32,
    /// Maximum U coordinate.
    pub u1: f32,
    /// Maximum V coordinate.
    pub v1: f32,
}

/// Audio format supported for embedded sound assets.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SoundFormat {
    /// MP3 audio format.
    Mp3,
    /// WAV (PCM) audio format.
    Wav,
    /// Ogg Vorbis audio format.
    Ogg,
    /// FLAC lossless audio format.
    Flac,
}

/// Description of an embedded sound asset in a slide bundle.
///
/// Sound data is embedded directly into the `.vzglyd` bundle alongside
/// textures and meshes, following the same pattern as [`TextureDesc`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SoundDesc {
    /// Unique asset key used to reference this sound (e.g., `"notify.mp3"`).
    pub key: String,
    /// Audio format of the embedded data.
    pub format: SoundFormat,
    /// Raw audio bytes (MP3, WAV, Ogg, or FLAC).
    pub data: Vec<u8>,
}

/// Which transform property an animation channel targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationPath {
    /// Node translation (XYZ).
    Translation,
    /// Node rotation as a quaternion (XYZW).
    Rotation,
    /// Node scale (XYZ).
    Scale,
}

/// A single keyframe channel within an animation clip.
///
/// Each channel animates one transform property on one scene node,
/// identified by `node_label` (matching a static mesh label or anchor ID).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnimationChannel {
    /// Label of the scene node this channel animates.
    pub node_label: String,
    /// Which transform property is animated.
    pub path: AnimationPath,
    /// Keyframe timestamps in seconds.
    pub keyframe_times: Vec<f32>,
    /// Keyframe values. For translation/scale: `[x, y, z, 0]`. For rotation: quaternion `[x, y, z, w]`.
    pub keyframe_values: Vec<[f32; 4]>,
}

/// An animation clip embedded in a slide bundle.
///
/// A clip groups multiple channels (one per animated node/property) that
/// share a common timeline. Most GLB files export a single default clip.
/// At render time, the host samples the clip to produce per-draw model
/// matrices that transform static meshes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnimationClip {
    /// Animation name (e.g., `"Action"` from Blender).
    pub name: String,
    /// Total duration of the clip in seconds.
    pub duration: f32,
    /// Whether the clip loops continuously.
    pub looped: bool,
    /// Per-node animation channels.
    pub channels: Vec<AnimationChannel>,
}

/// Named anchor extracted from an imported scene asset.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SceneAnchor {
    /// Stable machine-readable identifier for the anchor.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Source node name, if known.
    pub node_name: Option<String>,
    /// Optional author-defined tag.
    pub tag: Option<String>,
    /// World transform matrix for the anchor.
    pub world_transform: [[f32; 4]; 4],
}

impl SceneAnchor {
    /// Extract the translation component from [`SceneAnchor::world_transform`].
    pub fn translation(&self) -> [f32; 3] {
        [
            self.world_transform[3][0],
            self.world_transform[3][1],
            self.world_transform[3][2],
        ]
    }
}

/// Set of anchors extracted from a named scene asset.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SceneAnchorSet {
    /// Stable identifier for the scene asset.
    pub scene_id: String,
    /// Human-readable scene label, if present.
    pub scene_label: Option<String>,
    /// Source scene name, if present.
    pub scene_name: Option<String>,
    /// Anchors discovered in the scene.
    pub anchors: Vec<SceneAnchor>,
}

impl SceneAnchorSet {
    /// Look up an anchor by machine-readable identifier.
    pub fn anchor(&self, key: &str) -> Option<&SceneAnchor> {
        self.anchors.iter().find(|anchor| anchor.id == key)
    }

    /// Require an anchor to exist, returning a descriptive lookup error otherwise.
    pub fn require_anchor(&self, key: &str) -> Result<&SceneAnchor, SceneAnchorLookupError> {
        self.anchor(key)
            .ok_or_else(|| SceneAnchorLookupError::NotFound {
                scene_id: self.scene_id.clone(),
                key: key.to_string(),
                available: self
                    .anchors
                    .iter()
                    .map(|anchor| anchor.id.clone())
                    .collect(),
            })
    }
}

/// Error returned when looking up a missing scene anchor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneAnchorLookupError {
    /// The requested anchor key was not present in the scene.
    NotFound {
        /// Identifier of the scene that was searched.
        scene_id: String,
        /// Requested anchor key.
        key: String,
        /// Available anchor identifiers present in the scene.
        available: Vec<String>,
    },
}

impl fmt::Display for SceneAnchorLookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SceneAnchorLookupError::NotFound {
                scene_id,
                key,
                available,
            } => {
                let available = if available.is_empty() {
                    "none".to_string()
                } else {
                    available.join(", ")
                };
                write!(
                    f,
                    "scene '{scene_id}' does not define anchor '{key}' (available: {available})"
                )
            }
        }
    }
}

impl std::error::Error for SceneAnchorLookupError {}

/// Complete scene description returned by a slide.
///
/// The engine loads a `SlideSpec` when the slide starts, validates it against
/// [`Limits`], and uses it to allocate textures, mesh buffers, and render state.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(
    serialize = "V: Serialize",
    deserialize = "V: Serialize + DeserializeOwned"
))]
pub struct SlideSpec<V: Pod> {
    /// Human-readable slide name used for logging and diagnostics.
    pub name: String,
    /// Resource limits the slide promises to stay within.
    pub limits: Limits,
    /// Rendering space used by the slide.
    pub scene_space: SceneSpace,
    /// Camera animation used by world-space slides.
    pub camera_path: Option<CameraPath>,
    /// Optional custom WGSL overrides for the engine shader contract.
    pub shaders: Option<ShaderSources>,
    /// Optional runtime overlay drawn on top of the main scene.
    pub overlay: Option<RuntimeOverlay<V>>,
    /// Optional font atlas used by text helpers.
    pub font: Option<FontAtlas>,
    /// Number of texture slots this slide expects to occupy.
    pub textures_used: u32,
    /// Texture payloads embedded in the package.
    pub textures: Vec<TextureDesc>,
    /// Sound payloads embedded in the package.
    pub sounds: Vec<SoundDesc>,
    /// Animation clips embedded in the package.
    ///
    /// Each clip contains channels that animate node transforms
    /// (translation, rotation, scale) over time. Clips are sampled
    /// at render time to produce per-draw model matrices.
    pub animations: Vec<AnimationClip>,
    /// Static meshes uploaded once when the slide loads.
    pub static_meshes: Vec<StaticMesh<V>>,
    /// Dynamic meshes whose vertices may change at runtime.
    pub dynamic_meshes: Vec<DynamicMesh>,
    /// Draw plan executed by the engine every frame.
    pub draws: Vec<DrawSpec>,
    /// Optional lighting override for world-space slides.
    #[serde(default = "default_slide_lighting")]
    pub lighting: Option<WorldLighting>,
}

/// Validation error produced when a [`SlideSpec`] breaks the engine contract.
#[derive(Debug)]
pub enum SpecError {
    /// Too many static meshes were declared.
    StaticMeshesExceeded {
        /// Number of meshes declared by the slide.
        count: usize,
        /// Maximum static meshes allowed by [`Limits`].
        max: u32,
    },
    /// Too many dynamic meshes were declared.
    DynamicMeshesExceeded {
        /// Number of meshes declared by the slide.
        count: usize,
        /// Maximum dynamic meshes allowed by [`Limits`].
        max: u32,
    },
    /// The slide exceeded the total vertex budget.
    VertexBudget {
        /// Total vertices requested by the slide.
        total: u32,
        /// Maximum vertices allowed by [`Limits`].
        max: u32,
    },
    /// The slide exceeded the total index budget.
    IndexBudget {
        /// Total indices requested by the slide.
        total: u32,
        /// Maximum indices allowed by [`Limits`].
        max: u32,
    },
    /// The slide exceeded the declared texture slot budget.
    TextureBudget {
        /// Number of texture slots used by the slide.
        used: u32,
        /// Maximum texture slots allowed by [`Limits`].
        max: u32,
    },
    /// The slide exceeded the aggregate texture byte budget.
    TextureBytes {
        /// Total bytes consumed by all textures.
        total: u32,
        /// Maximum bytes allowed by [`Limits`].
        max: u32,
    },
    /// A texture exceeded the maximum allowed dimension.
    TextureDimension {
        /// Largest requested width or height.
        dim: u32,
        /// Maximum width or height allowed by [`Limits`].
        max: u32,
    },
    /// `textures_used` disagrees with the actual embedded textures.
    TextureCountMismatch {
        /// Declared texture count.
        declared: u32,
        /// Actual texture count.
        actual: u32,
    },
    /// A draw call referenced a mesh slot that does not exist.
    DrawMissingMesh {
        /// Label of the offending draw.
        label: String,
    },
    /// A draw call referenced more indices than the mesh contains.
    DrawRange {
        /// Label of the offending draw.
        label: String,
        /// Available indices in the referenced mesh.
        available: u32,
        /// Requested end index.
        requested: u32,
    },
    /// A range or dimension was structurally invalid.
    InvalidRange {
        /// Label of the offending asset or draw.
        label: String,
    },
    /// A camera path was present but contained no keyframes.
    CameraPathEmpty,
    /// Camera keyframes were not strictly increasing in time.
    CameraKeyframeOrder,
    /// A camera keyframe contained a negative timestamp.
    CameraKeyframeTimeNegative,
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpecError::StaticMeshesExceeded { count, max } => {
                write!(f, "{count} static meshes exceeds limit {max}")
            }
            SpecError::DynamicMeshesExceeded { count, max } => {
                write!(f, "{count} dynamic meshes exceeds limit {max}")
            }
            SpecError::VertexBudget { total, max } => {
                write!(f, "vertex budget exceeded: {total} > {max}")
            }
            SpecError::IndexBudget { total, max } => {
                write!(f, "index budget exceeded: {total} > {max}")
            }
            SpecError::TextureBudget { used, max } => {
                write!(f, "texture budget exceeded: {used} > {max}")
            }
            SpecError::TextureBytes { total, max } => {
                write!(f, "texture byte budget exceeded: {total} > {max}")
            }
            SpecError::TextureDimension { dim, max } => {
                write!(f, "texture dimension {dim} exceeds {max}")
            }
            SpecError::TextureCountMismatch { declared, actual } => {
                write!(f, "textures_used={declared} does not match actual {actual}")
            }
            SpecError::DrawMissingMesh { label } => {
                write!(f, "draw '{label}' references a missing mesh")
            }
            SpecError::DrawRange {
                label,
                available,
                requested,
            } => write!(
                f,
                "draw '{label}' requests {requested} indices but only {available} exist"
            ),
            SpecError::InvalidRange { label } => {
                write!(f, "draw '{label}' has an invalid index range")
            }
            SpecError::CameraPathEmpty => write!(f, "camera path has no keyframes"),
            SpecError::CameraKeyframeOrder => write!(
                f,
                "camera keyframes must be in strictly increasing time order"
            ),
            SpecError::CameraKeyframeTimeNegative => {
                write!(f, "camera keyframes must have non-negative time")
            }
        }
    }
}

impl<V: Pod> SlideSpec<V> {
    /// Total vertex budget consumed by this slide, including dynamic mesh capacity.
    pub fn total_vertex_budget(&self) -> u32 {
        let static_vertices: u32 = self
            .static_meshes
            .iter()
            .map(|mesh| mesh.vertices.len() as u32)
            .sum();
        let dynamic_vertices: u32 = self
            .dynamic_meshes
            .iter()
            .map(|mesh| mesh.max_vertices)
            .sum();
        static_vertices.saturating_add(dynamic_vertices)
    }

    /// Total index budget consumed by this slide.
    pub fn total_index_budget(&self) -> u32 {
        let static_indices: u32 = self
            .static_meshes
            .iter()
            .map(|mesh| mesh.indices.len() as u32)
            .sum();
        let dynamic_indices: u32 = self
            .dynamic_meshes
            .iter()
            .map(|mesh| mesh.indices.len() as u32)
            .sum();
        static_indices.saturating_add(dynamic_indices)
    }

    /// Validate that the slide stays within declared limits and references are sound.
    ///
    /// Validation checks mesh counts, vertex and index budgets, texture budgets,
    /// draw references, texture dimensions, and camera path ordering.
    pub fn validate(&self) -> Result<(), SpecError> {
        let _ = self.name; // keep name observable even if unused by callers

        if self.static_meshes.len() as u32 > self.limits.max_static_meshes {
            return Err(SpecError::StaticMeshesExceeded {
                count: self.static_meshes.len(),
                max: self.limits.max_static_meshes,
            });
        }
        if self.dynamic_meshes.len() as u32 > self.limits.max_dynamic_meshes {
            return Err(SpecError::DynamicMeshesExceeded {
                count: self.dynamic_meshes.len(),
                max: self.limits.max_dynamic_meshes,
            });
        }

        let total_vertices = self.total_vertex_budget();
        if total_vertices > self.limits.max_vertices {
            return Err(SpecError::VertexBudget {
                total: total_vertices,
                max: self.limits.max_vertices,
            });
        }

        let total_indices = self.total_index_budget();
        if total_indices > self.limits.max_indices {
            return Err(SpecError::IndexBudget {
                total: total_indices,
                max: self.limits.max_indices,
            });
        }

        if self.textures_used > self.limits.max_textures {
            return Err(SpecError::TextureBudget {
                used: self.textures_used,
                max: self.limits.max_textures,
            });
        }
        if self.textures.len() as u32 != self.textures_used {
            return Err(SpecError::TextureCountMismatch {
                declared: self.textures_used,
                actual: self.textures.len() as u32,
            });
        }
        if self.textures.len() as u32 > self.limits.max_textures {
            return Err(SpecError::TextureBudget {
                used: self.textures.len() as u32,
                max: self.limits.max_textures,
            });
        }
        let mut tex_bytes = 0u32;
        for tex in &self.textures {
            if tex.width == 0 || tex.height == 0 {
                return Err(SpecError::InvalidRange {
                    label: tex.label.clone(),
                });
            }
            if tex.width > self.limits.max_texture_dim || tex.height > self.limits.max_texture_dim {
                return Err(SpecError::TextureDimension {
                    dim: tex.width.max(tex.height),
                    max: self.limits.max_texture_dim,
                });
            }
            tex_bytes = tex_bytes.saturating_add(tex.data.len() as u32);
        }
        if tex_bytes > self.limits.max_texture_bytes {
            return Err(SpecError::TextureBytes {
                total: tex_bytes,
                max: self.limits.max_texture_bytes,
            });
        }

        if let Some(cam) = &self.camera_path {
            if cam.keyframes.is_empty() {
                return Err(SpecError::CameraPathEmpty);
            }
            let mut last = -1.0_f32;
            for k in &cam.keyframes {
                if k.time < 0.0 {
                    return Err(SpecError::CameraKeyframeTimeNegative);
                }
                if k.time <= last {
                    return Err(SpecError::CameraKeyframeOrder);
                }
                last = k.time;
            }
        }

        for draw in &self.draws {
            if draw.index_range.start > draw.index_range.end {
                return Err(SpecError::InvalidRange {
                    label: draw.label.clone(),
                });
            }
            match draw.source {
                DrawSource::Static(idx) => {
                    let Some(mesh) = self.static_meshes.get(idx) else {
                        return Err(SpecError::DrawMissingMesh {
                            label: draw.label.clone(),
                        });
                    };
                    let available = mesh.indices.len() as u32;
                    if draw.index_range.end > available {
                        return Err(SpecError::DrawRange {
                            label: draw.label.clone(),
                            available,
                            requested: draw.index_range.end,
                        });
                    }
                }
                DrawSource::Dynamic(idx) => {
                    let Some(mesh) = self.dynamic_meshes.get(idx) else {
                        return Err(SpecError::DrawMissingMesh {
                            label: draw.label.clone(),
                        });
                    };
                    let available = mesh.indices.len() as u32;
                    if draw.index_range.end > available {
                        return Err(SpecError::DrawRange {
                            label: draw.label.clone(),
                            available,
                            requested: draw.index_range.end,
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

// ── Canonical vertex types ─────────────────────────────────────────────────────

/// Canonical vertex for world-space (3-D) slides.
///
/// All 3-D slides must produce meshes using this layout so the engine's world
/// shader prelude can address the attributes at the fixed locations below.
///
/// | Location | Field | Format |
/// |----------|-------|--------|
/// | 0 | `position` | `Float32x3` |
/// | 1 | `normal` | `Float32x3` |
/// | 2 | `color` | `Float32x4` |
/// | 3 | `mode` | `Float32` |
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable, Serialize, Deserialize)]
pub struct WorldVertex {
    /// Object-space position.
    pub position: [f32; 3],
    /// Object-space surface normal.
    pub normal: [f32; 3],
    /// Per-vertex RGBA colour.
    pub color: [f32; 4],
    /// Shader-interpreted mode flag (0 = lit, 1 = sky/unlit, etc.).
    pub mode: f32,
}

#[cfg(feature = "gpu")]
impl WorldVertex {
    /// Vertex attribute descriptors for use in a wgpu pipeline.
    pub const ATTRIBS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
        2 => Float32x4,
        3 => Float32,
    ];

    /// Returns the [`wgpu::VertexBufferLayout`] for this vertex type.
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<WorldVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Canonical vertex for screen-space (2-D) slides.
///
/// All 2-D slides must produce meshes using this layout so the engine's screen
/// shader prelude can address the attributes at the fixed locations below.
///
/// | Location | Field | Format |
/// |----------|-------|--------|
/// | 0 | `position` | `Float32x3` |
/// | 1 | `tex_coords` | `Float32x2` |
/// | 2 | `color` | `Float32x4` |
/// | 3 | `mode` | `Float32` |
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable, Serialize, Deserialize)]
pub struct ScreenVertex {
    /// Clip-space position (z ignored; use 0.0).
    pub position: [f32; 3],
    /// Normalised texture coordinates.
    pub tex_coords: [f32; 2],
    /// Per-vertex RGBA colour.
    pub color: [f32; 4],
    /// Shader-interpreted mode flag.
    pub mode: f32,
}

#[cfg(feature = "gpu")]
impl ScreenVertex {
    /// Vertex attribute descriptors for use in a wgpu pipeline.
    pub const ATTRIBS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x2,
        2 => Float32x4,
        3 => Float32,
    ];

    /// Returns the [`wgpu::VertexBufferLayout`] for this vertex type.
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ScreenVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

// ── Optional configure protocol ───────────────────────────────────────────────

/// Declare a parameter buffer that the host can populate before `vzglyd_init`.
///
/// # Configure Protocol
///
/// A slide opts into parameterisation by invoking this macro and exporting a
/// `vzglyd_configure(len: i32) -> i32` function.  The host will then:
///
/// 1. Call `vzglyd_params_ptr()` to locate the buffer in WASM linear memory.
/// 2. Call `vzglyd_params_capacity()` to learn how many bytes the buffer holds.
/// 3. Write JSON parameter bytes (truncated to capacity) into the buffer.
/// 4. Call `vzglyd_configure(len)` with the byte count written.
/// 5. Proceed with the normal `vzglyd_init` / `vzglyd_spec_ptr` / `vzglyd_spec_len` sequence.
///
/// If any of `vzglyd_params_ptr`, `vzglyd_params_capacity`, or `vzglyd_configure` is
/// absent, the host skips the configure step entirely.
///
/// # Example
///
/// ```no_run
/// use vzglyd_slide::params_buf;
///
/// params_buf!(256);
///
/// # #[cfg(target_arch = "wasm32")]
/// #[unsafe(no_mangle)]
/// pub extern "C" fn vzglyd_configure(len: i32) -> i32 {
///     let bytes = unsafe { &VZGLYD_PARAMS_BUF[..len.max(0) as usize] };
///     // parse `bytes` as JSON and apply to slide state
///     let _ = bytes;
///     0
/// }
/// ```
#[macro_export]
macro_rules! params_buf {
    ($size:expr) => {
        #[cfg(target_arch = "wasm32")]
        static mut VZGLYD_PARAMS_BUF: [u8; $size] = [0u8; $size];

        /// Returns a pointer into linear memory where the host writes parameter bytes.
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn vzglyd_params_ptr() -> i32 {
            unsafe { VZGLYD_PARAMS_BUF.as_mut_ptr() as i32 }
        }

        /// Returns the byte capacity of the parameter buffer.
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn vzglyd_params_capacity() -> u32 {
            $size as u32
        }
    };
}

/// Export traced `vzglyd_configure`, `vzglyd_init`, and `vzglyd_update` entrypoints.
///
/// The generated exports preserve the stable slide ABI while automatically wrapping the
/// provided implementation functions in top-level trace spans. Inner slide logic can still add
/// more specific spans with [`trace_scope`] and [`trace_event`].
///
/// # Examples
///
/// ```no_run
/// fn slide_init() -> i32 {
///     0
/// }
///
/// fn slide_update(_dt: f32) -> i32 {
///     0
/// }
///
/// vzglyd_slide::export_traced_entrypoints! {
///     init = slide_init,
///     update = slide_update,
/// }
/// ```
///
/// ```no_run
/// fn slide_configure(_len: i32) -> i32 {
///     0
/// }
///
/// fn slide_init() -> i32 {
///     0
/// }
///
/// fn slide_update(_dt: f32) -> i32 {
///     0
/// }
///
/// vzglyd_slide::export_traced_entrypoints! {
///     configure = slide_configure,
///     init = slide_init,
///     update = slide_update,
/// }
/// ```
#[macro_export]
macro_rules! export_traced_entrypoints {
    (
        configure = $configure:path,
        init = $init:path,
        update = $update:path $(,)?
    ) => {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn vzglyd_configure(len: i32) -> i32 {
            $crate::traced_configure_entrypoint(len, $configure)
        }

        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn vzglyd_init() -> i32 {
            $crate::traced_init_entrypoint($init)
        }

        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn vzglyd_update(dt: f32) -> i32 {
            $crate::traced_update_entrypoint(dt, $update)
        }
    };
    (
        init = $init:path,
        update = $update:path $(,)?
    ) => {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn vzglyd_init() -> i32 {
            $crate::traced_init_entrypoint($init)
        }

        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn vzglyd_update(dt: f32) -> i32 {
            $crate::traced_update_entrypoint(dt, $update)
        }
    };
}

// ── Shared texture generators ──────────────────────────────────────────────────

/// The character order used by the 5×7 bitmap font atlas.
///
/// Each character occupies a 6-pixel-wide column in the atlas (5 pixels of
/// glyph data + 1 pixel gap). The atlas is `256 × 8` pixels, RGBA8.
pub const FONT_CHAR_ORDER: &[u8] = b" ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.-:";

/// Generates the built-in 5×7 bitmap font atlas used by world-space slides.
///
/// Returns a `256 × 8` RGBA8 pixel buffer (8 192 bytes). White pixels are set
/// for lit glyph bits; all other pixels are transparent black.
///
/// This atlas is also available as the engine's default font texture slot, so
/// slides do not need to bundle it unless they want to override it.
pub fn make_font_atlas() -> Vec<u8> {
    fn glyph(c: u8) -> [u8; 7] {
        match c {
            b' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            b'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
            b'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
            b'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
            b'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
            b'E' => [0x1F, 0x10, 0x10, 0x1C, 0x10, 0x10, 0x1F],
            b'F' => [0x1F, 0x10, 0x10, 0x1C, 0x10, 0x10, 0x10],
            b'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0E],
            b'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
            b'I' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1F],
            b'J' => [0x0F, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
            b'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
            b'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
            b'M' => [0x11, 0x1B, 0x15, 0x11, 0x11, 0x11, 0x11],
            b'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
            b'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
            b'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
            b'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x13, 0x0F],
            b'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
            b'S' => [0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E],
            b'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
            b'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
            b'V' => [0x11, 0x11, 0x11, 0x11, 0x0A, 0x0A, 0x04],
            b'W' => [0x11, 0x11, 0x11, 0x15, 0x1B, 0x11, 0x11],
            b'X' => [0x11, 0x0A, 0x04, 0x04, 0x04, 0x0A, 0x11],
            b'Y' => [0x11, 0x0A, 0x04, 0x04, 0x04, 0x04, 0x04],
            b'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
            b'0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
            b'1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
            b'2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
            b'3' => [0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E],
            b'4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
            b'5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
            b'6' => [0x0E, 0x10, 0x10, 0x1E, 0x11, 0x11, 0x0E],
            b'7' => [0x1F, 0x01, 0x02, 0x04, 0x04, 0x04, 0x04],
            b'8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
            b'9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x11, 0x0E],
            b'.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04],
            b'-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
            b':' => [0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x00],
            _ => [0x00; 7],
        }
    }
    const AW: usize = 256;
    const AH: usize = 8;
    let mut buf = vec![0u8; AW * AH * 4];
    for (ci, &c) in FONT_CHAR_ORDER.iter().enumerate() {
        let rows = glyph(c);
        let xb = ci * 6;
        for (row, &byte) in rows.iter().enumerate() {
            for col in 0..5usize {
                if (byte >> (4 - col)) & 1 == 1 {
                    let i = (row * AW + xb + col) * 4;
                    buf[i] = 255;
                    buf[i + 1] = 255;
                    buf[i + 2] = 255;
                    buf[i + 3] = 255;
                }
            }
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Pod, bytemuck::Zeroable)]
    #[repr(C)]
    struct V {
        pos: [f32; 3],
    }

    #[test]
    fn limits_pi4_are_conservative() {
        let l = Limits::pi4();
        assert!(l.max_vertices >= 25_000);
        assert!(l.max_indices >= 26_000);
    }

    #[test]
    fn validate_checks_vertex_budget() {
        let spec = SlideSpec {
            name: "test".to_string(),
            limits: Limits {
                max_vertices: 3,
                max_indices: 10,
                max_static_meshes: 2,
                max_dynamic_meshes: 1,
                max_textures: 1,
                max_texture_bytes: 64,
                max_texture_dim: 16,
            },
            scene_space: SceneSpace::Screen2D,
            camera_path: None,
            shaders: None,
            overlay: None,
            font: None,
            textures_used: 1,
            textures: vec![TextureDesc {
                label: "t".to_string(),
                width: 1,
                height: 1,
                format: TextureFormat::Rgba8Unorm,
                wrap_u: WrapMode::ClampToEdge,
                wrap_v: WrapMode::ClampToEdge,
                wrap_w: WrapMode::ClampToEdge,
                mag_filter: FilterMode::Nearest,
                min_filter: FilterMode::Nearest,
                mip_filter: FilterMode::Nearest,
                data: vec![255, 255, 255, 255],
            }],
            sounds: vec![],
            animations: vec![],
            static_meshes: vec![StaticMesh {
                label: "m".to_string(),
                vertices: vec![V { pos: [0.0; 3] }; 4],
                indices: vec![0, 1, 2],
            }],
            dynamic_meshes: vec![],
            draws: vec![DrawSpec {
                label: "d".to_string(),
                source: DrawSource::Static(0),
                pipeline: PipelineKind::Opaque,
                index_range: 0..3,
            }],
            lighting: None,
        };
        let err = spec.validate().unwrap_err();
        matches!(err, SpecError::VertexBudget { .. });
    }

    #[test]
    fn scene_anchor_translation_reads_transform_origin() {
        let anchor = SceneAnchor {
            id: "spawn".into(),
            label: "Spawn".into(),
            node_name: Some("Spawn".into()),
            tag: Some("spawn".into()),
            world_transform: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [3.0, 1.5, -2.0, 1.0],
            ],
        };

        assert_eq!(anchor.translation(), [3.0, 1.5, -2.0]);
    }

    #[test]
    fn scene_anchor_lookup_reports_available_ids() {
        let anchors = SceneAnchorSet {
            scene_id: "hero_world".into(),
            scene_label: Some("Hero World".into()),
            scene_name: Some("WorldScene".into()),
            anchors: vec![SceneAnchor {
                id: "spawn_marker".into(),
                label: "SpawnAnchor".into(),
                node_name: Some("SpawnAnchor".into()),
                tag: Some("spawn".into()),
                world_transform: [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [3.0, 0.0, 2.0, 1.0],
                ],
            }],
        };

        let error = anchors
            .require_anchor("missing")
            .expect_err("missing anchor should fail");
        assert_eq!(
            error.to_string(),
            "scene 'hero_world' does not define anchor 'missing' (available: spawn_marker)"
        );
    }
}
