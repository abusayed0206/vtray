# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Everyday Windows 10/11 users who download installers, tools and scripts and want a quick, trustworthy second opinion before running them, without having to understand security tooling. Some already know VirusTotal; most do not.

## Product Purpose

VT Tray is a small, always-on Windows tray app that checks new programs against VirusTotal by hash. It watches folders the user chooses, fingerprints new executables locally, looks up only the SHA-256, and shows VirusTotal's advisory verdict. Unknown files can be uploaded, but only after the user confirms that specific file. The website (vtray.sayed.app) exists to explain this clearly and get people to download it.

## Positioning

Consent first: VT Tray never uploads a file silently. Monitoring and quick checks send only fingerprints; an upload always needs a per-file confirmation that shows the name, size, fingerprint and a community-sharing warning, with No as the default.

## Operating Context

- Lives in the Windows notification area; right-click tray menu for checks, folders, settings, quota and API key status.
- Explorer integration under Show more options: "Check with VT Tray" (hash-only, never uploads) and "Upload with VT Tray..." (always asks first).
- Users bring their own free VirusTotal API key, stored in Windows Credential Manager.
- Distributed as a per-user Inno Setup installer (no admin rights) and a portable exe via GitHub Releases.

## Capabilities and Constraints

- Free VirusTotal API limits enforced by one persistent quota gate: 4/minute, 500/day, 15.5K/month, 100/day reserved for manual checks, warnings before limits.
- Never deletes, quarantines, blocks or modifies files; results are advisory reputation data, not an antivirus verdict.
- No telemetry, no accounts, no servers of its own; only file names (never folder paths) are sent with uploads.
- Uninstall removes startup and Explorer entries, the saved key, settings, cache and logs.
- Windows only, English UI.

## Brand Commitments

- Name: VT Tray (program file VTray.exe). Everything else visual is open.
- Do not mention abusayed.dev on the website.
- Not affiliated with VirusTotal or Google; say so.

## Evidence on Hand

- Real app screenshots: `docs/screenshots/` (tray menu, settings submenu, API key dialog, quick-check result, removal confirmation, installer wizard). The upload-consent and result dialogs have no screenshots yet; do not fake them.
- Measured: 0.4 MB working set and about 2 MB private memory, 0% CPU while idle (memprobe, checked in CI).
- Release v1.0.0 on GitHub (installer + portable), MIT license, CI build badge.
- No testimonials, user counts or press exist; do not invent any.

## Product Principles

1. Consent is the product: never imply automatic uploads; make the confirmation step visible and central.
2. Honest and advisory: VirusTotal is one reputation signal; never promise protection.
3. Invisible cost: tiny, quiet, and respectful of the user's machine and attention.
4. Plain language for non-experts: explain hashes and VirusTotal without jargon.
