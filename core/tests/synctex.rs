//! Note ↔ PDF lookups (synctex.rs, with convert.rs's LineMap) against real SyncTeX files.
//!
//! - `fixture`: tests/sync, two notes of every kind of part (paragraphs, headings with labels, lists, equations, tables,
//!   callouts, comments, quotations), converted by the plugin and compiled by `latexmk -lualatex -synctex=1` (MiKTeX,
//!   2026-10-25); main.synctex is the build's main.synctex.gz unpacked, with the folders in its Input lines shortened.
//!   Places are checked against `synctex view`, and every part's place must come back to itself through the PDF.
//! - `real_notes`: a folder of notes and the SyncTeX file of their build, unpacked (`LATEX_NOTES`, `LATEX_SYNCTEX`);
//!   skipped when the variables are not set.

use std::{fs, path::Path};

use latex_core::convert::{md_to_tex_mapped, Index, Options};
use latex_core::pipeline::tex_name;
use latex_core::synctex::{forward, reverse, Synctex};

const PAGE_H: f32 = 842.0; // A4

fn notes(dir: &Path) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
        .filter(|n| n.ends_with(".md") && n != "README.md")
        .map(|n| (n.clone(), fs::read_to_string(dir.join(&n)).unwrap()))
        .collect();
    v.sort();
    v
}

/// Every part of every note, at its start and half way through: to the PDF, back to the note, and to the PDF again.
/// (lookups, those that come back to the same place within 1 bp).
fn round_trips(dir: &Path, synctex: &str) -> (usize, usize) {
    let o = Options::default();
    let ns = notes(dir);
    let idx = Index::new(ns.iter().map(|(n, t)| (n.as_str(), t.as_str())), &o);
    let texs: Vec<String> = ns.iter().map(|(n, _)| tex_name(n)).collect();
    let st = Synctex::new(synctex, |f| texs.iter().any(|t| t == f)).unwrap();
    let (mut total, mut same) = (0, 0);
    for ((n, md), tex) in ns.iter().zip(&texs) {
        let (_, _, map) = md_to_tex_mapped(n, md, &idx, &o).unwrap();
        let mut line = 1;
        while let Some((first, last, _)) = map.part(line).filter(|_| line as usize <= map.tex.len()) {
            for frac in [0.0, 0.5] {
                let Some(p) = forward(&st, tex, &map, first, frac, PAGE_H) else { continue };
                total += 1;
                let back = st.at(p).filter(|(file, _)| file == tex).and_then(|(_, t)| reverse(&st, tex, &map, t, p, PAGE_H));
                match back.and_then(|(l, _, f)| forward(&st, tex, &map, l, f, PAGE_H)) {
                    Some(q) if q.page == p.page && (q.v - p.v).abs() < 1.0 => same += 1,
                    q => eprintln!("{n}:{first} at {frac}: {p:?} -> {back:?} -> {q:?}"),
                }
            }
            line = last + 1;
        }
    }
    (total, same)
}

#[test]
fn fixture() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sync");
    let text = fs::read_to_string(dir.join("main.synctex")).unwrap();
    let st = Synctex::new(&text, |f| f == "3-introduction.tex" || f == "5-methodology.tex").unwrap();
    assert_eq!(st.sheets, 3);
    // `synctex view -i line:0:file -o main.pdf` gives these baselines (y, in bp from the top of the page: 82.958893,
    // 128.322708)
    let intro = st.file("3-introduction.tex").unwrap();
    for (line, page, y) in [(4, 1, 82.95889), (11, 1, 128.3227)] {
        let p = st.line_pos(intro, line, |_| false).unwrap();
        assert!(p.page == page && (p.v - y).abs() < 0.01, "line {line}: {p:?}, synctex view: page {page} y {y}");
    }
    // a paragraph over two pages: synctex view lists page 2 at 769.889771 and 756.340576, page 3 at 96.508087 and
    // 82.958893; its place is where it begins
    let p = st.line_pos(st.file("5-methodology.tex").unwrap(), 11, |_| false).unwrap();
    assert!(p.page == 2 && (p.v - 756.3406).abs() < 0.01, "{p:?}");
    let (total, same) = round_trips(&dir, &text);
    println!("{same} of {total} places come back to themselves");
    // measured 2026-10-25: 167 of 168 (the one left is half way through the chapter's last part, where the next
    // baseline is the next chapter's)
    assert!(total > 150 && same * 100 >= total * 98, "{same} of {total}");
}

#[test]
fn real_notes() {
    let (Ok(notes_dir), Ok(synctex)) = (std::env::var("LATEX_NOTES"), std::env::var("LATEX_SYNCTEX")) else {
        eprintln!("LATEX_NOTES and LATEX_SYNCTEX are not set: skipped");
        return;
    };
    let (total, same) = round_trips(Path::new(&notes_dir), &fs::read_to_string(synctex).unwrap());
    println!("{same} of {total} places come back to themselves ({:.1}%)", 100.0 * same as f64 / total as f64);
    assert!(same * 100 >= total * 95, "{same} of {total}");
}
