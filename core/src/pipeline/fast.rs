//! Fast preview, the decisions (docs/fast-preview-design.md): the files that let a waiting LuaLaTeX process typeset one section of
//! the document alone, with the page state the last full build recorded, and what to make of its output.
//!
//! - The precompiled preamble (`fmt_files`): `0-preamble-static.tex` (the preamble without what a format cannot hold),
//!   `main-fmt.tex` (the main file on the format, which is also the full build's file) and `main-body.tex` (what TikZ's
//!   picture jobs read). The format's key (`format_key`) covers everything that goes into it.
//! - The records (`RECORD`): the full build writes into its .aux, for every section, list and bibliography, where it
//!   starts and the state there (`Start`: the physical page, the page builder's totals, every counter).
//! - The preview job (`driver`, `DRIVER_LUA`): it waits with the preamble loaded; a request (`request`) names regions,
//!   each a line range of a file with the state to start it in; it typesets them, a page apart, and writes where each
//!   began and the state where each ended (`results`).
//! - The splice (`plan`): which pages of the last full build each region replaces, whether the region's first page must
//!   be joined with the full build's (the part above the region), and which pages are current and which stale.

use std::collections::BTreeMap;

/// Pages shown in the PDF as they are while a full build is still to come, with why.
pub const STALE_PAGES: &str = "page numbers updating";
pub const STALE_LAYOUT: &str = "layout updating";
pub const STALE_NUMBERS: &str = "numbering updating";
pub const STALE_CITATIONS: &str = "citation numbers updating";
pub const STALE_TERMS: &str = "page lists updating";

// ------------------------------------------------------------------ the precompiled preamble

/// The three files made from the preamble's .tex and the main file.
#[derive(Clone, Debug, PartialEq)]
pub struct FmtFiles {
    pub static_preamble: String,
    pub main_fmt: String,
    pub main_body: String,
}

/// What a dumped format cannot hold, or must not (it changes between builds), loaded after it, in this order: microtype
/// (its Lua state cannot be dumped), new PDF objects for pgf (the numbers it reserved when loaded belong to the format's
/// run), the word counts, \makeglossaries (it opens a file), the records and the TikZ cache.
const AFTER_FORMAT: &str = r"\usepackage{microtype}
\makeatletter
% pgf reserves PDF objects when loaded; their numbers are not valid in a new job.
\pdfextension obj reserveobjnum \edef\pgf@sys@pdf@extgs@objnum{\the\numexpr\pdffeedback lastobj\relax}
\pdfextension obj reserveobjnum \edef\pgf@sys@pdf@pattern@objnum{\the\numexpr\pdffeedback lastobj\relax}
\pdfextension obj reserveobjnum \edef\pgf@sys@pdf@colrospaces@objnum{\the\numexpr\pdffeedback lastobj\relax}
\pgf@sys@pdf@install@pageresources
% glossaries declares each glossary in the .aux with a \write made while it loads, in the format's run, which a
% format cannot keep (it is a node of the page): written again (twice without the format, which makeglossaries allows).
\ifdefined\@glo@types
\AtBeginDocument{\protected@write\@auxout{}{\string\providecommand\string\@newglossary[4]{}}%
  \@for\fp@t:=\@glo@types\do{\ifx\fp@t\@empty\else\protected@write\@auxout{}{\string\@newglossary{\fp@t}%
    {\csname @glotype@\fp@t @log\endcsname}{\csname @glotype@\fp@t @in\endcsname}{\csname @glotype@\fp@t @out\endcsname}}\fi}}
\fi
\makeatother
";

/// The parts of the preamble taken out of the format: (what is cut, where it goes).
struct Cut {
    start: &'static str,
    /// The cut ends at the end of the line holding this, searched from `start` (or the start's own line).
    end: Option<&'static str>,
    /// Put back after the format (else only removed: microtype, which AFTER_FORMAT loads).
    after: bool,
    /// Without it the preamble cannot be precompiled.
    required: bool,
}

const CUTS: &[Cut] = &[
    Cut { start: r"\InputIfFileExists{wordcounts.tex}", end: Some("chapters)}}"), after: true, required: false },
    Cut { start: r"\makeglossaries", end: None, after: true, required: false },
    Cut { start: r"\usepackage{microtype}", end: None, after: false, required: false },
];

/// The first line of the main file on the format: \endofdump as nothing when there is no format. The name is not
/// written out: with the format, mylatexformat skips the main file's lines up to the first that holds it.
const ENDOFDUMP: &str = "\\ifcsname end\\detokenize{ofdump}\\endcsname\\else\\expandafter\\let\\csname end\\detokenize{ofdump}\\endcsname\\relax\\fi\n";

/// The format's files from the preamble's .tex and the main file (mkfmt.py's port). Err when the main file does not
/// `\input` the preamble.
pub fn fmt_files(preamble: &str, main: &str, preamble_stem: &str) -> Result<FmtFiles, String> {
    let mut pre = preamble.replace("\r\n", "\n");
    let mut after = String::from(AFTER_FORMAT);
    for c in CUTS {
        // a line of its own that starts with the cut (not a comment mentioning it)
        let Some(i) = line_starting(&pre, c.start) else {
            if c.required {
                return Err(format!("the preamble has no {}", c.start));
            }
            continue;
        };
        let j = c.end.and_then(|e| pre[i..].find(e).map(|k| i + k)).unwrap_or(i);
        let j = pre[j..].find('\n').map_or(pre.len(), |k| j + k);
        let cut = pre[i..j].to_string();
        pre.replace_range(i..j, &format!("% ({} is read after the format)", c.start.trim_start_matches('\\')));
        if c.after {
            // with @ a letter, as in the preamble (wordcounts.tex holds \wc@set lines)
            after += &format!("\\makeatletter\n{cut}\n\\makeatother\n");
        }
    }
    after += "\\input{fp-record}\n\\input{fp-tikz}\n";
    let main = main.replace("\r\n", "\n");
    let tok = format!("\\input{{{preamble_stem}}}");
    let k = main.find(&tok).ok_or_else(|| format!("the main file does not \\input{{{preamble_stem}}}"))?;
    let rest = &main[k + tok.len()..];
    // \endofdump is mylatexformat's; without the format (the fallback) it is nothing (\providecommand refuses names that
    // start with "end")
    let main_fmt = format!("{ENDOFDUMP}{}\\input{{0-preamble-static}}\n\\endofdump\n{after}{rest}", &main[..k]);
    Ok(FmtFiles { static_preamble: pre, main_fmt, main_body: format!("{after}{rest}") })
}

/// The byte offset of the first line that starts with `s` (after spaces).
fn line_starting(text: &str, s: &str) -> Option<usize> {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let t = line.trim_start();
        if t.starts_with(s) {
            return Some(at + line.len() - t.len());
        }
        at += line.len();
    }
    None
}

/// The format's run: `lualatex -ini` with mylatexformat, recording the files it reads (`-recorder`: <job>.fls).
pub fn format_command(job: &str) -> Vec<String> {
    ["lualatex", "-ini", "-recorder", "-interaction=nonstopmode", &format!("-jobname={job}"), "&lualatex", "mylatexformat.ltx", "main-fmt.tex"].iter().map(|s| s.to_string()).collect()
}

