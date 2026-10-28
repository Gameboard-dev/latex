//! The chapters' order: the number that starts a note's name is its place in the document, so the notes of the folder
//! sort in the file explorer as they print. They are numbered in the order
//! their parts appear: `0-preamble` and `1-title_page` always first, then the chapters (`2-introduction`, …), then the
//! glossary and the appendix, numbered after the chapters. Those four have fixed names. Settings → Chapter order keeps
//! the order of the chapters; "Apply ordering" (the notes folder's right-click menu) renames the notes to it, numbering a new note too. Renaming
//! goes through Obsidian (fileManager.renameFile), which rewrites every link to the note; the main file's `\input`
//! lines follow (`rewrite_main`). Pure functions, tested here; app.rs does the renaming.

/// A note's number and the rest of its name: `3-introduction` → (3, "introduction").
pub fn split_number(stem: &str) -> Option<(u32, &str)> {
    let (n, rest) = stem.split_once('-')?;
    if n.is_empty() || n.len() > 4 || !n.bytes().all(|b| b.is_ascii_digit()) || rest.is_empty() {
        return None;
    }
    Some((n.parse().ok()?, rest))
}

/// A name without its number: `3-introduction` → `introduction`; `glossary` stays.
pub fn base(stem: &str) -> &str {
    split_number(stem).map_or(stem, |(_, r)| r)
}

/// The fixed names of the notes with a role (user, 2026-10-27: there is no reason to rename them): the preamble and the
/// title page always first (`0-preamble`, `1-title_page`), the glossary and the appendix always last, numbered after
/// the chapters. A role note is found by its name without the number.
pub const PREAMBLE: &str = "preamble";
pub const TITLE_PAGE: &str = "title_page";
pub const GLOSSARY: &str = "glossary";
pub const APPENDIX: &str = "appendix";

/// The folder's notes by role, and the chapters: numbered notes in their order, then any note without a number (a new
/// chapter, numbered when the ordering is applied).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub preamble: Option<String>,
    pub title_page: Option<String>,
    pub chapters: Vec<String>,
    pub glossary: Option<String>,
    pub appendix: Option<String>,
}

impl Layout {
    /// `stems`: the notes of the folder (without .md), the notes left out already removed.
    pub fn of(stems: &[String]) -> Layout {
        let is = |s: &str, role: &str| base(s).eq_ignore_ascii_case(role);
        let mut l = Layout::default();
        let mut numbered: Vec<(u32, String)> = Vec::new();
        let mut loose: Vec<String> = Vec::new();
        for s in stems {
            if s.eq_ignore_ascii_case("readme") {
                continue; // the folder's README is not part of the document
            } else if is(s, PREAMBLE) {
                l.preamble = Some(s.clone());
            } else if is(s, TITLE_PAGE) {
                l.title_page = Some(s.clone());
            } else if is(s, GLOSSARY) {
                l.glossary = Some(s.clone());
            } else if is(s, APPENDIX) {
                l.appendix = Some(s.clone());
            } else if let Some((n, _)) = split_number(s) {
                numbered.push((n, s.clone()));
            } else {
                loose.push(s.clone());
            }
        }
        numbered.sort();
        loose.sort();
        l.chapters = numbered.into_iter().map(|(_, s)| s).chain(loose).collect();
        l
    }

    /// The chapters in `order` (names without their numbers, as the setting keeps them; a chapter it does not name
    /// keeps its place after those it does).
    pub fn ordered(&self, order: &[String]) -> Vec<String> {
        let mut seq: Vec<String> = order.iter().filter_map(|o| self.chapters.iter().find(|c| base(c).eq_ignore_ascii_case(base(o))).cloned()).collect();
        let rest: Vec<String> = self.chapters.iter().filter(|c| !seq.contains(c)).cloned().collect();
        seq.extend(rest);
        seq
    }

