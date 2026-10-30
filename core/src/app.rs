//! The plugin on Obsidian's main thread: its settings (data.json) and settings tab, the watcher (a saved note in the
//! notes folder becomes its chapter, and the document is compiled once the notes are quiet), the commands, the log and
//! the status bar, which always says what the plugin is doing: converting a note, waiting for quiet, which tool of the
//! build is running, or how the last build went.
//!
//! Obsidian's API: <https://docs.obsidian.md/Reference/TypeScript+API/Plugin> (loadData, saveData, addSettingTab,
//! addCommand, addStatusBarItem, registerEvent), <https://docs.obsidian.md/Reference/TypeScript+API/Vault> (on modify,
//! create, delete, rename; read, cachedRead, process: the last-updated stamp is written atomically),
//! <https://docs.obsidian.md/Reference/TypeScript+API/Workspace/onLayoutReady> (vault events are registered after it, so the
//! initial load's create events are not taken for new notes), <https://docs.obsidian.md/Reference/TypeScript+API/Notice>.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use js_sys::{Array, Date, Function};
use obsidian_kit::settings::{self, Ctl, Row};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::compile::{self, Build};
use crate::convert::{self, md_to_tex_code, CodeFile, Index, Options};
use crate::files::{self, vault_path, Place};
use crate::js::{self, call, f0, f1, f2, get, new, set, str};
use crate::pipeline;
use crate::scroll::{self, Sync};
use crate::wordcount::{self, count_chapter, Context, Glossary, Macros, Names, Rules};

pub const README: &str = "https://github.com/Gameboard-dev/latex#readme";
/// The preamble the rules are written for (user, 2026-10-28: bundled, and offered as a download from the repository).
pub const EXAMPLE_PREAMBLE: &str = "https://github.com/Gameboard-dev/latex/blob/main/examples/0-preamble.md";
/// A save arrives as a burst of writes; a note is converted once it has been quiet this long.
const SETTLE_MS: f64 = 400.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub notes_folder: String,
    /// A vault folder, or an absolute path (desktop only).
    pub output_folder: String,
    pub main_file: String,
    /// Not used since 2026-10-27 (ToDo 5: a chapter's .tex is its note's name; kept so an older data.json reads).
    pub tex_names: String,
    pub ignore: String,
    /// The chapter notes left out of the PDF (by name, without .md): still converted, each .tex opening with \endinput
    /// (pipeline::leave_out).
    pub left_out: Vec<String>,
    /// Not used since 2026-10-27: the preamble, title page, glossary and appendix have fixed names (order.rs); kept so
    /// an older data.json reads.
    pub preamble_note: String,
    pub glossary_note: String,
    pub title_page_note: String,
    pub appendix_note: String,
    pub number_new_notes: bool,
    /// The chapters' order, by name without the number (Settings → Chapter order); "Apply ordering" numbers the notes
    /// to it.
    pub chapter_order: Vec<String>,
    pub header: String,
    pub compile: bool,
    pub compile_command: String,
    /// What turns a note's block of code into the LaTeX the chapter inputs: %LANG% the fence's language, %SRC% the code
    /// file, %OUT% the .tex to write (paths in the output folder).
    pub highlight_command: String,
    pub quiet_seconds: f64,
    pub below_normal: bool,
    pub cancel_running: bool,
    /// Fast preview (docs/fast-preview-design.md): a save shows in the PDF within a second, the full build following.
    pub fast_preview: bool,
    /// "section" (the edited section and the next) or "chapter".
    pub preview_scope: String,
    /// The preview starts once no save has come for this long (ms).
    pub preview_delay: f64,
    pub cache_tikz: bool,
    /// The full build runs only what the change needs (preview::Plan); off: latexmk every time.
    pub direct_builds: bool,
    /// When fast preview has shown every change with every page current, the full build waits until the notes have been
    /// idle this long (seconds; 0: right after the quiet period, as without fast preview).
    pub idle_build_seconds: f64,
    /// Each build's decisions, commands and timings in the log (to find what makes a build slow).
    pub debug_log: bool,
    /// The TeX engine (pipeline::TEX_ENGINES): the default compile command's; fast preview runs only with LuaLaTeX.
    pub engine: String,
    /// Show the compiled PDF beside the notes when a chapter note is opened or a build succeeds.
    pub open_pdf: bool,
    /// Scroll a chapter note and the PDF together (scroll.rs).
    pub sync_scroll: bool,
    pub log_file: String,
    pub build_log: String,
    pub stamp_updated: bool,
    pub updated_property: String,
    pub backups: bool,
    pub backups_folder: String,
    pub wordcount_property: String,
    pub count_start: String,
    pub count_print: String,
    pub count_stop: String,
    /// The word counter's off command (`{\wordcountoff ...}` is not counted).
    pub count_off: String,
    /// In the output folder: the counts, for the preamble to read.
    pub counts_file: String,
    /// The .bib whose authors a narrative citation prints, for the counts: in the output folder, or an absolute path.
    pub bib_file: String,
    /// data.json's wordcountRules (wordcount::Rules::new); not in the settings tab since 2026-10-28.
    pub wordcount_rules: String,
    /// Inline maths counts as one word (wordcount::Rules::inline_math_word).
    pub inline_math_word: bool,
    pub tikz_libraries: String,
    pub listing_env: String,
    pub listing_command: String,
    pub term_command: String,
    pub title_prefix: String,
    pub make_title: String,
    pub lists: String,
    pub commands: String,
    pub ref_words: String,
    pub abbreviations: String,
    pub small_words: String,
    pub line_chars: f64,
    pub wrap_chars: f64,
    pub small_from: f64,
    /// Not a setting: the hash of each .tex as last written, to tell a hand edit (which is backed up) from the plugin's own.
    pub written: HashMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        let o = Options::default();
        Settings {
            notes_folder: "LaTeX".into(),
            output_folder: "LaTeX/compiled".into(),
            main_file: o.main,
            tex_names: String::new(),
            ignore: "README".into(),
            left_out: Vec::new(),
            preamble_note: o.preamble,
            glossary_note: o.glossary,
            title_page_note: o.title_page,
            appendix_note: "appendix".into(),
            number_new_notes: false,
            chapter_order: Vec::new(),
            header: o.header,
            compile: true,
            compile_command: pipeline::DEFAULT_COMMAND.into(),
            highlight_command: "python -m pygments -l %LANG% -f latex -o %OUT% %SRC%".into(),
            quiet_seconds: 0.3,
            below_normal: true,
            cancel_running: true,
            fast_preview: true,
            preview_scope: "section".into(),
            preview_delay: 300.0,
            cache_tikz: true,
            direct_builds: true,
            idle_build_seconds: 60.0,
            debug_log: false,
            engine: "lualatex".into(),
            open_pdf: true,
            sync_scroll: false,
            log_file: "latex-sync.log".into(),
            build_log: ".latex-sync-build.log".into(),
            stamp_updated: true,
            updated_property: o.updated,
            backups: true,
            backups_folder: ".sync-backups".into(),
            wordcount_property: o.wordcount,
            count_start: o.count_start,
            count_print: o.count_print,
            count_stop: o.count_stop,
            count_off: Macros::default().off,
            counts_file: "wordcounts.tex".into(),
            bib_file: "My Library.bib".into(),
            wordcount_rules: String::new(),
            inline_math_word: true,
            tikz_libraries: o.tikz_libraries,
            listing_env: o.listing_env,
            listing_command: o.listing_cmd,
            term_command: o.term,
            title_prefix: o.title_prefix,
            make_title: o.make_title,
            lists: convert::DEFAULT_LISTS.into(),
            commands: convert::DEFAULT_COMMANDS.into(),
            ref_words: convert::DEFAULT_REF_WORDS.into(),
            abbreviations: convert::DEFAULT_ABBREVIATIONS.into(),
            small_words: convert::DEFAULT_SMALL_WORDS.into(),
            line_chars: o.line_chars as f64,
            wrap_chars: o.wrap_chars as f64,
            small_from: o.small_from as f64,
            written: HashMap::new(),
        }
    }
}

impl Settings {
    /// The values that are no longer settings (user, 2026-10-28): the output folder is the notes folder's `compiled`,
    /// the main file `main`, every .tex opens with the converter's own first line, and no note is left out by name
    /// (the folder's README never counts).
    pub fn fixed(mut self) -> Self {
        let o = Options::default();
        let notes = self.notes_folder.trim().trim_matches('/');
        self.output_folder = if notes.is_empty() { "compiled".into() } else { format!("{notes}/compiled") };
        self.main_file = o.main;
        self.header = o.header;
        self.ignore = String::new();
        self
    }

    /// The main file's name without .tex.
    pub fn main(&self) -> String {
        self.main_file.trim().trim_end_matches(".tex").to_string()
    }

    /// Whether fast preview runs: it is on, and the engine is LuaLaTeX (its jobs are LuaLaTeX's).
    pub fn fast(&self) -> bool {
        self.fast_preview && self.engine == "lualatex"
    }
}

/// What the plugin is doing, as the status bar shows it.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Converting { note: String, tex: String },
    /// Converted, and no build follows (compiling is off).
    Converted { at: String },
    NotConverted { note: String },
    Waiting { until: f64 },
    /// Fast preview showed every change with every page current: the full build waits until the notes are idle.
    Previewed { until: f64 },
    Compiling,
    Compiled { at: String, errors: usize, undefined: usize },
    Failed,
    Stopped,
    /// Converted; this device cannot compile (a phone, or a browser).
    NoCompiler,
    /// What the computer's build last did, on a device that does not build (remote.rs).
    Remote(crate::remote::Report),
}

