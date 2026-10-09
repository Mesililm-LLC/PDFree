# Validation boundaries

The optimized portable executable was launched successfully. Its own renderer snapshots were inspected at 125% Windows scaling before and after page navigation/zoom. These show correctly placed PDF text, readable light-theme controls, and clipped zoomed output. Automated schedules execute real navigation/zoom callbacks; native integration tests cover replacement, malformed-file recovery, encryption detection, clipping, transparency, embedded fonts, and 1,000-page documents.

The following human interaction checks remain **unverified**, and must be completed before calling this a qualified viewer release. Automated callbacks are not evidence of physical pointer/keyboard behavior. This environment has no enabled native desktop interaction surface.

| Check | Procedure | Status |
|---|---|---|
| Native picker | Ctrl+O, choose PDF; cancel and reopen picker | Pending manual check |
| Continuous scrolling | Wheel/drag scrollbar rapidly through mixed.pdf and thousand.pdf | Pending manual check |
| Page entry | Type a distant page, Enter; edit the number without it being overwritten | Pending manual check |
| Keyboard | PgUp/PgDn, arrows, Home/End, Ctrl +/−, Ctrl+0 | Pending manual check |
| Zoom and fit | 25–400%, fit width/page while scrolled within a page | Automated callbacks/captures; physical interaction pending |
| Resize/DPI | Resize continuously, move between 100%, 125%, and 150% displays | Geometry tests and 125% capture; mixed-monitor interaction pending |
| Reopen/recovery | Open malformed, encrypted, then valid PDF through picker | Engine tested; full picker path pending |
| Rapid switching | Replace an in-flight vector/scan document repeatedly | Engine tested; full picker path pending |
| Fidelity | Compare independent PDFs with Edge/Sumatra at identical scale | PDF rendering tested; broad visual comparison pending |

No PDF was modified. No file associations, telemetry, cloud services, or remote repository were configured.
