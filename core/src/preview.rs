//! Fast preview, the steps (docs/fast-preview-design.md): the precompiled preamble kept current, the full build's files kept for
//! the previews, and a preview of an edit, from the change to the PDF shown. The steps are written once, against
//! `Host` (the files of the output folder and the programs run there): the plugin's host is Node's (compile.rs), the
//! tests' is std's (tests/fast).
//!
//! A preview: what the edit changed (pipeline::keys); biber first when the citation order changed (on a copy of the
//! last full build's .bcf, so one LaTeX run gives the right numbers); the waiting job typesets the edited section, and
//! the bibliography with it; a second pass when the first one's output changes a generated list (the contents, list of
//! figures or tables, the Terms list after makeglossaries, a label both defined and used in the section); then the
//! splice (splice.rs) into the last full build's PDF, which becomes the PDF shown.

use std::collections::{BTreeMap, BTreeSet};

use crate::pdf::Pdf;
use crate::pipeline::fast::{self, Region, Scope, Start};
use crate::pipeline::keys::{self, CiteChange};
use crate::splice::{self, Source};

/// The jobs' names: the format, the preview, biber's copy of the .bcf, makeglossaries' copy of the index.
pub const FMT: &str = "fp-preamble";
pub const JOB: &str = "fp-preview";
pub const CITE: &str = "fp-cite";
pub const GLS: &str = "fp-gls";
/// The last full build's files, kept for the previews (main.pdf becomes the spliced PDF, and a cancelled build deletes
/// main.aux).
pub const FULL: &str = "fp-full";
pub const KEPT: [&str; 12] = ["aux", "bbl", "bcf", "glo", "gls", "ist", "toc", "lof", "lot", "out", "pdf", "synctex.gz"];

/// How many starts after its end a region is typeset on through, to meet the full build again (fast::extend).
pub const CONVERGE: usize = 4;

/// The files and programs of the output folder.
#[allow(async_fn_in_trait)] // one implementation each: Node's, and std's in the tests
pub trait Host {
    async fn read(&self, name: &str) -> Option<Vec<u8>>;
    async fn write(&self, name: &str, data: &[u8]) -> Result<(), String>;
    /// A file's modification time (ms) and size; `path` absolute or in the output folder.
    async fn stat(&self, path: &str) -> Option<(f64, f64)>;
    /// A program run in the output folder (below-normal priority): its exit code and output.
    async fn run(&self, cmd: &[String]) -> Result<(i64, String), String>;
    /// The waiting preview job started; returns when it prints FPREADY (Err with its output if it ends first).
    async fn start_worker(&self, cmd: &[String]) -> Result<(), String>;
    /// Whether a started job is waiting.
    fn worker_waiting(&self) -> bool;
    /// The request sent to the waiting job; returns when it has ended: its exit code and output.
    async fn request(&self, line: &str) -> Result<(i64, String), String>;
    fn stop_worker(&self);
    fn log(&self, line: &str);
    fn now(&self) -> f64;
}