pub struct App {
    pub plugin: JsValue,
    pub obs: JsValue,
    pub vault: JsValue,
    pub adapter: JsValue,
    /// Node's `require` on the desktop; `None` on a phone (no compiling there, and no folder outside the vault).
    pub require: Option<Function>,
    pub settings: Settings,
    pub build: Build,
    persist: Option<f64>,
    /// The folder's notes (file name -> text): the references between them, and what a save changed.
    pub(crate) notes: BTreeMap<String, String>,
    settle: HashMap<String, f64>,
    pub(crate) quiet: Option<f64>,
    pub status: Status,
    status_el: JsValue,
    ticking: bool,
    /// The last conversion's warnings (they are in the log too) and which note gave them.
    warnings: (String, usize),
    /// The last build's summary line, for the tooltip.
    pub last_build: String,
    /// Whether builds go to Ignis now (ignis::check, before each conversion in a browser).
    pub ignis_ok: bool,
    log_pending: String,
    log_writing: bool,
    /// Sync scrolling: the last build's SyncTeX index, the notes' line maps, which pane leads.
    pub sync: Sync,
    /// The word counts of each counted chapter (by .tex name), in its order.
    counts: BTreeMap<String, Vec<(String, usize)>>,
    /// What the counter reads besides a chapter: its rules, the .bib's names (with the .bib's hash), the glossary.
    count_rules: Rules,
    names: (u64, Names),
    glossary: Glossary,
    /// Settings changed since data.json was last written: the notes to read again, the word-count rules to rebuild.
    pending: (bool, bool),
    /// The fast preview's slot (compile.rs).
    pub preview: compile::Preview,
    /// This device's commands (device.rs), from Obsidian's local storage.
    pub device: crate::device::Device,
    /// The settings tab, drawn again after the chapters are renamed.
    tab: JsValue,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Run `f` on the app (`None` once the plugin is unloaded).
pub fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

const UI: &[Row] = &[
    ("", "", "Each note in the notes folder becomes a LaTeX file whenever it is saved, by fixed rules (headings and labels, references, citations, lists, tables, figures, equations, the glossary), and the document is compiled once the notes have been quiet for a while. The rules and every setting are explained in the README: ", Ctl::Link(README)),
    ("", "", "The notes are written for the example preamble (its commands for the title page, the glossary, the word counts and listings); a notes folder without a preamble note gets a copy of it. A preamble of your own must define the same commands (README: Conventions). The example: ", Ctl::Link(EXAMPLE_PREAMBLE)),
    ("", "Folders and files", "", Ctl::Head),
    ("notesFolder", "Notes folder", "The folder in the vault whose notes are the document's parts (its subfolders are not read).", Ctl::Text),
    ("", "Chapter ordering and inclusion", "", Ctl::Head),
    ("", "", "The notes in the notes folder, in the order of the PDF; a note added to the folder appears here as a chapter. Turn a note off to leave it out of the PDF (its .tex then starts with \\endinput, so a reference to it prints ??). Drag a chapter, or use its arrows, to move it; then right-click the notes folder and choose \"Apply ordering\", which numbers the notes in this order (renaming them through Obsidian, so every link follows) and puts the main file's \\input lines in the same order. Four notes have fixed names and places: preamble and title_page first, glossary and appendix last; the plugin makes any that is missing.", Ctl::Note),
    ("", "", "", Ctl::Custom(chapter_box)),
    ("", "Compiling", "", Ctl::Head),
    ("compile", "Compile after a change", "Desktop only. On a phone, or in a browser, the computer's Obsidian converts and builds a saved note.", Ctl::Toggle),
    ("", "Installing LaTeX", "Compiling needs a TeX distribution on this computer (TinyTeX, TeX Live, MiKTeX or MacTeX) with latexmk, biber and makeglossaries. Which to install, and the extra steps for each:", Ctl::Link("https://github.com/Gameboard-dev/latex#requirements")),
    ("engine", "Engine", "The TeX engine the document is written for: LuaLaTeX or XeLaTeX for system fonts (fontspec), pdfLaTeX for a classic document. It chooses the default compile command; fast preview needs LuaLaTeX.", Ctl::Custom(engine_choice)),
    ("compileCommand", "Compile command", "One command per line, run in the output folder in turn until one fails; quoted as in a shell. %DOC% is the main file's path without .tex, %DOCFILE% its name, %DIR% the output folder, as in LaTeX Workshop's recipes. The default is latexmk with the engine above, biber and makeglossaries; clear the box to return to it. This device only.", Ctl::Custom(command_area)),
    ("texFolder", "TeX programs folder", "Where this device's TeX programs are (latexmk, lualatex, biber …), searched after the system's PATH: for a TeX installed after Obsidian started, which Obsidian does not see until it restarts. Empty: the usual places (TinyTeX, TeX Live, MiKTeX, MacTeX) that exist. This device only.", Ctl::Custom(tex_folder_row)),
    ("highlightCommand", "Highlight command", "Turns each block of code in a note (```python, ```rust …), whose code goes to the output's code folder, into the coloured LaTeX its listing shows; run in the output folder whenever a block's code changes. %LANG% is the block's language, %SRC% the code file, %OUT% the .tex to write. The default is Pygments, the highlighter latexmkrc uses for hand-made code/NAME.rs files, so the colours are the same. This device only.", Ctl::Custom(highlight_field)),
    ("quietSeconds", "Quiet period (seconds)", "How long after a save (Ctrl+S, or Obsidian's own every few seconds) the plugin decides about the full build. A save during a build cancels it and starts the next, which runs only what changed.", Ctl::Number),
    ("belowNormal", "Below-normal priority", "Run the build at a lower priority, so that Obsidian and the editors stay responsive.", Ctl::Toggle),
    ("cancelRunning", "Cancel a build still running", "When a chapter changes during a build, stop the build and start again. Off: the next build waits for it to finish.", Ctl::Toggle),
    ("", "Fast preview", "", Ctl::Head),
    ("fastPreview", "Fast preview", "LuaLaTeX only (with another engine the compile command runs). A save shows in the PDF within about a second: a waiting LuaLaTeX job typesets the edited section, which replaces its pages in the last full build's PDF; the full build follows after the quiet period. Pages that are not current yet say so at their top. With it on, the plugin runs its own build (on a precompiled preamble) instead of the compile command.", Ctl::Toggle),
    ("previewScope", "Preview scope", "What a preview typesets: the edited section and the next one, or the whole chapter.", Ctl::Choice(scopes)),
    ("previewDelay", "Preview delay (ms)", "The preview starts once no save has come for this long.", Ctl::Number),
    ("cacheTikz", "Cache TikZ pictures", "Each picture is made once, as a PDF, and again only when it changes (no shell escape).", Ctl::Toggle),
    ("directBuilds", "Build only what changed", "The full build runs only what the change since the last one needs: one LuaLaTeX run for prose; biber only when a citation key is cited for the first time or no longer cited (or the order of first citation changes, which numbers the list), or the .bib changed; makeglossaries only when a term is used for the first time or no longer used, moves to another page, or the glossary changed; another run only when a label, heading, caption, equation, table or figure is added, removed or renumbered. Nothing runs when the chapters are back as the last build had them. Compile now always runs the whole recipe (latexmk). Off: latexmk every time.", Ctl::Toggle),
    ("idleBuildSeconds", "Full build when idle (seconds)", "With fast preview, a save shows in the PDF within a second or two. When that preview has every page current, the PDF is already right, so the full build (one LuaLaTeX run of the whole document, which can take half a minute) waits until you have stopped saving for this long. When a preview leaves pages to update (numbering, citations, page numbers), the full build runs at once. 0: the full build after every quiet period.", Ctl::Number),
    ("debugLog", "Debug log", "Write each build's decisions, commands and the time of every step to the log (Open the log), to find out what makes a build slow.", Ctl::Toggle),
    ("openPdf", "Show the PDF beside the notes", "When a chapter note is opened or a build succeeds, open the compiled PDF in a pane on the right, unless it is already open; the focus stays in the note. The pane shows <main>.shown.pdf, a copy written once per finished build or preview, so the pane redraws once instead of after each of LaTeX's passes. Only for an output folder in the vault.", Ctl::Toggle),
    ("syncScroll", "Sync scrolling with the PDF", "Scrolling a chapter note scrolls the PDF to the same place, and scrolling the PDF with the pointer over it scrolls the note, if a pane shows it. Places come from the last build's SyncTeX file (the compile command's -synctex=1), so text written since is placed approximately. It uses Obsidian's own PDF viewer, which is not part of Obsidian's API: if a version of Obsidian changes it, the PDF only jumps to the page. Desktop only; the output folder must be in the vault.", Ctl::Toggle),
    ("logFile", "Log", "In the output folder: every save, conversion warning and build result.", Ctl::Text),
    ("buildLog", "Build output", "In the output folder: what the last build printed.", Ctl::Text),
    ("", "Notes and chapters", "", Ctl::Head),
    ("stampUpdated", "Stamp the last change", "Set a note's last-updated property (below) to the time whenever its text changes; the chapter's first line shows it.", Ctl::Toggle),
    ("updatedProperty", "Last-updated property", "", Ctl::Text),
    ("backups", "Keep a .tex edited by hand", "Before overwriting a .tex that was changed outside its note, copy it into the backups folder.", Ctl::Toggle),
    ("backupsFolder", "Backups folder", "In the output folder.", Ctl::Text),
    ("", "Bibliography", "", Ctl::Head),
    ("bibFile", "Bibliography file", "The .bib whose authors a narrative citation (\\citet) prints, for the word counts: in the output folder, or an absolute path.", Ctl::Text),
];

/// The compile command, a few lines tall.
fn command_area(s: &JsValue) {
    let value = with(|a| a.device.compile_command(&a.settings.engine)).unwrap_or_default();
    call!(s, addTextArea, f1(move |t| {
        let input = get(&t, "inputEl");
        set(&input, "rows", 4);
        call!(get(&input, "style"), setProperty, "width", "100%");
        call!(t, setValue, value.clone());
        call!(t, onChange, f1(|v| {
            with(|a| {
                let v = str(&v);
                a.device.compile_command = if crate::pipeline::is_default_command(&v) { String::new() } else { v };
                a.save_device();
            });
            JsValue::UNDEFINED
        }))
    }));
    call!(get(&get(s, "settingEl"), "classList"), add, "latex-command");
}

/// The highlight command (this device's).
fn highlight_field(s: &JsValue) {
    let value = with(|a| a.device.highlight_command().to_string()).unwrap_or_default();
    // a text area under the description, the whole command in view (user, 2026-10-28)
    call!(get(&get(s, "settingEl"), "classList"), add, "latex-command");
    call!(s, addTextArea, f1(move |t| {
        let input = get(&t, "inputEl");
        set(&input, "rows", 2);
        call!(get(&input, "style"), setProperty, "width", "100%");
        call!(t, setValue, value.clone());
        call!(t, onChange, f1(|v| {
            with(|a| {
                let v = str(&v);
                a.device.highlight_command = if v.trim() == crate::device::DEFAULT_HIGHLIGHT { String::new() } else { v };
                a.save_device();
            });
            JsValue::UNDEFINED
        }))
    }));
}

/// The engine: a choice among the known ones; the compile command box below shows the new default at once.
fn engine_choice(s: &JsValue) {
    let value = with(|a| a.settings.engine.clone()).unwrap_or_default();
    let row = get(s, "settingEl");
    call!(s, addDropdown, f1(move |d| {
        for (engine, name, _) in crate::pipeline::TEX_ENGINES {
            call!(d, addOption, engine, name);
        }
        call!(d, setValue, value.clone());
        let row = row.clone();
        call!(d, onChange, f1(move |v| {
            let engine = str(&v);
            let shown = with(|a| {
                a.set_field("engine", v.clone());
                a.device.compile_command(&engine)
            });
            let box_ = call!(get(&row, "parentElement"), querySelector, ".latex-command textarea");
            if let (Some(text), false) = (shown, box_.is_null()) {
                set(&box_, "value", text);
            }
            JsValue::UNDEFINED
        }))
    }));
}

fn tex_folder_row(s: &JsValue) {
    let value = with(|a| a.device.tex_folder.clone()).unwrap_or_default();
    call!(s, addText, f1(move |t| {
        call!(t, setPlaceholder, "the usual places");
        call!(t, setValue, value.as_str());
        call!(t, onChange, f1(|v| {
            with(|a| {
                a.device.tex_folder = str(&v).trim().to_string();
                a.save_device();
            });
            JsValue::UNDEFINED
        }))
    }));
}


/// Chapter ordering and inclusion (user, 2026-10-28): every note of the folder in the order of the PDF, in one scrolling
/// box, as Ignis Control lists its excluded plugins. The preamble and the glossary are always read; the others have a
/// toggle (left out: its .tex starts with \endinput); the chapters have arrows and a drag handle (a move keeps the order;
/// "Apply ordering" renames the notes to it, order.rs).
fn chapter_box(s: &JsValue) {
    let Some((obs, l, order)) = with(|a| {
        let l = a.layout();
        let order = l.ordered(&a.settings.chapter_order);
        (a.obs.clone(), l, order)
    }) else {
        return;
    };
    let setting_el = get(s, "settingEl");
    call!(get(&setting_el, "style"), setProperty, "display", "none");
    let doc = get(&setting_el, "ownerDocument");
    let bx = call!(doc, createElement, "div");
    set(&bx, "className", "latex-chapters");
    call!(get(&setting_el, "parentElement"), insertBefore, &bx, get(&setting_el, "nextSibling"));
    let setting = get(&obs, "Setting");
    let row = |name: &str, what: &str| {
        let r = new(&setting, std::slice::from_ref(&bx));
        call!(r, setName, name);
        call!(r, setDesc, what);
        r
    };
    let toggle = |r: &JsValue, stem: String| {
        let on = with(|a| !a.left_out(&stem)).unwrap_or(true);
        call!(r, addToggle, f1(move |t| {
            let stem = stem.clone();
            call!(call!(call!(t, setValue, on), setTooltip, "Include in the PDF"), onChange, f1(move |v| {
                with(|a| a.include(&stem, v.as_bool().unwrap_or(true)));
                JsValue::UNDEFINED
            }))
        }));
    };
    let fixed = |stem: &Option<String>, role: &str, what: &str, can_leave: bool| match stem {
        Some(n) => {
            let r = row(n, what);
            if can_leave {
                toggle(&r, n.clone());
            }
        }
        None => _ = row(role, &format!("{what}: missing; made by \"Apply ordering\"")),
    };
    fixed(&l.preamble, "0-preamble", "Preamble, always first and always read", false);
    fixed(&l.title_page, "1-title_page", "Title page", true);
    let n = order.len();
    let dragged = std::rc::Rc::new(std::cell::Cell::new(usize::MAX));
    for (i, stem) in order.iter().enumerate() {
        let r = row(stem, "Chapter");
        let row_el = get(&r, "settingEl");
        call!(get(&row_el, "classList"), add, "latex-order");
        set(&row_el, "draggable", true);
        let order = order.clone();
        let moved = move |from: usize, to: usize| {
            if from == to || from >= order.len() || to >= order.len() {
                return;
            }
            let mut next = order.clone();
            let c = next.remove(from);
            next.insert(to, c);
            // the order kept (names without numbers); the notes are renamed by "Apply ordering"
            with(|a| {
                a.settings.chapter_order = next.iter().map(|n| crate::order::base(n).to_string()).collect();
                a.save();
                a.redisplay();
            });
        };
        for (icon, label, to) in [("arrow-up", "Move up", i.checked_sub(1)), ("arrow-down", "Move down", (i + 1 < n).then_some(i + 1))] {
            let Some(to) = to else { continue };
            let moved = moved.clone();
            call!(r, addExtraButton, f1(move |b| {
                let moved = moved.clone();
                call!(call!(call!(b, setIcon, icon), setTooltip, label), onClick, f0(move || moved(i, to)))
            }));
        }
        toggle(&r, stem.clone());
        let d = dragged.clone();
        call!(row_el, addEventListener, "dragstart", f1(move |_| {
            d.set(i);
            JsValue::UNDEFINED
        }));
        call!(row_el, addEventListener, "dragover", f1(|e| {
            call!(e, preventDefault); // a drop target
            JsValue::UNDEFINED
        }));
        let d = dragged.clone();
        call!(row_el, addEventListener, "drop", f1(move |e| {
            call!(e, preventDefault);
            moved(d.replace(usize::MAX), i);
            JsValue::UNDEFINED
        }));
    }
    if order.is_empty() {
        row("No chapters yet", "Each note added to the notes folder is a chapter");
    }
    fixed(&l.glossary, "glossary", "Glossary, always read (its entries are printed where the document prints them)", false);
    fixed(&l.appendix, "appendix", "Appendix, always last", true);
}

/// "Apply ordering" (the notes folder's right-click menu): the notes with a role made where missing, then every note numbered to the order kept
/// in the settings (the preamble 0, the title page 1, the chapters, then the glossary and the appendix), and a new
/// chapter added to the main file after the last.
async fn apply_ordering() {
    make_role_notes().await;
    let Some((renames, order_new, added)) = with(|a| {
        let l = a.layout();
        let renames = l.renumber(&a.settings.chapter_order);
        let new_name = |o: &String| renames.iter().find(|(old, _)| old == o).map_or(o.clone(), |(_, n)| n.clone());
        let order_new: Vec<String> = l.ordered(&a.settings.chapter_order).iter().map(new_name).collect();
        // a chapter that had no number is new to the main file
        let added: Vec<String> = l.chapters.iter().filter(|c| crate::order::split_number(c).is_none()).map(new_name).collect();
        (renames, order_new, added)
    }) else {
        return;
    };
    if renames.is_empty() {
        with(|a| a.notice("the notes are already numbered in this order."));
        return;
    }
    let n = renames.len();
    rename_notes(renames, order_new, added.first().cloned()).await;
    with(|a| {
        a.notice(&format!("{n} note{} renamed to the chapter order.", if n == 1 { "" } else { "s" }));
        a.redisplay();
    });
}

/// The notes with fixed names that are missing from the notes folder, made with placeholders (user, 2026-10-27).
async fn make_role_notes() {
    let Some((vault, folder, l)) = with(|a| (a.vault.clone(), vault_path(&a.settings.notes_folder), a.layout())) else { return };
    let path = |name: &str| if folder.is_empty() { format!("{name}.md") } else { format!("{folder}/{name}.md") };
    let title_page = crate::starter::title_page_note();
    let notes: [(bool, &str, &str); 4] = [
        (l.preamble.is_none(), "0-preamble", crate::starter::PREAMBLE_NOTE),
        (l.title_page.is_none(), "1-title_page", title_page.as_str()),
        (l.glossary.is_none(), "glossary", "%% The glossary: one # heading per entry, with **Long form:** and a description under it. %%\n"),
        (l.appendix.is_none(), "appendix", "# Appendix\n\n%% The appendices: not counted in the word count. %%\n"),
    ];
    for (missing, name, text) in notes {
        if missing {
            _ = js::wait(call!(vault, create, path(name), text)).await;
            with(|a| a.log(&format!("made the missing note {name}.md (a placeholder)")));
        }
    }
}

/// Renames notes of the folder through Obsidian (which rewrites every link to them), then the settings that name
/// them, their old .tex files (if the plugin wrote them and they are unchanged) and the main file's \input lines.
/// `order`: the chapters' new names in order; `added`: a chapter the main file does not read yet.
async fn rename_notes(renames: Vec<(String, String)>, order: Vec<String>, added: Option<String>) {
    if renames.is_empty() {
        return;
    }
    let Some((app, vault, folder, out, appendix)) = with(|a| {
        let appendix = a.layout().appendix.map(|ap| renames.iter().find(|(o, _)| *o == ap).map_or(ap.clone(), |(_, n)| n.clone()));
        (get(&a.plugin, "app"), a.vault.clone(), vault_path(&a.settings.notes_folder), a.out(), appendix)
    }) else {
        return;
    };
    let path = |stem: &str| if folder.is_empty() { format!("{stem}.md") } else { format!("{folder}/{stem}.md") };
    let mut done = Vec::new();
    for (old, new) in &renames {
        let file = call!(vault, getAbstractFileByPath, path(old));
        if file.is_null() || file.is_undefined() {
            continue; // already renamed (another window, or Ignis)
        }
        if !call!(vault, getAbstractFileByPath, path(new)).is_null() {
            with(|a| {
                a.log(&format!("chapter order: not renamed {old} -> {new}: a note {new} is already there"));
                a.notice(&format!("{new}.md is already in the folder, so {old} was not renamed; rename one of them and move the chapter again."));
            });
            break;
        }
        match js::wait(call!(get(&app, "fileManager"), renameFile, file, path(new))).await {
            Ok(_) => done.push((old.clone(), new.clone())),
            Err(e) => {
                with(|a| a.log(&format!("chapter order: could not rename {old} -> {new}: {}", str(&e))));
                break;
            }
        }
    }
    // the settings that name a renamed note; its old .tex, if the plugin wrote it and nobody changed it since
    let Some(stale) = with(|a| {
        let s = &mut a.settings;
        for (old, new) in &done {
            for n in s.left_out.iter_mut() {
                if n.trim_end_matches(".md") == old {
                    *n = new.clone();
                }
            }
        }
        let stale: Vec<(String, Option<String>)> = done.iter().map(|(old, _)| format!("{old}.tex")).map(|t| (t.clone(), s.written.get(&t).cloned())).collect();
        a.save();
        stale
    }) else {
        return;
    };
    for (tex, written) in stale {
        let place = out.join(&tex);
        if written.is_some() && files::read(&place).await.map(|t| hash(&t)) == written {
            files::remove(&place).await;
            with(|a| _ = a.settings.written.remove(&tex));
        }
    }
    // the main file's \input lines
    let main = with(|a| out.join(&format!("{}.tex", a.settings.main())));
    let mut line = format!("chapter order: {}", done.iter().map(|(o, n)| format!("{o} -> {n}")).collect::<Vec<_>>().join(", "));
    if let Some(main) = main {
        if let Some(text) = files::read(&main).await {
            let next = crate::order::rewrite_main(&text, &done, &order, added.as_deref(), appendix.as_deref());
            if next != text {
                line += &match files::write(&main, &next).await {
                    Ok(()) => format!("; {} reads them in that order", main.shown("")),
                    Err(e) => format!("; could not update {}: {e}", main.shown("")),
                };
            }
        }
    }
    with(|a| a.log(&line));
}

fn scopes() -> Vec<(&'static str, &'static str)> {
    vec![("section", "The edited section"), ("chapter", "The whole chapter")]
}

/// The settings the word-count rules are made from: a change rebuilds them and recounts every chapter.
const RULE_KEYS: [&str; 6] = ["countStart", "countPrint", "countStop", "countOff", "wordcountRules", "inlineMathWord"];

#[wasm_bindgen]
pub struct Handle;

#[wasm_bindgen]
impl Handle {
    pub fn unload(&self) {
        compile::cancel();
        APP.with(|a| a.borrow_mut().take());
    }
}

/// Called by main.js: the plugin object, the `obsidian` module, `require` (null on a phone), the compiled module and
/// its bindings (unused: the plugin has no worker).
#[wasm_bindgen]
pub fn start(plugin: JsValue, obs: JsValue, require: JsValue, _module: JsValue, _glue: String) -> Handle {
    let vault = get(&get(&plugin, "app"), "vault");
    let app = App {
        adapter: get(&vault, "adapter"),
        require: require.dyn_into::<Function>().ok(),
        settings: Settings::default(),
        build: Build::default(),
        persist: None,
        notes: BTreeMap::new(),
        settle: HashMap::new(),
        quiet: None,
        status: Status::Idle,
        status_el: JsValue::UNDEFINED,
        ticking: false,
        warnings: (String::new(), 0),
        last_build: String::new(),
        ignis_ok: false,
        log_pending: String::new(),
        log_writing: false,
        sync: Sync::new(),
        counts: BTreeMap::new(),
        count_rules: Rules::default(),
        names: (0, Names::default()),
        glossary: Glossary::default(),
        pending: (false, false),
        preview: compile::Preview::default(),
        device: serde_wasm_bindgen::from_value(call!(get(&plugin, "app"), loadLocalStorage, crate::device::KEY)).unwrap_or_default(),
        tab: JsValue::UNDEFINED,
        plugin,
        obs,
        vault,
    };
    APP.with(|a| *a.borrow_mut() = Some(app));
    js::spawn(async {
        let Some(plugin) = with(|a| a.plugin.clone()) else { return };
        let data = js::wait(call!(plugin, loadData)).await.unwrap_or(JsValue::NULL);
        let settings = serde_wasm_bindgen::from_value::<Settings>(data).unwrap_or_default().fixed();
        with(|a| {
            a.settings = settings;
            a.move_commands();
            a.set_rules();
            a.register();
        });
    });
    Handle
}

/// Local time: "2026-10-25 12:04:31".
pub(crate) fn now_text() -> String {
    let d = Date::new_0();
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", d.get_full_year(), d.get_month() + 1, d.get_date(), d.get_hours(), d.get_minutes(), d.get_seconds())
}

/// "12:04", for the status bar.
pub fn clock() -> String {
    now_text()[11..16].to_string()
}

/// The settings as a plain object (maps as objects, not JavaScript Maps), which saveData stores as JSON.
fn to_js(s: &Settings) -> JsValue {
    s.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).unwrap_or_default()
}

