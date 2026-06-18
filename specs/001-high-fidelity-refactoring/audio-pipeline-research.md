# Audio Pipeline Research: High-Fidelity Gapless Bit-Perfect Playback

**Date**: 2026-05-22
**Sources**: Google Search, Context7 MCP, crate docs (cpal, symphonia, rtrb, rubato, lofty)

---

## Pipeline Architecture

```
┌──────────────┐    ┌─────────────┐    ┌──────────────┐
│  Decoder     │    │  Resampler  │    │  Ring        │
│  (symphonia) │───►│  (rubato)   │───►│  Buffer      │───►  CPAL
│  [std thread]│    │  [inline]   │    │  (rtrb)      │      callback
└──────────────┘    └─────────────┘    └──────────────┘
       │                                                   │
       │  pre-buffer next track                             │  lock-free, never
       │  ~2s before end                                    │  allocate, never block
       ▼                                                    ▼
  Gapless transition                                  Sample format
  logic (delay/padding                                 conversion ONLY if
  trim via gapless: true)                              device requires it
```

Two threads:
1. **Decoder thread** (`std::thread`) — reads file, decodes via symphonia, resamples via rubato (if needed), pushes PCM to ring buffer
2. **CPAL callback thread** — pulls from ring buffer, fills device buffer (real-time constrained, never allocate)

---

## Stage 1: Decoding — Symphonia

### Crate

```toml
symphonia = { version = "0.5", default-features = false, features = [
    "mp3", "aac", "flac", "vorbis", "opus", "pcm", "alac",
    "isomp4", "ogg",
] }
symphonia-core = "0.5"
```

### Format Support

| Format | Container | Gapless Support |
|--------|-----------|-----------------|
| FLAC | Native FLAC | ✅ Full (native delay/padding in STREAMINFO) |
| MP3 | MPEG | ✅ (LAME/Xing encoder delay/padding) |
| AAC | MP4/M4A | ✅ |
| Ogg Vorbis | OGG | ✅ Full (PREROLL handled automatically) |
| Opus | OGG | ✅ Full |
| WAV | WAV | ✅ |
| AIFF | AIFF | ✅ |
| ALAC | MP4/M4A | ✅ (delay in cookie atom) |

### Key API — Zero-Allocation Decode Loop

```rust
use symphonia::core::audio::{AudioBufferRef, GenericAudioBufferRef};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

// Pre-allocate output buffer — zero allocation on hot path
let mut pcm_buffer: Vec<f32> = Vec::new();

// 1. Open and probe
let src = std::fs::File::open(path)?;
let mss = MediaSourceStream::new(Box::new(src), Default::default());
let mut hint = Hint::new();
hint.with_extension("flac");
let meta_opts: MetadataOptions = Default::default();
let fmt_opts: FormatOptions = Default::default();
let mut format = symphonia::default::get_probe()
    .probe(&hint, mss, fmt_opts, meta_opts)?;

// 2. Select audio track
let track = format.default_track(TrackType::Audio)?;
let track_id = track.id;

// 3. Get codec parameters
let codec_params = track.codec_params.as_ref()
    .and_then(|p| p.audio())?;
let sample_rate = codec_params.sample_rate;
let channels = codec_params.channels as usize;

// 4. Create decoder with automatic gapless trim
let dec_opts = AudioDecoderOptions {
    gapless: true,
    verify: false,
};
let mut decoder = symphonia::default::get_codecs()
    .make_audio_decoder(codec_params, &dec_opts)?;

// 5. Zero-allocation decode loop
loop {
    let packet = match format.next_packet() {
        Ok(Some(p)) => p,
        Ok(None) => break,
        Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
        Err(e) => return Err(e),
    };
    if packet.track_id() != track_id { continue; }
    match decoder.decode(&packet) {
        Ok(decoded) => {
            let frames = decoded.frames();
            let sample_count = frames * channels;
            pcm_buffer.resize(sample_count, 0.0);
            decoded.copy_to_slice_interleaved(&mut pcm_buffer);
            // Push pcm_buffer to resampler or ring buffer
        }
        Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
        Err(e) => return Err(e),
    }
}
```

