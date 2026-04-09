# Adding Audio to Your VZGLYD Slide

This guide shows you how to embed sound files (MP3, WAV, Ogg, FLAC) into your slide bundle and control playback from your slide's Rust code.

## Overview

VZGLYD slides can play background music, sound effects, and ambient audio. Sounds are:

1. **Packaged into the `.vzglyd` bundle** alongside textures and meshes
2. **Referenced in `manifest.json`** under the `sounds` array
3. **Played from Rust code** using `vzglyd_slide` audio functions
4. **Identified by a `u32` ID** you choose, so you can control multiple sounds independently

Supported formats: **MP3**, **WAV**, **Ogg Vorbis**, **FLAC**.

---

## Step 1: Add Sound Files to Your Slide

Place your audio files somewhere inside your slide's package directory. A common convention is an `assets/` folder:

```
my-slide/
├── manifest.json
├── src/
│   └── lib.rs
└── assets/
    ├── background.mp3
    └── click.wav
```

You can use any filename. The file extension tells the host which decoder to use.

---

## Step 2: Declare Sounds in `manifest.json`

Add a `sounds` array inside the `assets` section of your `manifest.json`. Each entry needs a `path` and optionally an `id`:

```json
{
  "name": "My Audio Slide",
  "abi_version": 2,
  "scene_space": "world_3d",
  "assets": {
    "sounds": [
      { "id": "bg_music", "path": "assets/background.mp3" },
      { "id": "click",    "path": "assets/click.wav" }
    ]
  }
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `path` | ✅ | Path to the audio file, relative to the package root |
| `id`   | ❌ | Unique key used in code to reference this sound. If omitted, falls back to `label`, then the file stem (e.g., `"background"` for `background.mp3`) |

### What `id` Do I Use?

The `id` becomes the **key** you pass to `play_sound()`. Resolution order:

1. `id` field (e.g., `"bg_music"`)
2. `label` field (e.g., `"Click Sound"`)
3. File stem without extension (e.g., `"background"` from `background.mp3`)

**Recommendation**: always set an explicit `id` so your code is clear and stable.

---

## Step 3: Play Sounds from Your Slide Code

Import the audio functions from `vzglyd_slide`:

```rust
use vzglyd_slide::{play_sound, stop_sound, set_volume, pause_sound, resume_sound};
```

### Playing a Sound

```rust
// Play "bg_music" at 50% volume, not looped
play_sound(1, "bg_music", 0.5, false);

// Play "click" at full volume, not looped
play_sound(2, "click", 1.0, false);
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `id` | `u32` | Your chosen identifier for this playback instance. Pick any number — you'll use it later to control this specific sound |
| `key` | `&str` | The `id` from `manifest.json` that identifies which sound to play |
| `volume` | `f32` | Volume from `0.0` (silent) to `1.0` (full). Values outside this range are clamped |
| `looped` | `bool` | If `true`, the sound repeats indefinitely until you call `stop_sound(id)` |

### Looping Background Music

```rust
// Start looping background music on ID 1
play_sound(1, "bg_music", 0.3, true);

// Later, stop it
stop_sound(1);
```

### Stopping a Sound

```rust
stop_sound(1);
```

### Changing Volume

```rust
// Fade in: gradually increase volume
set_volume(1, 0.0);
// ... a few frames later ...
set_volume(1, 0.5);
// ... eventually ...
set_volume(1, 1.0);
```

### Pausing and Resuming

```rust
play_sound(1, "bg_music", 0.5, true);

// Pause (e.g., when slide goes inactive)
pause_sound(1);

// Resume (e.g., when slide comes back)
resume_sound(1);
```

---

## Complete Example: Slide with Background Music + Sound Effects

```rust
use vzglyd_slide::{
    play_sound, stop_sound, set_volume, pause_sound, resume_sound,
    export_traced_entrypoints,
};

static mut MUSIC_PLAYING: bool = false;
static mut CLICK_COUNT: u32 = 0;

#[cfg(target_arch = "wasm32")]
fn my_slide_init() -> i32 {
    unsafe {
        MUSIC_PLAYING = false;
        CLICK_COUNT = 0;
    }
    // Start background music on ID 1, looped, 30% volume
    play_sound(1, "bg_music", 0.3, true);
    unsafe { MUSIC_PLAYING = true; }
    0
}

#[cfg(target_arch = "wasm32")]
fn my_slide_update(dt: f32) -> i32 {
    // Example: play a click sound every 5 seconds
    unsafe {
        CLICK_COUNT += 1;
        if CLICK_COUNT % 300 == 0 {  // ~5 seconds at 60 fps
            play_sound(2, "click", 1.0, false);
        }
    }
    0
}

#[cfg(target_arch = "wasm32")]
export_traced_entrypoints! {
    init = my_slide_init,
    update = my_slide_update,
}
```

---

## Sound Instance IDs