fn hash(s: &str) -> String {
    format!("{:016x}", xxhash_rust::xxh3::xxh3_64(s.as_bytes()))
}

impl App {
    /// The settings tab, the status bar, the commands, and (once the vault is loaded) the notes and their events.
    fn register(&mut self) {
        let p = self.plugin.clone();
        let tab = new(&get(&self.obs, "PluginSettingTab"), &[get(&p, "app"), p.clone()]);
        let t = tab.clone();
        set(&tab, "display", f0(move || {
            let Some(obs) = with(|a| a.obs.clone()) else { return };
            settings::render(&t, &obs, UI, |k| with(|a| a.field(k)).unwrap_or(JsValue::UNDEFINED), |k, v| _ = with(|a| a.set_field(k, v)));
        }));
        call!(p, addSettingTab, tab.clone());
        self.tab = tab;
        self.status_el = call!(p, addStatusBarItem);
        call!(get(&self.status_el, "classList"), add, "latex-status", "mod-clickable");
        call!(p, registerDomEvent, self.status_el.clone(), "click", f1(|_| {
            open_log();
            JsValue::UNDEFINED
        }));
        self.show();
        let command = |id: &str, name: &str, f: JsValue| _ = call!(p, addCommand, crate::js::obj! { id: id, name: name, callback: f });
        command("build-all", "Build all chapters", f0(|| js::spawn(build_all(false))));
        command("build-and-compile", "Build all chapters and compile", f0(|| js::spawn(build_all(true))));
        command("compile", "Compile now", f0(compile::start_whole));
        command("stop", "Stop the build", f0(compile::cancel));
        command("open-log", "Open the log", f0(open_log));
        // Obsidian calls this at once when the layout is already ready (the plugin turned on later), that is inside this
        // borrow of the app, where `with` would do nothing: so the listening starts from a timer
        call!(get(&get(&p, "app"), "workspace"), onLayoutReady, f0(|| {
            js::timeout(0.0, || _ = with(|a| a.listen()));
            js::spawn(async {
                make_notes_folder().await;
                load_notes(true).await;
                load_glossary().await;
                load_bib().await;
                recount_all().await;
                write_counts().await;
                scroll::enable().await;
                crate::remote::read().await;
                compile::warm_up().await;
                // the note already open when the plugin starts, as if it had just been opened
                let active = with(|a| str(&get(&call!(get(&get(&a.plugin, "app"), "workspace"), getActiveFile), "path"))).unwrap_or_default();
                if !active.is_empty() {
                    scroll::opened(active);
                }
            });
        }));
    }