### Gapless Handling

Symphonia provides two complementary approaches:

**Primary — automatic via `AudioDecoderOptions { gapless: true }`**:
Enabling `gapless` tells Symphonia to automatically trim encoder delay and padding samples from the decoded output. This is the recommended approach for most codecs. Symphonia handles FLAC's native STREAMINFO delay/padding, MP3's LAME/Xing header, Vorbis PREROLL, and ALAC's cookie atom.

**Fallback — manual via `codec_params.delay` / `codec_params.padding`**:
For codecs where automatic trim is insufficient, inspect these fields:
- **Delay** (priming samples): Initial samples to discard (encoder-generated silence/fade-in)
- **Padding**: Trailing samples to discard

The next track's decoded data feeds the same ring buffer — the CPAL stream never stops:

```rust
// Track N decoding nears completion
// → Start decoding Track N+1 in parallel (~2s before end)
// → When Track N's last sample enters ring buffer,
//   Track N+1's first sample follows immediately
// → CPAL stream never underruns → gapless
```

### Custom MediaSource for Prefetch

Symphonia expects synchronous `Read + Seek`. Wrap file reads in a `MediaSource` that reads ahead to reduce syscall overhead:

```rust
struct PrefetchSource {
    file: File,
    prefetch_buf: Vec<u8>,
    pos: u64,
}

impl MediaSource for PrefetchSource {
    fn is_seekable(&self) -> bool { true }
    fn byte_len(&self) -> Option<u64> {
        Some(self.file.metadata().map(|m| m.len()).unwrap_or(0))
    }
}

impl Read for PrefetchSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Read-ahead: fetch larger chunks into prefetch_buf
        self.file.read(buf)
    }
}

impl Seek for PrefetchSource {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.file.seek(pos)
    }
}
```

---

## Stage 2: Resampling — Rubato

### Crate

```toml
rubato = "0.14"
```

### Decision Tree: Bypass or Resample

```
Is source sample rate == device native rate AND bit-perfect mode active?
    ├── YES → BYPASS resampler entirely (bit-perfect path)
    │         Decoder PCM → rtrb → CPAL (no conversion, bit-identical)
    │
    └── NO  → INSERT rubato resampler
              Decoder PCM → rubato → rtrb → CPAL
```

### Primary Strategy: FFT Synchronous — Highest Quality

For a session where the output sample rate is fixed, use `Fft::<f64>` (synchronous FFT resampler with Sinc interpolation). Provides maximum quality for high-fidelity playback:

```rust
use rubato::{Resampler, Fft, FixedSync};
use rubato::audioadapter_buffers::direct::InterleavedSlice;

let mut resampler = Fft::<f64>::new(
    input_rate,
    output_rate,
    1024,                        // chunk size
    channels,
    channels,
    FixedSync::Both,
)?;

// Pre-allocate all buffers (zero allocation on hot path)
let input_frames = resampler.input_frames_next();
let mut input = vec![0.0_f64; channels * input_frames];
let output_frames = resampler.output_frames_next();
let mut output = vec![0.0_f64; channels * output_frames];

let input_adapter = InterleavedSlice::new(&input, channels, input_frames)?;
let mut output_adapter = InterleavedSlice::new_mut(&mut output, channels, output_frames)?;

let (frames_read, frames_written) = resampler
    .process_into_buffer(&input_adapter, &mut output_adapter, None)?;
```

### Dynamic Strategy: Async Polynomial

For sample rate changes mid-playlist (e.g., 44.1kHz → 96kHz transition), use `Async::<f64>::new_poly(Septic)` to dynamically adjust the resampling ratio without creating a new resampler:

```rust
use rubato::{Async, FixedAsync, PolynomialDegree, Resampler};

let mut resampler = Async::<f64>::new_poly(
    output_rate as f64 / input_rate as f64,
    2.0,
    PolynomialDegree::Septic,
    1024,
    channels,
    FixedAsync::Output,
)?;

// Dynamically change ratio for next track
resampler.set_resample_ratio_relative(
    output_rate as f64 / next_input_rate as f64,
    false,
)?;
```

### Precision Choice

```rust
// f64 — recommended for high-fidelity, full double-precision accumulation
Fft::<f64>::new(...)
Async::<f64>::new_poly(...)

// f32 — only for resource-constrained targets
Fft::<f32>::new(...)
```

**Recommendation**: Use `f64` in the resampler for maximum precision. Convert to `f32` only when writing to the output ring buffer (CPAL consumes `f32`).

### Quality Tiers

| Resampler Type | Quality | Speed | Use Case |
|---|---|---|---|
| `Fft::<f64>` — Sinc | Highest | Moderate | Fixed-rate high-fidelity (default) |
| `Async::new_poly(Septic)` — deg 7 | High | Fast | Dynamic ratio changes |
| `Async::new_poly(Cubic)` — deg 3 | Medium | Very fast | Non-critical |
| `Async::new_poly(Linear)` — deg 1 | Low | Fastest | Preview/low-power |

### Buffer Adapter Types

| Adapter | Layout | Description |
|---|---|---|
| `InterleavedSlice` | `L R L R L R` | Standard interleaved audio |
| `InterleavedSliceMut` | `L R L R L R` | Mutable interleaved for output |
| `SequentialSlice` | `L L L R R R` | Planar audio per channel |

All adapters are **zero-copy** — structural views over existing memory.

---

## Stage 3: Lock-Free Ring Buffer — rtrb

### Crate

```toml
rtrb = "0.3"
```

### API

```rust
use rtrb::{RingBuffer, Chunk};

// Single SPSC ring buffer connecting decoder thread → CPAL callback
let (mut producer, mut consumer) = RingBuffer::<f32>::new(65536);

// --- Decoder thread (producer) ---
// Wait-free bulk write:
let decoded: &[f32] = &pcm_buffer[..frames * channels];
match producer.write_chunk(decoded.len()) {
    Ok(chunk) => {
        chunk.copy_from_slice(decoded);
        chunk.commit_all();
    }
    Err(_) => {
        // Buffer full — decoder faster than output. Spin briefly.
        // Should not occur with correct sizing.
    }
}

// --- CPAL callback (consumer) ---
// Real-time safe:
match consumer.read_chunk(data.len()) {
    Ok(chunk) => {
        data.copy_from_slice(&chunk);
        chunk.commit_all();
    }
    Err(_) => {
        // Underrun — write silence, increment counter
        data.fill(0.0);
        UNDERRUN_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}
```

### Key Properties
- **Wait-free** SPSC — O(1) push and pop, never block
- **Fixed capacity** — no allocation after creation
- **Bulk operations**: `write_chunk()`, `read_chunk()` for efficient batch transfer
- **Real-time safe**: Suitable for audio callback threads
- **Power-of-2 capacity**: Required for performance (enables bitmask wrap)

### Capacity Sizing

Buffer duration = capacity / sample_rate / channels

At 48 kHz stereo f32:
- 65536 samples → ~683 ms buffer (~256 KB)
- 32768 samples → ~341 ms buffer (~128 KB)
- 16384 samples → ~170 ms buffer (~64 KB)

**Recommended**: 65536 samples. Provides ~680ms headroom at 48kHz stereo — enough for decoding latency spikes during gapless transitions while staying under 256 KB.

The ring buffer must be sized to accommodate the 2-second preload trigger window: at 48kHz stereo, 2 seconds = 192,000 samples. The staging prebuffer holds ~0.5s of decoded PCM (~48,000 samples at 48kHz) before pushing to the ring buffer.