async fn text<H: Host>(h: &H, name: &str) -> String {
    h.read(name).await.map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

fn words(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// The fast preview's settings.
#[derive(Clone, Debug)]
pub struct Options {
    pub main: String,
    pub preamble: String,
    pub term: String,
    /// The glossary's .tex, without .tex.
    pub glossary: String,
    pub scope: Scope,
    pub cache_tikz: bool,
    /// Each step's command and time, and the build's decisions, in the log (the setting Debug log).
    pub debug: bool,
}

// ------------------------------------------------------------------ the format and the generated files

/// The format's key now: the preamble, the generated files, the engine, and the files the last format run read (its
/// recorder file).
async fn current_key<H: Host>(h: &H, o: &Options) -> Result<String, String> {
    let preamble = text(h, &format!("{}.tex", o.preamble)).await;
    let main = text(h, &format!("{}.tex", o.main)).await;
    let f = fast::fmt_files(&preamble, &main, &o.preamble)?;
    let version = h.run(&words(&["lualatex", "--version"])).await.map(|(_, out)| out.lines().next().unwrap_or("").to_string()).unwrap_or_default();
    let mut stats = Vec::new();
    for p in fast::fls_inputs(&text(h, &format!("{FMT}.fls")).await) {
        stats.push((p.clone(), h.stat(&p).await));
    }
    Ok(fast::format_key(&preamble, &f.static_preamble, &f.main_fmt, &version, &stats))
}

/// The generated files written (only those that changed) and the format rebuilt if its key changed. Ok(true): the
/// format is good; Ok(false): it could not be built, or a build failed on it, for this key (the log said why), and
/// builds run without it until the key changes.
pub async fn ensure_format<H: Host>(h: &H, o: &Options) -> Result<bool, String> {
    let preamble = text(h, &format!("{}.tex", o.preamble)).await;
    let main = text(h, &format!("{}.tex", o.main)).await;
    let f = fast::fmt_files(&preamble, &main, &o.preamble)?;
    let driver = fast::driver(&f.main_fmt, &replay_commands(&preamble))?;
    let tikz = tikz_file(o.cache_tikz);
    for (name, body) in [("0-preamble-static.tex", f.static_preamble.as_str()), ("main-fmt.tex", &f.main_fmt), ("main-body.tex", &f.main_body), ("fp-record.tex", fast::RECORD), ("fp-record.lua", fast::RECORD_LUA), ("fp-tikz.tex", &tikz), (&format!("{JOB}.tex"), &driver), (&format!("{JOB}.lua"), fast::DRIVER_LUA)] {
        if text(h, name).await != body {
            h.write(name, body.as_bytes()).await?;
        }
    }
    if o.cache_tikz && h.read("tc/.fast-preview").await.is_none() {
        h.write("tc/.fast-preview", b"TikZ pictures cached by the LaTeX plugin's fast preview.\n").await?; // the folder
    }
    let have = h.read(&format!("{FMT}.fmt")).await.is_some() && h.read(&format!("{FMT}.fls")).await.is_some();
    let stored = text(h, &format!("{FMT}.key")).await;
    if have {
        let key = current_key(h, o).await?;
        if stored == key {
            return Ok(true);
        }
        if stored == format!("failed:{key}") {
            return Ok(false);
        }
    }
    h.stop_worker(); // it runs on the old format
    let t0 = h.now();
    let run = h.run(&fast::format_command(FMT)).await;
    // a format run ends with exit code 1 ("no pages of output"): it worked if it dumped, without an error
    let log = text(h, &format!("{FMT}.log")).await;
    let dumped = log.contains("Beginning to dump on file") && !log.lines().any(|l| l.starts_with("! "));
    let key = current_key(h, o).await?;
    if run.is_ok() && dumped {
        h.write(&format!("{FMT}.key"), key.as_bytes()).await?;
        h.log(&format!("fast preview: the preamble was precompiled in {:.1} s", (h.now() - t0) / 1000.0));
        return Ok(true);
    }
    let why = match run {
        Err(e) => e,
        Ok(_) => log.lines().filter(|l| l.starts_with("! ")).take(2).collect::<Vec<_>>().join(" / "),
    };
    h.log(&format!("fast preview: the preamble could not be precompiled ({why}); builds run without it"));
    _ = h.write(&format!("{FMT}.key"), format!("failed:{key}").as_bytes()).await;
    Ok(false)
}

/// A build on the format failed where the same build without it worked: the format is not used again until its key
/// changes (the preamble, a package, the engine).
pub async fn format_broken<H: Host>(h: &H, o: &Options, why: &str) {
    if let Ok(key) = current_key(h, o).await {
        _ = h.write(&format!("{FMT}.key"), format!("failed:{key}").as_bytes()).await;
    }
    h.log(&format!("fast preview: a build failed on the precompiled preamble and worked without it ({why}); it is not used until the preamble or a package changes"));
}

/// The first error of a LaTeX log, for the log line that says why a build was run again.
pub fn first_error(log: &str) -> String {
    log.lines().find(|l| l.starts_with("! ") || l.contains(".tex:") && l.contains(": ") || l.contains("attempt to")).unwrap_or("it stopped").trim().to_string()
}

// The commands the preamble defines without arguments (\newcommand{\x}{...}): a region replays them from its file.
pub fn replay_commands(preamble: &str) -> Vec<String> {
    let mut out = Vec::new();
    for l in preamble.lines() {
        let l = l.trim_start();
        let Some(rest) = l.strip_prefix("\\newcommand").or_else(|| l.strip_prefix("\\renewcommand")) else { continue };
        let rest = rest.trim_start_matches('*');
        let rest = rest.strip_prefix('{').unwrap_or(rest);
        let Some(rest) = rest.strip_prefix('\\') else { continue };
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
        let after = rest[name.len()..].trim_start_matches('}').trim_start();
        if !name.is_empty() && after.starts_with('{') {
            out.push(name);
        }
    }
    out
}

/// TikZ pictures cached as PDFs, listed for the plugin to make (externalisation's list-and-make mode: no shell escape),
/// each named after its chapter: tc/3-introduction-0.pdf ...; or not cached.
pub fn tikz_file(cache: bool) -> String {
    if !cache {
        return "% TikZ pictures are not cached (the LaTeX plugin's setting Cache TikZ pictures is off).\n".into();
    }
    r"% TikZ pictures cached as PDFs (written by the LaTeX plugin): LaTeX lists them, the plugin makes those that changed.
\usetikzlibrary{external}
\tikzexternalize[prefix=tc/,mode=list and make]
\makeatletter
\AddToHook{file/before}{\filename@parse{\CurrentFile}\tikzset{external/figure name/.expanded={\filename@base-}}}
\makeatother
"
    .into()
}

/// The pictures to make: those of the full build's `<job>.makefile` marked FORCEREMAKE, and those listed in
/// `<job>.figlist` without a PDF (`exists`) or whose PDF was made from other content (`changed`). TikZ writes a
/// picture's key to `<name>.md5` whenever its content changes but goes on using the old PDF, leaving the remake to
/// `make`; pictures are named by their place in the chapter, so a picture removed or added changes those after it.
pub fn tikz_to_make(figlist: &str, makefile: &str, exists: impl Fn(&str) -> bool, changed: impl Fn(&str) -> bool) -> Vec<String> {
    let mut out: Vec<String> = makefile.lines().filter_map(|l| l.strip_suffix(": FORCEREMAKE")).map(|t| t.trim_end_matches(".pdf").to_string()).collect();
    for name in figlist.lines().map(str::trim).filter(|n| !n.is_empty()) {
        if (!exists(&format!("{name}.pdf")) || changed(name)) && !out.iter().any(|x| x == name) {
            out.push(name.to_string());
        }
    }
    out
}

/// The record of the key a picture's PDF was made from (a copy of its `.md5` taken when the plugin made it).
pub fn tikz_made(name: &str) -> String {
    format!("{name}.made")
}

/// Whether a LaTeX run met pictures to make (TikZ reports it as an error, so the run's exit code is not 0).
pub fn pictures_missing(log: &str) -> bool {
    log.contains("Some images are not up-to-date")
}

/// A picture's job: the main file, shipping out only that picture. It runs without the precompiled preamble: on it,
/// luaotfload fails at \begin{document} in a picture's job (measured 2026-10-25, "attempt to index a nil value
/// (local 'fontdata')" from microtype), and pictures are made only when they change, in the background.
pub fn tikz_command(name: &str, main: &str) -> Vec<String> {
    let mut c = words(&["lualatex", "-halt-on-error", "-interaction=batchmode"]);
    c.push(format!("-jobname={name}"));
    c.push(format!("\\def\\tikzexternalrealjob{{{main}}}\\input{{main-fmt}}"));
    c
}

// ------------------------------------------------------------------ the full build

/// latexmk, the whole recipe: it reruns until the citations, glossary and references settle; on the format if it is
/// good. For the first build, a changed preamble or main file, and the command Compile now.
pub fn recipe_steps(fmt: bool, main: &str) -> Vec<Vec<String>> {
    let engine = if fmt { format!("lualatex -fmt={FMT} %O %S") } else { "lualatex %O %S".to_string() };
    // -g: latexmk remembers a failed run (a picture missing) and otherwise will not run again until a source changes
    let mut c = words(&["latexmk", "-pdflua", "-g", "-synctex=1", "-interaction=nonstopmode", "-file-line-error"]);
    c.push(format!("-lualatex={engine}"));
    c.push(format!("-jobname={main}"));
    c.extend(words(&["-e", "sub run_mg { return system('makeglossaries', $_[0]); }", "-e", "add_cus_dep('glo', 'gls', 0, 'run_mg');", "-e", "add_cus_dep('acn', 'acr', 0, 'run_mg');", "main-fmt.tex"]));
    vec![c]
}

/// One LuaLaTeX pass of the main file, on the format if it is good.
pub fn lua_pass(fmt: bool, main: &str) -> Vec<String> {
    let mut c = words(&["lualatex"]);
    if fmt {
        c.push(format!("-fmt={FMT}"));
    }
    c.extend(words(&["-synctex=1", "-interaction=nonstopmode", "-file-line-error"]));
    c.push(format!("-jobname={main}"));
    c.push("main-fmt.tex".into());
    c
}

/// The most LuaLaTeX passes a build runs: one, biber or makeglossaries and a second, and two more while numbers settle.
pub const MAX_PASSES: usize = 4;

/// The full build's steps run in turn until one fails: the last one's exit code and output.
async fn run_steps<H: Host>(h: &H, steps: &[Vec<String>]) -> Result<(i64, String), String> {
    let mut last = (0, String::new());
    for step in steps {
        last = h.run(step).await?;
        if last.0 != 0 {
            break;
        }
    }
    Ok(last)
}

/// How a full build went.
#[derive(Clone, Debug, PartialEq)]
pub struct Built {
    /// It ran on the precompiled preamble.
    pub fmt: bool,
    /// The TikZ pictures made.
    pub pictures: Vec<String>,
    pub code: i64,
    /// The program that stopped with `code`, if one did.
    pub failed: String,
    pub output: String,
    /// What ran and why, for the log ("LuaLaTeX → biber (cited for the first time: x) → LuaLaTeX").
    pub ran: Vec<String>,
}

/// What the full build does, decided before it runs from the files LaTeX reads as the last full build had them and as
/// they are now.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    /// Nothing LaTeX reads changed: the last full build's PDF is shown again, and nothing runs.
    Unchanged,
    /// latexmk, with why.
    Recipe(String),
    /// LuaLaTeX passes from the last full build's generated files, with what is already known to be needed (the .bib or
    /// the glossary changed); each pass shows what else is (keys::after_pass).
    Passes(keys::Needs),
}

/// The .bib files the preamble or the main file reads (\addbibresource, \bibliography).
pub fn bib_files(tex: &str) -> Vec<String> {
    let mut out = Vec::new();
    for l in tex.lines().filter(|l| !l.trim_start().starts_with('%')) {
        for cmd in ["\\addbibresource", "\\bibliography{"] {
            let Some(i) = l.find(cmd) else { continue };
            let rest = &l[i + cmd.len()..];
            let rest = if cmd.ends_with('{') { rest } else { rest.split_once('{').map_or("", |(opts, r)| if opts.trim().is_empty() || opts.trim_start().starts_with('[') { r } else { "" }) };
            if let Some((names, _)) = rest.split_once('}') {
                for n in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                    out.push(if cmd.ends_with('{') && !n.ends_with(".bib") { format!("{n}.bib") } else { n.to_string() });
                }
            }
        }
    }
    out
}

