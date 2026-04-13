//! Guest-side audio helpers for slide WASM modules.
//!
//! Slides call these functions to trigger audio playback through the host's
//! audio subsystem. The host is responsible for decoding and playing sounds;
//! the slide only issues commands.

#[cfg(target_arch = "wasm32")]
use std::ffi::CString;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "vzglyd_host")]
unsafe extern "C" {
    #[link_name = "audio_play"]
    fn host_audio_play(id: u32, key_ptr: *const u8, key_len: i32, volume: f32, looped: i32) -> i32;

    #[link_name = "audio_stop"]
    fn host_audio_stop(id: u32) -> i32;

    #[link_name = "audio_set_volume"]
    fn host_audio_set_volume(id: u32, volume: f32) -> i32;

    #[link_name = "audio_pause"]
    fn host_audio_pause(id: u32) -> i32;

    #[link_name = "audio_resume"]
    fn host_audio_resume(id: u32) -> i32;
}

/// Play a sound asset embedded in the slide bundle.
///
/// - `id`: A unique identifier chosen by the slide for this playback instance.
///   The same ID can later be used to stop, pause, or change the volume.
/// - `key`: The asset key of a [`SoundDesc`](crate::SoundDesc) from the slide's `sounds` list.
/// - `volume`: Playback volume from `0.0` (silent) to `1.0` (full volume).
/// - `looped`: If `true`, the sound repeats until explicitly stopped.
///
/// Returns `0` on success, or a negative error code from the host.
pub fn play_sound(id: u32, key: &str, volume: f32, looped: bool) -> i32 {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let c_key = CString::new(key).unwrap_or_default();
        let bytes = c_key.as_bytes();
        host_audio_play(
            id,
            bytes.as_ptr(),
            bytes.len() as i32,
            volume,
            looped as i32,
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = id;
        let _ = key;
        let _ = volume;
        let _ = looped;
        0
    }
}

/// Stop a currently playing sound by its ID.
///
/// Returns `0` on success, or a negative error code from the host.
pub fn stop_sound(id: u32) -> i32 {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        host_audio_stop(id)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = id;
        0
    }
}

/// Change the volume of a playing sound.
///
/// - `volume`: New volume from `0.0` (silent) to `1.0` (full volume).
///
/// Returns `0` on success, or a negative error code from the host.
pub fn set_volume(id: u32, volume: f32) -> i32 {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        host_audio_set_volume(id, volume)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = id;
        let _ = volume;
        0
    }
}

/// Pause a playing sound (can be resumed with [`resume_sound`]).
///
/// Returns `0` on success, or a negative error code from the host.
pub fn pause_sound(id: u32) -> i32 {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        host_audio_pause(id)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = id;
        0
    }
}

/// Resume a previously paused sound.
///
/// Returns `0` on success, or a negative error code from the host.
pub fn resume_sound(id: u32) -> i32 {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        host_audio_resume(id)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = id;
        0
    }
}
