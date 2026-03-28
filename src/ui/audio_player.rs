// Audio playback for voice messages using rodio.
#![allow(dead_code)]

use std::io::Cursor;
use std::sync::{Arc, Mutex};

/// Shared playback state accessible from both async tasks and the UI.
#[derive(Debug, Clone)]
pub struct AudioPlayer {
    inner: Arc<Mutex<PlayerState>>,
}

#[derive(Debug)]
struct PlayerState {
    /// Currently playing URL (if any).
    playing_url: Option<String>,
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PlayerState { playing_url: None })),
        }
    }

    /// Returns true if the given URL is currently playing.
    pub fn is_playing(&self, url: &str) -> bool {
        self.inner
            .lock()
            .map(|s| s.playing_url.as_deref() == Some(url))
            .unwrap_or(false)
    }

    /// Play audio from in-memory WAV/OGG bytes.
    pub fn play_bytes(&self, url: String, data: Vec<u8>) {
        let inner = self.inner.clone();
        // Mark as playing
        if let Ok(mut s) = inner.lock() {
            s.playing_url = Some(url.clone());
        }
        let inner2 = inner.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                let (_stream, stream_handle) = rodio::OutputStream::try_default()?;
                let cursor = Cursor::new(data);
                let source = rodio::Decoder::new(cursor)?;
                let sink = rodio::Sink::try_new(&stream_handle)?;
                sink.append(source);
                sink.sleep_until_end();
                Ok(())
            })();
            if let Err(e) = result {
                tracing::warn!("audio playback failed: {e}");
            }
            // Clear playing state
            if let Ok(mut s) = inner2.lock() {
                if s.playing_url.as_deref() == Some(url.as_str()) {
                    s.playing_url = None;
                }
            }
        });
    }

    /// Stop playback (best-effort — playback thread will finish its current buffer).
    pub fn stop(&self) {
        if let Ok(mut s) = self.inner.lock() {
            s.playing_url = None;
        }
    }
}

/// Returns true if a URL looks like an audio file.
pub fn is_audio_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.ends_with(".wav")
        || lower.ends_with(".ogg")
        || lower.ends_with(".opus")
        || lower.ends_with(".m4a")
        || lower.ends_with(".mp3")
        || lower.ends_with(".flac")
}
