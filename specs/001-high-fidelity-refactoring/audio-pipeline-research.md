# Audio Pipeline Research: High-Fidelity Gapless Bit-Perfect Playback

**Date**: 2026-05-22
**Sources**: Google Search, Context7 MCP, crate docs (cpal, symphonia, rtrb, rubato, audioadapter-buffers)

---

## Pipeline Architecture

```
┌──────────────┐    ┌─────────────┐    ┌──────────────┐    ┌──────────────┐
│  Decoder     │    │  Ring       │    │  Resampler   │    │  Ring        │
│  (symphonia) │───►│  Buffer     │───►│  (rubato)    │───►│  Buffer      │───►  CPAL
│  [tokio]     │    │  (rtrb)     │    │  [tokio]     │    │  (rtrb)      │      callback
└──────────────┘    └─────────────┘    └──────────────┘    └──────────────┘
       │                                                         │
       │  pre-buffer next track                                   │  lock-free, never
       │  ~1s before end                                          │  allocate, never block
       ▼                                                         ▼
  Gapless transition                                       Sample format
  logic (trim delay/                                      conversion ONLY if
  padding)                                                 device requires it
```

Three threads/tasks:
1. **Decoder thread** — reads file, decodes via symphonia, pushes PCM frames to ring buffer
2. **Resampler thread** — pulls from ring buffer, resamples if needed, pushes to output ring buffer
3. **CPAL callback thread** — pulls from output ring buffer, fills device buffer (real-time constrained)

---

## Stage 1: Decoding — Symphonia

### Crate

```toml
symphonia = { version = "0.5", default-features = false, features = ["mp3", "aac", "flac", "vorbis", "opus", "pcm", "alac"] }
```

### Format Support

| Format | Container | Gapless Support |
|--------|-----------|-----------------|
| FLAC | Native FLAC | ✅ Full |
| MP3 | MPEG | ✅ (uses encoder delay/padding) |
| AAC | MP4/M4A | ✅ |
| Ogg Vorbis | OGG | ✅ Full |
| Opus | OGG | ✅ Full |
| WAV | WAV | ✅ |
| AIFF | AIFF | ✅ |

### Key API

```rust
use symphonia::core::audio::{AudioBufferRef, GenericAudioBufferRef};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

// 1. Open and probe
let src = std::fs::File::open(path).unwrap();
let mss = MediaSourceStream::new(Box::new(src), Default::default());
let mut hint = Hint::new();
hint.with_extension("flac");
let meta_opts: MetadataOptions = Default::default();
let fmt_opts: FormatOptions = Default::default();
let mut format = symphonia::default::get_probe()
    .probe(&hint, mss, fmt_opts, meta_opts)
    .expect("unsupported format");

// 2. Select audio track
let track = format.default_track(TrackType::Audio).expect("no audio track");
let track_id = track.id;

// 3. Get codec parameters — contains sample rate, channels, gapless info
let codec_params = track.codec_params.as_ref()
    .expect("codec parameters missing")
    .audio()
    .expect("not audio");

// Gapless: inspect delay/padding
let delay = codec_params.delay;     // priming samples (encoder delay)
let padding = codec_params.padding; // padding samples at end

let sample_rate = codec_params.sample_rate;
let channels = codec_params.channels as usize;

// 4. Create decoder
let dec_opts: AudioDecoderOptions = Default::default();
let mut decoder = symphonia::default::get_codecs()
    .make_audio_decoder(codec_params, &dec_opts)
    .expect("unsupported codec");

// 5. Decode loop
loop {
    let packet = match format.next_packet() {
        Ok(Some(p)) => p,
        Ok(None) => break,  // end of stream
        Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
        Err(e) => panic!("{e}"),
    };
    if packet.track_id() != track_id { continue; }
    match decoder.decode(&packet) {
        Ok(decoded) => {
            // Convert to interleaved f32 for ring buffer
            let mut out: Vec<f32> = Vec::new();
            decoded.copy_to_vec_interleaved(&mut out);
            // Push `out` to ring buffer
        }
        Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
        Err(e) => panic!("{e}"),
    }
}
```

### Gapless Handling

Symphonia provides `codec_params.delay` and `codec_params.padding`:
- **Delay** (priming samples): Number of initial samples to discard. These are encoder-generated silence/fade-in artifacts.
- **Padding**: Number of trailing samples to discard.

**Implementation**: On the first decoded packet of a track, skip `delay` samples. On the last packet, truncate `padding` samples. The next track's decoded data should be queued immediately to the same ring buffer — the CPAL stream never stops.

