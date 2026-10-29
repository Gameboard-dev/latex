# Disclosures and compliance

[← README](../README.md)

LaTeX is written to meet Obsidian's
[Developer policies](https://docs.obsidian.md/community-directory/developer-policies), the
[Submission requirements for plugins](https://docs.obsidian.md/community-directory/submission-requirements-for-plugins)
and the [Plugin guidelines](https://docs.obsidian.md/Plugins/Releasing/Plugin+guidelines). This page goes through what
those policies ask a plugin to disclose, and what they forbid, and says plainly what the plugin does about each and why.
The policy pages were read on 2026-10-29.

- [In short](#in-short)
- [Payment and accounts](#payment-and-accounts)
- [Programs the plugin runs](#programs-the-plugin-runs)
- [Network use](#network-use)
- [Files in the vault](#files-in-the-vault)
- [Files outside the vault](#files-outside-the-vault)
- [Telemetry](#telemetry)
- [Ads](#ads)
- [Source code and licence](#source-code-and-licence)
- [Desktop only](#desktop-only)
- [No self-installation or self-updating](#no-self-installation-or-self-updating)
- [The plugin guidelines](#the-plugin-guidelines)

## In short

LaTeX is free and open source, needs no account, shows no ads and collects nothing about you. To compile your
document it runs the TeX programs you installed yourself (LuaLaTeX, pdfLaTeX or XeLaTeX, latexmk, biber,
makeglossaries) and, for code listings, Python with Pygments. It runs them only in desktop Obsidian, with argument
arrays and no shell, at below-normal priority, and only the programs on a fixed list. On the desktop it makes no
network requests. In a browser through Ignis, it asks Ignis's build relay on the page's own address to compile. Every
file it writes is inside the vault, in the output folder (`<notes folder>/compiled/`). The plugin never downloads,
installs or updates anything, TeX included. Your TeX distribution may download missing LaTeX packages by its own
settings; the plugin does not ask it to.

## Payment and accounts

None. The plugin is free, every feature works without paying anyone, and there is no sign-in and no API key. The TeX
distributions it works with are free as well. The optional build through Ignis uses the sign-in of your own Ignis
server, which the plugin neither provides nor sees (the browser sends it, as it does for every request to that page).

## Programs the plugin runs

Compiling a LaTeX document means running TeX's programs, and a plugin cannot do that from inside Obsidian's JavaScript.
So, in desktop Obsidian only, the plugin starts them as child processes with Node's `child_process.spawn`:

| Program | What for | When |
|---|---|---|
| latexmk | The default compile command: it runs the engine, biber and makeglossaries as often as the document needs | After a save, once the notes have been quiet for the quiet period; and with **Compile now** or **Build all chapters and compile** |
| LuaLaTeX (or pdfLaTeX, XeLaTeX, by the **Engine** setting) | Typesetting the document into a PDF | Within a build. With **Fast preview** on (LuaLaTeX only): also one LuaLaTeX job that waits for the next save and typesets the edited section, started when Obsidian starts (after making a precompiled preamble if it is missing or out of date) and again after each preview; and one run per changed TikZ picture when **Cache TikZ pictures** is on |
| biber | The bibliography (biblatex) | Within a build, when citations change |
| makeglossaries | The glossary (glossaries-extra) | Within a build, when glossary use changes |
| Python with Pygments (`python -m pygments`) | Colouring a code block's listing | When a block of code in a note changes; not at all for a document without code |
| `taskkill` (Windows only) | Stopping a build and every program it started | When a save cancels a running build, on **Stop the build**, and when the plugin is unloaded (the waiting preview job too). On macOS and Linux the build's process group is stopped with a signal instead; no program is started |

How they are run:

- **No shell.** Each program is started with its arguments as an array (`spawn(program, [arguments])`), never as a
  command string given to a shell, so nothing in a setting or a note can add a second command. The compile command is
  split into arguments the way a POSIX shell quotes them, without any expansion.
- **Only TeX tools and Pygments.** Before anything runs, each step's program is checked against a fixed list of TeX
  tools (latexmk, the engines, biber, bibtex, makeglossaries, makeindex, xindy, dvipdfmx, dvips, ps2pdf, pygmentize)
  or Python running exactly `-m pygments`. Any other program is refused, and the log says why.
- **This device's settings only.** The **Compile command**, **Highlight command** and **TeX programs folder** are kept
  in this device's storage (Obsidian's local storage), not in the vault's `data.json`. A synced vault, another device or
  a browser therefore never chooses what a computer runs. The defaults are latexmk with the chosen engine, biber and
  makeglossaries, and `python -m pygments -l %LANG% -f latex -o %OUT% %SRC%`.
- **Below-normal priority**, so that Obsidian stays responsive: always for the fast preview's runs and for Pygments,
  and for the compile command's steps while the setting **Below-normal priority** is on (the default). Programs run in
  the output folder, with their windows hidden on Windows; a build's output goes to the build log.
- **Finding the programs.** They are looked up on the system's `PATH` and then in the **TeX programs folder** setting,
  or, when it is empty, in the usual installation folders of TinyTeX, TeX Live, MiKTeX and MacTeX that exist on this
  computer (the plugin checks only whether those folders exist). This is for a TeX installed after Obsidian started,
  whose folder Obsidian's `PATH` does not have yet.
- **Opening the log.** **Open the log** (or a click on the status bar) asks the system to open `latex-sync.log` in its
  usual program for text files (Electron's `shell.openPath`).

What the TeX tools do on their own, outside the plugin's control: latexmk reads a `latexmkrc` file from the folder it
runs in and from your home folder, if you have one, and the default command passes latexmk two lines of Perl with
`-e` (the custom rule that runs makeglossaries, as LaTeX Workshop's recipe does). A TeX run with `--shell-escape` can
run commands; the plugin never adds that option, and the TikZ cache works without it.

## Network use

**On the desktop: none.** The plugin makes no network request, at start or at any other time. The links in the settings (to
this repository's README and its example preamble) open in your browser only when you click them.

**In a browser through Ignis** (a self-hosted server that serves a vault to a web browser; detected, there is no
setting): a page in a browser cannot run TeX, so the plugin asks Ignis to compile on the computer that serves the vault.
It sends these requests, only to the page's own address (same origin) and with the browser's own sign-in to that page:

| Request | When | What is sent | Why |
|---|---|---|---|
| `GET /relay/latex/status` | When a note in the notes folder is saved, and every three seconds while a build runs | Nothing but the request | Whether a build is possible (the computer's side is set up and desktop Obsidian is not running there), and when it has ended |
| `POST /relay/latex/compile` | After the quiet period that follows a save | Nothing but the request (the notes are already in the vault, converted by the plugin) | To start the build |

Ignis accepts these only from a signed-in user and builds with one fixed command of its own; the plugin starts no
program there.

**The TeX distribution may use the network by itself**, not because the plugin asks it to:

- **MiKTeX** can install a missing package the moment a document needs it ("install missing packages on-the-fly"),
  depending on its own setting (always, ask first, or never). To keep builds offline, set it to "never" in the MiKTeX
  Console.
- **TeX Live, MacTeX and TinyTeX** download packages only when you run `tlmgr` yourself. The plugin never runs `tlmgr`
  or MiKTeX's package manager, and neither is on its list of programs.

## Files in the vault

- **The output folder** (`<notes folder>/compiled/`), written by the plugin and by the TeX programs it runs:
  - one `.tex` file per note, and the `\input` lines of `main.tex` when you run **Apply ordering** (the rest of
    `main.tex` is yours);
  - `wordcounts.tex` (the word counts the preamble reads) and `latex-status.json` (what the last build or preview did,
    for the status bar of a device that does not build);
  - `code/`: each code block's code, the listing Pygments makes of it, and `code/pygments-style.tex` (written once if
    missing, never overwritten);
  - the PDF (`main.pdf`), the copy the pane shows (`main.shown.pdf`), the SyncTeX file, and the files LaTeX, biber and
    makeglossaries make beside them (`.aux`, `.log`, `.bbl`, `.glo` and the like);
  - with fast preview, its own files: the precompiled preamble (`fp-preamble.fmt`), the driver files `fp-*.tex` and
    `fp-*.lua`, `main-fmt.tex`, `main-body.tex`, `0-preamble-static.tex`, and the TikZ cache folder `tc/`;
  - the logs: `latex-sync.log` (every save, warning and build result) and `.latex-sync-build.log` (what the last build
    printed);
  - `.sync-backups/`: a copy of a `.tex` or code file that was edited by hand, made before the plugin overwrites it.
- **The notes folder:** created if it does not exist. **Apply ordering** creates the fixed notes that are missing (the
  preamble, from the plugin's example; the title page, glossary and appendix, empty) and renames notes to the chapter
  order through Obsidian, which updates links to them.
- **Your notes:** with **Stamp the last change** on (the default), a note's last-updated property is set to the time
  whenever its text changes. Nothing else in a note is changed.
- **The plugin's settings:** `data.json` in its plugin folder.

## Files outside the vault

**The plugin writes nothing outside the vault.** It reads outside the vault only what a setting names: the
**Bibliography file**, when you set it to an absolute path (read, never written, for the authors' names in the word
counts), and the existence of the TeX installation folders above.

The programs it runs belong to the TeX distribution and to Python, and they keep their own files where they always do,
as they would if you ran them in a terminal. For example:

- **TeX Live, MacTeX and TinyTeX:** caches such as LuaLaTeX's font cache in the distribution's `TEXMFVAR` folder (by
  default a `.texlive<year>` folder in your home folder for TeX Live, `~/Library/texlive/<year>` for MacTeX, and inside
  the TinyTeX folder for TinyTeX).
- **MiKTeX:** its configuration, caches and the packages it installs, in its own folders (on Windows under
  `%LOCALAPPDATA%\MiKTeX` and `%APPDATA%\MiKTeX` for a per-user installation).
- **biber:** the biber program that TeX distributions ship is packed, and unpacks itself into the system's temporary
  folder the first time it runs.
- **Python:** Pygments writes only the listing it is asked for, in the output folder; Python may keep compiled bytecode
  in its own installation.

These are the distribution's files, not the plugin's: the plugin neither reads nor removes them.

## Telemetry

None, client-side or server-side. Nothing about you or your use is collected or sent anywhere. The build summaries
and timings the plugin records stay in the logs in your output folder.

## Ads

None, static or dynamic.

## Source code and licence

Open source, under the MIT licence ([LICENSE](../LICENSE)), built from the Rust source in this repository and the
shared crate and build tool in [obsidian-cargo-lib](https://github.com/Gameboard-dev/obsidian-cargo-lib)
([Development](../README.md#development)). The code is not obfuscated: `main.js` is a short loader, and the plugin's
code is a WebAssembly module compiled from that source (inlined in `main.js` in a release), with readable JavaScript
bindings generated by wasm-bindgen. The TeX and Lua text the plugin writes (the example preamble
note, the fast preview's driver files) is part of this repository's source, under its licence;
`code/pygments-style.tex` is Pygments' default style as Pygments generates it (Pygments is under the BSD licence).
No TeX distribution, Python or Pygments is shipped with the plugin.

## Desktop only

`isDesktopOnly` is `true`. Compiling needs Node's `child_process`, `fs` and `os` and Electron's `shell`, which exist
only in desktop Obsidian and which the submission requirements allow only with `isDesktopOnly`. The plugin also checks
that it is in desktop Obsidian before it starts any program. Obsidian's mobile app does not load it. In a browser
through Ignis (on any device, a phone's browser included), the plugin uses none of those modules: it converts the notes
and asks Ignis to build, as described above.

## No self-installation or self-updating

The plugin never downloads, installs or updates itself, TeX, Python, Pygments or any package. You install a TeX
distribution yourself; the plugin's settings and README only link to the distributions' own pages and say which tools
the default build uses. A program that is missing is reported in the log, with what to install, and nothing more.

## The plugin guidelines

| Guideline | How LaTeX meets it |
|---|---|
| No obfuscation | `main.js` is a short loader; the code is a WebAssembly module (inlined in `main.js` in a release) compiled from the Rust source here, with its bindings generated by wasm-bindgen and kept readable. Build instructions are in the README. |
| No client-side telemetry, no ads | None. |
| Disclosures | Programs run, network use and files, above. |
| Licence | `LICENSE` (MIT). |
| Desktop only | `isDesktopOnly` is `true` (above). |
| Security | No `innerHTML`, `outerHTML` or `insertAdjacentHTML`; text is set with the DOM API. No `eval`: in a release the bindings are part of `main.js`. Programs are started with argument arrays, never through a shell, and only those on the list above. |
| Commands and hotkeys | Command ids without the plugin id (`build-all`, `build-and-compile`, `compile`, `stop`, `open-log`); no default hotkeys. |
| Settings | Headings with `setHeading`, in sentence case, without the word "settings". |
| Vault | Notes are created, changed and renamed with the Vault and FileManager APIs (`create`, `process`, `createFolder`, `renameFile`); the output folder is written through the vault's adapter, and by a build on the desktop with Node's `fs` inside that folder. |
| Resources | Events are registered with `registerEvent` and `registerDomEvent`; on unload the plugin stops a running build and the waiting preview job. |
| `this.app` | The plugin uses its own `app`, never the global one. |