/// The plan for the next full build (`now`: input_texts as they are). `whole`: latexmk asked for (Compile now, or the
/// setting Build only what changed is off).
pub async fn plan<H: Host>(h: &H, o: &Options, now: &BTreeMap<String, String>, whole: bool) -> Plan {
    if whole {
        return Plan::Recipe("the whole recipe was asked for".into());
    }
    let Ok(then) = serde_json::from_str::<BTreeMap<String, String>>(&text(h, &format!("{FULL}.texs.json")).await) else {
        return Plan::Recipe("no earlier full build to compare with".into());
    };
    if h.read(&format!("{FULL}.aux")).await.is_none() {
        return Plan::Recipe("the last full build's files are missing".into());
    }
    let main = format!("{}.tex", o.main);
    if then.get(&main) != now.get(&main) {
        return Plan::Recipe("the main file changed".into());
    }
    // what LaTeX reads, without the header comment (its stamp changes on every save)
    let read = |m: &BTreeMap<String, String>, name: &str| m.get(name).map(|t| without_header(keys::read_part(t))).unwrap_or_default();
    let preamble = format!("{}.tex", o.preamble);
    if read(&then, &preamble) != read(&now, &preamble) {
        return Plan::Recipe("the preamble changed".into());
    }
    let mut known = keys::Needs::default();
    let bbl = h.stat(&format!("{FULL}.bbl")).await.map_or(0.0, |(t, _)| t);
    for bib in bib_files(&format!("{}\n{}", now.get(&preamble).map_or("", String::as_str), now.get(&main).map_or("", String::as_str))) {
        if h.stat(&bib).await.is_some_and(|(t, _)| t > bbl) {
            known.biber.push(format!("{bib} changed"));
        }
    }
    let glossary = format!("{}.tex", o.glossary);
    if read(&then, &glossary) != read(&now, &glossary) {
        known.glossaries.push("the glossary changed".into());
    }
    let same = then.len() == now.len() && now.keys().all(|name| then.contains_key(name) && read(&then, name) == read(&now, name));
    if same && known.nothing() {
        return Plan::Unchanged;
    }
    Plan::Passes(known)
}

/// The generated files a pass reads and rewrites (keys::GENERATED), as they are now.
async fn generated<H: Host>(h: &H, main: &str) -> BTreeMap<&'static str, String> {
    let mut out = BTreeMap::new();
    for ext in keys::GENERATED {
        out.insert(ext, text(h, &format!("{main}.{ext}")).await);
    }
    out
}

/// LuaLaTeX passes, starting from the last full build's generated files: after each, biber and makeglossaries if it
/// shows them needed (or `known` says so), and another pass while it leaves anything to settle (at most MAX_PASSES).
async fn passes<H: Host>(h: &H, o: &Options, fmt: bool, known: &keys::Needs, pagerefs: bool, built: &mut Built) -> Result<(), String> {
    let main = o.main.as_str();
    // the last full build's state, which `known` and every comparison start from (a cancelled or failed build since
    // may have left others)
    for ext in ["aux", "bbl", "bcf", "glo", "gls", "ist", "toc", "lof", "lot", "out"] {
        if let Some(b) = h.read(&format!("{FULL}.{ext}")).await {
            h.write(&format!("{main}.{ext}"), &b).await?;
        }
    }
    let mut todo = known.clone();
    for pass in 1..=MAX_PASSES {
        let before = generated(h, main).await;
        let t = h.now();
        (built.code, built.output) = h.run(&lua_pass(fmt, main)).await?;
        built.ran.push(format!("LuaLaTeX {:.1} s", (h.now() - t) / 1000.0));
        if o.debug {
            let pages = built.output.lines().find_map(|l| l.split_once("Output written on").map(|(_, r)| r.trim().to_string())).unwrap_or_default();
            h.log(&format!("debug: pass {pass}: {} → exit {}, {pages}, {:.1} s", lua_pass(fmt, main).join(" "), built.code, (h.now() - t) / 1000.0));
        }
        if built.code != 0 {
            built.failed = "lualatex".into();
            return Ok(());
        }
        let mut n = keys::after_pass(&before, &generated(h, main).await, pagerefs);
        n.add(std::mem::take(&mut todo));
        if n.nothing() {
            return Ok(());
        }
        if pass == MAX_PASSES {
            h.log(&format!("the build stopped after {MAX_PASSES} LuaLaTeX passes with numbers still changing ({}): compile again, or use Compile now", n.rerun.join("; ")));
            return Ok(());
        }
        for (program, why) in [("makeglossaries", &n.glossaries), ("biber", &n.biber)] {
            if why.is_empty() {
                continue;
            }
            let t = h.now();
            let (code, out) = h.run(&words(&[program, main])).await?;
            built.ran.push(format!("{program} {:.1} s ({})", (h.now() - t) / 1000.0, why.join("; ")));
            if code != 0 {
                (built.code, built.failed) = (code, program.to_string());
                built.output += &out;
                return Ok(());
            }
        }
        if !n.rerun.is_empty() {
            built.ran.push(format!("again: {}", n.rerun.join("; ")));
        }
    }
    Ok(())
}

