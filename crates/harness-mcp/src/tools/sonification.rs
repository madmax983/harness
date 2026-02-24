use serde::{Deserialize, Serialize};

/// Request parameters for generating hive music.
#[derive(Debug, Deserialize)]
pub struct GenerateHiveMusicRequest {
    /// Window of time to analyze in minutes (default: 60).
    #[serde(default = "default_window_minutes")]
    pub window_minutes: u64,
    /// Musical scale to use (default: "auto" based on hive merit).
    /// Options: "major", "minor", "diminished", "chromatic", "pentatonic", "auto".
    #[serde(default)]
    pub scale: Option<String>,
    /// Base tempo in BPM (default: "auto" based on task success rate).
    #[serde(default)]
    pub tempo: Option<u64>,
    /// Whether to include message activity in the mix.
    #[serde(default = "default_true")]
    pub include_messages: bool,
    /// Optional agent ID for context (unused for now).
    pub _agent_id: Option<String>,
}

fn default_window_minutes() -> u64 {
    60
}

fn default_true() -> bool {
    true
}

/// A musical event derived from hive activity.
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskMusicEvent {
    pub timestamp: String,
    pub kind: String,
    pub description: String,
    pub pitch: f32,    // Hz
    pub duration: f32, // Seconds
    pub volume: f32,   // 0.0 - 1.0
}

/// Response containing the generated music.
#[derive(Debug, Serialize)]
pub struct GenerateHiveMusicResponse {
    /// Base64 encoded WAV audio data.
    pub audio_data: String,
    /// Metadata about the generated music.
    pub metadata: MusicMetadata,
    /// List of significant events that generated sound.
    pub events: Vec<TaskMusicEvent>,
}

#[derive(Debug, Serialize)]
pub struct MusicMetadata {
    pub duration_seconds: f64,
    pub scale_used: String,
    pub tempo_bpm: u64,
    pub total_events: usize,
    pub merit_score: f64,
    pub chaos_level: f64,
}