Each sound you play gets a **`u32` ID** that you choose. This lets you manage multiple sounds independently:

```rust
// Three simultaneous sounds with different IDs
play_sound(10, "ambience",  0.2, true);   // looping background
play_sound(20, "notification", 0.8, false); // one-shot alert
play_sound(30, "music",     0.5, true);   // looping music

// Control them individually
set_volume(10, 0.0);  // mute the ambience
stop_sound(20);       // stop the notification
pause_sound(30);      // pause the music
resume_sound(30);     // resume the music
```

### Reusing an ID

If you call `play_sound()` with an ID that's already playing, the old sound is **stopped and replaced** by the new one:

```rust
play_sound(1, "old_track", 0.5, true);
// Later...
play_sound(1, "new_track", 0.7, true); // "old_track" is stopped, "new_track" starts
```

---

## Pack Your Slide

When you run the VZGLYD pack command, sound files are automatically collected from the paths in `manifest.json` and embedded into the `.vzglyd` archive:

```bash
cargo run -p VRX-64-native -- pack --input ./my-slide --output ./my-slide.vzglyd
```

The pack command validates that all sound file paths exist and are readable.

---

## Error Handling

All audio functions return an `i32`:

| Return Value | Meaning |
|-------------|---------|
| `0` | Success |
| Negative value | Host error (e.g., sound not found, decode error, device unavailable) |

In practice, most slide code ignores the return value since the host logs errors:

```rust
play_sound(1, "bg_music", 0.5, false); // Just fire and forget
```

---

## Testing Without a Host

When running tests on `wasm32-wasip1` or native targets, the audio functions are **stubs that return `0`**. They only do real work when the slide is loaded by a VZGLYD host (native or web):

```rust
// This compiles and runs in tests, but does nothing
play_sound(1, "bg_music", 0.5, false);
// Returns 0 in tests, actually plays audio on a real host
```

---

## Supported Formats

| Format | Extension | Notes |
|--------|-----------|-------|
| MP3 | `.mp3` | Most common, good compression |
| WAV | `.wav` | Uncompressed PCM, fast decode |
| Ogg Vorbis | `.ogg` | Open format, good compression |
| FLAC | `.flac` | Lossless, larger files |

**Recommendation**: Use MP3 for music/background (good size/quality tradeoff) and WAV for short sound effects (minimal decode latency).

---

## ABI Version Requirement

Audio requires **ABI version 2**. Your `manifest.json` must declare `"abi_version": 2`, and
your slide's `vzglyd_abi_version()` function must return `2`. If you're using `vzglyd_slide`
0.1.x (which ships ABI 2), this happens automatically — just use `ABI_VERSION` from the crate:

```rust
use vzglyd_slide::ABI_VERSION; // currently 2

#[unsafe(no_mangle)]
pub extern "C" fn vzglyd_abi_version() -> u32 {
    ABI_VERSION
}
```

### Why Was the ABI Bumped?

Three reasons, all of which make audio **incompatible with ABI 1** engines:

1. **`SlideSpec` serialization changed** — A new `sounds: Vec<SoundDesc>` field was inserted
   between `textures` and `static_meshes`. An ABI 1 engine deserializing an ABI 2 spec would
   read sound bytes as texture data, corrupting memory.

2. **New WASM imports** — The slide now imports `audio_play`, `audio_stop`, etc. from the
   `vzglyd_host` module. An ABI 1 host doesn't export these, so the WASM linker would fail
   at load time.

3. **Manifest format extended** — The `sounds` array under `assets` is new. Old engines
   would silently ignore it, but the new pack command requires it to bundle audio into
   `.vzglyd` archives.

The engine enforces the ABI check on load, so:
- **Old slides (ABI 1)** still work on new engines (compatibility window)
- **New slides (ABI 2)** are rejected by old engines (safe — no silent corruption)

---

## Troubleshooting

### Sound doesn't play at all

1. Check that `manifest.json` has `"abi_version": 2` (audio requires ABI v2)
2. Verify the sound file exists at the path specified in `manifest.json`
3. Check the `id` you pass to `play_sound()` matches the `id` in the manifest
4. Check the host logs — the engine logs warnings for missing sounds

### Sound plays but is silent

1. Check volume is not `0.0`
2. Check your system audio output is not muted
3. On native Linux, verify ALSA/PulseAudio is working

### Sound keeps looping when it shouldn't

Check the `looped` parameter — set it to `false` for one-shot sounds:

```rust
play_sound(1, "click", 1.0, false); // NOT looped
```

### Multiple sounds interfere with each other

Make sure each concurrent sound uses a **different ID**:

```rust
// WRONG: both use ID 1, second replaces first
play_sound(1, "music", 0.5, true);
play_sound(1, "click", 1.0, false); // stops music!

// RIGHT: different IDs
play_sound(1, "music", 0.5, true);
play_sound(2, "click", 1.0, false);
```
