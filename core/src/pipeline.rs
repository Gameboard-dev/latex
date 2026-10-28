//! What the watcher decides without touching Obsidian or the disk, so it can be tested natively: which .tex a note
//! becomes, when its last-updated property is stamped, the compile command's steps, which tool latexmk is running (from
//! its output, line by line), and the one-line summary of a build (latex_sync.py's log_summary).

pub mod fast;
pub mod keys;

use std::sync::LazyLock;

use regex_lite::Regex;

use crate::convert::{prop, rx, split_front, unquote};

rx!(RUN_NUMBER, r"^Run number (\d+) of rule '([^']+)'");
rx!(RUNNING, r"^Running '[ \t]*([^ \t']+)");
rx!(LOG_ERROR, r"(?m)^(?:\S+\.tex:\d+: .*|! .*)$");
rx!(UNDEFINED, r"(?:Reference|Citation) [`']([^']+)' (?:on page \d+ )?undefined");
rx!(TOOL_ERROR, r"\bERROR\b|^Latexmk: .*(?:[Ee]rror|failed)|did not complete");

/// The .tex a note becomes: the note's own name, so that renaming a note renames its chapter and nothing else (no
/// name map to keep in step, no `tex` property to override it).
pub fn tex_name(note: &str) -> String {
    format!("{}.tex", note.strip_suffix(".md").unwrap_or(note))
}

/// What a chapter left out of the PDF (the setting Chapters to compile) starts with: \endinput ends the file at the end
/// of this line, whose rest is a comment, so the main file's \input of it reads nothing. The chapter is still written
/// in full beneath it, and on the same lines, so its word counts and the note's line map are the same.
pub const LEFT_OUT: &str = "\\endinput% left out of the PDF (LaTeX plugin settings: Chapters to compile). ";

pub fn leave_out(tex: &str) -> String {
    format!("{LEFT_OUT}{tex}")
}

/// Whether a note's sections are counted: its word-count property (`key`) is true, as the converter reads it.
pub fn counted(md: &str, key: &str) -> bool {
    let props = split_front(&md.replace("\r\n", "\n")).map(|(p, _)| p).unwrap_or_default();
    prop(&props, key).and_then(|v| unquote(v).ok()).is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

/// A note's text without its properties: a change here is a change to the chapter's text.
pub fn body(md: &str) -> String {
    let md = md.replace("\r\n", "\n");
    match split_front(&md) {
        Ok((_, lines)) => lines.join("\n"),
        Err(_) => md,
    }
}

/// The note with the property `key` set to `when`, if its properties have that line and it says something else
/// (latex_sync.py replaces the first such line anywhere in the note; here only within the properties).
pub fn stamp(md: &str, key: &str, when: &str) -> Option<String> {
    let lines: Vec<&str> = md.split('\n').collect();
    if lines.first().map(|l| l.trim_end_matches('\r')) != Some("---") {
        return None;
    }
    let end = lines.iter().skip(1).position(|l| l.trim_end_matches('\r') == "---")? + 1;
    let k = (1..end).find(|&k| lines[k].strip_prefix(key).is_some_and(|r| r.starts_with(':')))?;
    let cr = if lines[k].ends_with('\r') { "\r" } else { "" };
    let new = format!("{key}: {when}{cr}");
    if lines[k] == new {
        return None;
    }
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    out[k] = new;
    Some(out.join("\n"))
}

/// latexmk with LuaLaTeX, biber and makeglossaries: the LaTeX Workshop recipe latex_sync.py ran (the tool "latexmk" in
/// .vscode/settings.json), as one command line; `default_command("lualatex")`.
pub const DEFAULT_COMMAND: &str = "latexmk -lualatex -synctex=1 -interaction=nonstopmode -file-line-error -e \"sub run_mg { return system('makeglossaries', $_[0]); }\" -e \"add_cus_dep('glo', 'gls', 0, 'run_mg');\" -e \"add_cus_dep('acn', 'acr', 0, 'run_mg');\" %DOC%";

/// The engines the Engine setting offers: its value, the name shown, and latexmk's option for it.
pub const TEX_ENGINES: [(&str, &str, &str); 3] = [("lualatex", "LuaLaTeX", "-lualatex"), ("pdflatex", "pdfLaTeX", "-pdf"), ("xelatex", "XeLaTeX", "-xelatex")];

/// The default compile command for an engine: latexmk with it, biber (as the document's biblatex asks) and
/// makeglossaries. An unknown engine is LuaLaTeX.
pub fn default_command(engine: &str) -> String {
    let flag = TEX_ENGINES.iter().find(|(e, _, _)| *e == engine).map_or("-lualatex", |(_, _, f)| f);
    DEFAULT_COMMAND.replacen("-lualatex", flag, 1)
}

/// Whether a command is one of the engines' defaults (so not this device's own).
pub fn is_default_command(command: &str) -> bool {
    TEX_ENGINES.iter().any(|(e, _, _)| default_command(e) == command.trim())
}

/// The compile command: one command per line, each split as a POSIX shell would (quotes, no expansion), and the
/// placeholders LaTeX Workshop's recipes use filled in within each argument: %DOC% (the main file without .tex),
/// %DOCFILE%, %DOC_EXT%, %DOCFILE_EXT%, %DIR%, %OUTDIR%, %TMPDIR%.
pub fn steps(command: &str, dir: &str, main: &str, sep: &str) -> Result<Vec<Vec<String>>, String> {
    let doc = format!("{}{sep}{main}", dir.trim_end_matches(['/', '\\']));
    let subs = [
        ("%DOC_EXT%", format!("{doc}.tex")),
        ("%DOCFILE_EXT%", format!("{main}.tex")),
        ("%DOCFILE%", main.to_string()),
        ("%DOC%", doc.clone()),
        ("%OUTDIR%", dir.to_string()),
        ("%TMPDIR%", dir.to_string()),
        ("%DIR%", dir.to_string()),
    ];
    let mut out = Vec::new();
    for line in command.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let words = shlex::split(line).ok_or_else(|| format!("the compile command {line:?} has an unclosed quote"))?;
        out.push(words.into_iter().map(|w| subs.iter().fold(w, |w, (k, v)| w.replace(k, v))).collect());
    }
    if out.is_empty() {
        return Err("the compile command is empty".into());
    }
    Ok(out)
}