    /// The vault's events for notes in the notes folder.
    fn listen(&self) {
        let p = self.plugin.clone();
        let on = |event: &str, f: JsValue| _ = call!(p, registerEvent, call!(self.vault, on, event, f));
        // a device that does not build also follows the computer's builds through two files of the output folder
        // (remote.rs); from a timer, as Obsidian may fire these inside a call made from within `with`
        let followed = |path: String| _ = js::timeout(0.0, move || crate::remote::changed(&path));
        on("modify", f1(move |file| {
            let path = str(&get(&file, "path"));
            with(|a| a.saved(&path));
            followed(path);
            JsValue::UNDEFINED
        }));
        on("create", f1(move |file| {
            let path = str(&get(&file, "path"));
            with(|a| a.saved(&path));
            followed(path);
            JsValue::UNDEFINED
        }));
        on("delete", f1(|file| {
            with(|a| {
                if let Some(name) = a.note_name(&str(&get(&file, "path"))) {
                    a.notes.remove(&name);
                    a.sync.maps.remove(&name);
                }
            });
            JsValue::UNDEFINED
        }));
        on("rename", f2(|file, old| {
            with(|a| {
                if let Some(name) = a.note_name(&str(&old)) {
                    a.notes.remove(&name);
                    a.sync.maps.remove(&name);
                }
                let path = str(&get(&file, "path"));
                a.saved(&path);
            });
            JsValue::UNDEFINED
        }));
        // the PDF beside a chapter note when it is opened, and the panes sync scrolling listens to; Obsidian fires these
        // inside its own calls, some made from within `with`, so they act from a timer
        let workspace = get(&get(&p, "app"), "workspace");
        let on_workspace = |event: &str, f: JsValue| _ = call!(p, registerEvent, call!(workspace, on, event, f));
        on_workspace("file-open", f1(|file| {
            let path = str(&get(&file, "path"));
            if !path.is_empty() {
                js::timeout(0.0, move || scroll::opened(path));
            }
            JsValue::UNDEFINED
        }));
        for event in ["layout-change", "active-leaf-change"] {
            on_workspace(event, f1(|_| {
                js::timeout(0.0, scroll::attach);
                JsValue::UNDEFINED
            }));
        }
        // "Apply ordering" in the notes folder's right-click menu, and only there (user, 2026-10-27: not a command): the
        // notes numbered to the chapter order kept in the settings
        on_workspace("file-menu", f2(|menu, file| {
            let path = str(&get(&file, "path"));
            let path = if path == "/" { String::new() } else { path };
            let is_folder = get(&file, "children").is_object();
            if is_folder && with(|a| vault_path(&a.settings.notes_folder) == path).unwrap_or(false) {
                call!(menu, addItem, f1(|item| {
                    call!(item, setTitle, "Apply ordering");
                    call!(item, setIcon, "list-ordered");
                    call!(item, onClick, f0(|| js::spawn(apply_ordering())));
                    JsValue::UNDEFINED
                }));
            }
            JsValue::UNDEFINED
        }));
    }

