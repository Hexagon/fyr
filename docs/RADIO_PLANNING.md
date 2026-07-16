# RADIO_PLANNING.md — RTL-SDR Integration Roadmap

## Overview

This document outlines the vision and planning for radio support in Offline Nexus using Software-Defined Radio (SDR) technology, specifically RTL-SDR devices.

**Current Status**: v2+ Planning (Stubs only in v0.1-v1.0)

## Vision

Enable Offline Nexus to receive, process, and display radio signals for off-grid communication and information access:

- **Emergency Communication**: Listen to radio broadcasts when cellular/internet unavailable
- **Weather Data**: Receive NOAA weather radio for forecasts, alerts
- **Emergency Frequencies**: Monitor local/regional emergency services
- **Weak Signal Reception**: Amplify and decode low-power transmissions
- **Data Decoding**: Extract text/data from radio broadcasts (SSTV, APRS, etc.)

## RTL-SDR Hardware

### Compatible Devices

| Device | Cost | Performance | Use Case |
|--------|------|-----------|----------|
| RTL-SDR v3 | $25-35 | Good | General purpose, weather, VHF/UHF |
| HackRF | $100-150 | Excellent | Advanced TX/RX, full spectrum |
| Lime SDR | $150-200 | Excellent | Wideband, research |
| Bladerf | $300+ | Professional | High performance, lab use |

**Recommended for Nexus**: RTL-SDR v3 (low cost, sufficient for most off-grid scenarios)

### Typical Setup

```
┌─────────────────────────┐
│  RTL-SDR Dongle        │
│  Frequency: 25-1700 MHz│
│  Bandwidth: 2.4 MHz    │
└──────────┬──────────────┘
           │ USB
    ┌──────▼──────────────┐
    │  Raspberry Pi       │
    │  (or any Linux PC)  │
    │                     │
    │  Offline Nexus      │
    └──────┬──────────────┘
           │ HTTP/WebSocket
    ┌──────▼──────────────┐
    │  Browser UI        │
    │  (Phone/Laptop)    │
    └────────────────────┘
```

## Architecture (v2+)

### Module Structure

```
nexus-radio/
├── src/
│   ├── lib.rs           # Module exports
│   ├── device.rs        # RTL-SDR device management
│   ├── tuner.rs         # Frequency tuning
│   ├── decoder.rs       # Signal decoders (FM, SSB, etc.)
│   ├── recorder.rs      # Audio/data recording
│   └── server.rs        # API endpoints
├── Cargo.toml
└── examples/
    └── basic_listen.rs  # Example usage
```

### Core Components

#### 1. Device Management (`device.rs`)

```rust
pub struct RadioDevice {
    device: rtlsdr::Device,
    frequency: u32,
    gain: u32,
    sample_rate: u32,
}

impl RadioDevice {
    pub fn new(device_index: u32) -> Result<Self> { ... }
    pub fn list_devices() -> Vec<RadioInfo> { ... }
    pub fn close(&mut self) { ... }
}
```

**API Endpoints**:
- `GET /api/radio/devices` — List connected RTL-SDR devices
- `POST /api/radio/connect` — Connect to device
- `POST /api/radio/disconnect` — Disconnect

#### 2. Frequency Tuning (`tuner.rs`)

```rust
pub struct FrequencyTuner {
    device: &mut RadioDevice,
}

impl FrequencyTuner {
    pub fn tune(&mut self, frequency_hz: u32) -> Result<()> { ... }
    pub fn get_signal_strength(&self) -> Result<i32> { ... }
    pub fn enable_auto_gain(&mut self) { ... }
}
```

**API Endpoints**:
- `POST /api/radio/tune` — Tune to frequency
- `GET /api/radio/signal-strength` — Get signal strength (dBm)
- `POST /api/radio/gain` — Set gain (0-29.7 dB)

#### 3. Decoders (`decoder.rs`)

```rust
pub enum DecoderType {
    FM,      // Frequency Modulation (broadcast radio)
    SSB,     // Single Sideband (amateur radio)
    CW,      // Continuous Wave (morse code)
    Digital, // FSK, PSK, others
}

pub trait Decoder {
    fn decode(&self, samples: &[f32]) -> Result<AudioData>;
}
```