---

## Stage 4: Output — CPAL

### Crate

```toml
cpal = "0.15"
```

### Device Enumeration and Configuration

```rust
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SampleRate, StreamConfig, BufferSize};

// For bit-perfect: use ALSA host directly, not PulseAudio/PipeWire
let host = cpal::host_from_id(cpal::HostId::Alsa)?;
let device = host.output_devices()?
    .find(|d| d.name().map_or(false, |n| n.contains("hw:")))
    .or_else(|| host.default_output_device())?;

// Source-tracked config
let source_sample_rate = 44100u32;
let config = device.supported_output_configs()?
    .find(|c| c.min_sample_rate().0 <= source_sample_rate
            && c.max_sample_rate().0 >= source_sample_rate)
    .ok_or_else(|| anyhow!("Device does not support sample rate"))?
    .with_sample_rate(SampleRate(source_sample_rate));

let stream_config = StreamConfig {
    channels: config.channels(),
    sample_rate: config.sample_rate(),
    buffer_size: BufferSize::Fixed(1024),
};
```

### Output Stream — Real-Time Callback

```rust
use rtrb::Consumer;
use std::sync::atomic::{AtomicU64, Ordering};

static UNDERRUN_COUNT: AtomicU64 = AtomicU64::new(0);

let stream = device.build_output_stream::<f32>(
    &stream_config,
    move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
        match consumer.read_chunk(data.len()) {
            Ok(chunk) => {
                data.copy_from_slice(&chunk);
                chunk.commit_all();
            }
            Err(_) => {
                data.fill(0.0);
                UNDERRUN_COUNT.fetch_add(1, Ordering::Relaxed);
                warn!("Audio underrun (total: {})",
                      UNDERRUN_COUNT.load(Ordering::Relaxed));
            }
        }
    },
    move |err| {
        error!(error = %err, "Audio output stream error");
    },
    None,
)?;

stream.play()?;
```

### Bit-Perfect Configuration

Requirements for bit-perfect output:
1. **Exact format match**: Source sample rate, bit depth, channel count = device configuration
2. **Bypass system mixer**: Use ALSA `hw:` device (Linux), WASAPI exclusive mode (Windows)
3. **No DSP in application**: No volume scaling, EQ, or resampling in the signal path

```rust
fn find_bit_perfect_config(
    device: &cpal::Device,
    source_sample_rate: u32,
    source_channels: u16,
    source_format: SampleFormat,
) -> Option<StreamConfig> {
    for range in device.supported_output_configs().ok()? {
        if range.channels() != source_channels { continue; }
        if range.sample_format() != source_format { continue; }
        let sr = SampleRate(source_sample_rate);
        if range.min_sample_rate() <= sr && range.max_sample_rate() >= sr {
            return Some(StreamConfig {
                channels: source_channels,
                sample_rate: sr,
                buffer_size: BufferSize::Default,
            });
        }
    }
    None
}
```

#### Platform-Specific Bit-Perfect Notes

| Platform | Approach | Notes |
|---|---|---|
| **Linux (ALSA)** | Use `hw:` device names via `cpal` with `HostId::Alsa` | Bypasses PulseAudio/PipeWire; user needs `audio` group |
| **Linux (PipeWire)** | Use `pro-audio` profile, set quantum to match | Requires PipeWire config changes; still goes through graph |
| **Linux (PulseAudio)** | PulseAudio resamples — avoid for bit-perfect | Use ALSA `hw:` devices instead |
| **Windows (WASAPI)** | CPAL only supports shared mode natively | Use `wasapi` crate directly for exclusive mode |
| **macOS (CoreAudio)** | Hog mode | CPAL does not expose this natively; use `coreaudio-rs` directly |

**Recommendation for Linux**: Expose a preference allowing users to select between ALSA `default` (convenience) and ALSA `hw:0,0` (bit-perfect direct hardware).