    /// The folder's notes by role and the chapters in order (order.rs).
    pub(crate) fn layout(&self) -> crate::order::Layout {
        let stems: Vec<String> = self.notes.keys().map(|n| n.trim_end_matches(".md").to_string()).collect();
        crate::order::Layout::of(&stems)
    }

    /// The settings tab drawn again (after a change it shows), from a timer: Obsidian's display() calls back into the app.
    fn redisplay(&self) {
        let tab = self.tab.clone();
        if !tab.is_undefined() {
            js::timeout(0.0, move || _ = call!(tab, display));
        }
    }

    /// A note's file name if `path` is a note of the folder (not in a subfolder, not left out).
    pub(crate) fn note_name(&self, path: &str) -> Option<String> {
        let folder = vault_path(&self.settings.notes_folder);
        let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
        // Markdown notes only (a PDF or picture in the folder is not a chapter), and never the folder's README
        let stem = name.strip_suffix(".md")?;
        (dir == folder && !stem.eq_ignore_ascii_case("readme") && !convert::words(&self.settings.ignore).iter().any(|x| x.trim_end_matches(".md") == stem)).then(|| name.to_string())
    }

    /// A note was saved (or created, or renamed into the folder): converted once its writes have settled.
    fn saved(&mut self, path: &str) {
        if self.note_name(path).is_none() {
            return;
        }
        if let Some(h) = self.settle.remove(path) {
            js::clear_timeout(h);
        }
        let p = path.to_string();
        let h = js::timeout(SETTLE_MS, move || {
            with(|a| a.settle.remove(&p));
            js::spawn(handle(p));
        });
        self.settle.insert(path.to_string(), h);
    }

    /// The converter's settings from the plugin's.
    pub fn options(&self) -> Options {
        let s = &self.settings;
        let cmd = |c: &str| c.trim().trim_start_matches('\\').to_string();
        let l = self.layout();
        Options {
            header: s.header.clone(),
            vault: self.vault_name(),
            folder: vault_path(&s.notes_folder),
            main: s.main(),
            preamble: l.preamble.clone().unwrap_or_else(|| "0-preamble".into()),
            glossary: self.glossary_note(),
            title_page: l.title_page.clone().unwrap_or_else(|| "1-title_page".into()),
            tikz_libraries: s.tikz_libraries.clone(),
            listing_env: s.listing_env.trim().to_string(),
            listing_cmd: cmd(&s.listing_command),
            wordcount: s.wordcount_property.trim().to_string(),
            count_start: cmd(&s.count_start),
            count_print: cmd(&s.count_print),
            count_stop: cmd(&s.count_stop),
            updated: s.updated_property.trim().to_string(),
            commands: convert::words(&s.commands).iter().map(|c| cmd(c)).collect(),
            ref_words: convert::words(&s.ref_words),
            abbreviations: convert::words(&s.abbreviations),
            lists: convert::parse_lists(&s.lists),
            term: cmd(&s.term_command),
            title_prefix: s.title_prefix.trim().to_string(),
            make_title: cmd(&s.make_title),
            line_chars: s.line_chars.max(0.0) as usize,
            wrap_chars: s.wrap_chars.max(0.0) as usize,
            small_from: s.small_from.max(1.0) as usize,
            small_words: convert::words(&s.small_words),
        }
    }

    pub fn vault_name(&self) -> String {
        str(&call!(self.vault, getName))
    }

    /// The output folder: always in the vault (the notes folder is a vault path; an absolute path typed there is read
    /// as one), so that the plugin writes nothing outside the vault.
    pub fn out(&self) -> Place {
        Place::Vault(vault_path(&self.settings.output_folder))
    }

    /// The vault's folder on this computer (FileSystemAdapter.getBasePath; desktop only).
    pub fn base(&self) -> Option<String> {
        get(&self.adapter, "getBasePath").is_function().then(|| str(&call!(self.adapter, getBasePath))).filter(|b| !b.is_empty())
    }

    /// A Node module, from within `with` (files::node is the same outside it).
    pub fn node(&self, name: &str) -> Option<JsValue> {
        files::node_in(&self.require, name)
    }

    /// Whether this device can compile: desktop Obsidian (Ignis in a browser has a child_process stand-in, which must
    /// not be asked to run programs), Node's child_process and a vault on disk.
    pub fn can_compile(&self) -> bool {
        js::desktop() && self.node("child_process").is_some() && self.base().is_some()
    }

    /// Commands an older version kept in data.json move to this device once (desktop only, where they run); data.json
    /// keeps the defaults, which are never run from there.
    fn move_commands(&mut self) {
        let (compile, highlight) = (self.settings.compile_command.trim().to_string(), self.settings.highlight_command.trim().to_string());
        let custom_compile = !compile.is_empty() && !crate::pipeline::is_default_command(&compile);
        let custom_highlight = !highlight.is_empty() && highlight != crate::device::DEFAULT_HIGHLIGHT;
        if !js::desktop() || !(custom_compile || custom_highlight) {
            return;
        }
        if custom_compile && self.device.compile_command.is_empty() {
            self.device.compile_command = compile;
        }
        if custom_highlight && self.device.highlight_command.is_empty() {
            self.device.highlight_command = highlight;
        }
        self.settings.compile_command = crate::pipeline::DEFAULT_COMMAND.into();
        self.settings.highlight_command = crate::device::DEFAULT_HIGHLIGHT.into();
        self.save_device();
        self.save();
    }

    pub fn save_device(&self) {
        let v = serde_wasm_bindgen::to_value(&self.device).unwrap_or_default();
        call!(get(&self.plugin, "app"), saveLocalStorage, crate::device::KEY, v); // a plain object: no maps in Device
    }

    fn compiling(&self) -> bool {
        self.settings.compile && (self.can_compile() || self.ignis_ok)
    }

    // ----- the log

    /// One timestamped entry in the log; continuation lines are indented (latex_sync.py's format). Entries are
    /// appended in order, one write at a time.
    pub fn log(&mut self, message: &str) {
        self.log_pending += &format!("{} {message}\n", now_text());
        if !self.log_writing {
            self.log_writing = true;
            js::spawn(flush_log());
        }
    }

    /// A build's or a preview's start or end, for the devices that do not build (remote.rs): written to the output
    /// folder's latex-status.json, by the desktop app only and only for an output folder in the vault.
    pub(crate) fn report(&self, mut r: crate::remote::Report) {
        if !self.can_compile() || !matches!(self.out(), Place::Vault(_)) {
            return;
        }
        r.at = now_text();
        r.when = Date::now();
        if let Ok(text) = serde_json::to_string_pretty(&r) {
            crate::remote::write_later(self.out().join(crate::remote::FILE), text);
        }
    }

    // ----- the status bar

    pub fn set_status(&mut self, s: Status) {
        self.status = s;
        self.show();
    }

