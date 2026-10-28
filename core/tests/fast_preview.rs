//! The fast preview's correctness suite, against real TeX: each scenario edits the fixture's notes (tests/fast/fixture,
//! made by make_fixture.py), lets the plugin's steps (latex_core::preview, on std's files and processes) preview the edit,
//! and compares the PDF shown with a clean full build of the same sources (latexmk with the plugin's default command,
//! from scratch, in a fresh copy): tests/fast/check.py checks the text of every page that claims to be current, every
//! citation number, the bibliography, the contents and lists, the Terms list, every link's destination and the name
//! tree; this file checks SyncTeX (20 sampled lines). Then the background full build lands and everything must match.
//!
//! FAST_TEX=<scratch folder> cargo test --release -p latex-core --test fast_preview -- --nocapture --test-threads=1
//! (FAST_ONLY=<name,...> runs some scenarios.) Skipped without FAST_TEX. Needs LuaLaTeX, latexmk, biber, makeglossaries,
//! and Python with PyMuPDF. It writes <scratch>/report.md: a table of the scenarios and their checks.

#[path = "fast/host.rs"]
mod host;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use host::{block_on, convert_all, copy_dir, full_build, Native};
use latex_core::pipeline::fast::Scope;
use latex_core::preview::{self, Edit, Host, Options, Outcome};
use latex_core::splice;
use latex_core::synctex::Synctex;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fast/fixture")
}

fn options() -> Options {
    Options { main: "main".into(), preamble: "0-preamble".into(), term: "term".into(), glossary: "glossary".into(), scope: Scope::Section { span: 2 }, cache_tikz: true, debug: false }
}