/// The program of a step, as the status bar names it: `lualatex`, not `C:\texlive\bin\lualatex.exe`.
pub fn program(cmd: &str) -> String {
    let name = cmd.rsplit(['/', '\\']).next().unwrap_or(cmd);
    name.strip_suffix(".exe").unwrap_or(name).to_string()
}

/// Which tool a build is running, from its output: latexmk announces each rule ("Run number 2 of rule 'lualatex'",
/// "Run number 1 of rule 'biber main'", "Run number 1 of rule 'cusdep glo gls main'") and a custom dependency's tool
/// prints its own banner ("makeglossaries version 5.1").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stage {
    /// What is running now: "lualatex (pass 2)", "biber", "makeglossaries" ...
    pub now: String,
    partial: String,
}

const ENGINES: [&str; 5] = ["lualatex", "pdflatex", "xelatex", "latex", "platex"];

impl Stage {
    /// A step of the command starts (before latexmk says anything): the step's own program.
    pub fn start(&mut self, program: &str, step: usize, steps: usize) {
        self.now = if steps > 1 { format!("{program} (step {step} of {steps})") } else { program.to_string() };
        self.partial.clear();
    }

    /// Output as it streams (in pieces that need not end at a line): true when the stage changed.
    pub fn feed(&mut self, chunk: &str) -> bool {
        self.partial.push_str(chunk);
        let mut changed = false;
        while let Some(i) = self.partial.find('\n') {
            let line: String = self.partial.drain(..=i).collect();
            if let Some(s) = Self::stage_of(line.trim_end()) {
                changed |= s != self.now;
                self.now = s;
            }
        }
        changed
    }

    fn stage_of(line: &str) -> Option<String> {
        if let Some(m) = RUN_NUMBER.captures(line) {
            let rule = &m[2];
            let first = rule.split_whitespace().next().unwrap_or(rule);
            if ENGINES.contains(&first) {
                return Some(format!("{first} (pass {})", &m[1]));
            }
            if first == "cusdep" {
                let p: Vec<&str> = rule.split_whitespace().collect();
                return Some(match p.as_slice() {
                    [_, from, to, ..] => format!("custom dependency {from} → {to}"),
                    _ => "custom dependency".into(),
                });
            }
            return Some(first.to_string());
        }
        if line.starts_with("makeglossaries version") || line.starts_with("This is makeglossaries") {
            return Some("makeglossaries".into());
        }
        if let Some(m) = RUNNING.captures(line) {
            let p = program(m[1].trim_matches('"'));
            if p == "makeglossaries" || p == "makeindex" || p == "xindy" {
                return Some(p);
            }
        }
        None
    }
}

/// A finished build: what the log and the status bar say.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub ok: bool,
    pub errors: usize,
    pub undefined: usize,
    /// The log entry, as latex_sync.py writes it.
    pub text: String,
}