/// The full build with fast preview on: the plan (Plan); the format checked (and rebuilt if its key changed); the build
/// on it, or without it if it fails on it and works without (the format is then set aside, and the log says why); the
/// TikZ pictures that changed made, and a pass again; then the files kept for previews and the worker started.
/// `whole`: latexmk, whatever changed.
pub async fn full_build<H: Host>(h: &H, o: &Options, whole: bool) -> Result<Built, String> {
    let mut texs = input_texts(h, &o.main).await;
    texs.insert(format!("{}.tex", o.main), text(h, &format!("{}.tex", o.main)).await);
    let plan = plan(h, o, &texs, whole).await;
    if o.debug {
        h.log(&format!("debug: the full build's plan: {plan:?}"));
    }
    let mut built = Built { fmt: false, pictures: Vec::new(), code: 0, failed: String::new(), output: String::new(), ran: Vec::new() };
    if plan == Plan::Unchanged {
        // the PDF shown may be a preview's: the full build's is the current one
        for ext in ["pdf", "synctex.gz"] {
            let b = h.read(&format!("{FULL}.{ext}")).await.ok_or_else(|| format!("the last full build's {ext} is missing"))?;
            h.write(&format!("{}.{ext}", o.main), &b).await?;
        }
        let key = text(h, &format!("{FMT}.key")).await;
        built.fmt = !key.is_empty() && !key.starts_with("failed:");
        built.ran.push("nothing: unchanged since the last full build, whose PDF is shown".into());
        prepare(h, built.fmt).await?;
        return Ok(built);
    }
    let t = h.now();
    let mut fmt = ensure_format(h, o).await?;
    built.fmt = fmt;
    if o.debug {
        h.log(&format!("debug: the precompiled preamble {} ({:.1} s to check)", if fmt { "is used" } else { "is not used (it failed for this preamble; see the log above)" }, (h.now() - t) / 1000.0));
    }
    if let Plan::Recipe(why) = &plan {
        built.ran.push(format!("latexmk ({why})"));
    }
    let pagerefs = texs.values().any(|t| t.contains("\\pageref"));
    let log_of = async || text(h, &format!("{}.log", o.main)).await;
    let attempt = async |fmt: bool, built: &mut Built| -> Result<(), String> {
        match &plan {
            Plan::Passes(known) => passes(h, o, fmt, known, pagerefs, built).await?,
            _ => {
                let t = h.now();
                (built.code, built.output) = run_steps(h, &recipe_steps(fmt, &o.main)).await?;
                built.failed = if built.code != 0 { "latexmk".into() } else { String::new() };
                built.ran.push(format!("latexmk {:.1} s", (h.now() - t) / 1000.0));
            }
        }
        // pictures to make (missing ones stop the run with an error; changed ones are used as they were): made,
        // then a pass again
        let mut to_make = Vec::new();
        if o.cache_tikz && (built.code == 0 || pictures_missing(&log_of().await)) {
            let figlist = text(h, &format!("{}.figlist", o.main)).await;
            let makefile = text(h, &format!("{}.makefile", o.main)).await;
            let (mut exists, mut changed) = (BTreeSet::new(), BTreeSet::new());
            for name in figlist.lines().map(str::trim).filter(|n| !n.is_empty()) {
                if h.read(&format!("{name}.pdf")).await.is_some() {
                    exists.insert(format!("{name}.pdf"));
                }
                if h.read(&format!("{name}.md5")).await != h.read(&tikz_made(name)).await {
                    changed.insert(name.to_string());
                }
            }
            to_make = tikz_to_make(&figlist, &makefile, |f| exists.contains(f), |n| changed.contains(n));
        }
        if !to_make.is_empty() {
            for name in to_make {
                let (c, out) = h.run(&tikz_command(&name, &o.main)).await?;
                if c != 0 {
                    return Err(format!("the picture {name} could not be made: {}", first_error(&out)));
                }
                // recorded only once made, so a build stopped before this point makes it again next time
                h.write(&tikz_made(&name), &h.read(&format!("{name}.md5")).await.unwrap_or_default()).await?;
                built.pictures.push(name);
            }
            built.ran.push(format!("TikZ pictures made: {}", built.pictures.join(", ")));
            (built.code, built.output) = h.run(&lua_pass(fmt, &o.main)).await?;
            built.ran.push("LuaLaTeX".into());
            built.failed = if built.code != 0 { "lualatex".into() } else { String::new() };
        }
        Ok(())
    };
    attempt(fmt, &mut built).await?;
    if built.code != 0 && fmt {
        let why = first_error(&log_of().await);
        built.ran.push("again without the precompiled preamble".into());
        attempt(false, &mut built).await?;
        if built.code == 0 {
            format_broken(h, o, &why).await;
            fmt = false;
            built.fmt = false;
        }
    }
    if built.code == 0 {
        keep_full(h, &o.main, &texs).await?;
        prepare(h, fmt).await?;
    }
    Ok(built)
}

// The full build succeeded: its files kept for the previews, and the waiting job started on them.
pub async fn keep_full<H: Host>(h: &H, main: &str, texs: &BTreeMap<String, String>) -> Result<(), String> {
    for ext in KEPT {
        if let Some(b) = h.read(&format!("{main}.{ext}")).await {
            h.write(&format!("{FULL}.{ext}"), &b).await?;
        }
    }
    // the files it was built from: a preview typesets every change since
    h.write(&format!("{FULL}.texs.json"), serde_json::to_string(texs).unwrap_or_default().as_bytes()).await
}

/// The text of every file the main file \inputs (`3-introduction.tex` ...), as they are now.
pub async fn input_texts<H: Host>(h: &H, main: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for f in keys::inputs(&text(h, &format!("{main}.tex")).await) {
        out.insert(format!("{f}.tex"), text(h, &format!("{f}.tex")).await);
    }
    out
}