/// One save: a note and what it becomes.
type Save = (&'static str, fn(&str) -> String);

struct Scenario {
    name: &'static str,
    what: &'static str,
    saves: Vec<Save>,
    /// Only a full build follows (the preamble): no preview.
    build_only: bool,
}

fn after(s: &str, at: &str, add: &str) -> String {
    let i = s.find(at).unwrap_or_else(|| panic!("{at:?} not in the note")) + at.len();
    format!("{}{add}{}", &s[..i], &s[i..])
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "01-prose", what: "a prose edit in an early section", saves: vec![("3-introduction.md", |s| s.replacen("The local layer matches", "The local layer, edited, matches", 1))], build_only: false },
        Scenario { name: "02-cite-lowest", what: "a new citation with the lowest number", saves: vec![("3-introduction.md", |s| s.replacen(" [@src01]", " [@new01] [@src01]", 1))], build_only: false },
        Scenario { name: "03-cite-last", what: "a new citation cited last", saves: vec![("9-appendix.md", |s| after(s, "^app--procedures\n\n", "A later source agrees [@new02]. "))], build_only: false },
        Scenario { name: "04-cite-removed", what: "removing a source's only citation", saves: vec![("3-introduction.md", |s| s.replacen(" [@src05]", "", 1))], build_only: false },
        Scenario { name: "05-two-cites", what: "two new citations in one save", saves: vec![("4-review.md", |s| after(s, "^sec--lit--aggregation\n\n", "Two new sources [@new01; @new02]. "))], build_only: false },
        Scenario {
            name: "06-cite-moved",
            what: "a citation moved earlier (first-cite order)",
            saves: vec![("4-review.md", |s| s.replacen(" [@src10]", "", 1)), ("3-introduction.md", |s| s.replacen(" [@src05]", " [@src05; @src10]", 1))],
            build_only: false,
        },
        Scenario {
            name: "07-refs",
            what: "a new label in the appendix referenced from the body; a new reference to an appendix item",
            saves: vec![
                ("9-appendix.md", |s| after(s, "This is item B. ^app--item-b\n", "\nThis is item C, added. ^app--item-c\n")),
                ("5-method.md", |s| after(s, "^sec--method--data\n\n", "Item [[9-appendix#^app--item-c|app:item-c]] is new, and item [[9-appendix#^app--item-b|app:item-b]] is old. ")),
            ],
            build_only: false,
        },
        Scenario {
            name: "08-heading-caption",
            what: "a renamed heading (contents) and a changed caption (list of figures)",
            saves: vec![("4-review.md", |s| s.replacen("## Estimators\n", "## Estimator Families\n", 1)), ("5-method.md", |s| s.replacen("The measures and their thresholds.", "The measures, their thresholds and their units.", 1))],
            build_only: false,
        },
        Scenario { name: "09a-gloss-description", what: "a glossary description", saves: vec![("glossary.md", |s| s.replacen("The share of intrusion attempts accepted as known good.", "The proportion of intrusion attempts wrongly accepted.", 1))], build_only: false },
        Scenario { name: "09b-gloss-printed", what: "a glossary entry's printed form", saves: vec![("glossary.md", |s| s.replacen("# NIT\n", "# NIT\n**Printed as:** P.A.I.\n", 1))], build_only: false },
        Scenario {
            name: "09c-gloss-first-use",
            what: "a term's first use",
            saves: vec![("glossary.md", |s| format!("{s}\n# FAR\n**Long form:** False Acceptance Rate\n\nThe share of impostors accepted.\n")), ("3-introduction.md", |s| s.replacen("The local layer matches", "The [[glossary#FAR|FAR]] of the local layer matches", 1))],
            build_only: false,
        },
        Scenario { name: "09d-gloss-last-use", what: "a term's last use removed", saves: vec![("3-introduction.md", |s| s.replacen("Does a [[glossary#NIT|NIT]] change", "Does an instrument change", 1))], build_only: false },
        Scenario {
            name: "10-page-count",
            what: "an edit that changes a section's page count",
            saves: vec![("3-introduction.md", |s| after(s, "^sec--intro--background\n\n", &"This paragraph was added to make the section longer than it was, so that the pages after it move. ".repeat(40)))],
            build_only: false,
        },
        Scenario { name: "11-float-boundary", what: "an edit next to a float at a section boundary", saves: vec![("4-review.md", |s| s.replacen("((<tag|Figure)) Where the gap lies.", "((<tag|Figure)) Where the gap lies.\n\nA sentence added after the figure, at the end of the section.", 1))], build_only: false },
        Scenario {
            name: "14-line-over-break",
            what: "an edit that pushes a line onto the next page, in a section ending mid-page with the next one on the same page",
            saves: vec![("3-introduction.md", |s| after(s, "^sec--intro--background

", "These added words make the paragraph one or two lines longer, so that a line moves over the page break. "))],
            build_only: false,
        },
        Scenario { name: "13a-preamble", what: "a preamble change (the format is rebuilt)", saves: vec![("0-preamble.md", |s| s.replacen("{6pt plus 2pt minus 1pt}", "{8pt plus 2pt minus 1pt}", 1))], build_only: true },
        Scenario { name: "13b-format-fallback", what: "the format fails (OpenType fonts): builds and previews run without it", saves: vec![("0-preamble.md", |s| s.replacen("\\usepackage[T1]{fontenc}\n\\usepackage{lmodern}\n", "", 1))], build_only: true },
    ]
}

fn run(dir: &Path, cmd: &[&str]) -> (i32, String) {
    let mut c = Command::new(cmd[0]);
    c.args(&cmd[1..]).current_dir(dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0000_4000 | 0x0800_0000);
    }
    let o = c.output().unwrap_or_else(|e| panic!("{}: {e}", cmd[0]));
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr))
}

/// The oracle: the notes converted in a fresh copy, built from scratch with the plugin's default command on main.tex.
fn oracle(notes: &Path, at: &Path) -> PathBuf {
    _ = std::fs::remove_dir_all(at);
    std::fs::create_dir_all(at.join("compiled")).unwrap();
    for e in std::fs::read_dir(notes).unwrap().filter_map(|e| e.ok()) {
        let n = e.file_name().to_string_lossy().to_string();
        if n.ends_with(".md") {
            std::fs::copy(e.path(), at.join(&n)).unwrap();
        }
    }
    for f in ["main.tex", "fixture.bib"] {
        std::fs::copy(notes.join("compiled").join(f), at.join("compiled").join(f)).unwrap();
    }
    convert_all(at);
    let steps = latex_core::pipeline::steps(latex_core::pipeline::DEFAULT_COMMAND, &at.join("compiled").to_string_lossy(), "main", "\\").unwrap();
    for s in steps {
        let a: Vec<&str> = s.iter().map(String::as_str).collect();
        let (code, out) = run(&at.join("compiled"), &a);
        assert!(code == 0 || at.join("compiled/main.pdf").exists(), "oracle build failed: {}", out.lines().rev().take(10).collect::<Vec<_>>().join("\n"));
    }
    at.join("compiled")
}

