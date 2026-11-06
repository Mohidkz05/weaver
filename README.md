# Weaver

Weaver sends your computer's sound and your microphone into Discord as one mic signal, so friends on a call hear both your voice and the sound effects or music you play.

It copies computer sound with WASAPI loopback (nothing is rerouted, so your own audio keeps playing normally), mixes it with your real mic, and plays the result into [VB-CABLE](https://vb-audio.com/Cable/), a free virtual audio cable. Discord then uses "CABLE Output" as its mic.

> Work in progress: this is a terminal app for now. A window with knobs, Discord echo exclusion, a limiter, and automatic mic switching are planned.

## Setup

1. Install [VB-CABLE](https://vb-audio.com/Cable/) (run the installer as administrator, then reboot).
2. In Windows Sound settings, make sure your headset or speakers are still the **default playback device**. The installer sometimes changes it to the cable.
3. Install [Rust](https://rustup.rs/) with the MSVC toolchain.
4. In Discord, open **Settings → Voice & Video** and set **Input Device** to **CABLE Output (VB-Audio Virtual Cable)**. Leave Output Device on your headset.

## Running

```powershell
cargo run --release
```

Weaver uses Windows' default mic. To choose a different one, pass part of its name:

```powershell
cargo run --release -- devices          # list audio devices
cargo run --release -- --mic realtek    # use the mic whose name contains "realtek"
cargo run --release -- --help
```

### Controls

| Key | Does |
|---|---|
| `c` | Computer sound on/off |
| `[` / `]` | Computer volume down/up (2 dB steps) |
| `m` | Mic on/off |
| `-` / `=` | Mic volume down/up (2 dB steps) |
| `q` | Quit |

Computer sound starts at -6 dB, because sound effects are often as loud as audio can go and would drown out your voice.

### Reading the display

- **buf**: how much audio is waiting in that source's buffer. About 20 ms is normal.
- **gaps** (mic): times the mic ran dry, each heard as a tiny dropout. This should stay at 0.
- **pauses** (computer): times computer sound stopped. This goes up whenever a sound ends, which is normal.
- **skip**: audio dropped to stop the delay creeping up. Rare.
- **lost**: audio dropped because the output stalled. This should stay at 0.
- **clipped**: time the mix went past full scale and was clamped, heard as crackle. If it climbs, turn something down.

## Discord settings that matter

- **Noise Suppression**: set it to **None**. Krisp treats music and sound effects as noise and removes them.
- **Echo Cancellation**: leave it **on** for now. Weaver doesn't exclude Discord's own audio yet, so friends' voices go back into the mix. Echo cancellation removes most of that.
- **Input Sensitivity**: if quiet sounds get cut off, turn off automatic sensitivity and lower the threshold.
- **Automatic Gain Control**: turn it off if the music level keeps pumping up and down.

## Known issues

- Friends may hear a faint echo of themselves, because Discord's audio is included in the computer sound. Discord exclusion is planned.
- On some laptops the built-in Realtek mic array drops about 100 ms of audio whenever a sound starts or stops on another device. This happens in the driver, outside Weaver. A headset mic doesn't have this problem.
- If a device is unplugged while Weaver runs, it stops with an error. Run it again.