    /// The status bar's text and tooltip, from the status; while waiting or compiling it is redrawn every second.
    pub fn show(&mut self) {
        let now = js::now();
        let (count, from) = (self.warnings.1, &self.warnings.0);
        let warned = if count > 0 { format!(" · {count} warning{}", if count == 1 { "" } else { "s" }) } else { String::new() };
        let (text, detail) = match &self.status {
            Status::Idle => ("idle".to_string(), "Watching the notes folder.".to_string()),
            Status::Converting { note, tex } => (format!("converting {note} → {tex}"), "Markdown to LaTeX: the note's chapter is being written.".into()),
            Status::Converted { at } => (format!("converted {at}{warned}"), "The chapters are up to date.".into()),
            Status::NotConverted { note } => (format!("{note} not converted — see log"), "The note could not be converted; its .tex is unchanged.".into()),
            Status::Previewed { until } => {
                let left = ((until - now) / 1000.0).max(0.0).ceil();
                (format!("PDF current{} · full build when idle ({left:.0} s)", self.preview_note()), "Fast preview has put every change into the PDF and every page is current; the full build, which only confirms it, runs once the notes have been idle for the setting Full build when idle.".into())
            }
            Status::Waiting { until } => {
                let left = ((until - now) / 1000.0).max(0.0).ceil();
                let still = if self.build.running { format!("; {} still running", self.build.stage.now) } else { String::new() };
                (format!("waiting for quiet ({left:.0} s){still}{}{warned}", self.preview_note()), "The chapters are written; the document is compiled once the notes have been quiet.".into())
            }
            Status::Compiling => (format!("compiling — {} · {:.0} s", self.build.stage.now, (now - self.build.since) / 1000.0), "The LaTeX build is running.".into()),
            Status::Compiled { at, errors, undefined } => (
                format!("compiled {at} ({errors} error{}, {undefined} undefined ref{}){warned}", if *errors == 1 { "" } else { "s" }, if *undefined == 1 { "" } else { "s" }),
                "The last build finished.".into(),
            ),
            Status::Failed => (format!("build failed — see log{warned}"), "The last build stopped with an error.".into()),
            Status::Stopped => ("build stopped".into(), "The build was stopped.".into()),
            Status::NoCompiler => (format!("converted {}{warned}", clock()), "Compiling needs Obsidian's desktop app; the chapters are converted.".into()),
            Status::Remote(r) => crate::remote::text(r, Date::now(), &now_text()[..10]),
        };
        let mut tip = detail;
        if count > 0 {
            tip += &format!("\nThe last conversion ({from}) gave {count} warning{} (in the log).", if count == 1 { "" } else { "s" });
        }
        if !self.last_build.is_empty() {
            tip += &format!("\nLast build: {}", self.last_build);
        }
        let flat = self.flat_counts();
        if !flat.is_empty() {
            let total: usize = flat.iter().map(|(_, n)| n).sum();
            let all: Vec<String> = flat.iter().map(|(k, n)| format!("{k} {n}")).collect();
            tip += &format!("\nWords: {total} ({})", all.join(", "));
        }
        tip += "\nClick to open the log.";
        set(&self.status_el, "textContent", format!("LaTeX: {text}"));
        call!(self.status_el, setAttribute, "aria-label", tip);
        call!(self.status_el, setAttribute, "data-tooltip-position", "top");
        let live = matches!(self.status, Status::Waiting { .. } | Status::Compiling);
        if live && !self.ticking {
            self.ticking = true;
            js::timeout(1000.0, tick);
        }
    }

    /// A notice: <https://docs.obsidian.md/Reference/TypeScript+API/Notice>.
    pub(crate) fn notice(&self, text: &str) {
        new(&get(&self.obs, "Notice"), &[format!("LaTeX: {text}").into(), 8000.0.into()]);
    }

    pub fn cancel_quiet(&mut self) {
        if let Some(h) = self.quiet.take() {
            js::clear_timeout(h);
        }
    }

    /// The quiet period starts again: the build follows when it ends.
    fn compile_later(&mut self) {
        self.cancel_quiet();
        // a change not yet previewed (the preamble, a glossary, a chapter turned on or off): the PDF is not current
        // until a preview says so
        self.preview.current = false;
        let ms = self.settings.quiet_seconds.max(0.0) * 1000.0;
        self.quiet = Some(js::timeout(ms, || {
            if with(|a| {
                a.quiet = None;
                a.settle.is_empty() // a note still settling restarts the wait when it is converted
            }) == Some(true)
            {
                compile::quiet_ended();
            }
        }));
        self.set_status(Status::Waiting { until: js::now() + ms });
    }

    // ----- settings

    fn field(&self, key: &str) -> JsValue {
        get(&to_js(&self.settings), key)
    }

    fn set_field(&mut self, key: &str, v: JsValue) {
        let o = to_js(&self.settings);
        set(&o, key, v);
        if let Ok(s) = serde_wasm_bindgen::from_value::<Settings>(o) {
            self.settings = s.fixed();
        }
        self.pending.0 |= key == "notesFolder" || key == "ignore";
        self.pending.1 |= RULE_KEYS.contains(&key);
        self.save();
        if key == "syncScroll" && self.settings.sync_scroll {
            js::timeout(0.0, || js::spawn(scroll::enable()));
        }
    }

    /// Whether a note (its name, with or without .md) is left out of the PDF.
    pub(crate) fn left_out(&self, note: &str) -> bool {
        let stem = note.trim_end_matches(".md");
        let base = crate::order::base(stem);
        self.settings.left_out.iter().any(|n| crate::order::base(n.trim_end_matches(".md")).eq_ignore_ascii_case(base))
    }

    /// A chapter turned on or off in the settings: its .tex is written again, with or without its \endinput.
    fn include(&mut self, stem: &str, on: bool) {
        let base = crate::order::base(stem).to_string();
        self.settings.left_out.retain(|n| !crate::order::base(n.trim_end_matches(".md")).eq_ignore_ascii_case(&base));
        if !on {
            self.settings.left_out.push(base);
        }
        self.save();
        let name = format!("{stem}.md");
        if let Some(md) = self.notes.get(&name).cloned() {
            js::timeout(0.0, move || js::spawn(toggled(name, md)));
        }
    }

    /// data.json after a pause, so typing in a text field writes once; then the notes are read again if the folder
    /// changed, and the word-count rules rebuilt (and every chapter recounted) if one of their settings did.
    fn save(&mut self) {
        if let Some(h) = self.persist.take() {
            js::clear_timeout(h);
        }
        self.persist = Some(js::timeout(500.0, || {
            let Some((reload, rules)) = with(|a| {
                a.persist = None;
                call!(a.plugin, saveData, to_js(&a.settings));
                std::mem::take(&mut a.pending)
            }) else {
                return;
            };
            if reload {
                js::spawn(load_notes(false));
            }
            if rules {
                with(|a| a.set_rules());
                js::spawn(async {
                    recount_all().await;
                    write_counts().await;
                });
            }
        }));
    }

    // ----- fast preview

    /// " · preview 0.3 s ..." after the status, while a preview is the newest PDF.
    fn preview_note(&self) -> String {
        if self.preview.last.is_empty() {
            String::new()
        } else {
            format!(" · {}", self.preview.last)
        }
    }

    pub fn host(&self, build: Option<(u32, JsValue)>) -> crate::host::Node {
        crate::host::Node { dir: self.out().absolute(self.base().as_deref()).unwrap_or_default(), build }
    }

    pub fn preview_options(&self) -> crate::preview::Options {
        let s = &self.settings;
        let o = self.options();
        let scope = if s.preview_scope == "chapter" { crate::pipeline::fast::Scope::Chapter } else { crate::pipeline::fast::Scope::Section { span: 2 } };
        crate::preview::Options { main: s.main(), preamble: o.preamble, term: o.term, glossary: o.glossary, scope, cache_tikz: s.cache_tikz, debug: s.debug_log }
    }

