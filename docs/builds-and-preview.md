# Builds, fast preview and the PDF pane

How the plugin decides what to run after a save, how the fast preview puts a change into the PDF within about a second,
and how the PDF pane follows the notes. The design record of the fast preview is
[fast-preview-design.md](fast-preview-design.md).

## Contents

- [Builds](#builds)
- [Fast preview](#fast-preview)
- [The PDF beside the notes](#the-pdf-beside-the-notes)
- [Sync scrolling](#sync-scrolling)
- [Tests](#tests)

## Builds

After a save, the note is converted at once. What follows depends on the settings:

- **Fast preview off, or another engine than LuaLaTeX:** the full build starts after the **Quiet period** (0.3 s after
  the last save).
- **Fast preview on:** the preview shows the change first. When it leaves every page current, the PDF is already
  right, and the full build, which only confirms it, waits until the notes have been idle for **Full build when idle**
  (60 s). The status bar then says `PDF current · full build when idle`. When a preview leaves pages to update
  (numbering, citation numbers, page numbers), the full build starts after the quiet period.

A save during a build cancels it (**Cancel a build still running**) and starts the next. After a cancelled build, the
main file's auxiliary files (`.aux`, `.bcf`, `.bbl`, `.toc` ...) are deleted, since a run stopped part-way may leave them
half-written; the PDF is kept.

### Only what changed

With **Build only what changed** on (the default), a full build runs only the steps the change since the last full
build needs. The plugin compares what LaTeX reads with what it read then, and what each LuaLaTeX run writes (`.bcf`,
`.glo`, `.aux`, `.toc`, `.lof`, `.lot`, `.out`) with what the run before wrote.

| Change | What runs |
| --- | --- |
| none (an edit undone) | nothing: the last full build's PDF is shown again |
| prose | one LuaLaTeX run |
| a citation key cited for the first time or no longer cited; a change in the order of first citation; the `.bib` exported again | LuaLaTeX, biber, LuaLaTeX |
| a glossary term used for the first time, no longer used, or moved to another page; the glossary note | LuaLaTeX, makeglossaries, LuaLaTeX |
| a label added, removed or renumbered (heading, figure, table, equation, listing, labelled list item); the contents, list of figures or tables, or the PDF bookmarks | LuaLaTeX twice (up to four times while numbers still change) |
| the preamble or the main file; the first build | latexmk with the whole recipe |

Citations missing from the `.bib` and undefined references do not trigger another run; they stay undefined until they
are added. **Compile now** and **Build all chapters and compile** always run the whole recipe. With the option off,
latexmk runs every time.

This applies to the plugin's own build, which it runs while fast preview is on. With fast preview off, or with an
engine other than LuaLaTeX, every full build runs the **Compile command**.

Each build's line in the log ends with its steps and why each ran. **Debug log** adds each build's decisions, commands
and step times.

## Fast preview

With **Fast preview** on (LuaLaTeX only), a save shows in the PDF within about a second, and the full build follows.

- **The waiting job.** After each full build, a LuaLaTeX job starts on a precompiled preamble, reads the build's `.aux`,
  `.bbl` and `.gls`, and waits. On a save (once no save has come for the **Preview delay**; only the newest save waits
  while a preview runs) it typesets the sections changed since the last full build, each from the place and state where
  the full build had it. The PDF shown is the last full build's, with those pages in place of the old ones, so the
  bibliography, the glossary and every other page are always there.
- **What is typeset again.**

  | Change | Typeset again |
  | --- | --- |
  | Prose | the section (or the chapter, with **Preview scope** set to chapter) |
  | A citation added after all the others | the section and the bibliography |
  | A citation that renumbers the others | biber first, on a copy of the `.bcf` in the new order; then the section and the bibliography; the full build starts at once |
  | A glossary description | the glossary list |
  | A glossary entry's printed form | the glossary list and the sections that use it |
  | A term's first or last use | the section and the glossary list, after makeglossaries |
  | A heading or caption | the section and the contents, list of figures or list of tables |
  | A label both defined and used in the preview | a second pass |

- **Current or stale.** A region is current when the preview ends it in exactly the state the full build was in there
  (the page, the page builder's totals, every counter that numbers something). Otherwise the pages after it say at
  their top what is updating (page numbers, numbering, citation numbers, layout), the status bar says so, and a shift
  in numbering starts the full build at once. A region is typeset on through up to four more sections and ends at the
  first where it meets the full build's state again; if none does, its last page is kept and the full build's page
  after it shows only what follows the region, so no text is shown twice. Renumbered citations mark every page stale.
- **Links.** The spliced PDF's named destinations are rebuilt so every link lands where the full build's would.
- **The precompiled preamble** (`fp-preamble.fmt`, made with the `mylatexformat` package) is keyed by the preamble, the
  generated `0-preamble-static.tex` and `main-fmt.tex`, the engine's version, and the time and size of every file its
  run read. It is rebuilt before a build whenever the key changes. If it cannot be built, or a build on it fails where
  one without it works, builds and previews run without it until the key changes, and the log says why. A format works
  with Type 1 fonts (`fontenc` T1, `lmodern`); with OpenType fonts and `microtype`, a build on it can fail, and the
  plugin falls back.
- **TikZ pictures** (**Cache TikZ pictures**) are cached as PDFs in `tc/` with TikZ externalisation's list-and-make
  mode: LaTeX lists them, the plugin makes those that changed, with no shell escape. The plugin keeps a copy of the key
  each PDF was made from (`tc/<name>.made`) and remakes every picture whose key (`tc/<name>.md5`) differs. Pictures are
  named by their place in the chapter, so adding or removing one remakes those after it.
- **Files** (in the output folder): `main-fmt.tex` (the main file on the format, which the full build compiles),
  `0-preamble-static.tex`, `main-body.tex`, `fp-record.*`, `fp-tikz.tex`, `fp-preview.*` (the job), `fp-full.*` (the
  last full build's files), `fp-cite.*`, `fp-gls.*`.

## The PDF beside the notes

With **Show the PDF beside the notes** on, opening a chapter note, or a successful build, opens the compiled PDF in a
pane on the right unless a pane already shows it. The focus stays in the note, and there is never a second PDF pane. The
pane shows `main.shown.pdf`, a copy written once per finished build or preview, so it redraws once rather than after
each LaTeX pass. The output folder must be in the vault.

## Sync scrolling

With **Sync scrolling with the PDF** on, scrolling a chapter note scrolls the PDF to the same place, and scrolling the
PDF with the pointer over it scrolls the note. A newly opened PDF opens at the place of the note you are in, and after
each build it returns to it.

- **Which notes.** Only a chapter note takes part: a note of the notes folder whose `.tex` the last build typeset. Only
  the pane showing the main PDF takes part.
- **Which pane leads.** The editor you scroll (each editor on its own), or the PDF while the pointer is over it. The
  pane the plugin scrolls ignores its own scrolling for 150 ms, so the two never chase each other. A place in the PDF
  whose note is not open does nothing; if several panes show the note, the one used last moves.
- **How.** Each note line maps to the `.tex` line where its part begins (the conversion is line by line), and SyncTeX
  says where each `.tex` line was typeset. The line at the top of the editor goes to the top of the PDF pane, and
  within a long part (a paragraph, a table) the place moves through it as you scroll. On a 53-page document, 98% of
  places in the notes map back to themselves through the PDF; the rest are floats printed away from their text and the
  ends of chapters.
- **Limits.**
  - Places come from the last build, so text written since is placed approximately until the next one.
  - The compile command must write SyncTeX (`-synctex=1`, in the default).
  - It needs the editing view (Live Preview or Source mode); Reading view does not lead.
  - It uses Obsidian's own PDF viewer (pdf.js), which is not part of Obsidian's API. If an Obsidian version changes
    it, the PDF only jumps to the right page and no longer leads.
  - When the SyncTeX file is missing or unreadable, or the chapter notes are not in it, the log says so once per build
    and nothing is synced.

## Tests

`core/tests/synctex.rs` checks the note-to-PDF lookups on a real LuaLaTeX build (`core/tests/sync`) against
`synctex view`.

The fast preview's correctness suite runs the preview steps on real TeX for 15 scenarios (prose; citations added, moved
and removed; references to the appendix; headings and captions; the glossary; page-count changes; floats at section
boundaries; a preamble change and the format's fallback). It compares the PDF shown with a clean latexmk build of the
same sources: the text of every current page, citation numbers, the bibliography, the contents and lists, the glossary
list, every link's destination, the name tree and SyncTeX; then again after the background build lands. It needs
LuaLaTeX, latexmk, biber, makeglossaries and Python with PyMuPDF:

```
FAST_TEX=<scratch folder> cargo test --release -p latex-core --test fast_preview -- --nocapture
```

It writes `<scratch folder>/report.md`. The fixture is `core/tests/fast/fixture` (made by `make_fixture.py`), the checks
`core/tests/fast/check.py`.