/// The changes since the last full build: each chapter whose .tex differs (then and now), and the glossary's .tex
/// if it differs. Err when the preamble changed (only the full build shows that) or there is no full build yet.
pub async fn changes<H: Host>(h: &H, o: &Options) -> Result<(Vec<Edit>, Option<(String, String)>), String> {
    let then: BTreeMap<String, String> = serde_json::from_str(&text(h, &format!("{FULL}.texs.json")).await).map_err(|_| "no full build with fast preview yet")?;
    let now = input_texts(h, &o.main).await;
    let main = format!("{}.tex", o.main);
    let main_now = text(h, &main).await;
    if then.get(&main).is_some_and(|m| *m != main_now) {
        return Err("the main file changed: the full build shows it".into());
    }
    let mut edits = Vec::new();
    let mut glossary = None;
    for (name, new) in &now {
        let old = then.get(name).cloned().unwrap_or_default();
        if keys::read_part(&old) == keys::read_part(new) {
            continue; // the same, or changed only where LaTeX stops reading (a chapter left out of the PDF)
        }
        let left_out = |t: &str| keys::read_part(t).len() < t.len();
        if left_out(&old) != left_out(new) {
            return Err(format!("{name} was left out of the PDF or put back: the full build shows it"));
        }
        match name.trim_end_matches(".tex") {
            n if n == o.preamble => return Err("the preamble changed: the full build shows it".into()),
            n if n == o.glossary => glossary = Some((old, new.clone())),
            _ => edits.push(Edit { tex: name.clone(), old, new: new.clone() }),
        }
    }
    Ok((edits, glossary))
}
// ------------------------------------------------------------------ the preview

/// A saved chapter: its .tex before and after.
#[derive(Clone, Debug)]
pub struct Edit {
    pub tex: String,
    pub old: String,
    pub new: String,
}

/// What a preview did.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// The spliced PDF was written as the main PDF.
    pub shown: bool,
    pub pages: Vec<Source>,
    /// The result's pages that are not current, with why.
    pub stale: Vec<((u32, u32), String)>,
    pub build_now: bool,
    /// How the citation numbers changed, if the citations did.
    pub cite_change: Option<CiteChange>,
    /// Nothing differs from the last full build (an edit undone): its PDF is shown again.
    pub restored: bool,
    pub problems: Vec<String>,
    /// For the log: what was typeset and how long each step took.
    pub steps: Vec<String>,
    pub ms: f64,
}

/// The last full build's files, read once per preview.
struct Full {
    aux: String,
    recs: Vec<Start>,
    bcf: String,
    glo: String,
    toc: BTreeMap<&'static str, String>,
    pdf: Vec<u8>,
}

async fn full<H: Host>(h: &H) -> Option<Full> {
    let aux = text(h, &format!("{FULL}.aux")).await;
    let recs = fast::records(&aux);
    let pdf = h.read(&format!("{FULL}.pdf")).await?;
    if recs.is_empty() {
        return None;
    }
    let mut toc = BTreeMap::new();
    for ext in ["toc", "lof", "lot"] {
        toc.insert(ext, text(h, &format!("{FULL}.{ext}")).await);
    }
    Some(Full { bcf: text(h, &format!("{FULL}.bcf")).await, glo: text(h, &format!("{FULL}.glo")).await, aux, recs, toc, pdf })
}

/// The worker's input files from the full build's (with what a pass changes), then the worker started: on the format,
/// or without it if that fails.
async fn start<H: Host>(h: &H, fmt: bool, files: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    h.stop_worker();
    for ext in ["aux", "bbl", "gls", "toc", "lof", "lot"] {
        let name = format!("{JOB}.{ext}");
        match files.get(ext) {
            Some(b) => h.write(&name, b).await?,
            None => {
                if let Some(b) = h.read(&format!("{FULL}.{ext}")).await {
                    h.write(&name, &b).await?;
                }
            }
        }
    }
    let cmd = |fmt: bool| {
        let mut c = words(&["lualatex"]);
        if fmt {
            c.push(format!("-fmt={FMT}"));
        }
        c.extend(words(&["-synctex=1", "-interaction=nonstopmode", "-file-line-error", &format!("{JOB}.tex")]));
        c
    };
    match h.start_worker(&cmd(fmt)).await {
        Err(e) if fmt => {
            h.log(&format!("fast preview: the preview job did not start on the precompiled preamble ({}); started without it", e.lines().rev().find(|l| l.starts_with('!')).unwrap_or("it stopped")));
            h.start_worker(&cmd(false)).await
        }
        r => r,
    }
}

/// The waiting job started for the next preview, if none is (after a preview, a full build, a glossary change).
pub async fn prepare<H: Host>(h: &H, fmt: bool) -> Result<(), String> {
    if h.worker_waiting() || h.read(&format!("{FULL}.aux")).await.is_none() {
        return Ok(());
    }
    start(h, fmt, &BTreeMap::new()).await
}

/// One request to the worker: the regions typeset, the job's .fpl read.
async fn typeset<H: Host>(h: &H, recs: &[Start], regions: &[Region]) -> Result<(Vec<fast::Typeset>, u32, String), String> {
    let (code, out) = h.request(&fast::request(recs, regions)).await?;
    let (t, done) = fast::results(&text(h, &format!("{JOB}.fpl")).await);
    match done {
        Some(pages) => Ok((t, pages, out)),
        None => Err(format!("the preview job stopped (exit code {code}): {}", out.lines().filter(|l| l.starts_with('!') || l.contains(".tex:")).take(3).collect::<Vec<_>>().join(" / "))),
    }
}

/// The destinations LuaTeX made up in a job's PDF for links whose target it did not typeset (its log: "unreferenced
/// destination with name '...'", where a long line is wrapped at 79 characters): the full build's are the real ones.
pub fn placeholders(log: &str) -> std::collections::HashSet<Vec<u8>> {
    let mut joined = String::new();
    for l in log.lines() {
        joined += l;
        if l.chars().count() != 79 {
            joined.push('\n');
        }
    }
    joined.lines().filter_map(|l| l.split_once("unreferenced destination with name '")).map(|(_, rest)| rest.split('\'').next().unwrap_or("").as_bytes().to_vec()).collect()
}

/// The record whose source line holds a list command of `kind` (toc, lof, lot, bib, terms).
fn list_record(recs: &[Start], files: &BTreeMap<String, String>, kind: &str) -> Option<usize> {
    (0..recs.len()).find(|&k| {
        let r = &recs[k];
        files.get(&r.file).and_then(|t| t.lines().nth(r.line as usize - 1)).and_then(fast::list_kind) == Some(kind)
    })
}