/// The input files of a run, from its recorder file (`INPUT path` lines), each once, in order.
pub fn fls_inputs(fls: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in fls.lines() {
        if let Some(p) = l.strip_prefix("INPUT ") {
            let p = p.trim().to_string();
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// The format's key: everything that goes into it (the preamble, the two generated files, the engine's version) and the
/// modification time and size of every file its run read (`files`: path, mtime in ms, size; a file that is gone is
/// None). When it changes the format is rebuilt.
pub fn format_key(preamble: &str, static_preamble: &str, main_fmt: &str, version: &str, files: &[(String, Option<(f64, f64)>)]) -> String {
    let mut s = format!("{preamble}\0{static_preamble}\0{main_fmt}\0{version}\0");
    for (p, st) in files {
        match st {
            Some((m, z)) => s += &format!("{p}\0{m}\0{z}\0"),
            None => s += &format!("{p}\0gone\0"),
        }
    }
    format!("{:016x}", xxhash_rust::xxh3::xxh3_64(s.as_bytes()))
}

// ------------------------------------------------------------------ the records

/// Records in the full build's .aux where each section, list and the bibliography starts, and the state there; only
/// `\immediate\write` and Lua, so that nothing enters the page and the layout is unchanged (a \write whatsit between a
/// paragraph and a heading changes the spacing). The counters are every counter LaTeX checkpoints (`\cl@@ckpt`, as
/// \include saves them), hyperref's link counter, and the page style's numbering.
pub const RECORD: &str = r#"% Fast preview (written by the LaTeX plugin): the full build records in its .aux where each section, list and the
% bibliography starts, and the state there, so that a preview can typeset one of them alone. Nothing enters the page.
\directlua{dofile("fp-record.lua")}
\makeatletter
\AddToHook{begindocument/end}{\immediate\write\@auxout{\string\providecommand\string\fpsec[4]{}}}
\def\fp@sec{\ifx\@onlypreamble\@notprerr\par\immediate\write\@auxout{\directlua{fp_record()}}\fi}
\AddToHook{cmd/section/before}{\fp@sec}
\AddToHook{cmd/subsection/before}{\fp@sec}
\AddToHook{cmd/subsubsection/before}{\fp@sec}
\AddToHook{cmd/tableofcontents/before}{\fp@sec}
\AddToHook{cmd/listoffigures/before}{\fp@sec}
\AddToHook{cmd/listoftables/before}{\fp@sec}
\AddToHook{cmd/printbibliography/before}{\fp@sec}
\AddToHook{cmd/printglossary/before}{\fp@sec}
\makeatother
"#;

/// The records' Lua: one `\fpsec{file}{line}{physical page}{key=value;...}` line, as other characters (catcode table
/// -2) so that \write does not expand it.
pub const RECORD_LUA: &str = r#"-- Fast preview records (written by the LaTeX plugin).
local function counters()
  local t = {}
  for name in (token.get_macro("cl@@ckpt") or ""):gmatch("{([^}]+)}") do
    local ok, v = pcall(function() return tex.count["c@" .. name] end)
    if ok and v then t[#t + 1] = "c:" .. name .. "=" .. v end
  end
  local ok, v = pcall(function() return tex.count["Hy@linkcounter"] end)
  if ok and v then t[#t + 1] = "link=" .. v end
  return table.concat(t, ";")
end
FP_COUNTERS = counters
local function empty(m) local v = token.get_macro(m); return v == nil or v == "" or v == "\\relax " end
function fp_state()
  local style = (token.get_macro("thepage") or ""):match("@(%a+)") or "arabic"
  return table.concat({
    "total=" .. tex.pagetotal, "stretch=" .. tex.pagestretch, "shrink=" .. tex.pageshrink,
    "goal=" .. tex.pagegoal, "depth=" .. tex.pagedepth, "prevdepth=" .. (tex.nest[tex.nest.ptr].prevdepth or 0),
    "colroom=" .. tex.dimen["@colroom"], "colht=" .. tex.dimen["@colht"],
    "floats=" .. ((empty("@toplist") and empty("@botlist")) and 0 or 1), "pending=" .. ((empty("@deferlist") and empty("@currlist")) and 0 or 1),
    "style=" .. style, "texttop=" .. (tex.voffset + tex.dimen["topmargin"] + tex.dimen["headheight"] + tex.dimen["headsep"] + 4736286),
    counters()}, ";")
end
function fp_record()
  local file = status.filename or ""
  file = file:gsub("^%./", ""):match("[^/\\]+$") or file
  tex.sprint(-2, "\\fpsec{" .. file .. "}{" .. tex.inputlineno .. "}{" .. (status.total_pages + 1) .. "}{" .. fp_state() .. "}")
end
"#;

/// A recorded start: a section (or a list, or the bibliography) of the full build.
#[derive(Clone, Debug, PartialEq)]
pub struct Start {
    /// The input file's name (`3-introduction.tex`), and the line of the command.
    pub file: String,
    pub line: u32,
    /// The physical page being built there, from 1.
    pub phys: u32,
    /// key=value pairs: the page builder's totals in sp (total, stretch, shrink, goal, depth, prevdepth, colroom),
    /// pending (floats waiting), style (the page numbering), texttop (from the paper's top to the text, sp), topskip,
    /// c:<counter> and link.
    pub state: BTreeMap<String, String>,
}

impl Start {
    pub fn num(&self, k: &str) -> i64 {
        self.state.get(k).and_then(|v| v.parse::<f64>().ok()).map_or(0, |f| f as i64)
    }

    /// Whether a region can start here: no floats waiting and nothing inserted (footnotes) on the page so far, so the
    /// full build's page above the start is the only thing that page holds besides the region.
    pub fn clean(&self) -> bool {
        let page_empty = self.num("total") == 0;
        self.num("pending") == 0 && self.num("floats") == 0 && (page_empty || self.num("goal") >= self.num("colroom"))
    }

    /// The counters that number something (and the link counter), as the state's own text: equal at two places when
    /// nothing between them stepped such a counter that the other did not.
    pub fn counters(&self) -> String {
        let silent = |name: &str| SILENT.contains(&name) || name.starts_with("saved") || self.state.contains_key(&format!("c:saved{name}"));
        self.state.iter().filter(|(k, _)| k.strip_prefix("c:").is_some_and(|n| !silent(n)) || *k == "link").map(|(k, v)| format!("{k}={v};")).collect()
    }
}

/// Counters that print nothing and number nothing, so that a region ending with another value of one is no shift:
/// biblatex's state for the citation in hand (and each name or list field's count, with its saved copy: the rule in
/// `counters`), longtable's and the outline's bookkeeping. A counter not known here counts: an unknown difference
/// marks the pages after it stale rather than claiming they are current.
const SILENT: &[&str] = &[
    "abbrvpenalty", "blx@maxsection", "citecounter", "citecount", "citetotal", "highnamepenalty", "instcount", "listcount", "liststart", "liststop", "listtotal",
    "lownamepenalty", "maxcitecounter", "maxcomprange", "maxextraalpha", "maxextradate", "maxextraname", "maxextratitleyear", "maxextratitle", "maxitems",
    "maxnames", "maxparens", "mincomprange", "mincompwidth", "minitems", "minnames", "multicitecount", "multicitetotal", "parenlevel", "refsection",
    "refsegment", "tabx@nest", "uniquelist", "uniquename", "textcitecount", "textcitetotal", "textcitemaxnames", "smartand", "bbx:relatedcount",
    "bbx:relatedtotal", "biburlbigbreakpenalty", "biburlbreakpenalty", "biburlnumpenalty", "biburlucpenalty", "biburllcpenalty", "LT@tables", "LT@chunks",
    "section@level", "bookmark@seq@number",
];

/// The records of a full build's .aux, in document order (the order they were written).
pub fn records(aux: &str) -> Vec<Start> {
    let mut out = Vec::new();
    for l in aux.lines() {
        let Some(rest) = l.strip_prefix("\\fpsec{") else { continue };
        let parts: Vec<&str> = rest.trim_end_matches('}').split("}{").collect();
        if parts.len() != 4 {
            continue;
        }
        let (Ok(line), Ok(phys)) = (parts[1].parse(), parts[2].parse()) else { continue };
        let state = parts[3].split(';').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect();
        out.push(Start { file: parts[0].to_string(), line, phys, state });
    }
    out
}

// ------------------------------------------------------------------ regions

/// What a preview typesets: lines `start..end` of `file` (from 1, `end` excluded; u32::MAX: to the end of the file),
/// starting in the state of `state` (a record at or after `start`).
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub file: String,
    pub start: u32,
    pub end: u32,
    pub state: usize,
    /// The record where the full build was when it reached `end` (None: the end of the document).
    pub next: Option<usize>,
    /// Re-typeset because a generated list changed (not the edit itself): the TOC, a list of figures, the
    /// bibliography, the Terms list.
    pub list: bool,
    /// Its TikZ pictures typeset, not taken from the cache (one of them changed).
    pub tikz: bool,
    /// Starts within it (from its first end on) where the preview writes its state: the first where it is the full
    /// build's ends the region there (`extend`).
    pub cands: Vec<usize>,
}

/// How much a preview of an edit typesets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scope {
    /// The edited section and the `span - 1` after it.
    Section { span: usize },
    Chapter,
}

/// The region for an edit at `line` of `file`: from the last clean start at or before it (stepping back over starts
/// where floats or footnotes were waiting) to the start `span` sections later, or the whole chapter.
pub fn edit_region(recs: &[Start], file: &str, line: u32, scope: Scope) -> Option<Region> {
    let starts: Vec<usize> = (0..recs.len()).filter(|&k| recs[k].file == file).collect();
    let first = *starts.first()?;
    let (s, e) = match scope {
        Scope::Chapter => (first, None),
        Scope::Section { span } => {
            let at = starts.iter().rposition(|&k| recs[k].line <= line)?; // before the first section: no preview
            let back = starts[..=at].iter().rposition(|&k| recs[k].clean()).unwrap_or(0);
            (starts[back], starts.get(at + span.max(1)).copied())
        }
    };
    let next = e.or_else(|| next_in_document(recs, starts.last().copied().unwrap_or(first)));
    Some(Region { file: file.to_string(), start: recs[s].line, end: e.map_or(u32::MAX, |k| recs[k].line), state: s, next, list: false, tikz: false, cands: Vec::new() })
}

/// The first record after `k` that is in another file (where the document goes on after this file).
fn next_in_document(recs: &[Start], k: usize) -> Option<usize> {
    (k + 1..recs.len()).find(|&j| recs[j].file != recs[k].file)
}

/// The region of a list command (`\tableofcontents` ...) recorded at `k`: the block of lines around it, between blank
/// lines (its group and its \clearpage with it; not an \input or \end{document}), of the file's text `text`.
pub fn list_region(recs: &[Start], k: usize, text: &str) -> Region {
    let lines: Vec<&str> = text.lines().collect();
    let at = recs[k].line as usize; // from 1
    let stop = |l: &str| l.trim().is_empty() || l.contains("\\input") || l.contains("\\include") || l.contains("\\end{document}");
    let mut s = at;
    while s > 1 && !stop(lines.get(s - 2).copied().unwrap_or("")) {
        s -= 1;
    }
    let mut e = at + 1;
    while e <= lines.len() && !stop(lines[e - 1]) {
        e += 1;
    }
    // the block may hold several lists (\listoffigures and \listoftables): it starts in the state of its first
    // (and its closing line: biblatex reads past printbibliography before its heading is recorded)
    let in_block = |j: &usize| recs[*j].file == recs[k].file && (s as u32..=e as u32).contains(&recs[*j].line);
    let state = (0..recs.len()).find(in_block).unwrap_or(k);
    let last = (0..recs.len()).rev().find(in_block).unwrap_or(k);
    let next = (last + 1..recs.len()).next();
    Region { file: recs[k].file.clone(), start: s as u32, end: e as u32, state, next, list: true, tikz: false, cands: Vec::new() }
}

/// A region typeset on past its end through `n` more starts of its file: if the edit moved the page breaks, the
/// preview may meet the full build's state again at one of them (a page with stretch takes the line back), and the
/// region ends there instead (plan). Each start costs a section's typesetting, a tenth of a second or two.
pub fn extend(recs: &[Start], g: &mut Region, n: usize) {
    if g.list || g.end == u32::MAX {
        return;
    }
    let later: Vec<usize> = (0..recs.len()).filter(|&k| recs[k].file == g.file && recs[k].line >= g.end).collect();
    g.cands = later.iter().take(n + 1).copied().collect();
    match later.get(n + 1) {
        Some(&k) => {
            g.end = recs[k].line;
            g.next = Some(k);
        }
        None => {
            g.end = u32::MAX;
            g.next = later.last().and_then(|&k| next_in_document(recs, k));
        }
    }
}

/// The totals a candidate start must match for the preview job to stop there (plan checks the counters as well).
const EXPECT: [&str; 9] = ["total", "stretch", "shrink", "goal", "depth", "prevdepth", "pending", "colroom", "floats"];

/// The records of the generated lists, by what they print: `toc`, `lof`, `lot`, `bib`, `terms` (the line's command).
pub fn list_kind(line: &str) -> Option<&'static str> {
    [("\\tableofcontents", "toc"), ("\\listoffigures", "lof"), ("\\listoftables", "lot"), ("\\printbibliography", "bib"), ("\\printglossary", "terms"), ("\\printterms", "terms")]
        .iter()
        .find(|(c, _)| line.contains(c))
        .map(|(_, k)| *k)
}

/// The request a waiting preview job reads: one line, the regions separated by `;`, each `file start end` and the
/// state's pairs.
pub fn request(recs: &[Start], regions: &[Region]) -> String {
    regions
        .iter()
        .map(|r| {
            let st = &recs[r.state];
            let mut state: Vec<String> = st.state.iter().map(|(k, v)| format!("{k}={v}")).collect();
            if r.tikz {
                state.push("tikz=1".into());
            }
            if !r.cands.is_empty() {
                // line@page@totals: the driver stops at the first where its page and totals are the full build's
                let expect = |k: usize| {
                    let c = &recs[k];
                    let mut f = vec![c.line.to_string(), c.phys.to_string()];
                    f.extend(EXPECT.iter().map(|key| c.state.get(*key).cloned().unwrap_or_default()));
                    f.join("@")
                };
                state.push(format!("cands={}", r.cands.iter().map(|&k| expect(k)).collect::<Vec<_>>().join(":")));
                state.push(format!("phys={}", st.phys));
            }
            format!("{} {} {} {}", r.file, r.start, if r.end == u32::MAX { 0 } else { r.end }, state.join(","))
        })
        .collect::<Vec<_>>()
        .join(";")
}

// ------------------------------------------------------------------ the preview job

/// The preview job: the main file's preamble (without the records, with cached TikZ pictures used as they are) and
/// a driver that waits for a request. `replay`: the commands the preamble defines without arguments, which a region
/// replays from the lines of its file before it (\startappendices ...).
pub fn driver(main_fmt: &str, replay: &[String]) -> Result<String, String> {
    let m = main_fmt.replace("\r\n", "\n");
    let b = m.find("\\begin{document}").ok_or("the main file has no \\begin{document}")?;
    let head = m[..b].replace("\\input{fp-record}\n", "").replace("\\input{fp-tikz}\n", "\\input{fp-tikz}\n\\ifdefined\\tikzexternalize\\tikzset{external/mode=graphics if exists}\\fi\n");
    let replay: Vec<String> = replay.iter().map(|c| format!("{c} = true")).collect();
    Ok(format!(
        "{head}\\directlua{{dofile(\"fp-record.lua\")}}\n\\directlua{{FP_REPLAY = {{{}}}; dofile(\"fp-preview.lua\")}}\n{DRIVER_TEX}",
        replay.join(", ")
    ))
}

/// The driver's TeX: fonts loaded once (the first use of a font costs most of a small preview), then the wait.
const DRIVER_TEX: &str = r"\makeatletter
% The page state of a region's start (fp-preview.lua passes the recorded values, in sp): when the start was not at the
% top of a page, the page's natural height with its stretch and shrink, its goal (floats and footnotes above) and the
% depth of the last line, so that the page builder breaks where the full build did.
\def\fp@place#1#2#3#4#5#6#7{%
  \ifnum#1>\z@
    \hbox{}\pagegoal=#4sp\relax
    \ifdim#6sp<#4sp\relax\pagegoal=#6sp\relax\fi
    \global\@colroom=#6sp\relax\global\vsize=#6sp\relax
    \vskip\dimexpr#1sp-\topskip\relax plus #2sp minus #3sp\relax
    \nointerlineskip\hbox{\vrule height\z@ depth#5sp width\z@}%
  \else\ifdim#6sp<\textheight
    \global\@colroom=#6sp\relax\global\vsize=#6sp\relax
  \fi\fi}
\def\fp@end{\par\directlua{fp_end()}}
\def\fp@region#1{\clearpage\directlua{fp_begin(#1)}}
\makeatother
\begin{document}
\setbox0=\vbox{\normalsize a\textbf{a}\textit{a}\small a\footnotesize a\scriptsize a\tiny a{\Large\bfseries a}{\large\bfseries a}{\small\itshape a}\texttt{a}{\small\ttfamily a}\textsc{a}$a^{a_a}\sum\int\mathrm{a}$\[\sum_a^a\]}
\directlua{fp_wait()}
\clearpage
\directlua{fp_done()}
\end{document}
";

/// The driver's Lua (fp-preview.lua).
pub const DRIVER_LUA: &str = r#"-- Fast preview driver (written by the LaTeX plugin). It prints FPREADY and reads one request from stdin: regions
-- separated by ";", each "file start end state" (state: key=value pairs separated by commas, as recorded by
-- fp-record.lua in the full build). Each region is typeset on its own pages: the lines of its file before start are
-- read as empty lines (but for the commands in FP_REPLAY on a line of their own), so that SyncTeX keeps the file's line
-- numbers; at start the recorded state is applied; at end the state there is written. What it writes goes to
-- <job>.fpl, one line each: FPREGION k pages (pages shipped before the region), FPEND k pages state, FPWARN text.
local NOREPLAY = {clearpage = true, newpage = true, cleardoublepage = true, FloatBarrier = true, printterms = true,
  printbibliography = true, printglossary = true, printglossaries = true, tableofcontents = true, listoffigures = true,
  listoftables = true, maketitle = true, maketitlepage = true, item = true}
local out = io.open(tex.jobname .. ".fpl", "w")
local function put(s) out:write(s, "\n"); out:flush() end
local R, cur = {}, nil
function fp_wait()
  texio.write_nl("term", "FPREADY")
  io.stdout:flush()
  local line = (io.read("*l") or ""):gsub("[\r\n]", "")
  for spec in line:gmatch("[^;]+") do
    local file, s, e, st = spec:match("^(%S+) (%d+) (%d+) ?(.*)$")
    if file then
      local state = {}
      for k, v in st:gmatch("([^,=]+)=([^,]*)") do state[k] = v end
      e = tonumber(e)
      local cand = {}
      for c in (state.cands or ""):gmatch("[^:]+") do
        local f = {}
        for v in c:gmatch("[^@]*") do f[#f + 1] = v end
        cand[tonumber(f[1])] = f
      end
      R[#R + 1] = {file = file, s = tonumber(s), e = (e == 0) and math.huge or e, state = state, cand = cand}
    end
  end
  for k = 1, #R do tex.print("\\csname fp@region\\endcsname{" .. k .. "}") end
end
local function filter(buf)
  if not cur or not status.filename or status.filename:sub(-#cur.file) ~= cur.file then return buf end
  local n = status.linenumber
  if n < cur.s then
    local cmd = buf:match("^\\([A-Za-z]+) *$")
    if cmd and FP_REPLAY[cmd] and not NOREPLAY[cmd] then return buf end
    return ""
  end
  if n == cur.s then return "\\directlua{fp_apply()}" .. buf end
  if cur.stop then return "" end
  if cur.cand[n] and n < cur.e then return "\\par\\directlua{fp_cand(" .. n .. ")}" .. buf end
  if n >= cur.e then
    if not cur.ended then cur.ended = true; return "\\csname fp@end\\endcsname" end
    return ""
  end
  return buf
end
function fp_begin(k)
  cur = R[k]
  cur.k = k
  cur.first = status.total_pages
  put("FPREGION " .. k .. " " .. status.total_pages)
  luatexbase.add_to_callback("process_input_buffer", filter, "fp_filter")
  tex.print("\\input{" .. cur.file .. "}\\directlua{fp_after()}")
end
function fp_after()
  luatexbase.remove_from_callback("process_input_buffer", "fp_filter")
  if not cur.ended then cur.ended = true; tex.print("\\csname fp@end\\endcsname") end
end
function fp_apply()
  -- the numbering style first (\pagenumbering sets the page to 1), then the counters
  tex.print("\\pagenumbering{" .. (cur.state.style or "arabic") .. "}\\directlua{fp_counters()}")
end
function fp_counters()
  local st = cur.state
  for k, v in pairs(st) do
    local name = k:match("^c:(.+)$")
    if name then tex.setcount("global", "c@" .. name, tonumber(v)) end
  end
  if st.link then tex.setcount("global", "Hy@linkcounter", tonumber(st.link)) end
  if st.tikz == "1" then tex.print("\\ifdefined\\tikzexternaldisable\\tikzexternaldisable\\fi") end
  if tonumber(st.pending or 0) > 0 then put("FPWARN " .. cur.k .. " floats were waiting at the start: pages are approximate") end
  tex.print(string.format("\\csname fp@place\\endcsname{%d}{%d}{%d}{%d}{%d}{%d}{0}", tonumber(st.total or 0),
    tonumber(st.stretch or 0), tonumber(st.shrink or 0), tonumber(st.goal or 0), tonumber(st.prevdepth or 0),
    tonumber(st.colroom or 0)))
end
local EXPECT = {"total", "stretch", "shrink", "goal", "depth", "prevdepth", "pending", "colroom", "floats"}
function fp_cand(n)
  local st = fp_state()
  put("FPCAND " .. cur.k .. " " .. n .. " " .. status.total_pages .. " " .. st)
  -- the full build's page and totals here, nothing pending (a float would be put on this page by the earpage that
  -- follows a region): what follows is the full build's, so the region stops (the rest of the file is skipped)
  local f, have = cur.cand[n], {}
  for k, v in st:gmatch("([^;=]+)=([^;]*)") do have[k] = v end
  local same = f and have.pending == "0" and tonumber(f[2]) and status.total_pages == cur.first + tonumber(f[2]) - tonumber(cur.state.phys or 0)
  for i, key in ipairs(EXPECT) do same = same and have[key] == f[i + 2] end
  if same then cur.stop = true; cur.ended = true; put("FPEND " .. cur.k .. " " .. status.total_pages .. " " .. st) end
end
function fp_end()
  put("FPEND " .. cur.k .. " " .. status.total_pages .. " " .. fp_state())
end
function fp_done()
  put("FPDONE " .. status.total_pages)
  out:close()
end
"#;

/// What the preview job wrote about each region.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Typeset {
    /// Pages shipped before the region: its first page in the preview's PDF (from 0).
    pub first: u32,
    /// Pages shipped when it reached its end, and the state there (None when it never did: it failed).
    pub end: Option<(u32, BTreeMap<String, String>)>,
    /// At each candidate start (line), the pages shipped and the state.
    pub cands: Vec<(u32, u32, BTreeMap<String, String>)>,
    pub warnings: Vec<String>,
}

/// The preview job's .fpl: each region's pages and end state, and the pages of the whole preview (None when the job
/// did not finish).
pub fn results(fpl: &str) -> (Vec<Typeset>, Option<u32>) {
    let mut out: Vec<Typeset> = Vec::new();
    let mut done = None;
    for l in fpl.lines() {
        let f: Vec<&str> = l.splitn(4, ' ').collect();
        let k = f.get(1).and_then(|k| k.parse::<usize>().ok()).unwrap_or(0);
        if k > out.len() && f[0] != "FPDONE" {
            out.resize(k, Typeset::default());
        }
        match f[0] {
            "FPREGION" => out[k - 1].first = f.get(2).and_then(|p| p.parse().ok()).unwrap_or(0),
            "FPEND" => {
                let pages = f.get(2).and_then(|p| p.parse().ok()).unwrap_or(0);
                let state = f.get(3).unwrap_or(&"").split(';').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect();
                out[k - 1].end = Some((pages, state));
            }
            "FPCAND" => {
                let g: Vec<&str> = l.splitn(5, ' ').collect();
                let num = |i: usize| g.get(i).and_then(|p| p.parse().ok()).unwrap_or(0);
                let state = g.get(4).unwrap_or(&"").split(';').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect();
                out[k - 1].cands.push((num(2), num(3), state));
            }
            "FPWARN" => out[k - 1].warnings.push(f[2..].join(" ")),
            "FPDONE" => done = f.get(1).and_then(|p| p.parse().ok()),
            _ => {}
        }
    }
    (out, done)
}

// ------------------------------------------------------------------ the splice

/// One replacement: the full build's pages `main` (from 0, end excluded) give way to the preview's `preview`. With
/// `top` (in bp from the page's top), the first page is joined: the full build's above the cut, the preview's below
/// it. With `bottom`, the last page is joined: the preview's above the cut, the full build's below it (both cuts on the
/// same page when the region is within one page).
#[derive(Clone, Debug, PartialEq)]
pub struct Replace {
    pub main: (u32, u32),
    pub preview: (u32, u32),
    pub top: Option<f64>,
    /// How far the preview's first page is moved down to meet the full build's (bp): the floats at the top of that
    /// page, which the full build placed and the preview's page leaves empty.
    pub shift: f64,
    pub bottom: Option<f64>,
    /// Its last preview page is the region's last, kept although the region did not end as the full build did (it is
    /// stale, with what follows).
    pub tail: bool,
    /// With a tail: the full build's page after it shows only what lies below this cut (bp from the top), the start of
    /// what followed the region, so that nothing appears twice.
    pub crop: Option<(f64, f64)>,
    /// The first page's top band comes from this preview page (the region before, which ended where this one starts)
    /// instead of the full build's page.
    pub top_page: Option<u32>,
    /// With a shift: where the page's foot begins (bp from the top); below it the full build's page is shown (the
    /// page number), since the moved preview page's foot would be pushed off.
    pub foot: Option<f64>,
}

/// The splice: the replacements, and the pages of the result that are not current, with why (from 0, end excluded).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub replace: Vec<Replace>,
    pub stale: Vec<((u32, u32), String)>,
    /// Something the full build must settle soon (numbering shifted): start it at once.
    pub build_now: bool,
}

const SP_PER_BP: f64 = 65781.76;

/// Where the text block ends on a record's page (bp from the top): what the page builder had left for text there,
/// below the floats at the top; the footnotes inserted so far lie below it.
fn text_bottom(r: &Start) -> f64 {
    let floats = if r.num("floats") > 0 { (r.num("colht") - r.num("colroom")).max(0) } else { 0 };
    (r.num("texttop") + floats + r.num("goal").min(r.num("colht").max(r.num("goal")))) as f64 / SP_PER_BP
}

/// Where a record's place is on its page, in bp from the paper's top: below the last line typeset before it.
fn cut(r: &Start) -> f64 {
    // the floats at the top of the page come before the text (colroom is what they leave of the column)
    let floats = if r.num("floats") > 0 { (r.num("colht") - r.num("colroom")).max(0) } else { 0 };
    // 3 bp into the skip before the heading that starts there, clear of the last line's descenders
    (r.num("texttop") + floats + r.num("total") + r.num("depth")) as f64 / SP_PER_BP + CUT_MARGIN
}

/// The cuts lie this far (bp) below the last line before a region, inside the skip above its heading.
const CUT_MARGIN: f64 = 3.0;

/// The splice of a preview's regions into the last full build of `main_pages` pages (`preview_pages`: the preview's).
/// A region is current when the preview reached its end in the state the full build was in there (the same page
/// offset, totals and counters): what follows is then the full build's, unchanged, and the page where the region ends
/// is joined there. Otherwise the pages from there on are stale until the full build lands: with the same totals and
/// counters but another page count, only their page numbers (STALE_PAGES); with other counters, their numbering
/// (STALE_NUMBERS, and a full build at once); else their layout.
pub fn plan(recs: &[Start], regions: &[Region], typeset: &[Typeset], preview_pages: u32, main_pages: u32) -> Plan {
    let mut p = Plan::default();
    // where the region before ended without meeting the full build (its record): a region starting there starts on a
    // page of its own, since the full build's page above it holds the old end of the one before
    let mut ran_on: Option<usize> = None;
    // where the region before ended exactly at this one's start, mid-page: (record, its last preview page)
    let mut joined_into: Option<(usize, u32)> = None;
    for (k, r) in regions.iter().enumerate() {
        let st = &recs[r.state];
        let Some(t) = typeset.get(k) else { continue };
        let Some((pages, end)) = &t.end else { continue }; // not typeset: the full build's pages stay
        let from = st.phys - 1;
        let after_stale = ran_on.take() == Some(r.state);
        let top_page = joined_into.take().filter(|(j, _)| *j == r.state).map(|(_, pg)| pg);
        let top = (st.num("total") > 0 && !after_stale).then(|| cut(st));
        let shift = if top.is_some() && st.num("floats") > 0 { (st.num("colht") - st.num("colroom")).max(0) as f64 / SP_PER_BP } else { 0.0 };
        let foot = (shift > 0.0).then(|| (st.num("texttop") + st.num("colht")) as f64 / SP_PER_BP);
        // the first page of a region that starts on the page where the one before ended (three bands), and a generated
        // list after an edited region (the bibliography, the Terms list: their start moves with what comes before):
        // not claimed current
        if p.replace.last().is_some_and(|q| q.main.1 == from + 1 && q.bottom.is_some()) && top.is_some() {
            p.stale.push(((from, from + 1), STALE_LAYOUT.to_string()));
        }
        if r.list && regions[..k].iter().any(|q| !q.list) {
            p.stale.push(((from, main_pages), STALE_LAYOUT.to_string()));
        }
        if after_stale {
            if let Some(prev) = p.replace.last_mut() {
                prev.crop = None;
            }
            p.stale.push(((from, from + 1), STALE_LAYOUT.to_string()));
        }
        if top.is_some() && (st.num("pending") > 0 || st.num("goal") < st.num("colroom")) {
            // floats waiting or footnotes above the start (no clean start before it in its file): not exact
            p.stale.push(((from, from + 1), STALE_LAYOUT.to_string()));
        }
        let Some(last) = r.next else {
            // the end of the document: everything the region typeset
            let to = typeset.get(k + 1).map_or(preview_pages, |n| n.first);
            p.replace.push(Replace { main: (from, main_pages), preview: (t.first, to), top, bottom: None, shift, tail: false, crop: None, top_page, foot });
            continue;
        };
        // the ends to try, in order: each candidate start, then where the region stopped
        let line_rec = |line: u32| (0..recs.len()).find(|&j| recs[j].file == r.file && recs[j].line == line);
        let mut ends: Vec<(usize, u32, &BTreeMap<String, String>)> = t.cands.iter().filter_map(|(l, pg, s)| line_rec(*l).map(|j| (j, *pg, s))).collect();
        // where the job stopped at a candidate (its end is that candidate's state), the region ends there
        let last = t.cands.iter().rev().find(|c| &c.2 == end && c.1 == *pages).and_then(|c| line_rec(c.0)).unwrap_or(last);
        ends.push((last, *pages, end));
        let meets = |j: usize, pg: u32, e: &BTreeMap<String, String>| {
            let nx = &recs[j];
            let totals = ["total", "stretch", "shrink", "goal", "depth", "prevdepth", "pending", "colroom", "floats"].iter().all(|key| e.get(*key) == nx.state.get(*key));
            let counters = Start { file: String::new(), line: 0, phys: 0, state: e.clone() }.counters() == nx.counters();
            // on an overfull page (a list that may not break there) the end's page is not yet known: no match
            let offset = pg.checked_sub(t.first) == nx.phys.checked_sub(st.phys) && e.get("total").and_then(|v| v.parse::<i64>().ok()) <= e.get("goal").and_then(|v| v.parse::<i64>().ok());
            (offset, totals, counters)
        };
        if let Some(&(j, pg, _)) = ends.iter().find(|(j, pg, e)| meets(*j, *pg, e) == (true, true, true)) {
            // the page where it ends is joined with the full build's (unless the end is the top of a page); when the
            // next region starts there, its first page is joined with this one's last instead
            let nx = &recs[j];
            let mid_page = nx.num("total") > 0;
            if mid_page && regions.get(k + 1).is_some_and(|n| n.state == j) {
                p.replace.push(Replace { main: (from, nx.phys - 1), preview: (t.first, pg), top, bottom: None, shift, tail: false, crop: None, top_page, foot });
                joined_into = Some((j, pg));
                continue;
            }
            let m = mid_page as u32;
            p.replace.push(Replace { main: (from, nx.phys - 1 + m), preview: (t.first, pg + m), top, bottom: mid_page.then(|| cut(nx)), shift, tail: false, crop: None, top_page, foot });
            continue;
        }
        let (nx, pages) = (&recs[last], pages);
        let (_, totals, counters) = meets(last, *pages, end);
        let mid_page = nx.num("total") > 0;
        // its last page kept, and the full build's page there cropped to what follows the region: nothing twice
        // every page it typeset (to the next region's first: the page count at its end may be short), then the full
        // build's page there cropped to what follows the region: nothing twice
        let to = typeset.get(k + 1).map_or(preview_pages, |n| n.first).max(*pages);
        p.replace.push(Replace { main: (from, nx.phys - 1), preview: (t.first, to), top, bottom: None, shift, tail: mid_page, crop: mid_page.then(|| (cut(nx), text_bottom(nx))), top_page, foot });
        ran_on = Some(last);
        let shift = (*pages as i64 - t.first as i64) - (nx.phys as i64 - st.phys as i64);
        let why = if !counters {
            STALE_NUMBERS
        } else if totals && shift != 0 {
            STALE_PAGES
        } else {
            STALE_LAYOUT
        };
        p.build_now |= !counters;
        // a generated list that did not end where it did is approximate as a whole
        p.stale.push(((if r.list { from } else { nx.phys - 1 }, main_pages), why.to_string()));
    }
    p
}

// The pages of the spliced PDF that were stale in the full build's numbering, in the result's numbering (pages before
/// a replacement keep their index; after it they move by the difference in length).
pub fn shifted(plan: &Plan) -> Vec<((u32, u32), String)> {
    plan.stale
        .iter()
        .map(|((a, b), why)| {
            // each end moves by the replacements before it; a kept last page (tail) of a replacement ending at the start
            // is stale too; a range to the end of the document stays to the end
            let moved = |x: u32| plan.replace.iter().filter(|r| r.main.1 <= x).map(|r| (r.preview.1 - r.preview.0) as i64 - (r.main.1 - r.main.0) as i64).sum::<i64>();
            let tail = plan.replace.iter().any(|r| r.tail && r.main.1 == *a) as i64;
            let start = (*a as i64 + moved(*a) - tail).max(0) as u32;
            let end = (*b as i64 + moved(*b)).max(0) as u32;
            ((start, end), why.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PREAMBLE: &str = "\\usepackage{microtype}\n\\usepackage{glossaries-extra}\n% \\makeglossaries in a comment\n\\makeglossaries\n\\makeatletter\n\\InputIfFileExists{wordcounts.tex}{}{%\n  \\PackageWarningNoLine{preamble}{wordcounts.tex not found, so the word\n    counts print as ??: convert the chapters (LaTeX plugin: Build all\n    chapters)}}\n\\makeatother\n\\newcommand{\\printterms}{x}\n";
    const MAIN: &str = "\\documentclass{article}\n\\input{0-preamble}\n\\input{glossary}\n\\begin{document}\n\\input{3-introduction}\n\\end{document}\n";

    #[test]
    fn format_files() {
        let f = fmt_files(PREAMBLE, MAIN, "0-preamble").unwrap();
        assert!(!f.static_preamble.contains("\\usepackage{microtype}") && !f.static_preamble.contains("\n\\makeglossaries\n"));
        assert!(f.static_preamble.contains("% \\makeglossaries in a comment"), "a comment is not the command");
        assert!(!f.static_preamble.contains("\\InputIfFileExists{wordcounts.tex}") && f.static_preamble.contains("\\makeatother\n\\newcommand{\\printterms}"));
        assert!(f.main_fmt.starts_with(&format!("{ENDOFDUMP}\\documentclass{{article}}\n\\input{{0-preamble-static}}\n\\endofdump\n\\usepackage{{microtype}}\n")));
        let after = f.main_fmt.split("\\endofdump\n").nth(1).unwrap();
        let order: Vec<usize> = ["\\usepackage{microtype}", "pgf@sys@pdf@install@pageresources", "\\InputIfFileExists{wordcounts.tex}", "\\makeglossaries", "\\input{fp-record}", "\\input{fp-tikz}", "\\input{glossary}", "\\begin{document}"].iter().map(|s| after.find(s).unwrap_or_else(|| panic!("{s}"))).collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
        assert!(f.main_fmt.ends_with(&f.main_body));
        assert!(fmt_files(PREAMBLE, "\\documentclass{article}\n", "0-preamble").is_err());
    }

    #[test]
    fn format_keys() {
        let fls = "PWD C:/x\nINPUT C:/tex/article.cls\nINPUT ./0-preamble-static.tex\nOUTPUT fp-preamble.fmt\nINPUT C:/tex/article.cls\n";
        let inputs = fls_inputs(fls);
        assert_eq!(inputs, ["C:/tex/article.cls", "./0-preamble-static.tex"]);
        let st = |m: f64| inputs.iter().map(|p| (p.clone(), Some((m, 10.0)))).collect::<Vec<_>>();
        let k = format_key("p", "s", "m", "LuaHBTeX 1.25", &st(1.0));
        assert_eq!(k, format_key("p", "s", "m", "LuaHBTeX 1.25", &st(1.0)));
        for other in [format_key("p2", "s", "m", "LuaHBTeX 1.25", &st(1.0)), format_key("p", "s", "m2", "LuaHBTeX 1.25", &st(1.0)), format_key("p", "s", "m", "LuaHBTeX 1.26", &st(1.0)), format_key("p", "s", "m", "LuaHBTeX 1.25", &st(2.0))] {
            assert_ne!(k, other, "every part is in the key");
        }
        let mut gone = st(1.0);
        gone[0].1 = None;
        assert_ne!(k, format_key("p", "s", "m", "LuaHBTeX 1.25", &gone));
    }

    fn rec(file: &str, line: u32, phys: u32, state: &str) -> Start {
        Start { file: file.into(), line, phys, state: state.split(';').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.into(), v.into())).collect() }
    }

    fn doc() -> Vec<Start> {
        let base = "goal=46000000;colroom=46000000;pending=0;texttop=9000000;depth=100000;c:section=";
        vec![
            rec("main-fmt.tex", 27, 3, &format!("total=0;{base}0;c:figure=0")),
            rec("3-intro.tex", 10, 5, &format!("total=0;{base}0;c:figure=0")),
            rec("3-intro.tex", 40, 6, &format!("total=20000000;{base}1;c:figure=1")),
            rec("3-intro.tex", 70, 6, &format!("total=30000000;goal=40000000;colroom=46000000;pending=0;c:section=2;c:figure=1")),
            rec("3-intro.tex", 90, 7, &format!("total=10000000;{base}3;c:figure=2;pending=1")),
            rec("5-meth.tex", 5, 8, &format!("total=0;{base}4;c:figure=2")),
            rec("5-meth.tex", 30, 9, &format!("total=5000000;{base}5;c:figure=2")),
        ]
    }

    #[test]
    fn reading_records() {
        let aux = "\\relax\n\\fpsec{3-intro.tex}{10}{5}{total=0;goal=1;c:section=0;link=4}\n\\newlabel{x}{{1}{2}}\n\\fpsec{}{27}{3}{total=5}\n";
        let r = records(aux);
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].file.as_str(), r[0].line, r[0].phys, r[0].num("link")), ("3-intro.tex", 10, 5, 4));
        assert_eq!(r[0].counters(), "c:section=0;link=4;");
        assert_eq!((r[1].file.as_str(), r[1].num("total")), ("", 5));
    }

    #[test]
    fn regions() {
        let r = doc();
        // an edit at line 45: the section from 40 and the next one, ending where the one after starts (90)
        let g = edit_region(&r, "3-intro.tex", 45, Scope::Section { span: 2 }).unwrap();
        assert_eq!((g.start, g.end, g.state, g.next), (40, 90, 2, Some(4)));
        // at line 75 the start (70) has a footnote above it (goal < colroom): step back to 40
        let g = edit_region(&r, "3-intro.tex", 75, Scope::Section { span: 2 }).unwrap();
        assert_eq!((g.start, g.state), (40, 2));
        assert_eq!(g.end, u32::MAX, "two sections after 40: the end of the file");
        assert_eq!(g.next, Some(5), "then the document goes on in 5-meth.tex");
        // at 95 the start (90) has floats waiting: step back over it and 70 to 40
        assert_eq!(edit_region(&r, "3-intro.tex", 95, Scope::Section { span: 1 }).unwrap().state, 2);
        let g = edit_region(&r, "3-intro.tex", 45, Scope::Chapter).unwrap();
        assert_eq!((g.start, g.end, g.next), (10, u32::MAX, Some(5)));
        assert!(edit_region(&r, "9-none.tex", 1, Scope::Chapter).is_none());
        // the last section of the document
        assert_eq!(edit_region(&r, "5-meth.tex", 31, Scope::Section { span: 2 }).unwrap().next, None);
    }

    #[test]
    fn list_regions() {
        let main = "\\begin{document}\n\\input{2-exec}\n\\clearpage\n\n\\pdfbookmark[1]{Contents}{toc}\n\\begingroup\n\\hypersetup{linkcolor=black}\n\\tableofcontents\n\\endgroup\n\\clearpage\n\n\\input{3-intro}\n";
        let recs = vec![rec("main-fmt.tex", 8, 3, "total=0"), rec("3-intro.tex", 1, 5, "total=0")];
        let g = list_region(&recs, 0, main);
        assert_eq!((g.start, g.end, g.next, g.list), (5, 11, Some(1), true));
        assert_eq!(list_kind("\\printterms"), Some("terms"));
        assert_eq!(list_kind("Text."), None);
    }

    #[test]
    fn requests_and_results() {
        let r = doc();
        let g = edit_region(&r, "3-intro.tex", 45, Scope::Section { span: 2 }).unwrap();
        let q = request(&r, &[g]);
        assert!(q.starts_with("3-intro.tex 40 90 c:figure=1,c:section=1,colroom=46000000,"), "{q}");
        let fpl = "FPREGION 1 0\nFPWARN 1 floats were waiting\nFPEND 1 2 total=10000000;goal=46000000\nFPREGION 2 3\nFPEND 2 4 total=0\nFPDONE 5\n";
        let (t, done) = results(fpl);
        assert_eq!(done, Some(5));
        assert_eq!(t[0].first, 0);
        assert_eq!(t[0].end.as_ref().unwrap().0, 2);
        assert_eq!(t[0].warnings, ["floats were waiting"]);
        assert_eq!((t[1].first, t[1].end.as_ref().unwrap().0), (3, 4));
        assert_eq!(results("FPREGION 1 0\n").1, None, "a job that did not finish");
    }

    fn end_like(r: &Start) -> BTreeMap<String, String> {
        r.state.clone()
    }

    #[test]
    fn plans() {
        let r = doc();
        let g = edit_region(&r, "3-intro.tex", 45, Scope::Section { span: 2 }).unwrap(); // 40..90; record 4 is mid-page 7
        let top = Some((9000000 + 20000000 + 100000) as f64 / SP_PER_BP + CUT_MARGIN);
        let bottom = Some((9000000 + 10000000 + 100000) as f64 / SP_PER_BP + CUT_MARGIN);
        // the preview reached line 90 one page after its start, as the full build (page 6 -> 7), in the same state:
        // pages 6 and 7 (from 0: 5, 6) are replaced, the first joined below the cut, the last above it
        let t = vec![Typeset { first: 0, end: Some((1, end_like(&r[4]))), warnings: vec![], cands: vec![] }];
        let p = plan(&r, &[g.clone()], &t, 3, 12);
        assert_eq!(p.replace, [Replace { main: (5, 7), preview: (0, 2), top, bottom, shift: 0.0, tail: false, crop: None, top_page: None, foot: None }]);
        assert!(p.stale.is_empty() && !p.build_now);
        // a page more, the same state: the pages from the end are stale (their page numbers)
        let t = vec![Typeset { first: 0, end: Some((2, end_like(&r[4]))), warnings: vec![], cands: vec![] }];
        let p = plan(&r, &[g.clone()], &t, 3, 12);
        assert_eq!(p.replace, [Replace { main: (5, 6), preview: (0, 3), top, bottom: None, shift: 0.0, tail: true, crop: Some((bottom.unwrap(), (9000000 + 46000000) as f64 / SP_PER_BP)), top_page: None, foot: None }]);
        // the same, but the preview meets the full build again at a later start (a candidate): it ends there, exact
        let mut g2 = g.clone();
        g2.next = Some(5);
        let t = vec![Typeset { first: 0, end: Some((2, end_like(&r[5]))), warnings: vec![], cands: vec![(90, 2, end_like(&r[4]))] }];
        assert!(plan(&r, &[g2.clone()], &t, 4, 12).stale.is_empty(), "the second end (record 5) meets it");
        let t = vec![Typeset { first: 0, end: Some((2, end_like(&r[5]))), warnings: vec![], cands: vec![(90, 1, end_like(&r[4]))] }];
        assert_eq!(plan(&r, &[g2], &t, 4, 12).replace[0].main, (5, 7), "the first candidate (record 4) meets it: joined there");
        assert_eq!(p.stale, [((6, 12), STALE_PAGES.to_string())]);
        assert_eq!(shifted(&p), [((7, 14), STALE_PAGES.to_string())]); // 12 - 1 + 3 pages: stale to the end
        // a figure more: the numbering after it shifts; a full build at once
        let mut end = end_like(&r[4]);
        end.insert("c:figure".into(), "3".into());
        let t = vec![Typeset { first: 0, end: Some((1, end)), warnings: vec![], cands: vec![] }];
        let p = plan(&r, &[g.clone()], &t, 3, 12);
        assert_eq!(p.stale, [((6, 12), STALE_NUMBERS.to_string())]);
        assert!(p.build_now);
        // another position on the page: the layout after it
        let mut end = end_like(&r[4]);
        end.insert("total".into(), "11000000".into());
        let t = vec![Typeset { first: 0, end: Some((1, end)), warnings: vec![], cands: vec![] }];
        assert_eq!(plan(&r, &[g.clone()], &t, 3, 12).stale[0].1, STALE_LAYOUT);
        // ending at the top of a page (record 5, 5-meth.tex, total 0): no join at the end
        let g = edit_region(&r, "3-intro.tex", 45, Scope::Section { span: 9 }).unwrap();
        assert_eq!(g.next, Some(5));
        let t = vec![Typeset { first: 0, end: Some((2, end_like(&r[5]))), warnings: vec![], cands: vec![] }];
        assert_eq!(plan(&r, &[g], &t, 3, 12).replace, [Replace { main: (5, 7), preview: (0, 2), top, bottom: None, shift: 0.0, tail: false, crop: None, top_page: None, foot: None }]);
        // the last region of the document: to the preview's last page, from a fresh page (no cut)
        let g = edit_region(&r, "5-meth.tex", 6, Scope::Section { span: 2 }).unwrap();
        let t = vec![Typeset { first: 0, end: Some((3, BTreeMap::new())), warnings: vec![], cands: vec![] }];
        let p = plan(&r, &[g], &t, 4, 12);
        assert_eq!(p.replace, [Replace { main: (7, 12), preview: (0, 4), top: None, bottom: None, shift: 0.0, tail: false, crop: None, top_page: None, foot: None }]);
        // an edit before a file's first section cannot be previewed
        assert!(edit_region(&r, "3-intro.tex", 3, Scope::Section { span: 2 }).is_none());
    }
}