```rust
// Track N decoding nears completion
// → Start decoding Track N+1 in parallel
// → When Track N's last sample enters ring buffer,
//   Track N+1's first sample (after delay trim) follows immediately
// → CPAL stream never underruns → gapless
```

### Symphonia Buffer Types

| Buffer Format | Access Method | Use Case |
|---|---|---|
| `AudioBufferRef::F32(buf)` | `buf.planes()`, `buf.copy_to_vec_interleaved()` | Universal processing |
| `AudioBufferRef::S16(buf)` | Same API | Legacy/exact formats |
| `AudioBufferRef::U8(buf)` | Same API | Low-bitrate |
| `AudioBufferRef::S24(buf)` | Same API | 24-bit files |
| `AudioBufferRef::U24(buf)` | Same API | 24-bit files |

**Recommendation**: Convert everything to `f32` on decode for uniform pipeline processing. If bit-perfect path is active (device matches source format exactly), use the native format to avoid any conversion.

---

## Stage 2: Lock-Free Ring Buffer — rtrb

### Crate

```toml
rtrb = "0.2"
```

### API

```rust
use rtrb::{RingBuffer, PushError, PopError};

// Create with capacity (in frames — each frame is channels * sizeof(f32))
let (mut producer, mut consumer) = RingBuffer::<f32>::new(65536);

// Producer (decoder thread) — wait-free
producer.push(0.5_f32).unwrap();
producer.write_slice(&samples);  // bulk write when available

// Consumer (resampler or CPAL thread) — wait-free
match consumer.pop() {
    Ok(sample) => /* use sample */,
    Err(PopError::Empty) => /* buffer underrun */,
}
```

### Key Properties
- **Wait-free** SPSC — both push and pop are O(1) and never block
- **Fixed capacity** — no allocation after creation
- **Bulk operations**: `write_slice()`, `read_slice()` for efficient multi-sample transfer
- **Real-time safe**: Suitable for audio callback threads
- No `std` dependency requirement (needs `alloc`)

### Capacity Sizing

```
Buffer duration = capacity / sample_rate / channels

At 48 kHz stereo f32:
  capacity = 65536 samples → ~683 ms buffer
  capacity = 32768 samples → ~341 ms buffer
  capacity = 16384 samples → ~170 ms buffer
```

**Recommended**: 65536 frames (interleaved) gives ~680ms at 48kHz stereo — enough headroom for decoding latency spikes while keeping memory under 256KB per buffer.

---

## Stage 3: Resampling — Rubato

### Crate

```toml
rubato = "0.14"
audioadapter-buffers = "0.2"
```

### Resampler Strategy: Synchronous

For a music player where the output sample rate is known and fixed per session, use the **synchronous FFT resampler** (`rubato::Fft`). It provides the highest quality (Sinc interpolation) with acceptable latency for gapless playback.

```rust
use rubato::{Resampler, Fft, FixedSync};
use audioadapter_buffers::direct::InterleavedSlice;

// Create FFT resampler
// Parameters: input_rate, output_rate, chunk_size, input_channels, output_channels, sync_mode
let mut resampler = Fft::<f64>::new(
    44100,    // input sample rate
    48000,    // output sample rate
    1024,     // chunk size (frames per process call)
    2,        // input channels
    2,        // output channels
    FixedSync::Both,  // both input and output sizes are fixed
).unwrap();

// Input buffer (interleaved f64)
let input_frames = resampler.input_frames_next();
let mut input = vec![0.0_f64; 2 * input_frames];  // stereo
let input_adapter = InterleavedSlice::new(&input, 2, input_frames).unwrap();

// Output buffer (pre-allocated)
let output_frames = resampler.output_frames_next();  // = 1024 typically
let mut output = vec![0.0_f64; 2 * output_frames];
let mut output_adapter = InterleavedSlice::new_mut(&mut output, 2, output_frames).unwrap();

// Process
let (frames_read, frames_written) = resampler
    .process_into_buffer(&input_adapter, &mut output_adapter, None)
    .unwrap();
```

### Resampler Strategy: Asynchronous / Polynomial

For dynamic sample rate changes (e.g., quickly transitioning between 44.1kHz and 96kHz tracks), use the asynchronous resampler:

```rust
use rubato::{Async, FixedAsync, PolynomialDegree, Resampler};

let mut resampler = Async::<f64>::new_poly(
    48000.0 / 44100.0,       // resample ratio
    2.0,                      // max relative ratio
    PolynomialDegree::Septic, // highest quality polynomial
    1024,                     // chunk size
    2,                        // channels
    FixedAsync::Output,       // fixed output size
).unwrap();

// Can dynamically change ratio for different tracks
resampler.set_resample_ratio_relative(96000.0 / 44100.0, false).unwrap();
```

