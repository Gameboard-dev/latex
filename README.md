# LaTeX

An Obsidian plugin that turns a folder of Markdown notes into a LaTeX document and keeps its PDF compiled.

## Overview

LaTeX is for people who write a long document, such as a thesis, a report or a book, as Obsidian notes and need a
typeset PDF. Each note in one folder is one part of the document: the preamble, the title page, a chapter, the glossary
or the appendix. Whenever you save a note, the plugin converts it to its `.tex` file by fixed rules and compiles the
document. The notes are the master copy. The conversion runs one way only, from note to `.tex`, so you edit the notes
and never the `.tex` files.

The plugin is written in Rust and compiled to WebAssembly; `main.js` only loads the module (in a release it is
inlined in `main.js`).

## Disclosures (Obsidian's developer policies)

Free, open source (MIT), no account, no ads and no telemetry. To compile, the plugin runs the TeX programs you installed
yourself (LuaLaTeX, pdfLaTeX or XeLaTeX, latexmk, biber, makeglossaries) and Python with Pygments for code listings:
only in desktop Obsidian, only programs on a fixed list, with argument arrays and no shell, at below-normal priority,
from commands kept on each device rather than in the synced vault. On the desktop it makes no network requests; in a
browser through Ignis it asks Ignis's build relay on the page's own address to compile. Everything it writes is in the
vault's output folder (`<notes folder>/compiled/`); outside the vault, only the TeX distribution keeps its own caches.
It never downloads, installs or updates anything; a TeX distribution may download missing packages by its own settings
(MiKTeX's "install on-the-fly"). Desktop only, because compiling needs Node's `child_process`. Each program, request
and file, and how the plugin meets Obsidian's [Developer policies](https://docs.obsidian.md/community-directory/developer-policies):
[Disclosures and compliance](docs/disclosures.md).

## Documentation

- [Conversion rules](docs/conversion.md): every rule from note to `.tex`.
- [Builds, fast preview and the PDF pane](docs/builds-and-preview.md).
- [Word counts](docs/word-counts.md).
- [Fast preview: design notes](docs/fast-preview-design.md).
- [Disclosures and compliance](docs/disclosures.md): programs run, network use, files, and Obsidian's policies.

## Features

- **Conversion on save.** Headings and labels, cross-references, Pandoc-style citations to biblatex, lists, tables,
  TikZ figures, equations, code listings, footnotes and a glossary, written line by line into `.tex` files.
- **Automatic builds.** The document is compiled shortly after each save. Each build runs only what the change needs:
  one LuaLaTeX run for prose, biber only when citations change, makeglossaries only when glossary use changes.
- **Fast preview.** With LuaLaTeX, a save shows in the PDF within about a second. The edited section is typeset by a
  waiting LuaLaTeX job and spliced into the last full PDF; the full build follows.
- **Word counts.** Counted per section by the plugin itself and written to a file the preamble reads, so the counts
  are right on the first LaTeX run.
- **PDF beside the notes.** The compiled PDF opens in a pane next to the chapter, and can scroll together with the
  note through SyncTeX.
- **Chapter ordering.** Reorder chapters in the settings, include or leave out each one, and renumber the notes with
  one command; links and `main.tex` follow.
- **Status and log.** The status bar shows what the plugin is doing; every save, conversion warning and build result
  is written to a log.
- **Building through Ignis** (optional). With the vault open in a browser through Ignis, the browser converts the
  notes and the PC compiles them.

## Requirements

- Obsidian 1.8.7 or later, the desktop app (the plugin is desktop-only, so Obsidian's mobile app does not load it). In
  a browser through Ignis, on any device, the notes are converted and Ignis's computer builds them
  ([Building through Ignis](#building-through-ignis)).
- A LaTeX distribution:

  | OS | Distribution | Link |
  | --- | --- | --- |
  | Windows, macOS, Linux | TinyTeX | <https://yihui.org/tinytex/> |
  | Windows, Linux | TeX Live | <https://tug.org/texlive/> |
  | Windows | MiKTeX | <https://miktex.org/download> |
  | macOS | MacTeX | <https://tug.org/mactex/> |

- The tools the default build uses:

  | Tool | Used for |
  | --- | --- |
  | LuaLaTeX | the default engine; required for fast preview (pdfLaTeX and XeLaTeX are also supported) |
  | latexmk | the default compile command; needs Perl |
  | biber | the bibliography (biblatex) |
  | makeglossaries | the glossary (glossaries-extra) |
  | Python with Pygments | colouring code listings (`pip install pygments`); not needed for a document without code |

Notes on the distributions:

- **TinyTeX** includes latexmk and the engines but not biber or the glossary packages, and it does not install missing
  packages by itself. Run `tlmgr install biber biblatex glossaries glossaries-extra`, then `tlmgr path add`. Install any
  other package a build reports missing with `tlmgr install <package>`; `tlmgr search --global --file "/name.sty"` finds
  the package that holds a `.sty` file.
- **MiKTeX** installs missing packages when asked. latexmk needs Perl, which MiKTeX does not include; install
  [Strawberry Perl](https://strawberryperl.com). TeX Live, MacTeX and TinyTeX include Perl on Windows and macOS.
- After installing a distribution, restart Obsidian so that it finds the new programs, or set **TeX programs folder**.
- Without latexmk, write the build steps as lines of the **Compile command** (see [Settings](#settings)).

## Installation

1. Download `main.js` (the WebAssembly module is inlined in it), `manifest.json` and `styles.css` from the
   [latest release](https://github.com/Gameboard-dev/latex/releases/latest).
2. Copy them into `<vault>/.obsidian/plugins/latex/`.
3. In Obsidian, open **Settings → Community plugins** and enable **LaTeX**.

To build the plugin from source, see [Development](#development).

## Quick start

### Vault layout

```
LaTeX/                      the notes folder (setting: Notes folder)
  0-preamble.md             the preamble: one ```latex block
  1-title_page.md           the title page: its properties
  2-introduction.md         a chapter
  3-background.md           a chapter
  glossary.md               the glossary: one # heading per entry
  appendix.md               the appendices
  compiled/                 the output folder: .tex files, log, PDF
    main.tex                written by you
    My Library.bib          your bibliography
```

- Each note becomes the `.tex` file of the same name in `compiled/` (`2-introduction.md` becomes `2-introduction.tex`).
- `main.tex` is the only file you write by hand. The plugin changes only its `\input` lines, when chapters are
  renumbered.
- The preamble, title page, glossary and appendix notes have fixed names, with or without a number; **Apply ordering**
  numbers the glossary and appendix after the last chapter (`4-glossary.md`). Every other note in the folder is a
  chapter. Subfolders and a `README` note are ignored.

### First build

1. Enable the plugin. It creates the notes folder (`LaTeX`) if it does not exist.
2. Right-click the notes folder and choose **Apply ordering**. This creates the missing fixed notes: `0-preamble.md`
   (a copy of the [example preamble](examples/0-preamble.md)), `1-title_page.md`, `glossary.md` and `appendix.md`.
3. Add a chapter note, for example `2-introduction.md`:

   ```markdown
   ---
   wordcount: true
   ---
   # Introduction
   ^sec--intro

   This chapter introduces the topic [@smith2020, p. 4].
   ```

4. Write `LaTeX/compiled/main.tex`:

   ```latex
   \documentclass[11pt,a4paper]{article}
   \input{0-preamble}
   \input{glossary}
   \begin{document}
   \input{1-title_page}
   \pagenumbering{arabic}
   \tableofcontents
   \input{2-introduction}
   \input{appendix}
   \printbibliography
   \end{document}
   ```

   The glossary is read before `\begin{document}`, because its entries are definitions. Print the list of terms with a
   `^printterms` line in a note (the appendix, for example).

5. Put your `.bib` file in `compiled/` under the name the preamble loads (`My Library.bib` in the example preamble) and
   set **Bibliography file** if it differs.
6. Run **Build all chapters and compile** from the command palette. The status bar shows the progress, and the PDF
   opens beside the note.

From then on, saving a note converts it and rebuilds the PDF. When you add or reorder chapters, run **Apply ordering**
again: it renumbers the notes and rewrites the `\input` lines of `main.tex` to match.

A new document builds the first time with the example preamble; nothing has to be run or defined by hand:

- The title page reads the properties `university`, `department`, `title`, `subtitle`, `author`, `student_id`,
  `degree`, `supervisor`, `module`, `word_count_note` and `date` of `1-title_page.md`. The placeholder note lists them
  all, empty. Every one is optional: the title page prints those that have a value and leaves out the rest, with their
  labels (no "Student ID:" without an ID). The word count prints once a chapter has `wordcount: true`. To change the
  layout, edit `\maketitlepage` in the preamble.
- Code listings need `code/pygments-style.tex`, the colours the highlighted code uses. The plugin writes it (Pygments'
  default style) the first time a note has a block of code, and never overwrites it, so you may replace it with another
  style (`pygmentize -S monokai -f latex`). The preamble reads it only if it exists, so a document without code builds
  without it, and without Python.

A preamble of your own must define the commands the conversion writes: the title page, glossary, word-count and listing
commands (see [Writing conventions](#writing-conventions)).

## Writing conventions

The full rules are in [docs/conversion.md](docs/conversion.md). The most used conventions:

| In the note | Becomes in LaTeX |
| --- | --- |
| `# Heading` to `#### Heading` | `\section` to `\paragraph` |
| `# Heading {-}` | `\section*`, still listed in the contents |
| `^sec--intro--scope` on the line under a heading | `\label{sec:intro:scope}` (`:` is written `--`, `_` is written `---`) |
| `[[3-background#Scope\|Scope]]` | `\ref` of that heading's label |
| `Table [[#^tab--results\|tab:results]]` | `Table~\ref{tab:results}` |
| `[@key]`, `[@key, p. 27]` | `\cite{key}`, `\cite[p.~27]{key}` |
| `[@a; @b]`, `[see @a, p. 2]` | `\cite{a, b}`, `\cite[see][p.~2]{a}` |
| `@key`, `@key [p. 4]` | `\citet{key}`, `\citet[p.~4]{key}` |
| `[[glossary#API\|API]]` | `\gls{API}` (`\Gls`, `\glspl` for a capitalised or plural alias) |
| `*x*`, `**x**`, `` `x` `` | `\emph{x}`, `\textbf{x}`, `\texttt{x}` |
| `^[note text]` | `\footnote{note text}` |
| `[text](https://...)` | `\href{https://...}{text}` |
| `$$ ... $$`, then a blank line and `^eq--name` | a numbered equation labelled `eq:name` (no `\label` inside the block) |
| a Markdown table, a blank line, `^tab--name`, a blank line, `((<tag\|Table)) Caption.` | a `table` float with booktabs rules, sized automatically |
| a ```` ```tikz ```` block, a blank line, `^fig--name`, a blank line, `((<tag\|Figure)) Caption.` | a `figure` float with the picture, shrunk to the line width if wider |
| a ```` ```python ```` block, a blank line, `^lst--name`, a blank line, `((<tag\|Listing)) Caption.` | a listing float; the code is highlighted by Pygments |
| `- **H1** text ^hyp1` (a list with bold labels and ids) | a labelled `enumerate`, referenced as `\hypref{1}` |
| `^FloatBarrier`, `^clearpage` on a line of its own | `\FloatBarrier`, `\clearpage` (also any argument-free command of the preamble) |
| `%% comment %%` | `% comment` |
| `%%[note] text%%` | nothing (an Obsidian-only comment) |
| callouts, links to other notes, Zotero links | nothing |

Other points:

- Anything else, such as `$x$` or `\gls{API}`, is already LaTeX and is copied unchanged. In prose, write `%` as `\%`.
- Only the preamble note holds a ```` ```latex ```` block; in another note it is copied with a warning.
- Caption lines use an arrow tag of the Tags for Markdown plugin, `((<tag|Figure))`, and must hold no other `|`.
- Whatever cannot be represented is written as well as it can be and reported in the log with its line number.

Details: [properties and the title page](docs/conversion.md#properties), [the glossary note](docs/conversion.md#the-glossary-note),
[table sizing](docs/conversion.md#tables), [code listings](docs/conversion.md#code-listings),
[word counts](docs/word-counts.md), [builds and fast preview](docs/builds-and-preview.md).

## Settings

The settings tab has a link to this README at the top.

**Folders and files**

| Setting | Default | Description |
| --- | --- | --- |
| Notes folder | `LaTeX` | The folder whose notes are the document (subfolders are not read). The output folder is always its `compiled` subfolder, and the main file is always `main.tex`. |

**Chapter ordering and inclusion**

Every note of the notes folder in the order of the PDF. Each note except the preamble and the glossary has a toggle to
include it in the PDF or leave it out. Drag a chapter, or use its arrows, to change the order; then right-click the
notes folder and choose **Apply ordering** to renumber the notes. See
[docs/conversion.md](docs/conversion.md#notes-with-fixed-names-and-chapter-order).

**Compiling**

| Setting | Default | Description |
| --- | --- | --- |
| Compile after a change | on | Build the document after each save. Desktop only. |
| Engine | LuaLaTeX | LuaLaTeX, pdfLaTeX or XeLaTeX. Chooses the default compile command; fast preview needs LuaLaTeX. |
| Compile command | latexmk with the engine, biber and makeglossaries | One command per line, run in the output folder in turn until one fails. Placeholders as in LaTeX Workshop: `%DOC%` (main file without `.tex`), `%DOCFILE%`, `%DOC_EXT%`, `%DOCFILE_EXT%`, `%DIR%`, `%OUTDIR%`. Each step must run a TeX tool (latexmk, lualatex, biber, makeglossaries ...) or `python -m pygments`. Clear the box to restore the default. |
| TeX programs folder | (empty) | A folder searched for TeX programs after the system `PATH`. Empty: the usual TinyTeX, TeX Live, MiKTeX and MacTeX folders that exist. |
| Highlight command | `python -m pygments -l %LANG% -f latex -o %OUT% %SRC%` | Turns a code block into the LaTeX its listing inputs. |
| Quiet period (seconds) | 0.3 | How long after the last save the full build is decided. |
| Below-normal priority | on | Run builds at a lower priority so Obsidian stays responsive. |
| Cancel a build still running | on | A save during a build stops it and starts the next. Off: the next build waits. |

The compile command, highlight command and TeX programs folder are stored on each device (in Obsidian's local storage),
not in the vault's `data.json`. A synced vault or a browser therefore never chooses the programs a computer runs.

**Fast preview**

| Setting | Default | Description |
| --- | --- | --- |
| Fast preview | on | LuaLaTeX only. A save shows in the PDF within about a second; the plugin then runs its own build instead of the compile command. |
| Preview scope | section | Typeset the edited section and the next one, or the whole chapter. |
| Preview delay (ms) | 300 | The preview starts once no save has come for this long. |
| Cache TikZ pictures | on | Make each picture once as a PDF, and again only when it changes. |
| Build only what changed | on | Each full build runs only the steps the change needs. Off: latexmk every time. |
| Full build when idle (seconds) | 60 | When a preview leaves every page current, the full build waits until the notes have been idle this long. 0: build after every quiet period. |
| Debug log | off | Write each build's decisions, commands and step times to the log. |
| Show the PDF beside the notes | on | Open the compiled PDF in a pane on the right when a chapter note is opened or a build succeeds. |
| Sync scrolling with the PDF | off | Scroll the chapter note and the PDF together. |
| Log | `latex-sync.log` | In the output folder. |
| Build output | `.latex-sync-build.log` | In the output folder: what the last build printed. |

**Notes and chapters**

| Setting | Default | Description |
| --- | --- | --- |
| Stamp the last change | on | Set the last-updated property to the current time whenever a note's text changes. |
| Last-updated property | `last_updated` | The property that holds the stamp. |
| Keep a .tex edited by hand | on | Copy a `.tex` changed outside its note to the backups folder before overwriting it. |
| Backups folder | `.sync-backups` | In the output folder. |

**Bibliography**

| Setting | Default | Description |
| --- | --- | --- |
| Bibliography file | `My Library.bib` | The `.bib` whose author names a narrative citation prints, for the word counts. In the output folder, or an absolute path. |

The names of the commands the conversion writes (word-count macros, listing and glossary commands, title-page prefix,
TikZ libraries, table layout limits) are keys in the plugin's `data.json`, not in the settings tab. They are listed in
[docs/conversion.md](docs/conversion.md#keys-in-datajson).

## Commands

| Command | What it does |
| --- | --- |
| Build all chapters | Convert every note in the notes folder. |
| Build all chapters and compile | Convert every note, then build with the whole recipe. |
| Compile now | Build with the whole recipe (latexmk). |
| Stop the build | Stop the running build. |
| Open the log | Open `latex-sync.log`. Clicking the status bar does the same. |
| Apply ordering | In the notes folder's right-click menu only. Create missing fixed notes, renumber the notes to the chapter order, and update `main.tex`. |

## Logs and troubleshooting

### Status bar

| Status bar | Meaning |
| --- | --- |
| `LaTeX: idle` | Watching the notes folder. |
| `LaTeX: converting 2-introduction.md → 2-introduction.tex` | A saved note is being converted. |
| `LaTeX: waiting for quiet (1 s)` | The chapters are written; the build starts after the quiet period. |
| `LaTeX: PDF current · full build when idle (60 s)` | The fast preview has put every change into the PDF; the confirming full build waits. |
| `LaTeX: compiling — lualatex (pass 2) · 14 s` | A build is running: the current tool and the time so far. |
| `LaTeX: compiled 12:04 (0 errors, 2 undefined refs)` | The last build finished. |
| `LaTeX: build failed — see log` | The last build stopped with an error. |
| `LaTeX: 3-background.md not converted — see log` | A note could not be converted; its `.tex` was left unchanged. |
| `LaTeX: converted 12:04` | Converted on a device that cannot compile, or with compiling off. |

A suffix `· 2 warnings` counts what the last conversion could not represent. Hover over the status bar for the last
build's summary and the word counts.

### Files

All in the output folder (`<notes folder>/compiled/`):

| File | Contents |
| --- | --- |
| `latex-sync.log` | Every save, conversion warning and build result, with the steps each build ran and why. Read this first. |
| `.latex-sync-build.log` | What the last build printed (latexmk's or the plugin's own build output). |
| `main.log` | LaTeX's own log. |
| `main.shown.pdf` | The PDF the pane shows: a copy of `main.pdf` written once per finished build or preview. |
| `latex-status.json` | What the desktop app's last build or preview did (state, time, errors), for the status bar of devices that do not build ([below](#following-the-pcs-builds-from-a-browser-or-a-phone)). |
| `.ignis-build.log` | The transcript of the last build through Ignis (written by Ignis Control). |
| `.sync-backups/` | Copies of `.tex` files that were edited by hand before the plugin overwrote them. |

Files whose names start with a dot are hidden in Obsidian's file explorer; open them by path.

### Common problems

| Problem | What to do |
| --- | --- |
| A TeX program is not found right after installing a distribution | Restart Obsidian, or set **TeX programs folder** to the distribution's `bin` folder. |
| A build reports a missing `.sty` file (TinyTeX) | `tlmgr install <package>`; find the package with `tlmgr search --global --file "/name.sty"`. |
| latexmk fails to start (MiKTeX) | Install Perl ([Strawberry Perl](https://strawberryperl.com)). |
| A compile step is refused as "not a TeX tool" | The compile command may run only TeX tools and `python -m pygments`. |
| Code listings are not coloured | Install Pygments (`pip install pygments`); check the **Highlight command**. |
| `??` in the PDF | An undefined reference: the label is missing, or it is in a chapter left out of the PDF. The log lists undefined references. |
| Fast preview does not run | The **Engine** must be LuaLaTeX. |
| The PDF does not open beside the note | The output folder must be in the vault, and the desktop app is required. |
| Sync scrolling does nothing | The compile command must write SyncTeX (`-synctex=1`, as the default does). The log says when the SyncTeX file is missing. |
| In a browser through Ignis the status says `converted` and nothing builds | Expected while desktop Obsidian runs on the PC: the desktop app builds when it sees the saved notes; the status bar then says `built on the PC at …` once the PC's build ends. Otherwise, see the next section. |
| In a browser through Ignis the PDF stays old, even after reloading the page | Ignis does not watch the output folder: let it watch `main.pdf`, `main.shown.pdf`, `main.synctex.gz` and `latex-status.json` ([below](#following-the-pcs-builds-from-a-browser-or-a-phone)). |
| A `.tex` file edited by hand was overwritten | It is in `.sync-backups/`. Edit the note instead. |

## Building through Ignis

[Ignis](https://github.com/Gameboard-dev/ignis-control#which-ignis) serves an Obsidian vault to a web browser. A page in
a browser cannot run TeX, so normally only the desktop app on the PC builds the document. When the plugin finds itself
in a browser through Ignis (detected; there is no setting), the browser converts the notes and the PC compiles them, so
the PDF stays current while Obsidian is closed on the PC.

### How it works

1. On each save in the browser, the plugin asks Ignis whether a build is possible (`GET /relay/latex/status` on the
   page's own address). A build is possible when the PC side is set up and desktop Obsidian is not running there.
2. If it is, the plugin converts the note as the desktop app would and writes the `.tex` files, code files and word
   counts into the vault's output folder.
3. After the quiet period, it asks for a build (`POST /relay/latex/compile`). Ignis Control on the PC compiles
   `LaTeX/compiled/main.tex` in a sandbox.
4. The plugin checks the build's status every three seconds until it ends, writes the result to `latex-sync.log` (for
   example `Ignis build: built in 31 s`, or the errors), updates the status bar and shows the PDF.

If a build is not possible, the log says why (for example `Ignis build: not available (HTTP 503: ...)`), and the
browser converts notes only for their word counts and warnings. While desktop Obsidian runs on the PC,
the desktop plugin builds instead.

### Setting it up

1. On the PC, set up [Ignis Control](https://github.com/Gameboard-dev/ignis-control) with its LaTeX build:
   - [README: "With the LaTeX plugin: remote Markdown-to-LaTeX compilation"](https://github.com/Gameboard-dev/ignis-control#with-the-latex-plugin-remote-markdown-to-latex-compilation)
     explains what the PC side does and does not do.
   - [SETUP.md: "The LaTeX build"](https://github.com/Gameboard-dev/ignis-control/blob/main/SETUP.md#the-latex-build)
     covers the PC-side set-up: TinyTeX, Pygments and the sign-in settings.
2. Nothing to turn on in the plugin: in a browser through Ignis it asks Ignis to build by itself, and the desktop app
   builds as usual.
3. Keep the PC side's assumptions:
   - The notes folder must be `LaTeX` (the default), because Ignis Control builds `LaTeX/compiled/main.tex`.
   - `main.tex` must exist in `LaTeX/compiled/`.
   - The document must compile with LuaLaTeX.

### Differences from a desktop build

- Ignis Control runs one fixed sequence: LuaLaTeX, biber if needed, makeglossaries if needed, then LuaLaTeX twice. The
  **Compile command**, **Engine**, fast preview and **Build only what changed** do not apply.
- Code listings are highlighted by Ignis Control with Pygments before the first LuaLaTeX run.
- Only one build runs at a time, and a build is stopped after 600 seconds.
- Builds are accepted only from a signed-in user (Ignis Control checks the sign-in on every request).

### Following the PC's builds from a browser or a phone

While desktop Obsidian runs on the PC, it builds after every save made anywhere, a browser through Ignis included. A
device that does not build follows those builds through two files of the output folder:

- **The status bar.** The desktop app writes `latex-status.json` when a build or a preview starts and when it ends.
  A device that does not build reads it at start and whenever it changes, and shows it: `building on the PC… (started
  23:28)`, `built on the PC at 23:28 (preview)`, `built on the PC at 23:30 (0 errors, 10 undefined refs)`,
  `build failed on the PC at 23:31 (3 errors) — see log`. The tooltip has the build's summary.
- **The PDF.** When `main.shown.pdf` changes, Obsidian's PDF view loads the new copy by itself; with sync scrolling on,
  the plugin reads the SyncTeX file again and puts the PDF at the note's place, as after a build on the desktop. No
  pane is opened for it.

Both rely on the device being told that the files changed. Obsidian Sync and similar tools do so. **Ignis does so only
for files it watches**: if Ignis's ignored paths (its `IGNORED_PATHS` setting) cover the whole output folder, the
browser never hears of a new PDF, keeps the file's old modification time, and asks for the same address as before, so
the old PDF is shown even after the page is reloaded. Ignore the folder's contents except those four files, for example
`/LaTeX/compiled/*,!/LaTeX/compiled/main.pdf,!/LaTeX/compiled/main.shown.pdf,!/LaTeX/compiled/main.synctex.gz,!/LaTeX/compiled/latex-status.json`
(gitignore patterns: `*` keeps the folder itself watched, and each `!` line watches one file again).

## Development

The plugin is one crate of the Rust workspace [obsidian-cargo-lib](https://github.com/Gameboard-dev/obsidian-cargo-lib), which holds
the shared crate and the build tool `pack`. Clone this repository into it as `latex`:

```bash
git clone https://github.com/Gameboard-dev/obsidian-cargo-lib
git clone https://github.com/Gameboard-dev/latex obsidian-cargo-lib/latex
cd obsidian-cargo-lib
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129   # must match the wasm-bindgen crate
```

Build and test from the `obsidian-cargo-lib` folder:

```bash
cargo run -p pack --release -- latex                      # writes latex/latex.wasm
cargo run -p pack --release -- latex --fast               # a quicker build while working
cargo run -p pack --release -- latex --deploy <vault>/.obsidian/plugins/latex
cargo run -p pack --release -- latex --release <folder>   # a release's three files, the module inlined in main.js
cargo test --release -p latex-core                        # the rules, word counts, ordering, SyncTeX
```

Some tests need real data or TeX and are skipped unless their variables are set:

| Test | Variables | Checks |
| --- | --- | --- |
| `core/tests/python.rs` | `LATEX_NOTES`, `LATEX_EXPECTED` (optional) | The conversion against `core/tests/fixtures.json`, the output of the Python script the plugin replaced; optionally a whole folder of notes. |
| `core/tests/synctex.rs` | `LATEX_NOTES`, `LATEX_SYNCTEX` (optional) | Note-to-PDF lookups against `synctex view` on a real build. |
| `core/tests/fast_preview.rs` | `FAST_TEX` | The fast preview on 15 scenarios against clean latexmk builds ([details](docs/builds-and-preview.md#tests)). |

### Code layout

| File | Contents |
| --- | --- |
| `core/src/app.rs` | the plugin: settings, settings tab, watcher, commands, status bar, log |
| `core/src/convert.rs` | the conversion rules, and the note-line to tex-line map |
| `core/src/wordcount.rs` | the word counter and its built-in rules |
| `core/src/order.rs` | chapter ordering and `main.tex`'s `\input` lines |
| `core/src/pipeline.rs` | what the watcher decides: names, stamps, compile-command steps, latexmk's stages |
| `core/src/compile.rs` | the full build and the preview worker |
| `core/src/preview.rs`, `core/src/pipeline/fast.rs`, `core/src/pipeline/keys.rs` | the fast preview |
| `core/src/pdf.rs`, `core/src/splice.rs` | the spliced PDF |
| `core/src/synctex.rs`, `core/src/scroll.rs` | SyncTeX lookups, the PDF pane and sync scrolling |
| `core/src/ignis.rs` | building through Ignis |
| `core/src/remote.rs` | `latex-status.json`: the PC's builds, written by the desktop app, shown by other devices |
| `core/src/device.rs` | the per-device commands and the programs they may run |
| `core/src/files.rs`, `core/src/host.rs` | the output folder's files; the Node side of the preview |

Dependencies: `regex-lite`, `shlex` (splitting the compile command without a shell), `percent-encoding`, `serde_json`,
`xxhash-rust` (telling a hand edit of a `.tex` from the plugin's own), `miniz_oxide` (PDF streams), and `serde` with
`serde-wasm-bindgen` for the settings.

## Licence

MIT. See [LICENSE](LICENSE).