/// A list's region: the block around its command, or (the Terms list, printed inside a section) its section.
fn list_region_of(recs: &[Start], files: &BTreeMap<String, String>, kind: &str) -> Option<Region> {
    let k = list_record(recs, files, kind)?;
    let r = &recs[k];
    let mut g = if kind == "terms" { fast::edit_region(recs, &r.file, r.line, Scope::Section { span: 1 })? } else { fast::list_region(recs, k, files.get(&r.file)?) };
    g.list = true;
    Some(g)
}

/// The preview of the chapters changed since the last full build (`edits`: each chapter's .tex then and now; the
/// glossary's .tex then and now if it changed): typeset, spliced into the last full build's PDF, and written as
/// `<main>.pdf`. Every change since the full build is typeset again, since the splice starts from its PDF. Err when it
/// cannot be previewed (the full build will show it).
pub async fn preview<H: Host>(h: &H, o: &Options, fmt: bool) -> Result<Outcome, String> {
    let (edits, glossary) = changes(h, o).await?;
    let edits = edits.as_slice();
    let t0 = h.now();
    let mut out = Outcome::default();
    let mut full = full(h).await.ok_or("no full build with fast preview's records yet")?;
    // the records' lines are the full build's: in an edited chapter, those after the change move with it
    for e in edits {
        if let Some(span) = keys::changed_span(&e.old, &e.new) {
            for r in full.recs.iter_mut().filter(|r| r.file == e.tex) {
                r.line = keys::new_line(r.line, span);
            }
        }
    }
    let main_tex = text(h, &format!("{}.tex", o.main)).await;
    // every input file's text as it is now, and as the full build had it (the records name main-fmt.tex for the
    // main file)
    let mut files: BTreeMap<String, String> = BTreeMap::new();
    for f in keys::inputs(&main_tex) {
        files.insert(format!("{f}.tex"), text(h, &format!("{f}.tex")).await);
    }
    files.insert("main-fmt.tex".into(), text(h, "main-fmt.tex").await);
    let mut then = files.clone();
    for e in edits {
        files.insert(e.tex.clone(), e.new.clone());
        then.insert(e.tex.clone(), e.old.clone());
    }
    let stems = |m: &BTreeMap<String, String>| m.iter().map(|(k, v)| (k.trim_end_matches(".tex").to_string(), v.clone())).collect::<BTreeMap<_, _>>();
    let mut regions: Vec<Region> = Vec::new();
    let mut lists: BTreeSet<&str> = BTreeSet::new();
    let mut data: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut respawn = false;
    let mut stale_all: Option<&str> = None;
    let add = |regions: &mut Vec<Region>, g: Region| {
        // one region per start; a region inside another's is that one
        if !regions.iter().any(|r| r.file == g.file && r.start <= g.start && g.end <= r.end) {
            regions.retain(|r| !(r.file == g.file && g.start <= r.start && r.end <= g.end));
            regions.push(g);
        }
    };
    if let Some((old_gls, new_gls)) = &glossary {
        // the glossary: its entries are defined before \begin{document}, so the job starts again
        respawn = true;
        let c = keys::glossary_change(&keys::glossary_entries(old_gls, &o.term), &keys::glossary_entries(new_gls, &o.term));
        if !(c.printed.is_empty() && c.described.is_empty() && c.added.is_empty() && c.removed.is_empty()) {
            lists.insert("terms");
        }
        // the sections that print a changed entry: at most three, else the pages wait for the full build
        let mut using: Vec<Region> = Vec::new();
        for (f, t) in &files {
            for (n, line) in t.lines().enumerate() {
                if c.printed.iter().any(|k| keys::gls_keys(line).contains(k)) {
                    if let Some(g) = fast::edit_region(&full.recs, f, n as u32 + 1, Scope::Section { span: 1 }) {
                        if !using.iter().any(|u| u.state == g.state) {
                            using.push(g);
                        }
                    }
                }
            }
        }
        if using.len() > 3 {
            stale_all = Some("glossary entries updating");
            out.build_now = true;
        } else {
            for g in using {
                add(&mut regions, g);
            }
        }
        out.steps.push(format!("glossary: {} printed, {} described, {} added, {} removed", c.printed.len(), c.described.len(), c.added.len(), c.removed.len()));
    }
    for e in edits {
        // the header comment changes on every save (its "last updated" stamp): not an edit of the chapter
        let Some((first, last)) = keys::changed_lines(&without_header(&e.old), &without_header(&e.new)) else { continue };
        let mut g = fast::edit_region(&full.recs, &e.tex, first, o.scope).ok_or_else(|| format!("{} has no recorded section before line {first}", e.tex))?;
        if g.end <= last {
            g = fast::edit_region(&full.recs, &e.tex, first, Scope::Chapter).ok_or("no chapter region")?;
        }
        // a changed TikZ picture is typeset, not taken from the cache
        let span: String = e.new.lines().skip(first as usize - 1).take((last - first + 1) as usize).collect();
        if span.contains("tikzpicture") || span.contains("\\draw") || span.contains("\\node") {
            g.tikz = true;
            out.steps.push("a TikZ picture changed: typeset in the preview".into());
        }
        add(&mut regions, g);
        lists.extend(keys::lists_touched(&e.new, first, last, &e.old));
    }
    if regions.is_empty() && lists.is_empty() {
        if glossary.is_none() {
            // every chapter is as the last full build had it (an edit undone): its PDF is the current one
            let pdf = h.read(&format!("{FULL}.pdf")).await.ok_or("no full build yet")?;
            h.write(&format!("{}.pdf", o.main), &pdf).await?;
            return Ok(Outcome { shown: true, restored: true, steps: vec!["as at the last full build: its PDF is shown".into()], ms: h.now() - t0, ..Outcome::default() });
        }
        return Err("nothing to preview".into());
    }
    // citations: the order predicted from the chapters, if the chapters as the full build had them give its order
    let cites_changed = edits.iter().any(|e| keys::cite_keys(&e.old) != keys::cite_keys(&e.new));
    if cites_changed {
        let bcf_order = keys::bcf_keys(&full.bcf);
        let change = if keys::citation_order(&main_tex, &stems(&then)) == bcf_order {
            let new_order = keys::citation_order(&main_tex, &stems(&files));
            let c = keys::cite_change(&bcf_order, &new_order);
            if c != CiteChange::Same {
                let bcf = keys::bcf_with_keys(&full.bcf, &new_order).ok_or("the .bcf has no citations to reorder")?;
                h.write(&format!("{CITE}.bcf"), bcf.as_bytes()).await?;
                let t = h.now();
                let (code, log) = h.run(&words(&["biber", CITE])).await?;
                let bbl = h.read(&format!("{CITE}.bbl")).await.filter(|_| code == 0).ok_or_else(|| format!("biber failed: {}", log.lines().filter(|l| l.contains("ERROR")).take(2).collect::<Vec<_>>().join(" / ")))?;
                data.insert("bbl".into(), bbl);
                respawn = true;
                lists.insert("bib");
                out.steps.push(format!("biber on the new citation order ({:.1} s)", (h.now() - t) / 1000.0));
            }
            c
        } else {
            out.steps.push("the citation order cannot be predicted from the chapters: the numbers wait for the full build".into());
            CiteChange::Renumbered
        };
        if change == CiteChange::Renumbered {
            stale_all = Some(fast::STALE_CITATIONS);
            out.build_now = true;
        }
        out.cite_change = Some(change);
    }
    // a term used for the first time, or no longer used anywhere: the Terms list, after makeglossaries
    let before = keys::glo_keys(&full.glo);
    let now_used: BTreeSet<String> = files.values().flat_map(|t| keys::gls_keys(keys::read_part(t))).collect();
    let first_use = edits.iter().any(|e| keys::gls_keys(&e.new).iter().any(|k| !before.contains(k)));
    let last_use = before.iter().any(|k| !now_used.contains(k));
    let terms_pass2 = first_use || last_use;
    if respawn || !h.worker_waiting() {
        start(h, fmt, &data).await?;
    }
    // pass 1: the regions in the document's order (each typeset on through a few more starts, to meet the full build
    // again if the edit moved its page breaks), and the bibliography or the Terms list if their data is ready
    for g in regions.iter_mut() {
        fast::extend(&full.recs, g, CONVERGE);
    }
    regions.sort_by_key(|r| r.state);
    let mut pass: Vec<Region> = regions.clone();
    for kind in ["bib", "terms"] {
        if lists.contains(kind) && !(kind == "terms" && terms_pass2) {
            if let Some(g) = list_region_of(&full.recs, &files, kind) {
                add(&mut pass, g);
            }
        }
    }
    pass.sort_by_key(|r| r.state);
    let t = h.now();
    let (mut typeset_out, mut pages, _) = typeset(h, &full.recs, &pass).await?;
    out.steps.push(format!("typeset {} region{} ({:.2} s)", pass.len(), if pass.len() == 1 { "" } else { "s" }, (h.now() - t) / 1000.0));
    // pass 2, when the first one's output changes a generated list or a label it uses
    let paux = text(h, &format!("{JOB}.aux")).await;
    let plog = text(h, &format!("{JOB}.log")).await;
    let mut second: BTreeMap<String, Vec<u8>> = data.clone();
    let mut extra: Vec<Region> = Vec::new();
    for ext in ["toc", "lof", "lot"] {
        if lists.contains(ext) && keys::list_changed(&full.toc[ext], &paux, ext) {
            match keys::list_merge(&full.toc[ext], &paux, ext) {
                Some(m) => {
                    second.insert(ext.into(), m.into_bytes());
                    extra.extend(list_region_of(&full.recs, &files, ext));
                }
                None => {
                    // an entry added or removed: the numbering moved
                    stale_all.get_or_insert(fast::STALE_NUMBERS);
                    out.build_now = true;
                }
            }
        }
    }
    let full_labels = keys::labels(&full.aux);
    let new_labels: BTreeMap<String, String> = keys::labels(&paux).into_iter().filter(|(k, v)| full_labels.get(k) != Some(v)).collect();
    let undefined = new_labels.keys().any(|k| plog.contains(&format!("Reference `{k}'")) || plog.contains(&format!("`{k}' on page")));
    if undefined {
        let mut aux = full.aux.clone();
        for (k, v) in &new_labels {
            aux += &format!("\\newlabel{{{k}}}{{{v}}}\n");
        }
        second.insert("aux".into(), aux.into_bytes());
    }
    // labels the preview changed that the chapters not typeset refer to: their pages wait for the full build
    let edited: BTreeSet<&str> = edits.iter().map(|e| e.tex.as_str()).collect();
    let elsewhere: BTreeSet<String> = files.iter().filter(|(f, _)| !edited.contains(f.as_str())).flat_map(|(_, t)| keys::label_refs(t)).collect();
    if new_labels.keys().any(|k| elsewhere.contains(k)) {
        stale_all.get_or_insert("references updating");
        out.build_now = true;
    }
    if terms_pass2 {
        let preview_glo = text(h, &format!("{JOB}.glo")).await;
        // the pages a region covers whole (not its first and last, which hold other text too) are the preview's in the
        // index; the full build's entries for the others stay
        let mut printed: BTreeSet<String> = BTreeSet::new();
        for r in &pass {
            let (a, b) = (full.recs[r.state].num("c:page"), r.next.map_or(i64::MAX, |n| full.recs[n].num("c:page")));
            if r.next.is_some_and(|n| full.recs[n].state.get("style") == full.recs[r.state].state.get("style")) {
                printed.extend((a + 1..b).map(|p| p.to_string()));
            }
        }
        h.write(&format!("{GLS}.glo"), keys::glo_merge(&full.glo, &preview_glo, &now_used, &printed).as_bytes()).await?;
        h.write(&format!("{GLS}.aux"), keys::gls_aux(&full.aux, GLS).as_bytes()).await?;
        if let Some(ist) = h.read(&format!("{FULL}.ist")).await {
            h.write(&format!("{GLS}.ist"), &ist).await?;
        }
        let t = h.now();
        let (code, glog) = h.run(&words(&["makeglossaries", GLS])).await?;
        match h.read(&format!("{GLS}.gls")).await.filter(|_| code == 0) {
            Some(gls) => {
                second.insert("gls".into(), gls);
                extra.extend(list_region_of(&full.recs, &files, "terms"));
                out.steps.push(format!("makeglossaries on the preview's index ({:.1} s)", (h.now() - t) / 1000.0));
            }
            None => {
                h.log(&format!("fast preview: makeglossaries failed: {}", glog.lines().rev().take(2).collect::<Vec<_>>().join(" / ")));
                stale_all.get_or_insert(fast::STALE_TERMS);
            }
        }
    }
    if !extra.is_empty() || undefined {
        for g in extra {
            add(&mut pass, g);
        }
        pass.sort_by_key(|r| r.state);
        start(h, fmt, &second).await?;
        let t = h.now();
        (typeset_out, pages, _) = typeset(h, &full.recs, &pass).await?;
        out.steps.push(format!("second pass: {} regions ({:.2} s)", pass.len(), (h.now() - t) / 1000.0));
    }
    // the splice
    let t = h.now();
    let main_pdf = Pdf::read(&full.pdf)?;
    let main_pages = main_pdf.pages().len() as u32;
    let preview_pdf = Pdf::read(&h.read(&format!("{JOB}.pdf")).await.ok_or("the preview job wrote no PDF")?)?;
    let plan = fast::plan(&full.recs, &pass, &typeset_out, pages, main_pages);
    out.build_now |= plan.build_now;
    let mut stale = fast::shifted(&plan);
    let layout = splice::layout(&plan, main_pages);
    // pages moved: the lists that print page numbers wait for the full build too
    if stale.iter().any(|(_, why)| why == fast::STALE_PAGES || why == fast::STALE_NUMBERS) {
        for kind in ["toc", "lof", "lot", "terms"] {
            if let Some(g) = list_region_of(&full.recs, &files, kind).filter(|g| !pass.iter().any(|p| p.state == g.state)) {
                let a = full.recs[g.state].phys - 1;
                let b = g.next.map_or(main_pages, |n| full.recs[n].phys);
                stale.extend(fast::shifted(&fast::Plan { replace: plan.replace.clone(), stale: vec![((a, b), fast::STALE_PAGES.to_string())], build_now: false }));
            }
        }
    }
    if stale_all == Some(fast::STALE_CITATIONS) {
        // renumbered citations change the width of citations everywhere, so even the preview's pages start where the
        // full build had them, which is no longer so: all wait for the full build (started at once)
        stale.push(((0, layout.len() as u32), fast::STALE_CITATIONS.to_string()));
    } else if let Some(why) = stale_all {
        // every page that is not wholly the preview's (a joined page holds the full build's part too)
        let mut run: Option<u32> = None;
        for (k, s) in layout.iter().enumerate() {
            match (matches!(s, Source::Preview(_)), run) {
                (false, None) => run = Some(k as u32),
                (true, Some(a)) => {
                    stale.push(((a, k as u32), why.to_string()));
                    run = None;
                }
                _ => {}
            }
        }
        if let Some(a) = run {
            stale.push(((a, layout.len() as u32), why.to_string()));
        }
    }
    let s = splice::splice(&main_pdf, &preview_pdf, &plan, &stale, &placeholders(&plog_of(h).await))?;
    h.write(&format!("{}.pdf", o.main), &s.pdf).await?;
    out.steps.push(format!("spliced into {} pages ({:.2} s)", s.pages.len(), (h.now() - t) / 1000.0));
    out.shown = true;
    out.pages = s.pages;
    out.stale = s.stale;
    out.problems = s.problems;
    for w in typeset_out.iter().flat_map(|t| t.warnings.iter()) {
        out.problems.push(w.clone());
    }
    out.ms = h.now() - t0;
    Ok(out)
}

