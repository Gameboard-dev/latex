//! Word counts. The counter is the `latex-wordcount` crate (../wordcount), shared with Quarry's command-line tools so
//! that both give the same counts; here is what ties it to this plugin: a chapter's warnings placed at their line in its
//! note (the conversion's `LineMap`), and the file of counts the preamble reads.

pub use latex_wordcount::*;

use crate::convert::LineMap;

/// The note line (from 1) where a chapter's .tex line came from: the line whose part begins nearest before it.
pub fn note_line(map: &LineMap, tex_line: u32) -> u32 {
    let best = map.tex.iter().copied().filter(|&t| t <= tex_line).max();
    best.and_then(|b| map.tex.iter().position(|&t| t == b)).map_or(1, |k| k as u32 + 1)
}

/// A chapter's warnings as the log writes the converter's: `3-introduction.md:12: ...`.
pub fn in_note(note: &str, map: &LineMap, warnings: &[(u32, String)]) -> Vec<String> {
    warnings.iter().map(|(t, w)| format!("{note}:{}: {w}", note_line(map, *t))).collect()
}

/// The file the preamble reads the counts from: one `\wc@set{key}{n}` line per counted section, in the order given
/// (the chapters in the document's order).
pub fn counts_file(counts: &[(String, usize)]) -> String {
    let mut out = String::from("% Word counts, written by the LaTeX plugin whenever a chapter is converted (do not edit).\n");
    for (k, n) in counts {
        out += &format!("\\wc@set{{{k}}}{{{n}}}\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIB: &str = "@article{one, author = {Smith, John}, title = {One}, year = {2020}}";
    const GLOSSARY: &str = r"\term{FNRATE}{FNRATE}{Attack Presentation Classification Error Rate}{The share of attacks accepted.}";

    #[test]
    fn line_numbers_in_the_note() {
        let map = LineMap { tex: vec![1, 1, 3, 3, 5, 9], empty: vec![] };
        assert_eq!(note_line(&map, 4), 3, "the part that begins nearest before");
        assert_eq!(note_line(&map, 9), 6);
        assert_eq!(in_note("n.md", &map, &[(5, "x".into())]), ["n.md:5: x"]);
        assert_eq!(counts_file(&[("intro".into(), 12)]).lines().nth(1), Some("\\wc@set{intro}{12}"));
    }

    #[test]
    fn a_chapter_left_out_counts_the_same() {
        let tex = "% Generated from 5-m.md\n% !TEX root = main.tex\n\\startwordcount{m}\n\\section{Methods}\nSome words here \\cite{one}.\n\\stopwordcount\n";
        let ctx = |f: &dyn Fn(&Context) -> Counts| f(&Context { rules: &Rules::default(), names: &Names::from_bib(BIB), glossary: &Glossary::from_tex(GLOSSARY, "term"), cites: true });
        let (all, out) = (ctx(&|c| count_chapter(tex, c)), ctx(&|c| count_chapter(&crate::pipeline::leave_out(tex), c)));
        assert_eq!((all.sections, all.warnings), (out.sections, out.warnings));
    }
}