    /// A preview's outcome: logged, the status bar, the merged SyncTeX index and the PDF shown. True when the full build
    /// should start at once.
    pub fn previewed(&mut self, r: Result<crate::preview::Outcome, String>) -> bool {
        self.preview.count += 1;
        self.preview.current = matches!(&r, Ok(out) if out.shown && out.stale.is_empty() && !out.build_now && out.problems.iter().all(|p| p.contains("destinations are both")));
        if self.settings.debug_log {
            self.log(&format!("debug: the preview {} every page current", if self.preview.current { "left" } else { "did not leave" }));
        }
        match r {
            Ok(out) => {
                let stale: Vec<&str> = out.stale.iter().map(|(_, w)| w.as_str()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
                self.preview.last = format!("preview {:.1} s{}", out.ms / 1000.0, if stale.is_empty() { String::new() } else { format!(" · {}", stale.join(", ")) });
                let problems: String = out.problems.iter().map(|p| format!("\n  warning: {p}")).collect();
                let line = format!("Preview in {:.2} s: {}{}", out.ms / 1000.0, out.steps.join("; "), problems);
                // a preview that put nothing into the PDF leaves it to the full build
                let state = if out.shown { "built" } else { "skipped" };
                self.report(crate::remote::Report { seconds: out.ms / 1000.0, summary: line.clone(), ..crate::remote::Report::new(state, "preview") });
                self.log(&line);
                if out.restored {
                    js::timeout(0.0, || js::spawn(scroll::built()));
                } else {
                    let bands = crate::splice::bands(&out.pages);
                    js::timeout(0.0, move || js::spawn(scroll::spliced(bands)));
                }
                self.show();
                out.build_now
            }
            Err(e) => {
                self.report(crate::remote::Report { summary: format!("Not previewed: {e}"), ..crate::remote::Report::new("skipped", "preview") });
                self.log(&format!("Not previewed: {e}"));
                false
            }
        }
    }

    // ----- word counts

    /// The word-count rules from the settings: the built-in table, the counter's macros, the user's rules; each line of
    /// those that is not a rule is logged.
    fn set_rules(&mut self) {
        let s = &self.settings;
        let cmd = |c: &str| c.trim().trim_start_matches('\\').to_string();
        let m = Macros { start: cmd(&s.count_start), print: cmd(&s.count_print), stop: cmd(&s.count_stop), off: cmd(&s.count_off) };
        let (mut rules, errors) = Rules::new(&s.wordcount_rules, &m);
        rules.inline_math_word = s.inline_math_word;
        self.count_rules = rules;
        for e in errors {
            self.log(&format!("warning: {e}")); // "wordcountRules, line n: ..."
        }
    }

    /// A chapter counted with the rules, names and glossary as they are now.
    fn count(&self, tex: &str) -> wordcount::Counts {
        count_chapter(tex, &Context { rules: &self.count_rules, names: &self.names.1, glossary: &self.glossary, cites: true })
    }

    /// A chapter's counts kept, or dropped when it has no counted section.
    fn keep_counts(&mut self, tex_name: &str, sections: Vec<(String, usize)>) {
        if sections.is_empty() {
            self.counts.remove(tex_name);
        } else {
            self.counts.insert(tex_name.to_string(), sections);
        }
    }

    /// Every counted section, the chapters in their order (their names sort as the document does: 3-, 4- ...).
    fn flat_counts(&self) -> Vec<(String, usize)> {
        self.counts.values().flatten().cloned().collect()
    }

    /// The .bib, in the output folder or at an absolute path.
    fn bib_place(&self) -> Place {
        let b = self.settings.bib_file.trim();
        if files::is_absolute(b) {
            Place::Disk(b.to_string())
        } else {
            self.out().join(b)
        }
    }

    /// Whether a note's sections are counted (its word-count property).
    fn counted(&self, md: &str) -> bool {
        pipeline::counted(md, self.settings.wordcount_property.trim())
    }

    /// The .tex of every counted note of the folder.
    fn counted_texs(&self) -> Vec<String> {
        self.notes.iter().filter(|(_, md)| self.counted(md)).map(|(n, _)| pipeline::tex_name(n)).collect()
    }

    /// The glossary note's name: the folder's note whose name, without its number, is the setting's (so `glossary`
    /// still finds `9-glossary` after the chapters were renumbered by hand), or the setting as it is.
    pub(crate) fn glossary_note(&self) -> String {
        self.layout().glossary.unwrap_or_else(|| crate::order::GLOSSARY.to_string())
    }

    /// The glossary note's .tex.
    fn glossary_tex(&self) -> String {
        pipeline::tex_name(&self.glossary_note())
    }
}

fn tick() {
    with(|a| {
        a.ticking = false;
        a.show();
    });
}

async fn flush_log() {
    loop {
        let Some((place, text)) = with(|a| {
            let text = std::mem::take(&mut a.log_pending);
            if text.is_empty() {
                a.log_writing = false;
            }
            (a.out().join(&a.settings.log_file), text)
        }) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        if let Err(e) = files::append(&place, &text).await {
            with(|a| a.notice(&format!("could not write the log {}: {e}", place.shown(&a.vault_name()))));
        }
    }
}

/// Every note of the folder, read for the index; `starting`: also the log's first line (and its trim), as the watcher's.
/// The notes folder made when it is missing (a new install). Nothing is downloaded into it: the README is linked from
/// the settings (user, 2026-10-27: no network call users did not agree to).
async fn make_notes_folder() {
    let Some((vault, folder)) = with(|a| (a.vault.clone(), vault_path(&a.settings.notes_folder))) else { return };
    if folder.is_empty() || call!(vault, getAbstractFileByPath, folder.as_str()).is_object() {
        return;
    }
    let line = match js::wait(call!(vault, createFolder, folder.as_str())).await {
        Ok(_) => format!("made the notes folder {folder}: put the chapters' notes in it (the README, linked in the settings, says how)"),
        Err(e) => format!("could not make the notes folder {folder}: {}", str(&get(&e, "message"))),
    };
    with(|a| a.log(&line));
}

async fn load_notes(starting: bool) {
    let Some((vault, files)) = with(|a| (a.vault.clone(), Array::from(&call!(a.vault, getMarkdownFiles)))) else { return };
    let mut notes = BTreeMap::new();
    for f in files.iter() {
        let path = str(&get(&f, "path"));
        if let Some(name) = with(|a| a.note_name(&path)).flatten() {
            if let Ok(text) = js::wait(call!(vault, cachedRead, f)).await {
                notes.insert(name, str(&text));
            }
        }
    }
    let Some((log, notes_place, out, vault_name, base)) = with(|a| {
        a.notes = notes;
        (a.out().join(&a.settings.log_file), Place::folder(&a.settings.notes_folder), a.out(), a.vault_name(), a.base())
    }) else {
        return;
    };
    if starting {
        if let Some(cut) = files::read(&log).await.as_deref().and_then(pipeline::trimmed) {
            _ = files::write(&log, &cut).await;
        }
        let shown = |p: &Place| p.absolute(base.as_deref()).unwrap_or_else(|| p.shown(&vault_name));
        let line = format!("watching {} -> {}", shown(&notes_place), shown(&out));
        with(|a| a.log(&line));
    }
}

/// A saved note: its last-updated stamp, its chapter, and the build after the quiet period.
async fn handle(path: String) {
    let Some((vault, file)) = with(|a| (a.vault.clone(), call!(a.vault, getAbstractFileByPath, path.as_str()))) else { return };
    let Some(name) = with(|a| a.note_name(&path)).flatten() else { return };
    if file.is_null() || file.is_undefined() {
        return;
    }
    let Ok(text) = js::wait(call!(vault, read, file.clone())).await else { return };
    let mut md = str(&text);
    // the text changed (not only the properties): the last-updated property is set to now, atomically
    let stamp = with(|a| {
        let s = &a.settings;
        let changed = a.notes.get(&name).is_some_and(|old| pipeline::body(old) != pipeline::body(&md));
        (s.stamp_updated && changed).then(|| s.updated_property.trim().to_string())
    })
    .flatten();
    if let Some(key) = stamp {
        let when = now_text()[..16].to_string();
        if pipeline::stamp(&md, &key, &when).is_some() {
            let f = f1(move |data| pipeline::stamp(&str(&data), &key, &when).unwrap_or_else(|| str(&data)).into());
            if let Ok(t) = js::wait(call!(vault, process, file, f)).await {
                md = str(&t);
            }
        }
    }
    if js::in_ignis() {
        crate::ignis::check().await;
    }
    let Some(building) = with(|a| {
        a.notes.insert(name.clone(), md.clone());
        a.compiling()
    }) else {
        return;
    };
    if convert_note(&name, &md, building).await {
        with(|a| {
            if building && a.settings.fast() && a.can_compile() {
                if name.trim_end_matches(".md") == a.options().preamble {
                    compile::stop_worker_later();
                } else if !a.left_out(&name) {
                    js::timeout(0.0, compile::preview_later);
                }
            }
            if building {
                a.compile_later();
            } else {
                a.set_status(if a.settings.compile { Status::NoCompiler } else { Status::Converted { at: clock() } });
            }
        });
    }
}

/// A note to its .tex in the output folder, logged with its warnings ("; building" when a build follows). True when the
/// .tex changed; when it did not, the status bar goes back to what it said before.
async fn convert_note(name: &str, md: &str, building: bool) -> bool {
    if load_bib().await {
        recount_all().await;
    }
    let Some((idx, o, tex_name, out, s, before, left_out)) = with(|a| {
        let tex = pipeline::tex_name(name);
        let before = std::mem::replace(&mut a.status, Status::Converting { note: name.to_string(), tex: tex.clone() });
        a.show();
        let o = a.options();
        (Index::new(a.notes.iter().map(|(n, t)| (n.as_str(), t.as_str())), &o), o, tex, a.out(), a.settings.clone(), before, a.left_out(name))
    }) else {
        return false;
    };
    let unchanged = move || {
        with(|a| {
            if matches!(a.status, Status::Converting { .. }) {
                a.set_status(before);
            }
        });
        false
    };
    let (tex, mut warn, sections, code) = match md_to_tex_code(name, md, &idx, &o) {
        Ok((tex, mut warn, map, code)) => {
            // the chapter's word counts, its warnings placed in the note; the note's line map, for sync scrolling
            let sections = with(|a| {
                let c = if a.counted(md) { a.count(&tex) } else { wordcount::Counts::default() };
                warn.extend(wordcount::in_note(name, &map, &c.warnings));
                a.sync.maps.insert(name.to_string(), std::rc::Rc::new(map));
                c.sections
            })
            .unwrap_or_default();
            (tex, warn, sections, code)
        }
        Err(e) => {
            with(|a| {
                a.log(&format!("{name}: NOT converted: {e}"));
                a.notice(&format!("{name} was not converted: {e}"));
                a.set_status(Status::NotConverted { note: name.to_string() });
            });
            return false;
        }
    };
    let tex = if left_out { pipeline::leave_out(&tex) } else { tex };
    // a device that cannot compile (Ignis in a browser, a phone) writes nothing: the computer's Obsidian converts the
    // saved note itself and builds it. Were the .tex written here first, the computer would find it unchanged and
    // not build (user, 2026-10-28), and take it for a hand edit and back it up.
    if with(|a| a.can_compile() || a.ignis_ok) != Some(true) {
        with(|a| {
            a.warnings = (name.to_string(), warn.len());
            a.keep_counts(&tex_name, sections);
        });
        return unchanged();
    }
    let code_changed = write_code(&code, &out, &s, &mut warn).await;
    let target = out.join(&tex_name);
    let old = files::read(&target).await.unwrap_or_default();
    if old == tex {
        if !code_changed {
            return unchanged();
        }
        // only a block's code changed: the chapter is the same, the build shows the new code
        with(|a| {
            a.log(&format!("{name}: code changed{}", warn.iter().map(|w| format!("\n  warning: {w}")).collect::<String>()));
            a.save();
        });
        return true;
    }
    let edited = !old.is_empty() && s.written.get(&tex_name) != Some(&hash(&old));
    if s.backups && edited {
        let stem = tex_name.strip_suffix(".tex").unwrap_or(&tex_name);
        let d = Date::new_0();
        let when = format!("{:04}{:02}{:02}-{:02}{:02}{:02}", d.get_full_year(), d.get_month() + 1, d.get_date(), d.get_hours(), d.get_minutes(), d.get_seconds());
        let copy = out.join(&s.backups_folder).join(&format!("{stem}.{when}.tex"));
        match files::copy(&target, &copy).await {
            Ok(()) => warn.push(format!("{tex_name} had been edited outside the note; the edited copy is in {}/", s.backups_folder)),
            Err(e) => warn.push(format!("{tex_name} had been edited outside the note, and could not be backed up ({e})")),
        }
    }
    if let Err(e) = files::write(&target, &tex).await {
        with(|a| {
            a.log(&format!("{name}: could not write {tex_name}: {e}"));
            a.notice(&format!("could not write {tex_name}: {e}"));
            a.set_status(Status::NotConverted { note: name.to_string() });
        });
        return false;
    }
    let glossary = with(|a| {
        a.settings.written.insert(tex_name.clone(), hash(&tex));
        a.save();
        let building = if building { "; building" } else { "" };
        let left_out = if left_out { " (left out of the PDF)" } else { "" };
        a.log(&format!("{name} -> {tex_name}{left_out}{building}{}", warn.iter().map(|w| format!("\n  warning: {w}")).collect::<String>()));
        a.warnings = (name.to_string(), warn.len());
        a.keep_counts(&tex_name, sections);
        // the glossary's entries are what \gls prints in every chapter
        let glossary = name.trim_end_matches(".md") == o.glossary;
        if glossary {
            a.glossary = Glossary::from_tex(&tex, &o.term);
        }
        glossary
    })
    .unwrap_or(false);
    if glossary {
        recount_all().await;
    }
    write_counts().await;
    true
}

/// A chapter turned on or off in the settings: converted again, then built after the quiet period by the whole recipe
/// (not one LuaLaTeX run), since its citations, labels and terms come or go with it.
async fn toggled(name: String, md: String) {
    let Some(building) = with(|a| a.compiling()) else { return };
    if convert_note(&name, &md, building).await {
        with(|a| {
            if building {
                a.compile_later();
            } else {
                a.set_status(if a.settings.compile { Status::NoCompiler } else { Status::Converted { at: clock() } });
            }
        });
    }
}

/// A note's blocks of code in the output's code folder (`code/NAME.EXT`), each highlighted into the `code/NAME.tex` its
/// listing inputs (the preamble's \handtypedcode). Only a block whose code changed is written and highlighted. A code
/// file edited by hand is backed up first, as a hand-edited .tex is. True when any changed.
async fn write_code(code: &[CodeFile], out: &files::Place, s: &Settings, warn: &mut Vec<String>) -> bool {
    if !code.is_empty() {
        write_pygments_style(out, warn).await;
    }
    let mut changed = false;
    for f in code {
        let rel = format!("code/{}", f.name);
        let place = out.join(&rel);
        let old = files::read(&place).await;
        if old.as_deref() == Some(f.text.as_str()) {
            continue;
        }
        if let Some(old) = old.filter(|o| s.written.get(&rel) != Some(&hash(o))) {
            let copy = out.join(&s.backups_folder).join(&f.name);
            match files::write(&copy, &old).await {
                Ok(()) => warn.push(format!("{rel} had been written outside its note; that copy is in {}/", s.backups_folder)),
                Err(e) => warn.push(format!("{rel} had been written outside its note, and could not be backed up ({e})")),
            }
        }
        if let Err(e) = files::write(&place, &f.text).await {
            warn.push(format!("could not write {rel}: {e}"));
            continue;
        }
        changed = true;
        let stem = f.name.rsplit_once('.').map_or(f.name.as_str(), |(stem, _)| stem);
        let Some(template) = with(|a| a.device.highlight_command().to_string()) else { continue };
        let line = template.replace("%LANG%", &f.lang).replace("%SRC%", &rel).replace("%OUT%", &format!("code/{stem}.tex"));
        let cmd = shlex::split(&line).unwrap_or_default();
        if let Some(Err(e)) = with(|a| a.device.allows(&cmd)) {
            warn.push(format!("{rel} was not highlighted: {e}"));
            continue;
        }
        let Some(host) = with(|a| a.host(None)) else { continue };
        use crate::preview::Host;
        match host.run(&cmd).await {
            Ok((0, _)) => {}
            Ok((code, output)) => warn.push(format!("highlighting {rel} failed ({code}): {}", output.trim().lines().last().unwrap_or(""))),
            Err(e) => warn.push(format!("could not highlight {rel}: {e}")),
        }
        with(|a| _ = a.settings.written.insert(rel, hash(&f.text)));
    }
    changed
}

/// `code/pygments-style.tex`, the colours (\PY, \PYZ..) every highlighted listing uses, written from the bundled
/// Pygments default style when it is missing, so that the first listing builds with no `pygmentize -S` run by hand
/// (starter.rs). A file already there is never touched: it may be another style.
async fn write_pygments_style(out: &files::Place, warn: &mut Vec<String>) {
    let place = out.join(crate::starter::PYGMENTS_STYLE_FILE);
    if files::exists(&place).await {
        return;
    }
    match files::write(&place, crate::starter::PYGMENTS_STYLE).await {
        Ok(()) => _ = with(|a| a.log(&format!("wrote {} (Pygments' default style), which the listings need", crate::starter::PYGMENTS_STYLE_FILE))),
        Err(e) => warn.push(format!("could not write {}: {e}; the listings cannot be set without it (\\PY undefined)", crate::starter::PYGMENTS_STYLE_FILE)),
    }
}

// ----- word counts

/// The glossary's entries from its .tex in the output folder.
async fn load_glossary() {
    let Some((place, term)) = with(|a| (a.out().join(&a.glossary_tex()), a.options().term)) else { return };
    let tex = files::read(&place).await.unwrap_or_default();
    with(|a| a.glossary = Glossary::from_tex(&tex, &term));
}

/// The .bib read and hashed: true when it changed (its names are read again).
async fn load_bib() -> bool {
    let Some(place) = with(|a| a.bib_place()) else { return false };
    let bib = files::read(&place).await.unwrap_or_default();
    let h = xxhash_rust::xxh3::xxh3_64(bib.as_bytes());
    with(|a| {
        let changed = a.names.0 != h;
        if changed {
            a.names = (h, Names::from_bib(&bib));
        }
        changed
    })
    .unwrap_or(false)
}

/// Every chapter counted again from its .tex in the output folder (the rules, the .bib or the glossary changed).
async fn recount_all() {
    let Some((out, texs)) = with(|a| (a.out(), a.counted_texs())) else { return };
    let mut texts = Vec::new();
    for tex_name in texs {
        if let Some(tex) = files::read(&out.join(&tex_name)).await {
            texts.push((tex_name, tex));
        }
    }
    with(|a| {
        a.counts.clear();
        for (tex_name, tex) in &texts {
            let sections = a.count(tex).sections;
            a.keep_counts(tex_name, sections);
        }
        a.show();
    });
}

/// The counts file, written when its text changes, and logged (by the device that compiles, as the .tex files are).
async fn write_counts() {
    if with(|a| a.can_compile() || a.ignis_ok) != Some(true) {
        return;
    }
    let Some((place, file, flat, repeated)) = with(|a| {
        let flat = a.flat_counts();
        let mut seen: HashMap<&str, &str> = HashMap::new();
        let mut repeated = Vec::new();
        for (tex, sections) in &a.counts {
            for (k, _) in sections {
                if let Some(first) = seen.insert(k.as_str(), tex.as_str()) {
                    repeated.push(format!("the word-count key {k} is in {first} and {tex}: the preamble adds both to the total"));
                }
            }
        }
        (a.out().join(&a.settings.counts_file), a.settings.counts_file.clone(), flat, repeated)
    }) else {
        return;
    };
    let text = wordcount::counts_file(&flat);
    if files::read(&place).await.as_deref() == Some(text.as_str()) {
        return;
    }
    let total: usize = flat.iter().map(|(_, n)| n).sum();
    let all: Vec<String> = flat.iter().map(|(k, n)| format!("{k} {n}")).collect();
    let line = match files::write(&place, &text).await {
        Ok(()) if all.is_empty() => format!("word counts: no counted section -> {file}"),
        Ok(()) => format!("word counts: {} (total {total}) -> {file}", all.join(", ")),
        Err(e) => format!("could not write the word counts to {file}: {e}"),
    };
    with(|a| {
        a.log(&format!("{line}{}", repeated.iter().map(|w| format!("\n  warning: {w}")).collect::<String>()));
        a.show();
    });
}

/// Every note to its chapter (the commands), then, if asked, a build at once.
async fn build_all(and_compile: bool) {
    let Some((notes, building)) = with(|a| (a.notes.clone(), and_compile && a.can_compile())) else { return };
    let mut changed = 0;
    for (name, md) in &notes {
        changed += convert_note(name, md, building).await as usize;
    }
    with(|a| {
        a.log(&format!("built {} chapters, {changed} changed", notes.len()));
        if !and_compile && !matches!(a.status, Status::NotConverted { .. }) {
            a.set_status(Status::Converted { at: clock() });
        }
    });
    if and_compile {
        compile::start_whole();
    }
}

/// The log in the system's viewer for text (desktop: Electron's shell, <https://www.electronjs.org/docs/latest/api/shell>);
/// elsewhere its last lines in a notice.
fn open_log() {
    let Some((log, base)) = with(|a| (a.out().join(&a.settings.log_file), a.base())) else { return };
    if let (Some(path), Some(electron)) = (log.absolute(base.as_deref()), files::node("electron")) {
        call!(get(&electron, "shell"), openPath, path);
        return;
    }
    js::spawn(async move {
        let text = files::read(&log).await.unwrap_or_else(|| "the log is empty".into());
        let tail: Vec<&str> = text.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect();
        with(|a| a.notice(&tail.join("\n")));
    });
}