/// latex_sync.py's log_summary: LaTeX's errors (`file.tex:12: ...`, `! ...`) and undefined references from main.log,
/// the tools' errors from the build's own output. `failed`: the program that stopped with `code`, if one did; `place`
/// is where the logs are, for the reader.
#[allow(clippy::too_many_arguments)] // the log line's parts, each from somewhere else
pub fn summary(seconds: f64, code: i64, failed: &str, main_log: &str, build_log: &str, place: &str, build_log_name: &str, main: &str) -> Summary {
    let main_log = main_log.replace("\r\n", "\n");
    let mut errors: Vec<String> = LOG_ERROR.find_iter(&main_log).map(|m| m.as_str().to_string()).collect();
    let mut undef: Vec<String> = UNDEFINED.captures_iter(&main_log).map(|c| c[1].to_string()).collect();
    undef.sort();
    undef.dedup();
    errors.extend(build_log.lines().filter(|l| TOOL_ERROR.is_match(l)).map(|l| l.trim().to_string()));
    let outcome = if code == 0 { "ok".to_string() } else { format!("{failed} exit code {code}") };
    let mut lines = vec![format!("Compiled in {seconds:.0} s: {outcome}; {} errors (details: {place}/{build_log_name} and {main}.log).", errors.len())];
    lines.extend(errors.iter().take(15).map(|e| format!("  error: {}", e.trim())));
    if !undef.is_empty() {
        lines.push(format!("  undefined references or citations: {}", undef.join(", ")));
    }
    Summary { ok: code == 0, errors: errors.len(), undefined: undef.len(), text: lines.join("\n") }
}

/// Files a run stopped part-way may leave half-written (a truncated .bcf makes biber fail, a truncated .aux makes LaTeX
/// fail): deleted after a cancellation, so latexmk makes them again. The PDF is kept for the viewer.
/// Whether an .aux file was cut off while being written: it does not end its last line, or a line's braces do not
/// balance (TeX writes each entry whole, on one line).
pub fn aux_truncated(aux: &str) -> bool {
    if aux.is_empty() {
        return false;
    }
    if !aux.ends_with('\n') {
        return true;
    }
    aux.lines().any(|l| {
        let (mut depth, mut escaped) = (0i32, false);
        for c in l.chars() {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '{') => depth += 1,
                (false, '}') => depth -= 1,
                _ => {}
            }
        }
        depth != 0
    })
}

#[cfg(test)]
mod aux_tests {
    #[test]
    fn cut_off() {
        assert!(!super::aux_truncated("\\relax\n\\abx@aux@cite{0}{ramoly2024}\n"));
        assert!(super::aux_truncated("\\relax\n\\abx@aux@cite{0}{ram"));
        assert!(super::aux_truncated("\\abx@aux@cite{0}{ram\n"));
        assert!(!super::aux_truncated("\\gdef \\@abspage@last{51}\n\\newlabel{x}{{1}{2}}\n"));
    }
}

pub const SCRATCH_EXTS: [&str; 18] = [
    ".aux", ".bcf", ".bbl", ".blg", ".glsdefs", ".glo", ".gls", ".glg", ".ist", ".toc", ".out", ".run.xml", ".fdb_latexmk", ".fls", ".synctex(busy)", ".bbl-SAVE-ERROR", ".bcf-SAVE-ERROR", ".aux-SAVE-ERROR",
];

/// The log's lines to keep when it has grown past twice that (latex_sync.py's trim_log).
pub const LOG_KEEP: usize = 2000;