**Decoder Support** (v2+):
- FM: Local radio broadcasts
- SSB: Amateur radio
- Digital: Packet radio, APRS
- Weather: NOAA weather radio (specialized demodulator)

#### 4. Recorder (`recorder.rs`)

```rust
pub struct AudioRecorder {
    output_file: PathBuf,
    format: AudioFormat, // WAV, MP3, etc.
}

impl AudioRecorder {
    pub fn start_recording(&mut self) -> Result<()> { ... }
    pub fn stop_recording(&mut self) -> Result<()> { ... }
}
```

**API Endpoints**:
- `POST /api/radio/record/start` — Begin recording
- `POST /api/radio/record/stop` — End recording
- `GET /api/radio/recordings` — List saved recordings

#### 5. API Server

Axum-based server routing:
```
POST   /api/radio/devices            → list_devices()
POST   /api/radio/connect            → connect_device()
POST   /api/radio/disconnect         → disconnect_device()
POST   /api/radio/tune               → tune_frequency()
GET    /api/radio/signal-strength    → get_signal_strength()
POST   /api/radio/gain               → set_gain()
POST   /api/radio/record/start       → start_recording()
POST   /api/radio/record/stop        → stop_recording()
GET    /api/radio/recordings         → list_recordings()
WS     /api/radio/spectrum           → spectrum_stream()
```

## Frequencies & Presets (v2+)

### Predefined Frequency Presets

```json
{
  "presets": [
    {
      "name": "NOAA Weather Radio",
      "frequencies": [162.400, 162.425, 162.450, 162.475, 162.500],
      "mode": "fm",
      "bandwidth": 25000,
      "description": "Emergency weather alerts and forecasts"
    },
    {
      "name": "AM Broadcast",
      "frequency_range": [540000, 1700000],
      "mode": "am",
      "bandwidth": 10000,
      "description": "Local AM radio stations"
    },
    {
      "name": "FM Broadcast",
      "frequency_range": [88000000, 108000000],
      "mode": "fm",
      "bandwidth": 200000,
      "description": "Local FM radio stations"
    },
    {
      "name": "Amateur Radio",
      "frequencies": [146500000, 146520000, 146940000],
      "mode": "fm",
      "bandwidth": 25000,
      "description": "2m band repeaters"
    },
    {
      "name": "Aviation",
      "frequency_range": [118000000, 137000000],
      "mode": "am",
      "bandwidth": 25000,
      "description": "Aircraft communications"
    }
  ]
}
```

### UI: Preset Buttons

```html
<div class="frequency-presets">
  <button onclick="tuneFrequency(162.4, 'NOAA')">🌦️ NOAA Weather</button>
  <button onclick="tuneFrequency(88, 'FM')">📻 FM Radio</button>
  <button onclick="tuneFrequency(146.5, 'Ham')">📡 Amateur Radio</button>
</div>
```

## Dependencies (v2+)

```toml
[dependencies]
rtlsdr = "0.2"           # RTL-SDR library
realfft = "2.0"          # FFT for spectrum analysis
dsp = "0.1"              # DSP filters
rubato = "0.12"          # Sample rate conversion
hound = "3.5"            # WAV file writing
tokio = "*"              # Async runtime (already used)
```

## UI Components (v2+)

### Radio Dashboard

```
┌──────────────────────────────────────┐
│         Radio Control Panel          │
├──────────────────────────────────────┤
│                                      │
│  Frequency: [  162.400  ] MHz        │
│  [◀]  [Play]  [◀]                    │
│                                      │
│  Signal Strength: ▓▓▓▓░░░░ -68 dBm   │
│  Gain: [═════════════════════]       │
│                                      │
│  ┌─────────────────────────────────┐ │
│  │ Spectrum Display                │ │
│  │  ▄▄▄▂▅▆▇█▇▆▅▂▄▄▄▂▅▆▇█▇▆▅▂▄▄   │ │
│  │  ▄▄▄▂▅▆▇█▇▆▅▂▄▄▄▂▅▆▇█▇▆▅▂▄▄   │ │
│  │  ▄▄▄▂▅▆▇█▇▆▅▂▄▄▄▂▅▆▇█▇▆▅▂▄▄   │ │
│  └─────────────────────────────────┘ │
│                                      │
│  Presets:                            │
│  [NOAA] [FM] [Ham] [Aviation]       │
│                                      │
│  [🔴 Record] [📁 Saved]              │
│                                      │
└──────────────────────────────────────┘
```

