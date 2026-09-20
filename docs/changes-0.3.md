# ReShiki 0.3.0

The app is now **ReShiki (リシキ)**, with its manual at [reshiki.com](https://reshiki.com/) and source at [Ameyanagi/ReShiki](https://github.com/Ameyanagi/ReShiki).

- New drawings use `.reshiki`; existing `.moruno` drawings still open.
- Templates, assistant preferences, and recovery drafts are copied from the previous installation on first launch. Existing ReShiki settings are kept, and the original files are untouched.
- Native and text clipboard data from the previous app remain editable.
- Executables and packages use `reshiki`. Environment overrides use `RESHIKI_*`; previous `MORUNO_*` overrides remain fallbacks.

Supported packages: Apple Silicon macOS, Windows x64/ARM64, and Linux x64/ARM64. Install uv before opening the app.