### Buffer Size Selection

- **1024-2048 frames**: Stable gapless playback on desktop (recommended: 1024)
- **256-512 frames**: Low-latency monitoring (higher underrun risk during transitions)
- Fixed buffer size recommended for deterministic latency

---

## Stage 5: Metadata — Lofty

### Crate

```toml
lofty = { version = "0.20", default-features = false, features = [
    "mp3", "flac", "ogg", "aac", "wav",
] }
```

### Role in Pipeline

Parse audio file metadata during library scanning (separate from the playback pipeline). Runs in a `tokio::task::spawn_blocking` to avoid blocking the async runtime.

```rust
use lofty::prelude::*;
use lofty::read_from_path;

fn parse_metadata(path: &str) -> Result<Metadata> {
    let tagged_file = read_from_path(path)?;
    let tag = tagged_file.primary_tag()
        .or_else(|| tagged_file.first_tag());
    let properties = tagged_file.properties();

    Ok(Metadata {
        title: tag.and_then(|t| t.title()).unwrap_or("Unknown"),
        artist: tag.and_then(|t| t.artist()).unwrap_or("Unknown"),
        album: tag.and_then(|t| t.album()).unwrap_or("Unknown"),
        genre: tag.and_then(|t| t.genre()),
        track: tag.and_then(|t| t.track()),
        track_total: tag.and_then(|t| t.track_total()),
        disc: tag.and_then(|t| t.disk()),
        duration: properties.duration().as_secs_f64(),
        sample_rate: properties.sample_rate(),
        bit_depth: properties.bit_depth(),
        channels: properties.channels().unwrap_or(0),
        bitrate: properties.audio_bitrate(),
    })
}
```

### Cover Art Extraction

```rust
fn extract_cover_art(tagged_file: &TaggedFile) -> Option<Vec<u8>> {
    let tag = tagged_file.primary_tag()
        .or_else(|| tagged_file.first_tag())?;
    tag.pictures().iter()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .map(|p| p.data().to_vec())
}
```

### Scanning Performance

- Lofty is synchronous and file-system bound
- Run in `spawn_blocking` during library scan
- Cache metadata in SQLite to avoid re-parsing on subsequent launches
- Cache cover art as separate files in XDG cache directory

---

## Gapless Transition Algorithm

This is the core of the pipeline — zero audible gap between tracks.

### State Machine

```
                      ┌──────────────────────────────────┐
                      │           IDLE                   │
                      │  No track loaded, buffer empty    │
                      └──────────┬───────────────────────┘
                                 │ play(track)
                                 ▼
                      ┌──────────────────────────────────┐
                      │       DECODING (current)          │
                      │  Decoder fills ring buffer         │
                      │  via rubato (if needed)            │
                      └──────────┬───────────────────────┘
                                 │ track N near end (< 2s remaining)
                                 ▼
                      ┌──────────────────────────────────┐
                      │    DECODING + PRELOADING          │
                      │  Track N: still decoding to buffer │
                      │  Track N+1: staging decoder        │
                      │  pre-buffers PCM                   │
                      └──────────┬───────────────────────┘
                                 │ track N stream complete
                                 ▼
                      ┌──────────────────────────────────┐
                      │   TRANSITION (gapless handoff)    │
                      │  Swap staging → active decoder     │
                      │  Reconfigure resampler if rate     │
                      │  change detected                   │
                      │  No silence inserted               │
                      └──────────┬───────────────────────┘
                                 │
                                 ▼
                      ┌──────────────────────────────────┐
                      │   DECODING (current = N+1)        │
                      │  Continue filling ring buffer      │
                      └──────────────────────────────────┘
```

### Detailed Flow

1. **DECODING stage** — Single decoder fills ring buffer through rubato (if resampling needed)

