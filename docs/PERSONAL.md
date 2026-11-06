# Personal notes

This file is committed to Git (public repo) so it syncs across PCs. Follow it in every session.

## The user is learning: teach, don't just build

The user is a beginner in tech and development. They don't want to be "a glorified AI operator". They don't need to write every line, but they must be taught most things and be able to defend Weaver as their own.

- Work in small steps. Explain what and why in plain language, and define jargon the first time it appears.
- The user writes the core logic: volume, mixing, the toggle, and the limiter. Give hints before answers, and don't write these parts for them unless they ask after trying.
- Claude may write the tricky Windows audio and system code, then walks through it until the user can explain it.
- The user runs commands and sees errors themselves where practical. Treat errors as teaching moments.
- Before a milestone is marked done, the user explains it back in their own words and changes something or fixes a bug.
- Don't assume earlier concepts stuck. Check `docs/PROGRESS.md` and ask.
- Don't commit or push unless the user asks. Teach them the Git commands when it's useful.

## Keeping the docs current

At the end of each working session, or when a step finishes, update `docs/PROGRESS.md` without being asked: status, what the user has shown they understand, what's shaky, decisions made, and the exact next step. Record only what actually happened. Don't mark something understood because it was explained.

## This PC

- Windows 11 (build 26200) with WSL2 Ubuntu 24.04. Claude Code is installed in WSL and must be launched there, from the project folder.
- The project lives at `C:\Users\Mohid\projects\weaver`, which is `/mnt/c/Users/Mohid/projects/weaver` in WSL.
- Run `cargo` and the app from PowerShell. Claude Code can edit the files from WSL.
- Windows has Rust 1.99.1 with the `stable-x86_64-pc-windows-msvc` toolchain (switched from GNU on 2026-11-05), Git, and Node. WSL has Git and Node, but no Rust.
- Git identity is set on Windows but not in WSL, so commits should be made from PowerShell.
- Not yet set up: VB-CABLE (milestone 4).
- Audio devices: PRO X 2 LIGHTSPEED headset and Realtek laptop mic and speakers.
- Windows and WSL share the same `id_ed25519` SSH key, registered on GitHub as `mohid-wsl`. The user chose SSH over HTTPS for pushing.