    /// The renames that apply the ordering: the preamble 0, the title page 1, the chapters in `order` after them, then
    /// the glossary and the appendix. Only the names that change.
    pub fn renumber(&self, order: &[String]) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut push = |old: &str, n: u32| {
            let new = format!("{n}-{}", base(old));
            if new != old {
                out.push((old.to_string(), new));
            }
        };
        if let Some(p) = &self.preamble {
            push(p, 0);
        }
        let mut n = 0;
        if let Some(t) = &self.title_page {
            n += 1;
            push(t, n);
        }
        for c in self.ordered(order) {
            n += 1;
            push(&c, n);
        }
        for role in [&self.glossary, &self.appendix].into_iter().flatten() {
            n += 1;
            push(role, n);
        }
        out
    }
}

/// Whether a line of the main file reads `name` (`\input{name}` or `\include{name}`, with or without .tex).
fn reads(line: &str, name: &str) -> bool {
    input_of(line).is_some_and(|n| n == name)
}

/// The file a line `\input{x}` or `\include{x}` reads (before any comment), without .tex.
fn input_of(line: &str) -> Option<&str> {
    let code = line.split('%').next().unwrap_or("").trim();
    let rest = code.strip_prefix("\\input{").or_else(|| code.strip_prefix("\\include{"))?;
    let (name, _) = rest.split_once('}')?;
    Some(name.trim().trim_end_matches(".tex"))
}

