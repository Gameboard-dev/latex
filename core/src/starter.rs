//! What a new document starts from (2026-10-28): the example preamble, the placeholder title page and the colours of
//! highlighted code, so that a new notes folder builds the first time with nothing run by hand.
//!
//! - The preamble note is a copy of `examples/0-preamble.md`.
//! - The title-page note lists every property the example preamble prints, all empty and all optional: the preamble
//!   prints a field only when its property exists and has a value (`\iftpfield`), so the converter's
//!   `\newcommand{\tp<Field>}{...}` lines may define any subset of them.
//! - `code/pygments-style.tex` defines `\PY` and the `\PYZ..` escapes that Pygments' LaTeX output uses. The plugin
//!   writes it (Pygments' default style, bundled here) when it writes a note's code and finds the file missing, so a
//!   listing needs no `pygmentize -S` run by hand and the plugin needs no Python for it; a file already there (another
//!   style, or one written by latexmkrc) is never overwritten. The example preamble inputs it only if it exists, so a
//!   document without listings builds without it.

/// The example preamble, copied into a notes folder that has no preamble note.
pub const PREAMBLE_NOTE: &str = include_str!("../../examples/0-preamble.md");

/// The title page's properties that the example preamble prints, in the order it prints them. The converter turns
/// each into `\tp<Field>` (`student_id` -> `\tpStudentId`).
pub const TITLE_FIELDS: &[&str] =
    &["university", "department", "title", "subtitle", "author", "student_id", "degree", "supervisor", "module", "word_count_note", "date"];

/// The placeholder title-page note: every property of TITLE_FIELDS, empty, and a comment saying they are optional.
pub fn title_page_note() -> String {
    let mut s = String::from("---\n");
    for f in TITLE_FIELDS {
        s.push_str(&format!("{f}: \"\"\n"));
    }
    s.push_str("---\n%% Every property is optional: the title page prints those that have a value and leaves out the rest, labels included. %%\n");
    s
}

/// Where the colours of highlighted code go, in the output folder (the path the example preamble inputs).
pub const PYGMENTS_STYLE_FILE: &str = "code/pygments-style.tex";

/// Pygments' default style for its LaTeX output (`pygmentize -S default -f latex`, Pygments 2.20), with a header.
pub const PYGMENTS_STYLE: &str = concat!(
    "% The colours of highlighted code: Pygments' default style (pygmentize -S default -f latex, Pygments 2.20),\n",
    "% written by the LaTeX plugin because this file was missing. The plugin never overwrites it, so it may be\n",
    "% replaced by another style's output (pygmentize -S <style> -f latex).\n",
    include_str!("pygments-style.tex")
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::{md_to_tex, Index, Options};

    /// The preamble inside the example note's ```latex block.
    fn preamble() -> &'static str {
        let start = PREAMBLE_NOTE.find("```latex\n").unwrap() + 9;
        let end = PREAMBLE_NOTE[start..].find("\n```").unwrap() + start;
        &PREAMBLE_NOTE[start..end]
    }

    fn camel(k: &str) -> String {
        k.split('_').map(|w| w[..1].to_uppercase() + &w[1..]).collect()
    }

    #[test]
    fn the_title_page_prints_exactly_the_placeholder_fields_each_optional() {
        let p = preamble();
        let body = &p[p.find(r"\newcommand{\maketitlepage}").unwrap()..];
        let body = &body[..body.find(r"\end{titlepage}").unwrap()];
        let mut used: Vec<String> = Vec::new();
        for s in body.split(r"\iftpfield{").skip(1) {
            let f = s[..s.find('}').unwrap()].to_string();
            if !used.contains(&f) {
                used.push(f);
            }
        }
        assert_eq!(used, TITLE_FIELDS.iter().map(|f| camel(f)).collect::<Vec<_>>());
        // each field's command appears only inside its own \iftpfield, so an undefined one is never expanded
        for f in TITLE_FIELDS {
            let cmd = format!(r"\tp{}", camel(f));
            for (i, _) in body.match_indices(&cmd) {
                let guard = format!(r"\iftpfield{{{}}}{{", camel(f));
                let open = body[..i].rfind(&guard).unwrap_or_else(|| panic!("{cmd} is printed outside \\iftpfield"));
                let mut depth = 0i32;
                for ch in body[open + guard.len()..i].chars() {
                    depth += match ch { '{' => 1, '}' => -1, _ => 0 };
                    assert!(depth >= 0, "{cmd} is printed outside its \\iftpfield");
                }
            }
        }
        // \ifcsvoid is etoolbox's, loaded before the title page
        assert!(p.find(r"\usepackage{etoolbox}").unwrap() < p.find(r"\newcommand{\iftpfield}").unwrap());
        assert!(p.contains(r"\newcommand{\iftpfield}[2]{\ifcsvoid{tp#1}{}{#2}}"));
    }

    #[test]
    fn the_placeholder_title_page_converts_to_every_field_empty() {
        let o = Options::default();
        let md = title_page_note();
        let idx = Index::new([("0-preamble.md", PREAMBLE_NOTE), ("1-title_page.md", md.as_str())], &o);
        let (tex, warn) = md_to_tex("1-title_page.md", &md, &idx, &o).unwrap();
        assert!(warn.is_empty(), "{warn:?}");
        let mut want: Vec<String> = TITLE_FIELDS.iter().map(|f| format!(r"\newcommand{{\tp{}}}{{}}", camel(f))).collect();
        want.push(r"\maketitlepage".into());
        let got: Vec<&str> = tex.lines().filter(|l| l.starts_with('\\')).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn the_style_file_defines_what_pygments_listings_use() {
        // Pygments' LaTeX formatter writes \PY{token}{text} and escapes special characters as \PYZ..{}
        assert!(PYGMENTS_STYLE.contains(r"\def\PY#1#2{"));
        for esc in ["bs", "us", "ob", "cb", "ca", "am", "lt", "gt", "sh", "pc", "dl", "hy", "sq", "dq", "ti", "at", "lb", "rb"] {
            assert!(PYGMENTS_STYLE.contains(&format!(r"\def\PYZ{esc}{{")), "\\PYZ{esc} is not defined");
        }
        assert!(PYGMENTS_STYLE.contains(r"\makeatletter") && PYGMENTS_STYLE.trim_end().ends_with(r"\makeatother"));
        assert!(!PYGMENTS_STYLE.contains('\r'));
        // the example preamble inputs it only when it exists
        let p = preamble();
        assert!(p.contains(&format!(r"\InputIfFileExists{{{PYGMENTS_STYLE_FILE}}}{{}}{{}}")));
        assert!(!p.contains(r"\input{code/pygments-style"));
    }
}