/// The preview job's log (of its last pass).
async fn plog_of<H: Host>(h: &H) -> String {
    text(h, &format!("{JOB}.log")).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replayed() {
        let pre = "\\newcommand{\\printterms}{%\n\\newcommand{\\rqref}[1]{x}\n\\newcommand*{\\startappendices}{y}\n\\renewcommand\\foo{z}\n% \\newcommand{\\no}{}\n";
        assert_eq!(replay_commands(pre), ["printterms", "startappendices", "foo"]);
    }

    #[test]
    fn placeholder_destinations() {
        let head = "warning  (pdf backend): unreferenced destination with name '";
        let xs = "x".repeat(79 - head.len());
        let log = format!("a\n{head}cite.0@src07' [9]\n{head}{xs}\nyz'\nb\n");
        let p = placeholders(&log);
        assert!(p.contains(b"cite.0@src07".as_slice()) && p.contains(format!("{xs}yz").as_bytes()), "{p:?}");
    }

    #[test]
    fn tikz() {
        let figlist = "tc/3-intro-0\ntc/3-intro-1\ntc/5-meth-0\n";
        let makefile = "ALL_FIGURE_NAMES=$(shell cat main.figlist)\ntc/3-intro-0.pdf: \ntc/3-intro-1.pdf: FORCEREMAKE\n";
        assert_eq!(tikz_to_make(figlist, makefile, |f| f != "tc/5-meth-0.pdf", |_| false), ["tc/3-intro-1", "tc/5-meth-0"]);
        // a picture whose PDF exists but was made from other content (a figure before it removed)
        assert_eq!(tikz_to_make(figlist, makefile, |_| true, |n| n == "tc/3-intro-0"), ["tc/3-intro-1", "tc/3-intro-0"]);
        assert_eq!(tikz_made("tc/3-intro-0"), "tc/3-intro-0.made");
        assert_eq!(tikz_command("tc/3-intro-1", "main"), ["lualatex", "-halt-on-error", "-interaction=batchmode", "-jobname=tc/3-intro-1", "\\def\\tikzexternalrealjob{main}\\input{main-fmt}"]);
        assert!(tikz_file(false).starts_with('%') && tikz_file(true).contains("list and make"));
    }

    #[test]
    fn full_build_steps() {
        assert_eq!(lua_pass(true, "main"), ["lualatex", "-fmt=fp-preamble", "-synctex=1", "-interaction=nonstopmode", "-file-line-error", "-jobname=main", "main-fmt.tex"]);
        let l = &recipe_steps(false, "main")[0];
        assert!(l.contains(&"-lualatex=lualatex %O %S".to_string()) && l.last().unwrap() == "main-fmt.tex" && l.contains(&"-jobname=main".to_string()));
        assert!(recipe_steps(true, "main")[0].contains(&"-lualatex=lualatex -fmt=fp-preamble %O %S".to_string()));
        let pre = "% \\addbibresource{old.bib}\n\\addbibresource{My Library.bib}\n\\addbibresource[label=x]{b.bib}\n\\bibliography{refs,more}\n";
        assert_eq!(bib_files(pre), ["My Library.bib", "b.bib", "refs.bib", "more.bib"]);
    }
}

/// A chapter's text with its opening comment lines (the "Generated from …" header, `% !TEX root`, a left-out chapter's
/// `\endinput%` line) emptied, keeping the line count, so a new stamp in the header is not seen as an edit: before,
/// every save changed line 1, and the preview gave way to a full build.
fn without_header(tex: &str) -> String {
    let mut head = true;
    tex.lines()
        .map(|l| {
            head = head && (l.starts_with('%') || l.starts_with("\\endinput%"));
            if head { "" } else { l }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod header_tests {
    use crate::pipeline::keys::changed_lines;

    #[test]
    fn a_new_stamp_is_not_an_edit() {
        let old = "% Generated from x.md (last updated 10:00)\n% !TEX root = main.tex\n\\section{A}\nText.\n";
        let new = old.replace("10:00", "10:05");
        assert_eq!(changed_lines(&super::without_header(old), &super::without_header(&new)), None);
        let edited = new.replace("Text.", "Text, edited.");
        assert_eq!(changed_lines(&super::without_header(old), &super::without_header(&edited)), Some((4, 4)));
    }
}