2. **PRELOADING stage** (~2 seconds before current track ends):
   - Determine remaining playback time: `ring_buffer_occupancy / (sample_rate * channels)`
   - When remaining < 2s, create second decoder for Track N+1
   - Start decoding Track N+1 into a staging prebuffer (~0.5s of PCM)
   - The 2-second window accounts for: file open (~10ms), probe+init (~50ms), initial decode (~500ms), safety margin (~1400ms)

3. **TRANSITION stage**:
   - Flush remaining Track N samples from resampler
   - Stop pushing Track N data to ring buffer
   - If sample rate changes: create new resampler with new input rate (same output rate)
   - Start pushing Track N+1 prebuffered data to ring buffer
   - CPAL stream never pauses → zero gap

4. **Sample Rate Change**:
   - If Track N+1's sample rate differs from Track N:
     - Create new `Fft::<f64>` or `Async::<f64>` with new input rate
     - Output ring buffer drain is handled seamlessly — new resampler output feeds same buffer
   - No CPAL stream restart → no audible gap

```rust
struct GaplessEngine {
    output_buffer: (rtrb::Producer<f32>, rtrb::Consumer<f32>),
    active_decoder: Option<DecoderState>,
    staging_decoder: Option<DecoderState>,
    resampler: Option<Box<dyn Resampler<f64>>>,
    output_rate: u32,
    current_rate: u32,
}

struct DecoderState {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    pcm_buffer: Vec<f32>,
    is_exhausted: bool,
}

impl GaplessEngine {
    fn preload_next(&mut self, next_track: &TrackRef) -> Result<()> {
        let decoder = self.create_decoder(next_track)?;
        self.staging_decoder = Some(decoder);
        while self.staging_decoder.as_ref().unwrap().pcm_buffer.len() < PRELOAD_THRESHOLD {
            self.decode_chunk(self.staging_decoder.as_mut().unwrap())?;
        }
        Ok(())
    }

    fn transition_gapless(&mut self) {
        if let Some(ref staging) = self.staging_decoder {
            if staging.codec_params.sample_rate != self.current_rate {
                self.reconfigure_resampler(staging.codec_params.sample_rate);
            }
        }
        self.active_decoder = self.staging_decoder.take();
        self.flush_active();
    }
}
```

### Comparison with Alternatives

| Approach | Quality | Complexity | Use Case |
|---|---|---|---|
| **Gapless (this design)** | Perfect — zero audible gap | High | True gapless playback |
| **Crossfade** | Alters audio | Low | DJ-style transitions |
| **Fade-out/Fade-in** | Short silence | Medium | Broadcast |

---

## Error Recovery & Diagnostics

### Underrun Detection

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static UNDERRUN_COUNT: AtomicU64 = AtomicU64::new(0);

