# Productivity Tracker

Local-only macOS menu-bar app that tracks what you are doing (by frontmost app + browser tab title) and shows a daily checklist based on the day of the week.

**Not a Chrome extension** — runs as its own lightweight app so it does not add RAM pressure to Chrome.

## Features

- Automatic activity tracking (app name + window/tab title)
- Idle detection (threshold configurable in Settings, default 5 minutes)
- Categories for Zendesk, Netflix, Brave/Chrome, WhatsApp, Windows App, Notes, ChatGPT, Cursor, Parsec, AnyDesk, iPhone Mirroring, Calendar, Calculator
- Day-of-week task templates (Sun–Sat), each day enable/disable in Settings
- Example Mon–Fri checklists (edit them in Settings)
- Ad-hoc tasks with notes / ticket links
- 100% local SQLite storage (no account, no sync)

## Install

Build it yourself (see below). On first launch, grant **Accessibility** in System Settings › Privacy & Security so browser tab titles can be read.

## Develop

Requirements: Node 20+, Rust (rustup), Xcode CLT.

```bash
npm install
npm run tauri:dev
```

Build release:

```bash
npm run tauri:build
```

If cargo fails with mysterious “can't find crate” errors on macOS beta, rebuild with:

```bash
cd src-tauri && CARGO_BUILD_JOBS=1 cargo build --release
npm run tauri:build
```

## Project layout

- `src/` — React UI (Today + Settings)
- `src-tauri/` — Rust backend (SQLite, activity collector, tray)

## Privacy

All data stays in your Mac app data directory. Nothing is uploaded.