### Quality Tiers

| Resampler Type | Quality | Speed | Use Case |
|---|---|---|---|
| `Fft::new()` — Sinc | Highest | Moderate | High-fidelity (default) |
| `Async::new_poly()` — Septic (deg 7) | High | Fast | Dynamic ratio changes |
| `Async::new_poly()` — Cubic | Medium | Very fast | Non-critical |
| `Async::new_poly()` — Linear | Low | Fastest | Preview/low-power |

**Recommendation**: Use `Fft::new()` (synchronous Sinc) when the output sample rate stays constant for a session. Use `Async::new_poly(Septic)` when tracks dynamically change the resampling ratio.

### Buffer Adapter Types

| Adapter | Layout | Description |
|---|---|---|
| `InterleavedSlice` | `L R L R L R` | Standard interleaved audio |
| `InterleavedSliceMut` | `L R L R L R` | Mutable interleaved for output |
| `SequentialSlice` | `L L L R R R` | Planar audio per channel |
| `SequentialSliceOfVecs` | `vec[L], vec[R]` | Vec-per-channel (legacy) |

All adapters are **zero-copy** — they are just structural views over existing memory.

### Float Precision

```rust
// f64 — higher precision, recommended for FFT resampling
Fft::<f64>::new(...)

// f32 — faster, less precision
Fft::<f32>::new(...)
```