/// The main file with every `\input{old}` renamed, then the chapters' `\input` lines put in `order` (the chapters'
/// new names): the lines that read a chapter keep their places, and the chapters fill them in order, so lines between
/// them (\pagenumbering, \clearpage …) stay where they are. `added`: a chapter the main file does not read yet, whose
/// line goes after the last chapter's (or before the appendix's, or before \end{document}).
pub fn rewrite_main(tex: &str, renames: &[(String, String)], order: &[String], added: Option<&str>, appendix: Option<&str>) -> String {
    let crlf = tex.contains("\r\n");
    let mut lines: Vec<String> = tex.replace("\r\n", "\n").split('\n').map(str::to_string).collect();
    for line in &mut lines {
        if let Some(name) = input_of(line).map(str::to_string) {
            if let Some((_, new)) = renames.iter().find(|(old, _)| *old == name) {
                *line = line.replacen(&format!("{{{name}}}"), &format!("{{{new}}}"), 1).replacen(&format!("{{{name}.tex}}"), &format!("{{{new}.tex}}"), 1);
            }
        }
    }
    let slots: Vec<usize> = (0..lines.len()).filter(|&i| order.iter().any(|c| reads(&lines[i], c))).collect();
    let present: Vec<&String> = order.iter().filter(|c| slots.iter().any(|&i| reads(&lines[i], c))).collect();
    let texts: Vec<String> = present.iter().map(|c| lines[slots.iter().copied().find(|&i| reads(&lines[i], c)).unwrap()].clone()).collect();
    for (k, &i) in slots.iter().enumerate() {
        if let Some(t) = texts.get(k) {
            lines[i] = t.clone();
        }
    }
    if let Some(new) = added.filter(|n| !lines.iter().any(|l| reads(l, n))) {
        let at = slots
            .last()
            .map(|i| i + 1)
            .or_else(|| appendix.and_then(|a| lines.iter().position(|l| reads(l, a))))
            .or_else(|| lines.iter().position(|l| l.trim_start().starts_with("\\end{document}")))
            .unwrap_or(lines.len());
        lines.insert(at, format!("\\input{{{new}}}"));
    }
    let out = lines.join("\n");
    if crlf { out.replace('\n', "\r\n") } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn numbers() {
        assert_eq!(split_number("3-introduction"), Some((3, "introduction")));
        assert_eq!(split_number("10-appendix"), Some((10, "appendix")));
        assert_eq!(split_number("glossary"), None);
        assert_eq!(split_number("x-y"), None);
        assert_eq!(split_number("3-"), None);
        assert_eq!(base("0-preamble"), "preamble");
    }

    fn layout() -> Layout {
        let stems = v(&["0-preamble", "1-title_page", "2-summary", "3-introduction", "4-review", "9-appendix", "glossary"]);
        Layout::of(&stems)
    }

    #[test]
    fn roles_and_order() {
        let l = layout();
        assert_eq!((l.preamble.as_deref(), l.title_page.as_deref()), (Some("0-preamble"), Some("1-title_page")));
        assert_eq!((l.glossary.as_deref(), l.appendix.as_deref()), (Some("glossary"), Some("9-appendix")));
        assert_eq!(l.chapters, v(&["2-summary", "3-introduction", "4-review"]));
        // the glossary and the appendix after the chapters
        assert_eq!(l.renumber(&[]), [("glossary".to_string(), "5-glossary".to_string()), ("9-appendix".into(), "6-appendix".into())]);
        // the order kept by the setting is of names without numbers
        let moved = l.renumber(&v(&["summary", "review", "introduction"]));
        assert_eq!(moved, [("4-review".into(), "3-review".into()), ("3-introduction".into(), "4-introduction".into()), ("glossary".into(), "5-glossary".into()), ("9-appendix".into(), "6-appendix".into())]);
        let done = Layout::of(&v(&["0-preamble", "1-title_page", "2-a", "3-b", "4-glossary", "5-appendix"]));
        assert!(done.renumber(&[]).is_empty());
    }

    #[test]
    fn a_new_note_is_numbered_when_the_ordering_is_applied() {
        let l = Layout::of(&v(&["0-preamble", "1-title_page", "2-introduction", "results", "3-glossary", "4-appendix", "README"]));
        assert_eq!(l.chapters, v(&["2-introduction", "results"]));
        assert_eq!(l.renumber(&[]), [("results".to_string(), "3-results".to_string()), ("3-glossary".into(), "4-glossary".into()), ("4-appendix".into(), "5-appendix".into())]);
        // a title page with no number is still put first
        let l = Layout::of(&v(&["preamble", "title_page", "intro"]));
        assert_eq!(l.renumber(&[]), [("preamble".to_string(), "0-preamble".to_string()), ("title_page".into(), "1-title_page".into()), ("intro".into(), "2-intro".into())]);
    }

    #[test]
    fn main_file() {
        let tex = "\\input{0-preamble}\n\\input{glossary}\n\\begin{document}\n\\input{1-title_page}\n\\pagenumbering{arabic}\n\\input{2-intro} % the introduction\n\\input{3-review}\n\\input{9-appendix}\n\\end{document}";
        let renames = v(&["3-review", "2-review", "2-intro", "3-intro", "9-appendix", "4-appendix"]);
        let renames: Vec<(String, String)> = renames.chunks(2).map(|c| (c[0].clone(), c[1].clone())).collect();
        let out = rewrite_main(tex, &renames, &v(&["1-title_page", "2-review", "3-intro"]), None, Some("4-appendix"));
        assert_eq!(out, "\\input{0-preamble}\n\\input{glossary}\n\\begin{document}\n\\input{1-title_page}\n\\pagenumbering{arabic}\n\\input{2-review}\n\\input{3-intro} % the introduction\n\\input{4-appendix}\n\\end{document}");
        let added = rewrite_main(&out, &[], &v(&["1-title_page", "2-review", "3-intro"]), Some("4-results"), Some("4-appendix"));
        assert!(added.contains("\\input{3-intro} % the introduction\n\\input{4-results}\n\\input{4-appendix}"), "{added}");
        assert_eq!(rewrite_main(&added, &[], &[], Some("4-results"), None), added, "a chapter already read is not added twice");
        assert!(rewrite_main("a\r\n\\input{1-x}\r\n", &[("1-x".into(), "2-x".into())], &[], None, None).contains("\\input{2-x}\r\n"));
    }
}
