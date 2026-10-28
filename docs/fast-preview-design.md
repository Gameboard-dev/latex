# Fast preview: design notes

This is the design record of the fast preview. What it does as built is described in
[Builds and fast preview](builds-and-preview.md); the code is `core/src/preview.rs`, `core/src/pipeline/fast.rs` and
`core/src/pipeline/keys.rs`.

Goal: a one-word edit in a chapter note shows in the PDF beside it in under a second, while the full build follows in
the background.

## Measurements (prototype)

Measured on a 53-page document.

| Build | Time from save to PDF |
|---|---|
| latexmk, with the plugin's word counter | ~12 s |
| One LuaLaTeX run, no latexmk | ~10 s |
| One run with TikZ pictures cached | ~8 s |
| One run with cached TikZ and a precompiled preamble | ~6 s (the floor: typesetting 53 pages takes ~4.5 s) |
| **Edited section in a waiting LuaLaTeX process** | **median 0.20 s, at most 0.76 s, over 48 sections** |

In 40 of 48 sections the preview matched the full build line for line on every page except the last. The mismatches
were floats placed across a section boundary.

## How it works

1. **The waiting process.** A LuaLaTeX process starts with the preamble and the last full build's `.aux`, `.bbl` and
   `.gls`. It waits, printing `FPREADY`.
2. **Save.** On save, the plugin converts the note. From the note-line to tex-line map it finds the edited section and
   sends `<tex stem> <line>` to the process. The process typesets that section and the next one, then writes
   `previewsec.pdf` and `previewsec.synctex.gz`.
3. **Respawn.** A new waiting process starts after each preview, after each full build (fresh `.aux`), and after any
   preamble or glossary change.
4. **The full build.** It still runs once the notes are quiet: a direct LuaLaTeX run for a prose-only change, more
   steps when citations, glossary entries or the `.bib` changed. Pygments runs only when a code block changed.

## Requirements

1. **New citations, labels and glossary entries must not show as undefined.**
   - The converter knows each chapter's citation keys, labels and glossary keys. On save it compares them with the
     last full build: the keys in `main.bcf`, the labels in `main.aux`, the entries in `main.glo`.
   - **A new key in the edited section** is handled for that section only:
     - **New citation:** biber runs on the preview's data, and the preview uses the new `.bbl`.
     - **New glossary entry:** makeglossaries runs the same way, so the preview uses the new `.gls`.
     - **New label:** a label that is both defined and referenced within the previewed sections resolves through a
       second pass of the same preview.
   - This adds about 1–2 s to that preview, not a full build.
   - **Elsewhere in the document:** a full build starts at once instead of waiting for the quiet period if the new key
     affects other sections (a label referenced elsewhere, or numbering that shifts).
2. **Edits that make the generated lists stale: do only the work each change needs.**
   - The background build still runs after the quiet period.
   - Only renumbered citations make it start at once.
   - When the bibliography is renumbered, the plugin runs biber *before* LaTeX: it edits the last build's `main.bcf` to
     the new citation order and runs biber on it. One LuaLaTeX run then gives correct numbers everywhere, about 7 s
     instead of ~15 s for latexmk's run, biber, and a second run.

   | What changed | Typeset again in the preview | Background full build |
   |---|---|---|
   | Prose only | The edited section | After the quiet period |
   | A citation added that is cited after all the others (no renumbering) | The edited section and the bibliography | After the quiet period |
   | A citation added or removed that shifts the numbers (a `numeric, sorting=none` style, so every later [n] moves) | The edited section and the bibliography | At once, one LaTeX run (biber already ran); until it lands, the status bar says "citation numbers updating" |
   | A glossary entry's description | The glossary list only | After the quiet period |
   | A glossary entry's printed form (short or long) | The glossary list and the sections that use that term | After the quiet period |
   | A term used for the first time, or its last use removed | The edited section and the glossary list, with makeglossaries on the preview's data; the list's page numbers are approximate until the full build | After the quiet period |
   | A heading or caption | The edited section and the contents, list of figures or list of tables | After the quiet period |

   **Links stay correct.** Glossary links, citation links and cross-references in the spliced pages point at named
   destinations. The splice rewrites those links to the matching pages of the shown PDF and keeps the PDF's name tree
   consistent.
3. **The PDF shown always has the bibliography and glossary.**
   - The PDF pane never shows a preview fragment on its own. The plugin splices the preview's pages into the last full
     `main.pdf`, replacing the pages of the edited sections, and shows that document.
   - The splice is pure Rust (`core/src/pdf.rs`, `core/src/splice.rs`).
   - Everything outside the edited sections, the bibliography and glossary included, comes from the previous build's
     output, and so does the SyncTeX outside them.
   - If the edit changes the section's page count, later page numbers are approximate until the full build replaces
     the whole PDF.
4. **Debounce, then queue.**
   - Each save resets a countdown (setting **Preview delay**, default 300 ms). The preview starts only after that long
     without another save.
   - If a preview is already running, only the newest request waits. Older ones are dropped, never queued one by one.
   - The full build keeps its own quiet period.

## Decisions

- **Word counts.** The plugin's own counter (Rust) replaced a LuaLaTeX counter first; the fast preview builds on it.
- **TikZ caching.** Externalisation's list-and-make mode: LaTeX lists the changed pictures and the plugin compiles
  them. There is no `-shell-escape`.
- **Precompiled preamble.** Used, and never stale.
  - **The key.** The format is keyed by a hash of everything that goes into it:
    - the preamble text, the generated `0-preamble-static.tex` and `main-fmt.tex`;
    - the engine version (`lualatex --version`);
    - the modification times and sizes of the package files the format loaded, read from the `.fls` recorder file of
      the format run.
  - **Before every build** the key is checked; if it differs, the format is rebuilt first (~3 s).
  - **If the format build fails, or a build with it fails,** the build reruns without the format and the log says
    why. The PDF is never left broken by a stale or bad format.

## Code

- **`pipeline/fast.rs`, `pipeline/keys.rs`:** generators for the preview driver, the record hooks and the TikZ
  externalisation; the full-build steps chosen from what changed; the new-key detection; parsing of the preview's
  `FPRANGE`, `FPPLACE` and `FPWARN` lines.
- **`compile.rs`:** a second slot, the preview worker, separate from the full build so that neither cancels the other;
  debounce and latest-only queue; the respawn rules above.
- **`scroll.rs`, `synctex.rs`:** the spliced PDF is shown while it is newer than `main.pdf`, with SyncTeX from the
  preview inside the spliced pages and from `main.synctex.gz` outside them.
- **`pdf.rs`, `splice.rs`:** replace the pages of the edited sections in `main.pdf`.
