use std::f32::consts::PI;
use std::sync::Arc;

use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use harness_persistence::{Repository, TaskStatus};
use harness_sona::CollectiveMerit;

use crate::state::HiveState;
use crate::tools::sonification::{
    GenerateHiveMusicRequest, GenerateHiveMusicResponse, MusicMetadata, TaskMusicEvent,
};

const SAMPLE_RATE: u32 = 44100;
const CHANNELS: u16 = 1;
const BITS_PER_SAMPLE: u16 = 16;

/// Musical scales (frequencies in Hz relative to C4 = 261.63)
/// Using Just Intonation ratios for simplicity
const SCALE_MAJOR: &[f32] = &[1.0, 9.0 / 8.0, 5.0 / 4.0, 4.0 / 3.0, 3.0 / 2.0, 5.0 / 3.0, 15.0 / 8.0, 2.0];
const SCALE_MINOR: &[f32] = &[1.0, 9.0 / 8.0, 6.0 / 5.0, 4.0 / 3.0, 3.0 / 2.0, 8.0 / 5.0, 9.0 / 5.0, 2.0];
const SCALE_DIMINISHED: &[f32] = &[1.0, 9.0 / 8.0, 6.0 / 5.0, 11.0 / 8.0, 3.0 / 2.0, 8.0 / 5.0, 15.0 / 8.0, 2.0]; // Approximate
const SCALE_PENTATONIC: &[f32] = &[1.0, 9.0 / 8.0, 5.0 / 4.0, 3.0 / 2.0, 5.0 / 3.0, 2.0];

/// Generate hive music based on activity.
pub async fn generate_music<R: Repository + 'static>(
    state: &Arc<HiveState<R>>,
    req: GenerateHiveMusicRequest,
) -> anyhow::Result<GenerateHiveMusicResponse> {
    let now = Utc::now();
    let window_start = now - Duration::minutes(req.window_minutes as i64);

    // 1. Fetch data
    let repo = state.repository();
    let session_id = state.session_id();

    // For simplicity in this experimental feature, we list all tasks and filter by time.
    // In production, we'd want a time-range query in the repository.
    let tasks = repo.list_tasks(session_id, None).await?;
    let active_tasks: Vec<_> = tasks
        .into_iter()
        .filter(|t| t.created_at >= window_start)
        .collect();

    // Messages (simulated fetching for now as repo.list_messages doesn't take time range easily without listing all)
    // We'll use knowledge entries as proxy for "chatter" if messages are expensive to filter
    let knowledge = repo.get_recent_knowledge(session_id, 100).await?;
    let recent_knowledge: Vec<_> = knowledge
        .into_iter()
        .filter(|k| k.created_at >= window_start)
        .collect();

    // 2. Compute metrics for musical parameters
    // Approximate merit based on visible activity
    let total_tasks = active_tasks.len() as f64;
    let completed_tasks = active_tasks.iter().filter(|t| t.status == TaskStatus::Completed).count() as f64;
    let success_rate = if total_tasks > 0.0 { completed_tasks / total_tasks } else { 0.5 };

    let knowledge_count = recent_knowledge.len() as f64;
    let participation_score = (knowledge_count / 20.0).clamp(0.0, 1.0); // Arbitrary scale

    let merit = CollectiveMerit {
        score: (success_rate * 0.6 + participation_score * 0.4),
        knowledge_coverage: participation_score,
        task_success_rate: success_rate,
        agent_participation: participation_score,
        knowledge_diversity: 0.5, // Default
    };

    // 3. Determine musical parameters
    let chaos_level = 1.0 - merit.score; // Lower score = more chaos
    let tempo = req.tempo.unwrap_or(
        // Map success rate 0.0-1.0 to 60-180 BPM
        (60.0 + merit.task_success_rate * 120.0) as u64
    );

    let scale_name = req.scale.unwrap_or_else(|| {
        if merit.score > 0.7 {
            "major".to_string()
        } else if merit.score < 0.3 {
            "diminished".to_string()
        } else {
            "minor".to_string()
        }
    });

    let scale_ratios = match scale_name.as_str() {
        "major" => SCALE_MAJOR,
        "minor" => SCALE_MINOR,
        "diminished" => SCALE_DIMINISHED,
        "pentatonic" => SCALE_PENTATONIC,
        _ => SCALE_MAJOR, // Default
    };

    // 4. Generate events
    let mut events = Vec::new();
    let root_freq = 261.63; // Middle C

    // Task events
    for task in active_tasks {
        let rel_time = (task.created_at - window_start).num_seconds() as f64;
        if rel_time < 0.0 { continue; }

        let (kind, pitch_idx, duration) = match task.status {
            TaskStatus::Completed => ("task_complete", 7, 0.5), // High octave root
            TaskStatus::Failed => ("task_failed", 1, 0.8),      // Low second (dissonant usually)
            TaskStatus::InProgress => ("task_work", 4, 0.2),    // Fifth
            TaskStatus::Pending | TaskStatus::Claimed => ("task_new", 2, 0.1), // Third
        };

        // Select pitch from scale
        let ratio = scale_ratios[pitch_idx % scale_ratios.len()];
        let octave_mult = if pitch_idx >= scale_ratios.len() { 2.0 } else { 1.0 };
        let pitch = root_freq * ratio * octave_mult;

        events.push(TaskMusicEvent {
            timestamp: task.created_at.to_rfc3339(),
            kind: kind.to_string(),
            description: task.title,
            pitch,
            duration,
            volume: 0.5,
        });
    }

    // Knowledge/Message events
    if req.include_messages {
        for k in recent_knowledge {
            let rel_time = (k.created_at - window_start).num_seconds() as f64;
            if rel_time < 0.0 { continue; }

            // Random "chatter" pitch
            let pitch = root_freq * (1.0 + (k.content.len() % 10) as f32 * 0.1);

            events.push(TaskMusicEvent {
                timestamp: k.created_at.to_rfc3339(),
                kind: "knowledge_share".to_string(),
                description: "Knowledge shared".to_string(),
                pitch,
                duration: 0.1,
                volume: 0.3,
            });
        }
    }

    // Sort events by timestamp
    events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    // 5. Synthesize audio
    let duration_seconds = (req.window_minutes * 60) as f64;
    // We compress time: 1 hour of activity -> 60 seconds of audio (60x speedup)
    // Or maybe 1 minute of audio? Let's say 30 seconds for a quick listen.
    let audio_duration = 30.0;
    let time_compression = duration_seconds / audio_duration;

    let wav_bytes = synthesize_wav(&events, audio_duration, time_compression, window_start)?;
    let audio_data = base64::engine::general_purpose::STANDARD.encode(wav_bytes);

    Ok(GenerateHiveMusicResponse {
        audio_data,
        metadata: MusicMetadata {
            duration_seconds: audio_duration,
            scale_used: scale_name,
            tempo_bpm: tempo,
            total_events: events.len(),
            merit_score: merit.score,
            chaos_level,
        },
        events: events.into_iter().take(50).collect(), // Limit returned events to keep JSON small
    })
}