**Recommendation**: Use `f64` for the resampling stage (max quality), then convert to output format. The CPAL callback can accept `f32` (which will be converted to the device's native format).

---

## Stage 4: Output — CPAL

### Crate

```toml
cpal = "0.15"
```

### Device Enumeration and Configuration

```rust
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SampleRate, StreamConfig, BufferSize, SupportedStreamConfig};

let host = cpal::default_host();
let device = host.default_output_device().expect("no output device");

// Query supported configurations
let supported_configs: Vec<SupportedStreamConfig> = device
    .supported_output_configs()
    .expect("failed to query configs")
    .filter_map(|range| range.try_with_sample_rate(SampleRate(44100)))
    .collect();

// Find best matching config
let config = device
    .supported_output_configs()?
    .find_map(|r| r.try_with_sample_rate(SampleRate(44100)))
    .expect("device doesn't support 44.1kHz");

// Build stream config
let stream_config = StreamConfig {
    channels: config.channels(),
    sample_rate: config.sample_rate(),
    buffer_size: BufferSize::Fixed(1024),  // Use fixed buffer for consistency
};
```

### Output Stream with Ring Buffer

```rust
use rtrb::Consumer;

// Real-time callback — NEVER block, NEVER allocate
let stream = device.build_output_stream::<f32>(
    stream_config,
    move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
        // Fill buffer from ring buffer consumer
        for sample in data.iter_mut() {
            *sample = match consumer.pop() {
                Ok(s) => s,
                Err(PopError::Empty) => 0.0,  // underrun protection
            };
        }
    },
    move |err| {
        error!(error = %err, "Audio output stream error");
    },
    None, // no timeout
).unwrap();

stream.play().unwrap();
// Stream stays alive for the application's lifetime
```

### Bit-Perfect Configuration

Bit-perfect means the digital audio signal reaches the DAC unaltered. Requirements:

1. **Exact format match**: Source sample rate, bit depth, channel count = device configuration
2. **Bypass system mixer**: Use exclusive mode or direct hardware access
3. **No DSP in application**: No volume scaling, EQ, or resampling in the signal path

```rust
// Bit-perfect: find exact match for source parameters
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
| **Linux (ALSA)** | Use `hw:` device names via `cpal` | Bypasses PulseAudio/PipeWire; user needs `audio` group. Use `cpal` features `alsa` (not `pulseaudio` or `pipewire`). |
| **Linux (PipeWire)** | Use `pipewire` feature, set quantum to match | Good low-level access but still goes through PipeWire graph. |
| **Linux (PulseAudio)** | PulseAudio typically resamples — avoid for bit-perfect | Use ALSA `hw:` devices instead. |
| **Windows (WASAPI)** | Exclusive mode | CPAL supports `Exclusive` config on WASAPI. Set `StreamConfig` to match device native format exactly. |
| **macOS (CoreAudio)** | Hog mode | Harder to achieve; CPAL doesn't have explicit hog mode API. |

**Recommendation for Linux**: Expose a preference allowing users to select between ALSA `default` (through PipeWire/PulseAudio for convenience) and ALSA `hw:0,0` (bit-perfect direct hardware).

### CPAL Buffer Size

```rust
// Fixed size for deterministic latency
stream_config.buffer_size = BufferSize::Fixed(1024);

// Or let the system decide (may vary by device)
stream_config.buffer_size = BufferSize::Default;
```

---

## Gapless Transition Algorithm

This is the core of the pipeline — the mechanism that ensures zero audible gap between tracks.

### State Machine

```
                  ┌────────────────────────────────────┐
                  │                                    │
                  ▼                                    │
┌────────┐   decode   ┌──────────┐  last packet   ┌────────┐
│ IDLE   │◄──────────►│ PLAYING  │───────────────►│ DRAIN  │
└────────┘            └──────────┘                └────────┘
                          │                            │
                    next_track()                  drained
                          │                            │
                          ▼                            ▼
                    ┌──────────┐  pre-buffer      ┌───────────┐
                    │ PRE-BUFF │─────────────────►│ TRANSITION│
                    └──────────┘   next track      └───────────┘
                                                         │
                                                    next track
                                                    first sample
                                                         │
                                                         ▼
                                                    ┌──────────┐
                                                    │ PLAYING  │
                                                    └──────────┘
```

### Detailed Flow

1. **PRE-BUFF stage** (~1 second before current track ends):
   - Start decoding Track N+1
   - Trim `delay` samples from Track N+1 beginning
   - Hold decoded frames in a separate pre-buffer

2. **TRANSITION stage** (current track's last samples processed):
   - Flush remaining Track N samples from resampler
   - Stop pushing Track N data to output ring buffer
   - If sample rate changes:
     - Reconfigure resampler with new input rate
     - Keep output rate constant (device rate)
   - Start pushing Track N+1 trimmed data to output ring buffer
   - The CPAL stream never pauses → zero gap

3. **Sample Rate Change**:
   - If Track N+1's sample rate differs from Track N:
     - Create new resampler instance with new input rate (same output rate)
     - The output ring buffer is drained before flip
     - New resampler output feeds the same output ring buffer
   - No CPAL stream restart → no audible gap

```rust
/// State for gapless transition manager
struct GaplessState {
    output_rate: u32,           // fixed device output rate
    current_rate: u32,          // current track's sample rate
    resampler: Option<Box<dyn Resampler<f64>>>,
    pre_buffer: Vec<f32>,       // pre-buffered next track frames
    is_transitioning: bool,
}
```

### Comparison with Basic Crossfade

| Approach | Quality | Complexity | Use Case |
|---|---|---|---|
| **Gapless (this design)** | Perfect — no audible gap | High | True gapless playback (FLAC, etc.) |
| **Crossfade** | Alters audio | Low | DJ-style transitions, radio |
| **Fade-out/Fade-in** | Short silence | Medium | Broadcast/voice-over |

**This pipeline implements gapless only** (no crossfade). Crossfade can be added as a post-processing option on top.

---

## Summary: Recommended Crate Versions and Configuration

| Component | Crate | Version | Feature Flags | Purpose |
|---|---|---|---|---|
| Decoder | `symphonia` | `0.5` | `mp3, aac, flac, vorbis, opus, pcm, alac` | Audio decoding |
| Ring Buffer | `rtrb` | `0.2` | (default) | Lock-free SPSC for audio data |
| Resampler | `rubato` | `0.14` | (default) | High-quality Sinc resampling |
| Buffer Adapter | `audioadapter-buffers` | `0.2` | (default) | Zero-copy adapters for rubato |
| Output | `cpal` | `0.15` | (default + `alsa` for Linux) | Cross-platform audio output |

### Pipeline Configuration

```
Buffer capacities: 65536 frames each (interleaved)
Resampler chunk:   1024 frames
CPAL buffer:       Fixed(1024) samples
Sample format:     f32 (internal), convert to device format at output
Resampler type:    Fft<f64> for fixed-rate, Async<f64> for dynamic-rate
Thread model:      3 dedicated OS threads (decode, resample, output callback)
```

### Performance Targets

| Metric | Target |
|---|---|
| Decode → output latency | < 500ms (configurable buffer size) |
| Track transition gap | 0 samples (true gapless) |
| Resampler CPU usage | < 5% on modern hardware @ 48kHz |
| Ring buffer underrun | < 0.1% of callbacks |
| Memory (audio buffers) | ~512 KB total (2 ring buffers) |
