//! The converter against latex_sync.py, the Python script it was ported from.
//!
//! - `fixtures`: tests/fixtures.json, synthetic notes (zotsearch/tests/test_latex_sync.py's cases, the same notes after a
//!   round trip through the script's LaTeX -> Markdown import, and a case for every rule and warning), each with the
//!   LaTeX and warnings the script gives. The JSON is made by a script that imports latex_sync and writes only the JSON.
//! - `real_notes`: a folder of notes and the script's output for each (`LATEX_NOTES`, `LATEX_EXPECTED`: a folder of
//!   `<note>.tex` files and a summary.json of warnings); skipped when the variables are not set.

use std::{fs, path::Path};

use latex_core::convert::{md_to_tex, Index, Options};
use serde_json::Value;

fn index_of(notes: &[(String, String)], o: &Options) -> Index {
    Index::new(notes.iter().map(|(n, t)| (n.as_str(), t.as_str())), o)
}

/// The first line where two texts differ, to show.
fn first_difference(a: &str, b: &str) -> String {
    let (al, bl): (Vec<&str>, Vec<&str>) = (a.split('\n').collect(), b.split('\n').collect());
    let k = al.iter().zip(&bl).position(|(x, y)| x != y).unwrap_or(al.len().min(bl.len()));
    format!("line {}:\n  python: {:?}\n  rust:   {:?}", k + 1, al.get(k), bl.get(k))
}

/// A warning in the plugin's words where they are deliberately not the script's: the labelled lists are a setting, so
/// their warning names no particular list.
fn reworded(w: &str) -> String {
    w.replace("a link to the research questions or hypotheses needs", "a link to a labelled list needs")
}

/// Cases where the plugin deliberately does what the script did not: a block of code (```python) is a hand-typed
/// listing since 2026-10-25, where the script wrote it verbatim (convert/tests.rs tests it).
const DEPARTED: [&str; 1] = ["fences"];

#[test]
fn fixtures() {
    let cases: Vec<Value> = serde_json::from_str(&fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures.json")).unwrap()).unwrap();
    let o = Options::default();
    let mut failed = Vec::new();
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        if DEPARTED.contains(&name) {
            continue;
        }
        let notes: Vec<(String, String)> = c["notes"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string())).collect();
        let idx = index_of(&notes, &o);
        let got = md_to_tex(c["note"].as_str().unwrap(), c["md"].as_str().unwrap(), &idx, &o);
        match (got, c.get("error")) {
            (Err(_), Some(_)) => {}
            (Ok(_), Some(e)) => failed.push(format!("{name}: converted, but the script fails with {e}")),
            (Err(e), None) => failed.push(format!("{name}: not converted: {e}")),
            (Ok((tex, warn)), None) => {
                let want = c["tex"].as_str().unwrap();
                if tex != want {
                    failed.push(format!("{name}: LaTeX differs at {}", first_difference(want, &tex)));
                }
                let want_w: Vec<String> = c["warnings"].as_array().unwrap().iter().map(|w| reworded(w.as_str().unwrap())).collect();
                if warn != want_w {
                    failed.push(format!("{name}: warnings differ\n  python: {want_w:?}\n  rust:   {warn:?}"));
                }
            }
        }
    }
    assert!(failed.is_empty(), "{} of {} cases differ:\n{}", failed.len(), cases.len(), failed.join("\n"));
}

#[test]
fn real_notes() {
    let (Ok(notes_dir), Ok(expected)) = (std::env::var("LATEX_NOTES"), std::env::var("LATEX_EXPECTED")) else {
        eprintln!("LATEX_NOTES and LATEX_EXPECTED are not set: skipped");
        return;
    };
    let summary: Value = serde_json::from_str(&fs::read_to_string(Path::new(&expected).join("summary.json")).unwrap()).unwrap();
    let names: Vec<String> = summary.as_object().unwrap().keys().cloned().collect();
    let notes: Vec<(String, String)> = names.iter().map(|n| (n.clone(), fs::read_to_string(Path::new(&notes_dir).join(n)).unwrap())).collect();
    let o = Options::default();
    let idx = index_of(&notes, &o);
    let mut failed = Vec::new();
    for (name, md) in &notes {
        let stem = name.strip_suffix(".md").unwrap();
        let (tex, warn) = md_to_tex(name, md, &idx, &o).unwrap_or_else(|e| panic!("{name}: {e}"));
        let want = fs::read_to_string(Path::new(&expected).join(format!("{stem}.tex"))).unwrap();
        let want_w: Vec<&str> = summary[name]["warnings"].as_array().unwrap().iter().map(|w| w.as_str().unwrap()).collect();
        let same = tex == want && warn == want_w;
        println!("{name}: {} lines, {} warnings, {}", tex.split('\n').count() - 1, warn.len(), if same { "identical" } else { "DIFFERENT" });
        if tex != want {
            failed.push(format!("{name}: {}", first_difference(&want, &tex)));
        }
        if warn != want_w {
            failed.push(format!("{name}: warnings {want_w:?} / {warn:?}"));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
