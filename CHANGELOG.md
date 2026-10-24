# Changelog

All notable changes to Phone Audio are documented here.

## v0.5.2 (2026-10-24)

- Persist the last window size while the app is running (debounced), restore it on launch.
- Show the app version at the bottom of the Settings menu.
- Fix a stale `dist` binary masking the window-size feature.

## v0.5.1 (2026-10-24)

- Remember the window size across restarts.
- Replace the first-close dialog with a themed modal (icon, backdrop, primary/secondary actions).

## v0.5.0 (2026-10-24)

- System tray icon with Show / Quit menu.
- Close-to-tray with a first-close prompt; the choice can be changed in Settings.
- Windows only; other platforms keep normal window close.

## v0.4.0 (2026-10-24)

- Start-on-login support:
  - Windows: current-user `HKCU\...\Run` entry (no administrator rights).
  - Linux: `~/.config/autostart/phone-audio.desktop`.
- `--enable-autostart` / `--disable-autostart` CLI flags.

## v0.3.8 (2026-10-24)

- Tighten the gap between the two right-hand cards.
- Align both columns to the same bottom edge.

## v0.3.7 (2026-10-24)

- Single-row device bar with a compact connection badge and action button.
- Compact source card; remove the redundant playback hint.
- Fixed-height columns so the two sides always end on the same line.

## v0.3.6 (2026-10-24)

- Replace the Tools & Diagnostics card with an Audio Source card.
- Move diagnostics into the log-panel "Diagnostics" menu.
- Full-width buffer slider, hidden content scrollbar.

## v0.3.5 (2026-10-24)

- Return to the compact v0.3.3 layout and polish column alignment.

## v0.3.4 (2026-10-24)

- Experimental fixed-height two-column layout. Replaced in v0.3.5.

## v0.3.3 (2026-10-24)

- Remove the extra header separator.
- Put the connection badge next to the Device title.
- Align both columns to the same bottom edge.

## v0.3.2 (2026-10-24)

- Merge the device picker and stream action into one card.
- Add Follow system / Dark / Light theme selection (follow system by default).
- Rename the auto-start switch and verify it only starts streaming.

## v0.3.1 (2026-10-24)

- Automatic wireless recovery:
  - discover `_adb-tls-connect._tcp` with `adb mdns services`;
  - match the old serial by host;
  - persist the refreshed `ip:port`.

## v0.3.0 (2026-10-24)

- New card-based UI on eframe/egui 0.37.
- Dark/light theme, status pill, log coloring, responsive single-column layout.

## v0.2.x

- Windows output-device patch for scrcpy (`--audio-output-device`,
  `--list-audio-devices`), packaging scripts, and the initial audio-only
  streaming workflow.