/// check.py on the shown PDF: each check's (pass, detail).
fn check(shown: &Path, oracle_pdf: &Path, stale: &[((u32, u32), String)], landed: bool, out: &Path) -> BTreeMap<String, (bool, String)> {
    let manifest = serde_json::json!({"landed": landed, "stale": stale.iter().map(|((a, b), w)| serde_json::json!([a, b, w])).collect::<Vec<_>>()});
    let mpath = out.with_extension("manifest.json");
    std::fs::write(&mpath, manifest.to_string()).unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fast/check.py");
    let (code, text) = run(Path::new("."), &["python", "-I", &script.to_string_lossy(), &shown.to_string_lossy(), &oracle_pdf.to_string_lossy(), &mpath.to_string_lossy(), &out.to_string_lossy()]);
    assert_eq!(code, 0, "check.py: {text}");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(out).unwrap()).unwrap();
    v.as_object().unwrap().iter().map(|(k, r)| (k.clone(), (r["pass"].as_bool().unwrap(), r["detail"].as_str().unwrap_or("").to_string()))).collect()
}

fn synctex(path: &Path, keep: &[String]) -> Option<Synctex> {
    let b = std::fs::read(path).ok()?;
    let text = String::from_utf8(latex_core::pdf::gunzip(&b)?).ok()?;
    Synctex::new(&text, |f| keep.iter().any(|k| k == f)).ok()
}

/// SyncTeX: for 20 lines (10 of the edited chapter, 10 of the others), the page the shown PDF's index gives against
/// the clean build's; lines on stale pages are left out.
fn check_synctex(dir: &Path, oracle_dir: &Path, out: &Outcome, edited: &str, chapters: &[String]) -> (bool, String) {
    let (Some(main), Some(prev), Some(orc)) = (synctex(&dir.join("fp-full.synctex.gz"), chapters), synctex(&dir.join("fp-preview.synctex.gz"), chapters), synctex(&oracle_dir.join("main.synctex.gz"), chapters)) else {
        return (false, "a SyncTeX file could not be read".into());
    };
    let merged = Synctex::spliced(&main, &prev, &splice::bands(&out.pages));
    let stale = |p: u32| out.stale.iter().any(|((a, b), _)| p > *a && p <= *b);
    let mut sample: Vec<(String, u32)> = Vec::new();
    for (file, n) in chapters.iter().map(|c| (c.clone(), if c == edited { 10 } else { 10 / (chapters.len() - 1).max(1) })) {
        let Some(f) = orc.file(&file) else { continue };
        let lines = orc.typeset_lines(f);
        let step = (lines.len() / n.max(1)).max(1);
        sample.extend(lines.iter().step_by(step).take(n).map(|l| (file.clone(), *l)));
    }
    let mut bad = Vec::new();
    let mut checked = 0;
    for (file, line) in &sample {
        let (Some(fo), Some(fm)) = (orc.file(file), merged.file(file)) else { continue };
        let (Some(po), Some(pm)) = (orc.line_pos(fo, *line, |_| false), merged.line_pos(fm, *line, |_| false)) else {
            bad.push(format!("{file}:{line} not in the index"));
            continue;
        };
        if stale(pm.page) {
            continue;
        }
        checked += 1;
        if po.page != pm.page {
            bad.push(format!("{file}:{line} page {} (full build {})", pm.page, po.page));
        }
    }
    (bad.is_empty(), format!("{checked} lines checked; {}", if bad.is_empty() { "all on the full build's page".to_string() } else { bad.join(", ") }))
}

struct Row {
    name: String,
    what: String,
    preview: Option<(f64, Vec<String>, usize)>,
    at_preview: BTreeMap<String, (bool, String)>,
    after_landing: BTreeMap<String, (bool, String)>,
    notes: Vec<String>,
}