### Waterfall Display (Advanced)

Time-frequency spectrogram showing signal activity over time (for signal detection/analysis).

## Development Roadmap

### v0.1-v1.0 (Current)
- ✅ Project structure
- ✅ Stub module (nexus-radio)
- ✅ Documentation (this file)

### v0.2 (Phase 1: Basic Tuning)
- RTL-SDR device detection
- Basic frequency tuning API
- Signal strength reporting
- Simple spectrum display

### v0.3 (Phase 2: Audio)
- FM/SSB demodulation
- Audio output (speaker or streaming)
- Recording to WAV
- Volume control

### v1.0 (Phase 3: Enhanced)
- Multiple decoders (FM, AM, SSB, CW)
- Preset frequencies
- Waterfall display
- Advanced filtering

### v2.0+ (Phase 4: Advanced)
- APRS/packet radio decoding
- NOAA weather decoding
- Emergency alert processing
- Multi-device support
- Radio frequency database

## Testing Strategy

### Unit Tests
```bash
cargo test -p nexus-radio
```

### Integration Tests
- Connect physical RTL-SDR device
- Tune to known frequency
- Verify signal detection
- Record and verify audio quality

### Hardware Requirements for Testing
- RTL-SDR v3 dongle (~$30)
- USB cable and power
- Linux/Raspberry Pi test environment

## Dependencies & Libraries

| Crate | Purpose | Status |
|-------|---------|--------|
| `rtlsdr` | Device control | Active |
| `realfft` | FFT processing | Active |
| `rubato` | Resampling | Active |
| `hound` | WAV output | Active |
| `num-complex` | DSP math | Active |

## Security Considerations

⚠️ **Important**: Radio reception is legal in most jurisdictions, but local laws vary:
- **Monitor-only mode**: Legal in most countries
- **Transmission**: Requires license (not planned for v0.1-v2.0)
- **Private frequencies**: Some frequencies protected; monitor-only is safe

Nexus radio features:
- ✅ Receive-only (no transmission planned)
- ✅ Configurable frequency limits (can restrict to public frequencies)
- ✅ Warning labels for restricted bands

## Resources & References

### Hardware
- [RTL-SDR.com](https://www.rtlsdr.com/) — Device docs and guides
- [RTL-SDR Wiki](https://www.rtl-sdr.com/rtl-sdr-quick-start-guide/) — Setup guide

### Software
- [rtlsdr-rs](https://github.com/rtlsdr/rtlsdr-rs) — Rust bindings
- [GQRX](https://gqrx.dk/) — Open-source SDR client (reference)
- [CubicSDR](https://www.cubic.ch/en/software) — Modern SDR interface

### Demodulation
- [GNU Radio](https://www.gnuradio.org/) — Modular DSP toolkit
- [Web SDR](https://www.websdr.org/) — Online SDR demos

### Amateur Radio
- [ARRL](https://www.arrl.org/) — Amateur Radio Relay League
- [Frequency Database](https://www.radioreference.com/) — Frequency lookup

## Future Enhancements

- [ ] TX capability (requires licensed hardware + user license)
- [ ] Scanning (auto-tune through frequency list)
- [ ] Signal identification (AI-based modulation detection)
- [ ] Collaborative frequency mapping (community database)
- [ ] MQTT bridge (stream audio to other devices)

## Known Limitations (v0.1-v1.0)

- RTL-SDR only (not HackRF, LimeSDR yet)
- Receive-only (no transmission)
- Single device per server
- Basic demodulation (FM only in early versions)
- No frequency scanning

---

## Getting Started (When Ready)

1. Acquire RTL-SDR v3 dongle ($25-35)
2. Connect to Raspberry Pi via USB
3. Install librtlsdr: `sudo apt-get install librtlsdr-dev`
4. Build Nexus with radio support: `cargo build --features radio`
5. Access radio control in UI: `http://localhost:8080/#/radio`

---

**Current Version**: Planning (v2+ roadmap)
**Last Updated**: 2026-07-17

**See Also**: [ARCHITECTURE.md](./ARCHITECTURE.md), [README.md](../README.md)
