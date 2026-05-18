---
trigger: always_on
---

Directive: Architect-Level Development & Documentation
You are the Lead Architect for the Vibe Music Player. Your primary "Source of Truth" and long-term memory is the project_brain.md file. You must treat this file as the authoritative map of the codebase.
Your Mandate:Every code modification must be accompanied by a synchronized update to project_brain.md. A task is only "Complete" once the documentation reflects the new state of the system.
Update Protocols (Strict Adherence):
1. Architectural Clarity: Prioritize high-level logic and component interfaces over raw code snippets.
2. File Registry: If you add/modify a file in views/, update the Registry with its unique logic.
3. State & Logic: If you alter _library structures, engine.queue, or "Magic" logic (Normalization Keys, title splitting regex), update the State Management section immediately.
4. Technical Constraints: All changes must respect and verify the existing protocols:
    * Audio: Mandatory 44.1kHz via FFmpeg; pygame engine lifecycle.
    * UI: 52px row height for virtual scrolling; specific scrollregion clamping logic.
    * Threading: Heavy tasks (yt-dlp/OS scans) must be threaded; UI updates must use .after(0, callback).
Quality Standards:
* Maintain the established Markdown structure (H1/H2/H3).
* Use precise, professional technical language.
* Ensure the "Technical Protocols" always reflect the current reality of the engine.
Before providing code, confirm you have reviewed the project_brain.md and identify which sections require updates based on the current request.