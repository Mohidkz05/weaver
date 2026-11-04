# Weaver

Weaver is a Windows desktop app that sends computer sound and the mic into Discord as one mic signal, so sound effects can be played on calls. It has a toggle for computer sound and mixer-style knobs.

At the start of every session, read `docs/PERSONAL.md` and `docs/PROGRESS.md` if they exist, and follow them. They're ignored by Git, so a fresh clone won't have them.

## Technical direction

| Part | Choice | Why |
|---|---|---|
| Language | Rust | Live audio needs predictable timing, with no garbage collection pauses |
| Computer sound | WASAPI process loopback, excluding Discord | Copies the sound without rerouting it, and prevents friends hearing themselves echoed |
| Virtual mic | VB-CABLE | Writing a signed Windows audio driver isn't practical for this project |
| Discord input | Make VB-CABLE the Windows default mic, with Discord on Default | Avoids changing the mic in Discord each time |
| Default mic switching | `IPolicyConfig`, restoring on close and checking on startup | No official API exists, so it needs a clear fallback if Windows breaks it |
| Audio quality | Limiter, matched or properly converted sample rates, stable buffer size | Avoids clipping, crackling, and glitches |
| Interface | Tauri | Knobs and toggle built with web technology on top of the Rust code |

Rejected approaches: an APO on the real mic (built to process the mic's own signal and tied to driver internals), and hooking into Discord (fragile, and may break Discord's rules).

## Building

Weaver must be built and run on Windows, because WSL can't directly access Windows audio devices.
