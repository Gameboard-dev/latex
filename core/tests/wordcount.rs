//! The word counter against the thesis's LuaLaTeX counter (wordcount.lua, now in the repository's
//! archived/lua-wordcount/), whose counts it replaces.
//!
//! - `lua_counts`: tests/wordcount/, two documents compiled once with the Lua counter and the thesis's preamble
//!   (2026-10-25), and the counts it gave (expected.json): 3-fixture.tex, the converter's output for 3-fixture.md, which
//!   uses every construct the converter writes, and probe.tex, raw LaTeX a note may hold, one counted section each.
//! - `fixture_is_converter_output`: 3-fixture.tex is still what the converter makes of 3-fixture.md (if the rules
//!   change, the fixture is made again and compiled with the archived Lua counter for new counts).
//! - `real_chapters`: the author's chapters (`WORDCOUNT_CHAPTERS`: the folder of the .tex files, the .bib and
//!   glossary.tex), read only; each chapter that is still as it was on 2026-10-25 (real-chapters.json, by its hash) must
//!   give the Lua counter's counts. Skipped when the variable is not set.
//! All three count with the Lua counter's rules (`LUA_RULES`); the built-in rules now also count captions, table cells
//! and picture labels, and no longer count verbatim blocks (`floats_and_code` in src/wordcount.rs).

use std::{collections::BTreeMap, fs, path::Path};

use latex_core::convert::{md_to_tex, Index, Options};
use latex_core::wordcount::{count_chapter, Context, Glossary, Macros, Names, Rules};
use serde_json::Value;

fn data() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/wordcount")
}

fn json(name: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(data().join(name)).unwrap()).unwrap()
}

fn counts_of(v: &Value) -> BTreeMap<String, usize> {
    v.as_object().unwrap().iter().map(|(k, n)| (k.clone(), n.as_u64().unwrap() as usize)).collect()
}

/// What the Lua counter did differently from today's built-in rules (2026-10-27: captions, table cells and picture
/// labels count; code does not).
const LUA_RULES: &str = "figure skip
figure* skip
table skip
table* skip
tabular skip
tabular* skip
tabularx skip
longtable skip
tikzpicture skip
verbatim verbatim
";

/// Counts every chapter of a folder that holds a .bib and glossary.tex.
fn count_in(dir: &Path, chapter: &str) -> (BTreeMap<String, usize>, Vec<(u32, String)>) {
    let names = Names::from_bib(&fs::read_to_string(dir.join("My Library.bib")).unwrap_or_default());
    let glossary = Glossary::from_tex(&fs::read_to_string(dir.join("glossary.tex")).unwrap_or_default(), "term");
    // the Lua counter's rules: floats, pictures and tables not counted, verbatim blocks counted
    let rules = Rules::new(LUA_RULES, &Macros::default()).0;
    let c = count_chapter(&fs::read_to_string(dir.join(chapter)).unwrap(), &Context { rules: &rules, names: &names, glossary: &glossary });
    (c.sections.into_iter().collect(), c.warnings)
}

#[test]
fn lua_counts() {
    let expected = json("expected.json");
    for (doc, want) in expected["documents"].as_object().unwrap() {
        let (got, warnings) = count_in(&data(), doc);
        let want = counts_of(want);
        let wrong: Vec<String> = want.iter().filter(|(k, n)| got.get(*k) != Some(n)).map(|(k, n)| format!("{k}: Lua {n}, here {:?}", got.get(k))).collect();
        assert!(wrong.is_empty() && got.len() == want.len(), "{doc}: {}", wrong.join("; "));
        assert!(warnings.is_empty(), "{doc}: {warnings:?}");
    }
}

#[test]
fn fixture_is_converter_output() {
    let o = Options::default();
    let notes: Vec<(String, String)> = ["0-preamble.md", "3-fixture.md", "glossary.md"].iter().map(|n| (n.to_string(), fs::read_to_string(data().join(n)).unwrap())).collect();
    let idx = Index::new(notes.iter().map(|(n, t)| (n.as_str(), t.as_str())), &o);
    for (note, tex) in [("3-fixture.md", "3-fixture.tex"), ("glossary.md", "glossary.tex")] {
        let md = &notes.iter().find(|(n, _)| n == note).unwrap().1;
        let (got, _) = md_to_tex(note, md, &idx, &o).unwrap();
        assert_eq!(got, fs::read_to_string(data().join(tex)).unwrap().replace("\r\n", "\n"), "{tex} is no longer the converter's output for {note}: make it again and recount it with the Lua counter");
    }
}

#[test]
fn real_chapters() {
    let Ok(dir) = std::env::var("WORDCOUNT_CHAPTERS") else { return };
    let dir = Path::new(&dir);
    let snapshot = json("real-chapters.json");
    let (mut checked, mut changed, mut wrong) = (Vec::new(), Vec::new(), Vec::new());
    for (chapter, c) in snapshot["chapters"].as_object().unwrap() {
        let Ok(bytes) = fs::read(dir.join(chapter)) else {
            changed.push(format!("{chapter} (missing)"));
            continue;
        };
        if format!("{:016x}", xxhash_rust::xxh3::xxh3_64(&bytes)) != c["xxh3"].as_str().unwrap() {
            changed.push(chapter.clone());
            continue;
        }
        let (got, warnings) = count_in(dir, chapter);
        let want = counts_of(&c["expected"]);
        if got != want {
            wrong.push(format!("{chapter}: expected {want:?}, counted {got:?}"));
        }
        if !warnings.is_empty() {
            wrong.push(format!("{chapter}: warnings {warnings:?}"));
        }
        checked.push(chapter.clone());
    }
    eprintln!("checked {checked:?}; changed since 2026-10-25, not checked: {changed:?}");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    assert!(!checked.is_empty(), "every chapter has changed since the snapshot");
}