#[test]
#[allow(unused_variables)]
fn correctness_suite() {
    let Ok(scratch) = std::env::var("FAST_TEX") else { return };
    let scratch = PathBuf::from(scratch);
    let only: Vec<String> = std::env::var("FAST_ONLY").map(|v| v.split(',').map(str::to_string).collect()).unwrap_or_default();
    let o = options();
    // the base: the fixture converted and built once, as after the plugin's first full build
    let base = scratch.join("base");
    if !base.join("compiled/fp-full.pdf").exists() {
        _ = std::fs::remove_dir_all(&base);
        copy_dir(&fixture(), &base);
        convert_all(&base);
        let h = Native::new(&base.join("compiled"));
        let s = full_build(&h, &o, false).expect("the base build");
        h.stop_worker();
        eprintln!("base built in {s:.1} s");
    }
    let chapters: Vec<String> = ["2-summary.tex", "3-introduction.tex", "4-review.tex", "5-method.tex", "9-appendix.tex"].iter().map(|s| s.to_string()).collect();
    let mut rows = Vec::new();
    for sc in scenarios().into_iter().filter(|s| only.is_empty() || only.iter().any(|o| s.name.starts_with(o.as_str()))) {
        eprintln!("--- {}: {}", sc.name, sc.what);
        let dir = scratch.join(sc.name);
        _ = std::fs::remove_dir_all(&dir);
        copy_dir(&base, &dir);
        let h = Native::new(&dir.join("compiled"));
        let mut row = Row { name: sc.name.into(), what: sc.what.into(), preview: None, at_preview: BTreeMap::new(), after_landing: BTreeMap::new(), notes: Vec::new() };
        let mut last: Option<(Outcome, String)> = None;
        let mut build_now = false;
        let fmt = h.text("fp-preamble.key").len() == 16;
        // the chapters as the last full build had them: every preview typesets all changes since
        let at_build: BTreeMap<String, String> = chapters.iter().chain(["glossary.tex".to_string()].iter()).map(|c| (c.clone(), h.text(c))).collect();
        for (note, f) in &sc.saves {
            let md = std::fs::read_to_string(dir.join(note)).unwrap();
            std::fs::write(dir.join(note), f(&md)).unwrap();
            let texs = convert_all(&dir);
            if sc.build_only {
                continue;
            }
            let tex = latex_core::pipeline::tex_name(note);
            let glossary = (texs["glossary.tex"] != at_build["glossary.tex"]).then(|| (at_build["glossary.tex"].clone(), texs["glossary.tex"].clone()));
            let edits: Vec<Edit> = chapters.iter().filter(|c| texs[*c] != at_build[*c]).map(|c| Edit { tex: c.clone(), old: at_build[c].clone(), new: texs[c].clone() }).collect();
            match block_on(preview::preview(&h, &o, fmt)) {
                Ok(out) => {
                    eprintln!("  preview {:.2} s: {:?}; stale {:?}; problems {:?}", out.ms / 1000.0, out.steps, out.stale, out.problems);
                    build_now |= out.build_now;
                    row.notes.extend(out.problems.iter().cloned());
                    _ = std::fs::copy(dir.join("compiled/fp-preview.pdf"), dir.join("preview.pdf"));
                    for ext in ["log", "fpl", "aux"] {
                        _ = std::fs::copy(dir.join(format!("compiled/fp-preview.{ext}")), dir.join(format!("preview.{ext}")));
                    }
                    last = Some((out, tex));
                }
                Err(e) => {
                    eprintln!("  not previewed: {e}");
                    row.notes.push(format!("not previewed: {e}"));
                }
            }
        }
        if !sc.build_only && last.is_none() {
            row.at_preview.insert("previewed".into(), (false, "the edit was not previewed".into()));
        }
        row.notes.extend(h.log.borrow().iter().cloned());
        let orc = oracle(&dir, &scratch.join(format!("{}-oracle", sc.name)));
        if let Some((out, tex)) = &last {
            let shown = dir.join("shown.pdf");
            std::fs::copy(dir.join("compiled/main.pdf"), &shown).unwrap();
            row.at_preview = check(&shown, &orc.join("main.pdf"), &out.stale, false, &dir.join("at-preview.json"));
            row.at_preview.insert("SyncTeX: line to page".into(), check_synctex(&dir.join("compiled"), &orc, out, tex, &chapters));
            row.preview = Some((out.ms, out.steps.clone(), out.stale.len()));
        }
        // the background build lands: only the steps the change needs (preview::Plan)
        h.stop_worker();
        match full_build(&h, &o, false) {
            Ok(s) => row.notes.push(format!("full build {s:.1} s{}", if build_now { " (started at once)" } else { "" })),
            Err(e) => row.notes.push(format!("full build failed: {e}")),
        }
        row.notes.extend(h.log.borrow().iter().filter(|l| !row.notes.contains(l)).cloned().collect::<Vec<_>>());
        h.stop_worker();
        row.after_landing = check(&dir.join("compiled/main.pdf"), &orc.join("main.pdf"), &[], true, &dir.join("after-landing.json"));
        if sc.name.starts_with("13b") {
            // a preview after the fallback: the worker starts without the format
            let md = std::fs::read_to_string(dir.join("3-introduction.md")).unwrap();
            let old = h.text("3-introduction.tex");
            std::fs::write(dir.join("3-introduction.md"), md.replacen("The local layer matches", "The local layer, edited, matches", 1)).unwrap();
            let texs = convert_all(&dir);
            let edit = Edit { tex: "3-introduction.tex".into(), old, new: texs["3-introduction.tex"].clone() };
            let r = block_on(preview::preview(&h, &o, false));
            row.notes.push(match r {
                Ok(out) => format!("a preview after the fallback: {:.2} s, {} stale ranges", out.ms / 1000.0, out.stale.len()),
                Err(e) => format!("a preview after the fallback failed: {e}"),
            });
            h.stop_worker();
        }
        for (k, (ok, d)) in row.at_preview.iter().chain(row.after_landing.iter()) {
            eprintln!("  {} {k}: {d}", if *ok { "ok  " } else { "FAIL" });
        }
        rows.push(row);
    }
    // the report
    let mut md = String::from("# Fast preview: correctness suite\n\nEach scenario's preview (the PDF shown right after the save) and the PDF after the background full build lands, against a clean full build of the same sources. A stale page is one the plugin marks as waiting for the full build (on the page and in the status bar); checks on it are left out at preview time, which is expected, and all must pass once the build lands.\n\n| Scenario | Preview | At preview | After the full build | Pass only after landing |\n|---|---|---|---|---|\n");
    let mut failed = Vec::new();
    for r in &rows {
        let fails = |m: &BTreeMap<String, (bool, String)>| m.iter().filter(|(_, (ok, _))| !ok).map(|(k, _)| k.clone()).collect::<Vec<_>>();
        let (fp, fl) = (fails(&r.at_preview), fails(&r.after_landing));
        let stale_note = r.preview.as_ref().map_or("no preview".to_string(), |(ms, _, n)| format!("{:.2} s, {n} stale range{}", ms / 1000.0, if *n == 1 { "" } else { "s" }));
        let after_only = if r.preview.as_ref().is_some_and(|p| p.2 > 0) { "the stale pages (see the notes)" } else { "" };
        md += &format!(
            "| {} ({}) | {} | {} | {} | {} |\n",
            r.name,
            r.what,
            stale_note,
            if r.at_preview.is_empty() { "-".into() } else if fp.is_empty() { format!("pass ({} checks)", r.at_preview.len()) } else { format!("FAIL: {}", fp.join(", ")) },
            if fl.is_empty() { format!("pass ({} checks)", r.after_landing.len()) } else { format!("FAIL: {}", fl.join(", ")) },
            after_only
        );
        if !fp.is_empty() || !fl.is_empty() {
            failed.push(r.name.clone());
        }
    }
    md += "\n## Details\n";
    for r in &rows {
        md += &format!("\n### {}\n\n{}\n\n", r.name, r.what);
        if let Some((_, steps, _)) = &r.preview {
            md += &format!("Preview steps: {}\n\n", steps.join("; "));
        }
        for (when, m) in [("at preview", &r.at_preview), ("after landing", &r.after_landing)] {
            for (k, (ok, d)) in m {
                md += &format!("- {when}: {} {k}: {d}\n", if *ok { "pass" } else { "FAIL" });
            }
        }
        for n in &r.notes {
            md += &format!("- note: {n}\n");
        }
    }
    std::fs::write(scratch.join("report.md"), &md).unwrap();
    eprintln!("report: {}", scratch.join("report.md").display());
    assert!(failed.is_empty(), "scenarios that failed: {failed:?}");
}