pub fn trimmed(log: &str) -> Option<String> {
    let lines: Vec<&str> = log.split_inclusive('\n').collect();
    (lines.len() > 2 * LOG_KEEP).then(|| lines[lines.len() - LOG_KEEP..].concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tex_names() {
        assert_eq!(tex_name("10-appendix.md"), "10-appendix.tex");
        assert_eq!(tex_name("3-intro"), "3-intro.tex");
    }

    #[test]
    fn left_out() {
        let tex = "% Generated from 5-methods.md\n% !TEX root = main.tex\n\\section{Methods}\n";
        let out = leave_out(tex);
        assert_eq!(out.lines().count(), tex.lines().count());
        assert!(out.starts_with("\\endinput%") && out.ends_with(tex.split_once('\n').unwrap().1));
        assert_eq!(keys::read_part(&out), out.split_inclusive('\n').next().unwrap());
    }

    #[test]
    fn stamping() {
        let md = "---\ntex: a.tex\nlast_updated: 2026-10-01 10:00\n---\nlast_updated: body\n";
        assert_eq!(stamp(md, "last_updated", "2026-10-25 12:00").unwrap(), "---\ntex: a.tex\nlast_updated: 2026-10-25 12:00\n---\nlast_updated: body\n");
        assert_eq!(stamp(md, "last_updated", "2026-10-01 10:00"), None);
        assert_eq!(stamp("---\ntex: a\n---\nText\n", "last_updated", "x"), None);
        assert_eq!(body(md), "last_updated: body");
        assert!(counted("---\nwordcount: true\n---\n# A\n", "wordcount") && counted("---\r\nwordcount: \"True\"\r\n---\r\n", "wordcount"));
        assert!(!counted("---\nwordcount: false\n---\n", "wordcount") && !counted("# No properties\n", "wordcount"));
    }

    #[test]
    fn compile_steps() {
        let cmd = "latexmk -lualatex -e \"sub run_mg { return system('makeglossaries', $_[0]); }\" %DOC%\n\n# a comment\nbiber %DOCFILE%";
        let s = steps(cmd, "C:\\My Thesis\\out", "main", "\\").unwrap();
        assert_eq!(s[0], ["latexmk", "-lualatex", "-e", "sub run_mg { return system('makeglossaries', $_[0]); }", "C:\\My Thesis\\out\\main"]);
        assert_eq!(s[1], ["biber", "main"]);
        assert!(steps("latexmk \"open", "/x", "main", "/").is_err() && steps("  ", "/x", "main", "/").is_err());
        assert_eq!(program("C:\\texlive\\bin\\windows\\latexmk.exe"), "latexmk");
        // the default is the arguments of the recipe latex_sync.py ran, %DOC% being the main file's path
        let recipe = [
            "latexmk", "-lualatex", "-synctex=1", "-interaction=nonstopmode", "-file-line-error",
            "-e", "sub run_mg { return system('makeglossaries', $_[0]); }",
            "-e", "add_cus_dep('glo', 'gls', 0, 'run_mg');",
            "-e", "add_cus_dep('acn', 'acr', 0, 'run_mg');",
            "C:\\Vault\\LaTeX\\compiled\\main",
        ];
        assert_eq!(steps(DEFAULT_COMMAND, "C:\\Vault\\LaTeX\\compiled", "main", "\\").unwrap(), [recipe]);
    }

    #[test]
    fn stages() {
        let mut s = Stage::default();
        s.start("latexmk", 1, 1);
        assert_eq!(s.now, "latexmk");
        assert!(!s.feed("Latexmk: applying rule 'lualatex'...\nRun number 1 of rule 'lual"));
        assert_eq!(s.now, "latexmk"); // the line is not complete yet
        assert!(s.feed("atex'\n"));
        assert_eq!(s.now, "lualatex (pass 1)");
        s.feed("Run number 1 of rule 'cusdep glo gls main'\n");
        assert_eq!(s.now, "custom dependency glo → gls");
        s.feed("\nmakeglossaries version 5.1 (2026-04-21)\n");
        assert_eq!(s.now, "makeglossaries");
        s.feed("Run number 1 of rule 'biber main'\nRunning 'biber  \"main.bcf\"'\n");
        assert_eq!(s.now, "biber");
        s.feed("Run number 2 of rule 'lualatex'\r\n");
        assert_eq!(s.now, "lualatex (pass 2)");
        s.start("biber", 2, 3);
        assert_eq!(s.now, "biber (step 2 of 3)");
    }

    #[test]
    fn summaries() {
        let log = "x\n./3-intro.tex:12: Undefined control sequence.\n! Emergency stop.\nLaTeX Warning: Reference `sec:x' on page 3 undefined\nLaTeX Warning: Citation 'a2020' undefined\nLaTeX Warning: Reference `sec:x' on page 4 undefined\n";
        let s = summary(12.4, 0, "latexmk", log, "Latexmk: Errors, so I did not complete making targets\n", "Vault/LaTeX/compiled", ".latex-sync-build.log", "main");
        assert_eq!(s.text, "Compiled in 12 s: ok; 3 errors (details: Vault/LaTeX/compiled/.latex-sync-build.log and main.log).\n  error: ./3-intro.tex:12: Undefined control sequence.\n  error: ! Emergency stop.\n  error: Latexmk: Errors, so I did not complete making targets\n  undefined references or citations: a2020, sec:x");
        assert_eq!((s.errors, s.undefined, s.ok), (3, 2, true));
        assert!(summary(1.0, 12, "latexmk", "", "", "p", "b", "main").text.starts_with("Compiled in 1 s: latexmk exit code 12; 0 errors"));
    }

    #[test]
    fn trimming() {
        let log: String = (0..4001).map(|k| format!("{k}\n")).collect();
        let t = trimmed(&log).unwrap();
        assert!(t.starts_with("2001\n") && t.ends_with("4000\n"));
        assert!(trimmed("a\nb\n").is_none());
    }
}
