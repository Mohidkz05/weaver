# Weaver progress

Last updated: 2026-11-05

## Where things stand

**Current milestone:** 2, a list of audio devices (just starting).

**Next step:** The user deletes the `?` on line 28 of `src/main.rs`, runs `cargo build`, reads the error, and guesses what `?` does. Then put it back and walk through the rest of the code until they can explain it. Milestone 2 isn't done until they've explained it back and changed something. The user still hasn't said what changed in `Cargo.toml` or what the version number is for.

**Left off:** The user paused on 2026-11-05 before reporting the `?` experiment. Check whether the `?` on line 28 is still there. `Cargo.toml`, `Cargo.lock`, and `src/main.rs` have uncommitted changes, and the user hasn't asked to commit them.

## Milestones

| # | What gets built | What it teaches | Status |
|---|---|---|---|
| 1 | A program that runs | Terminal, Git, and Rust basics | Done (explain-back skipped, see Understanding) |
| 2 | A list of audio devices | Libraries, functions, and handling errors | In progress |
| 3 | A live volume meter for computer sound | What digital audio is (just numbers) | Not started |
| 4 | Computer sound sent to a virtual mic, first Discord test | Threads and buffers | Not started |
| 5 | Mic and computer sound mixed, with a toggle | Mixing, and why loud audio clips | Not started |
| 6 | Discord excluded, limiter added | Echo and audio quality | Not started |
| 7 | The real app with knobs | Interfaces with Tauri | Not started |
| 8 | Automatic default mic switching | Using Windows system features and recovering from crashes | Not started |

## What the user has done

- 2026-11-04: Cloned the repo into WSL at `~/projects/weaver` on their own, using `cd`, `ls`, and `git clone`.
- 2026-11-04: Moved the project to the Windows drive using `mkdir -p`, `mv`, and `cd`.
- 2026-11-04: Ran `cargo init` and `cargo run`, saw `Hello, world!`, and asked what `src` and `target` are.
- 2026-11-04: Changed the program's message, rebuilt it, staged and committed from PowerShell with their own commit message, added `/docs` to `.gitignore` themselves, and switched the remote to SSH.
- 2026-11-05: Pushed `52d6d51` to GitHub from PowerShell on the first try (`9e988a9..52d6d51  main -> main`). Asked to move on to milestone 2.
- 2026-11-05: Ran `cargo add windows` successfully. `cargo build` failed with `error calling dlltool 'dlltool.exe': program not found` on the GNU toolchain.
- 2026-11-05: Installed the MSVC build tools and switched Rust to MSVC. `cargo build` then succeeded.
- 2026-11-05: Added the extra crate features after a pasted command got cut off, then ran the device lister with `cargo run`. It listed 3 outputs and 2 mics. Claude wrote this code, and the user hasn't explained it yet.

## Understanding

Only list something here once the user has shown they understand it.

**Solid:** Source code only changes the compiled program after the compiler rebuilds it. The user reasoned this out without running the experiment (2026-11-04). They also used `.gitignore` correctly on their own to keep `docs` out of Git.

**Said they know, not shown:** The difference between `git commit` and `git push`. On 2026-11-05 the user said they already know it and asked to move on, so the explain-back was skipped at their request. Why `docs` and `target` aren't on GitHub also went unanswered, though their own `.gitignore` edit points to understanding.

**Introduced but not confirmed:** What Rust is, virtual mics, WASAPI loopback, the echo problem, the rest of the Cargo project structure (`Cargo.toml`, `Cargo.lock`), Git staging, SSH key pairs, and LF vs CRLF. These were explained in conversation, but the user hasn't explained them back.

## Open questions

- **Is the push failure really the outage?** On 2026-11-04, SSH was refused intermittently with `Permission denied (publickey)`: 3 of 8 `git push --dry-run` attempts from Windows worked, using the correct key. GitHub had a major incident open covering Git Operations, started 09:49 UTC, which fits but doesn't name this symptom. One WSL refusal came before the incident was posted, and it isn't known whether the key was linked then. On 2026-11-05 the first push attempt worked, which fits the outage explanation but one success doesn't prove it. If pushes fail intermittently again, investigate further. The user was skeptical of the outage explanation, so show evidence.
- **Why did Voicemeeter sound terrible?** The user hasn't said what it sounded like or which connection their headset used. The PRO X 2 appears as a LIGHTSPEED device, so Bluetooth's low-quality mode probably isn't the cause if they use the dongle. Revisit this when audio first goes through Discord.
- **Git identity in WSL:** `user.name` and `user.email` aren't set in WSL, so commits from there will fail until they are. They are set on Windows.

## Decision log

- 2026-11-04: Named the app Weaver.
- 2026-11-04: Chose loopback capture over Voicemeeter-style rerouting, so a crash doesn't take down all computer audio. The default mic would still need restoring after a crash.
- 2026-11-04: Chose to make the virtual mic the Windows default, so Discord can stay on Default.
- 2026-11-04: Agreed the teaching approach, now in `docs/PERSONAL.md`.
- 2026-11-04: The repo is public, so the user ignored `/docs` in Git and chose to move personal details out of `CLAUDE.md` into `docs/PERSONAL.md`. These notes have no GitHub backup.
- 2026-11-05: Chose to switch to the MSVC toolchain now rather than at milestone 7, because the GNU toolchain is missing `dlltool` and MSVC was already planned for Tauri.
- 2026-11-04: Moved the project to `C:\Users\Mohid\projects\weaver`, because Rust is on Windows and the app must run there.