// In CPAL callback:
if consumer.read_chunk(data.len()).is_err() {
    data.fill(0.0);
    let count = UNDERRUN_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if count % 100 == 0 {
        warn!("Audio underrun count: {}", count);
    }
}
```

### Device Hotplug

- Poll `host.devices()` every 2 seconds on a background `tokio` task
- If current device disappears → pause playback, show `AdwToast`
- If preferred device reappears → attempt resume with new stream
- Track device identity by name (not handle) across hotplug events

### Decode Error Recovery

```rust
match decoder.decode(&packet) {
    Ok(buf) => { /* process */ }
    Err(Error::DecodeError(e)) => {
        warn!(error = %e, "Skipping corrupt packet");
        continue;
    }
    Err(Error::IoError(e)) => {
        error!(error = %e, "I/O error during decode - skipping track");
        transition_gapless();
    }
    Err(Error::ResetRequired) => {
        decoder.reset();
        continue;
    }
    Err(e) => {
        error!(error = %e, "Fatal decode error");
        transition_gapless();
    }
}
```

---

## Summary: Recommended Crate Versions and Configuration

| Component | Crate | Version | Feature Flags | Purpose |
|---|---|---|---|---|
| Decoder | `symphonia` | `0.5` | `mp3, aac, flac, vorbis, opus, pcm, alac, isomp4, ogg` | Audio decoding |
| Symphonia Core | `symphonia-core` | `0.5` | (default) | Types for custom MediaSource |
| Ring Buffer | `rtrb` | `0.3` | (default) | Lock-free SPSC for audio data |
| Resampler | `rubato` | `0.14` | (default) | High-quality Sinc resampling |
| Output | `cpal` | `0.15` | `alsa` for Linux bit-perfect | Cross-platform audio output |
| Metadata | `lofty` | `0.20` | `mp3, flac, ogg, aac, wav` | Tag parsing during library scan |

### Pipeline Configuration

```
Ring buffer capacity: 65536 samples (interleaved f32)
Resampler chunk:      1024 frames
Resampler precision:  f64 (full double-precision)
CPAL buffer:          Fixed(1024) frames
Sample format:        f32 (internal pipeline), native for bit-perfect path
Resampler type:       Fft<f64> for fixed-rate, Async<f64> for dynamic-rate
Thread model:         2 threads (decoder+resampler, CPAL callback)
Memory (buffers):     ~256 KB (single ring buffer)
```

### Performance Targets

| Metric | Target |
|---|---|
| Decode → output latency | < 500ms (configurable buffer size) |
| Track transition gap | 0 samples (true gapless) |
| Resampler CPU usage | < 5% on reference hardware @ 48kHz |
| Ring buffer underrun | < 0.1% of callbacks |
| Memory (audio buffers) | ~256 KB (single ring buffer) |

---

## Key Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Decoder thread type | `std::thread` (not tokio) | Symphonia is synchronous; blocking I/O must not run on async runtime |
| Audio callback | CPAL data callback | Real-time thread, no allocations, no locks |
| Inter-thread communication | Single `rtrb` SPSC ring buffer | Lock-free, wait-free, pre-allocated, single sync point |
| Sample format in pipeline | `f32` interleaved | Universal, all decoders produce it, CPAL supports natively |
| Gapless mechanism | Dual decoder (active + staging) | Pre-decode next track while current plays, seamless buffer swap |
| Gapless trim | `AudioDecoderOptions { gapless: true }` | Automatic handling per codec; manual delay/padding as fallback |
| Resampling | `rubato Fft::<f64>` (synchronous) | Highest quality for fixed-ratio; bypassed when rates match |
| Resampling precision | `f64` | Full double-precision accumulation for bit-transparent Sinc |
| Decode buffer | Pre-allocated `Vec<f32>` with `copy_to_slice_interleaved` | Zero allocation after initial resize |
| Bit-perfect on Linux | ALSA `hw:` device + matching sample rate | Bypasses PulseAudio/PipeWire resampling |
| Metadata | `lofty` during scan, cached in SQLite | Avoid re-parsing on launch, fast library loading |
| MediaSource | `PrefetchSource` wrapping `File` | Reduces syscall overhead during decode |
| Error feedback | `tracing` + `AdwToast` | Structured logs for developers, toast for users |
| Underrun tracking | `AtomicU64` counter | Zero-overhead diagnostic on hot path |

---

## Official Documentation References

| Resource | URL |
|---|---|
| Symphonia Getting Started | https://github.com/pdeljanov/symphonia/blob/main/GETTING_STARTED.md |
| Symphonia Docs | https://docs.rs/symphonia |
| CPAL Docs | https://docs.rs/cpal |
| CPAL GitHub | https://github.com/RustAudio/cpal |
| RTRB Docs | https://docs.rs/rtrb |
| Rubato Docs | https://docs.rs/rubato |
| Rubato GitHub | https://github.com/HEnquist/rubato |
| Lofty Docs | https://docs.rs/lofty |
| Lofty GitHub | https://github.com/Serial-ATA/lofty-rs |