/// Synthesize a simple WAV buffer from events.
fn synthesize_wav(
    events: &[TaskMusicEvent],
    duration_seconds: f64,
    time_compression: f64,
    window_start: DateTime<Utc>,
) -> anyhow::Result<Vec<u8>> {
    let num_samples = (duration_seconds * SAMPLE_RATE as f64) as usize;
    let mut buffer = vec![0.0f32; num_samples];

    for event in events {
        let event_time = DateTime::parse_from_rfc3339(&event.timestamp)?.with_timezone(&Utc);
        let rel_seconds = (event_time - window_start).num_seconds() as f64;

        // Map real time to audio time
        let audio_start_time = rel_seconds / time_compression;
        let start_sample = (audio_start_time * SAMPLE_RATE as f64) as usize;

        if start_sample >= num_samples { continue; }

        let event_samples = (event.duration * SAMPLE_RATE as f64 as f32) as usize;
        let end_sample = (start_sample + event_samples).min(num_samples);

        // Generate sine wave
        for (i, sample_val) in buffer.iter_mut().enumerate().take(end_sample).skip(start_sample) {
            let t = (i - start_sample) as f32 / SAMPLE_RATE as f32;
            let sample = (t * event.pitch * 2.0 * PI).sin() * event.volume;

            // Apply simple envelope (attack/decay) to avoid clicking
            let envelope = if t < 0.01 {
                t / 0.01
            } else if t > event.duration - 0.01 {
                (event.duration - t) / 0.01
            } else {
                1.0
            };

            *sample_val += sample * envelope;
        }
    }

    // Normalize and convert to PCM
    let max_amp = buffer.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    let gain = if max_amp > 1.0 { 0.9 / max_amp } else { 0.9 };

    let mut pcm_data = Vec::with_capacity(num_samples * 2);
    for sample in buffer {
        let s = (sample * gain * i16::MAX as f32) as i16;
        pcm_data.extend_from_slice(&s.to_le_bytes());
    }

    // Build WAV header
    let mut wav = Vec::new();
    // RIFF header
    wav.extend_from_slice(b"RIFF");
    let file_size = 36 + pcm_data.len() as u32;
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // Chunk size
    wav.extend_from_slice(&1u16.to_le_bytes());  // Audio format (1 = PCM)
    wav.extend_from_slice(&CHANNELS.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    let byte_rate = SAMPLE_RATE * CHANNELS as u32 * BITS_PER_SAMPLE as u32 / 8;
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    let block_align = CHANNELS * BITS_PER_SAMPLE / 8;
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(pcm_data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&pcm_data);

    Ok(wav)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synthesize_wav_header() {
        let events = vec![
            TaskMusicEvent {
                timestamp: Utc::now().to_rfc3339(),
                kind: "test".to_string(),
                description: "test".to_string(),
                pitch: 440.0,
                duration: 1.0,
                volume: 0.5,
            }
        ];

        let wav = synthesize_wav(
            &events,
            5.0,
            1.0,
            Utc::now() - Duration::seconds(10)
        ).unwrap();

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");

        // Check sample rate (44100 = 0xAC44)
        assert_eq!(wav[24], 0x44);
        assert_eq!(wav[25], 0xAC);
    }
}
