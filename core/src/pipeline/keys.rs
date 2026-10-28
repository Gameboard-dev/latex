//! Fast preview: what an edit changed besides its own text, read from the chapters and the last full build's files, and
//! the files a preview needs for it (docs/fast-preview-design.md, requirements 1 and 2).
//!
//! - Citations (`cite_keys`, `citation_order`, `CiteChange`): the bibliography is numeric in order of first citation
//!   (biblatex's sorting=none), so a citation added before others renumbers every later one. The new order is read from
//!   the chapters in the order the main file \inputs them, checked first against the last full build's .bcf (if the
//!   chapters as they were do not give the order biber was given, the order is not predicted), and written into a
//!   copy of the .bcf (`bcf_with_keys`) for biber to run on before LaTeX.
//! - Labels (`labels`, `label_defs`, `label_refs`): a label the preview defines that the full build did not have, or
//!   whose value changed.
//! - Glossary (`glossary_entries`, `glossary_change`, `glo_merge`, `gls_aux`): an entry's description or printed
//!   form changed, a term used for the first time or no longer used.
//! - Lists (`list_merge`): the preview's entries for the contents, figures and tables put in place of the full build's,
//!   by their anchors.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex_lite::Regex;

use crate::convert::rx;

rx!(CITE, r"\\(?:[Cc]ite[a-z]*|[Pp]arencites?|[Tt]extcites?|[Aa]utocites?|[Ss]martcites?|[Ff]ootcites?|nocite)\*?");
rx!(REF, r"\\(?:ref|eqref|pageref|autoref|[Cc]ref|nameref)\*?\{([^}]*)\}");
rx!(LABEL, r"\\label\{([^}]*)\}");
rx!(GLS, r"\\(?:[Gg]ls(?:pl|text|first|link)?|GLS(?:pl)?|glsxtrshort|glsxtrlong)\*?(?:\[[^\]]*\])?\{([^}]*)\}");
rx!(INPUT, r"\\(?:input|include)\{([^}]+)\}");
rx!(NEWLABEL, r"(?m)^\\newlabel\{([^}]*)\}\{(.*)\}\s*$");
rx!(WRITEFILE, r"(?m)^\\@writefile\{(toc|lof|lot)\}\{(.*)\}\s*$");
rx!(BCF_KEY, r#"(?m)^\s*<bcf:citekey order="\d+"[^>]*>([^<]*)</bcf:citekey>\s*$"#);

/// A balanced `{...}` at `i` (which is `{`): the text inside and the index after it.
fn braced(s: &str, i: usize) -> Option<(&str, usize)> {
    let b = s.as_bytes();
    if b.get(i) != Some(&b'{') {
        return None;
    }
    let mut depth = 0;
    for (k, &c) in b[i..].iter().enumerate() {
        match c {
            b'\\' => continue,
            b'{' if k == 0 || b[i + k - 1] != b'\\' => depth += 1,
            b'}' if b[i + k - 1] != b'\\' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&s[i + 1..i + k], i + k + 1));
                }
            }
            _ => {}
        }
    }
    None
}

/// Skips `[...]` groups (balanced) and spaces at `i`.
fn skip_opts(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    loop {
        while b.get(i) == Some(&b' ') {
            i += 1;
        }
        if b.get(i) != Some(&b'[') {
            return i;
        }
        let mut depth = 0;
        let mut k = i;
        while k < b.len() {
            match b[k] {
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            k += 1;
        }
        i = k + 1;
    }
}

/// The text without comments (a `%` not escaped to the end of its line), keeping the lines.
fn uncommented(tex: &str) -> String {
    tex.lines()
        .map(|l| {
            let mut prev = ' ';
            let cut = l.char_indices().find(|&(_, c)| {
                let hit = c == '%' && prev != '\\';
                prev = c;
                hit
            });
            cut.map_or(l, |(k, _)| &l[..k])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The citation keys of a chapter, each once, in the order first cited (\cite and its kin, with several keys and, for
/// \cites, several groups).
pub fn cite_keys(tex: &str) -> Vec<String> {
    let t = uncommented(tex);
    let mut out: Vec<String> = Vec::new();
    for m in CITE.find_iter(&t) {
        let mut i = m.end();
        // each [..][..]{keys} group (several for \cites)
        loop {
            let j = skip_opts(&t, i);
            let Some((keys, after)) = braced(&t, j) else { break };
            for k in keys.split(',').map(str::trim).filter(|k| !k.is_empty()) {
                if !out.iter().any(|x| x == k) {
                    out.push(k.to_string());
                }
            }
            i = after;
            if !m.as_str().ends_with('s') {
                break;
            }
        }
    }
    out
}

/// The files the main file \inputs, in order (without .tex).
pub fn inputs(main: &str) -> Vec<String> {
    INPUT.captures_iter(&uncommented(main)).map(|c| c[1].trim().trim_end_matches(".tex").to_string()).collect()
}

/// The part of a file LaTeX reads: up to the end of the line where `\endinput` stands, if one does (a chapter left out
/// of the PDF has it on its first line).
pub fn read_part(tex: &str) -> &str {
    let mut at = 0;
    for line in tex.split_inclusive('\n') {
        at += line.len();
        if uncommented(line).contains("\\endinput") {
            return &tex[..at];
        }
    }
    tex
}

/// The order of first citation through the document: the chapters (`files`: name without .tex, text) in the order the
/// main file inputs them.
pub fn citation_order(main: &str, files: &BTreeMap<String, String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in inputs(main) {
        for k in files.get(&f).map(|t| cite_keys(read_part(t))).unwrap_or_default() {
            if !out.contains(&k) {
                out.push(k);
            }
        }
    }
    out
}

/// The keys of a .bcf's first section, in its order (biber's input).
pub fn bcf_keys(bcf: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in BCF_KEY.captures_iter(bcf) {
        let k = c[1].to_string();
        if !out.contains(&k) {
            out.push(k);
        }
    }
    out
}

/// The .bcf with its citation keys replaced by `keys`, in that order (where the first was).
pub fn bcf_with_keys(bcf: &str, keys: &[String]) -> Option<String> {
    let first = BCF_KEY.find(bcf)?;
    let indent: String = first.as_str().chars().take_while(|c| c.is_whitespace() && *c != '\n').collect();
    let lines: String = keys.iter().enumerate().map(|(k, key)| format!("{indent}<bcf:citekey order=\"{}\" intorder=\"1\">{key}</bcf:citekey>\n", k + 1)).collect();
    let mut out = String::new();
    let mut done = false;
    for l in bcf.split_inclusive('\n') {
        if BCF_KEY.is_match(l.trim_end_matches(['\r', '\n'])) {
            if !done {
                out += &lines;
                done = true;
            }
            continue;
        }
        out += l;
    }
    Some(out)
}

/// How an edit changes the bibliography's numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CiteChange {
    Same,
    /// Keys added after all the others, or the last ones removed: no number changes, the bibliography does.
    Appended,
    /// Numbers change: every later [n] moves.
    Renumbered,
}

pub fn cite_change(old: &[String], new: &[String]) -> CiteChange {
    if old == new {
        CiteChange::Same
    } else if new.starts_with(old) || old.starts_with(new) {
        CiteChange::Appended
    } else {
        CiteChange::Renumbered
    }
}

/// Labels a chapter defines (`\label`), and references it makes.
pub fn label_defs(tex: &str) -> BTreeSet<String> {
    LABEL.captures_iter(&uncommented(tex)).map(|c| c[1].to_string()).collect()
}

pub fn label_refs(tex: &str) -> BTreeSet<String> {
    REF.captures_iter(&uncommented(tex)).flat_map(|c| c[1].split(',').map(|k| k.trim().to_string()).collect::<Vec<_>>()).collect()
}

/// An .aux's labels and their values.
pub fn labels(aux: &str) -> BTreeMap<String, String> {
    NEWLABEL.captures_iter(aux).map(|c| (c[1].to_string(), c[2].to_string())).collect()
}

/// The glossary keys a chapter uses.
pub fn gls_keys(tex: &str) -> BTreeSet<String> {
    GLS.captures_iter(&uncommented(tex)).map(|c| c[1].trim().to_string()).collect()
}

/// A glossary entry as the glossary's .tex defines it: `\term[text]{key}{name}{long}{description}`.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub text: Option<String>,
    pub name: String,
    pub long: String,
    pub description: String,
}

/// The entries of the glossary's .tex (`term`: the entry command).
pub fn glossary_entries(tex: &str, term: &str) -> BTreeMap<String, Entry> {
    let cmd = format!("\\{}", term.trim().trim_start_matches('\\'));
    let mut out = BTreeMap::new();
    let mut i = 0;
    while let Some(k) = tex[i..].find(&cmd).map(|k| k + i) {
        i = k + cmd.len();
        if tex[i..].starts_with(|c: char| c.is_ascii_alphabetic()) {
            continue;
        }
        let mut j = i;
        let text = if tex[j..].starts_with('[') {
            let e = skip_opts(tex, j);
            let t = tex[j + 1..e - 1].to_string();
            j = e;
            Some(t)
        } else {
            None
        };
        let mut args = Vec::new();
        for _ in 0..4 {
            match braced(tex, j) {
                Some((a, after)) => {
                    args.push(a.to_string());
                    j = after;
                }
                None => break,
            }
        }
        if args.len() == 4 {
            out.insert(args[0].clone(), Entry { text, name: args[1].clone(), long: args[2].clone(), description: args[3].clone() });
            i = j;
        }
    }
    out
}

/// What a glossary edit changed: entries whose printed form (text, name, long form) changed, whose description alone
/// changed, and entries added or removed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlossaryChange {
    pub printed: Vec<String>,
    pub described: Vec<String>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

pub fn glossary_change(old: &BTreeMap<String, Entry>, new: &BTreeMap<String, Entry>) -> GlossaryChange {
    let mut c = GlossaryChange::default();
    for (k, n) in new {
        match old.get(k) {
            None => c.added.push(k.clone()),
            Some(o) if (o.text.as_ref(), &o.name, &o.long) != (n.text.as_ref(), &n.name, &n.long) => c.printed.push(k.clone()),
            Some(o) if o.description != n.description => c.described.push(k.clone()),
            _ => {}
        }
    }
    c.removed = old.keys().filter(|k| !new.contains_key(*k)).cloned().collect();
    c
}

/// The key of a .glo line (`\glossaryentry{sort?\glossentry{key}|...}{page}`).
fn glo_key(line: &str) -> Option<&str> {
    let i = line.find("\\glossentry{")? + "\\glossentry{".len();
    let e = line[i..].find('}')?;
    Some(&line[i..i + e])
}

/// The keys a .glo indexes.
pub fn glo_keys(glo: &str) -> BTreeSet<String> {
    glo.lines().filter_map(glo_key).map(str::to_string).collect()
}

/// The glossary's index for a preview: the full build's lines for the keys still used (`used`) outside the previewed
/// pages (the lines whose page is not in `pages`, printed page numbers), and the preview's lines.
pub fn glo_merge(full: &str, preview: &str, used: &BTreeSet<String>, pages: &BTreeSet<String>) -> String {
    let page_of = |l: &str| l.rsplit_once("}{").map(|(_, p)| p.trim_end_matches('}').to_string());
    let mut out: String = full
        .lines()
        .filter(|l| glo_key(l).is_some_and(|k| used.contains(k)) && page_of(l).is_none_or(|p| !pages.contains(&p)))
        .map(|l| format!("{l}\n"))
        .collect();
    out.extend(preview.lines().filter(|l| glo_key(l).is_some()).map(|l| format!("{l}\n")));
    out
}

/// The .aux makeglossaries reads for a job `job`: the full build's glossary declarations, the style file renamed.
pub fn gls_aux(full_aux: &str, job: &str) -> String {
    full_aux
        .lines()
        .filter(|l| l.starts_with("\\@newglossary") || l.starts_with("\\@istfilename") || l.starts_with("\\@glsorder") || l.starts_with("\\glsxtr@"))
        .map(|l| if l.starts_with("\\@istfilename") { format!("\\@istfilename{{{job}.ist}}\n") } else { format!("{l}\n") })
        .collect()
}

/// The anchor of a list entry (`\contentsline {section}{...}{3}{section.1.2}%`): its last group.
fn anchor(entry: &str) -> Option<String> {
    let e = clean_entry(entry);
    let i = e.rfind('{')?;
    Some(e[i + 1..].trim_end_matches('}').to_string())
}

/// An entry as the list file has it: without the .aux's \protected@file@percent and the line's %.
fn clean_entry(entry: &str) -> &str {
    entry.trim_end().trim_end_matches('%').trim_end().trim_end_matches("\\protected@file@percent").trim_end()
}

/// The list file `ext` (toc, lof, lot) with the preview's entries (from its .aux's \@writefile lines) in place of the
/// full build's of the same anchors. None when an entry's anchor is not in the list (an entry added or removed: the
/// numbering moved, and the list waits for the full build); Some(unchanged) when the preview has no entries for it.
pub fn list_merge(full: &str, preview_aux: &str, ext: &str) -> Option<String> {
    let new: Vec<(String, String)> = WRITEFILE.captures_iter(preview_aux).filter(|c| &c[1] == ext).filter_map(|c| anchor(&c[2]).map(|a| (a, c[2].to_string()))).collect();
    let mut out: Vec<String> = full.lines().map(str::to_string).collect();
    for (a, entry) in new {
        let k = out.iter().position(|l| anchor(l).as_deref() == Some(a.as_str()))?;
        out[k] = format!("{}%", clean_entry(&entry));
    }
    Some(out.iter().map(|l| format!("{l}\n")).collect())
}

/// Whether two list entries differ, by anchor: the preview's entries (from its .aux) against the full build's list.
pub fn list_changed(full: &str, preview_aux: &str, ext: &str) -> bool {
    WRITEFILE.captures_iter(preview_aux).filter(|c| &c[1] == ext).any(|c| {
        let a = anchor(&c[2]);
        !full.lines().any(|l| anchor(l) == a && clean_entry(l) == clean_entry(&c[2]))
    })
}

/// The lines of the new text that differ from the old: (first, last), from 1, in the new text's numbering (None when
/// the texts are the same).
pub fn changed_lines(old: &str, new: &str) -> Option<(u32, u32)> {
    changed_span(old, new).map(|(first, _, new_last)| (first, new_last.max(first)))
}

/// The changed block: its first line (the same in both), its last line in the old text and in the new (from 1; one
/// before the first when the block is empty there: lines only added, or only removed).
pub fn changed_span(old: &str, new: &str) -> Option<(u32, u32, u32)> {
    let (o, n): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let head = o.iter().zip(&n).take_while(|(a, b)| a == b).count();
    if head == o.len() && head == n.len() {
        return None;
    }
    let tail = o.iter().rev().zip(n.iter().rev()).take_while(|(a, b)| a == b).count().min(o.len().min(n.len()) - head);
    Some((head as u32 + 1, (o.len() - tail) as u32, (n.len() - tail) as u32))
}

/// A line of the old text in the new one's numbering: before the changed block the same, after it moved by the
/// block's change in length, inside it the same line of the block (or the block's last).
pub fn new_line(line: u32, (first, old_last, new_last): (u32, u32, u32)) -> u32 {
    if line < first {
        line
    } else if line > old_last {
        (line as i64 + new_last as i64 - old_last as i64) as u32
    } else {
        line.min(new_last.max(first))
    }
}

/// Whether lines `first..=last` of a chapter hold a heading or a caption (what the contents, figures and tables list).
pub fn lists_touched(tex: &str, first: u32, last: u32, old: &str) -> BTreeSet<&'static str> {
    let mut out = BTreeSet::new();
    for t in [tex, old] {
        let lines: Vec<&str> = t.lines().collect();
        let to = (last as usize + 1).min(lines.len());
        for k in (first as usize - 1).min(to)..to {
            let l = lines[k];
            if ["\\section", "\\subsection", "\\subsubsection", "\\addcontentsline"].iter().any(|c| l.contains(c)) {
                out.insert("toc");
            }
            if l.contains("\\caption") {
                // the float it captions: the last one begun before it
                let env = lines[..=k].iter().rev().find_map(|x| if x.contains("\\begin{figure") { Some("lof") } else if x.contains("\\begin{table") { Some("lot") } else { None });
                out.extend(env);
            }
        }
    }
    out
}

// ------------------------------------------------------------------ the full build: what a pass leaves to do

/// What a full build runs after a LuaLaTeX pass, each with why (user, 2026-10-27): biber only when a citation key is
/// cited for the first time or no longer cited anywhere (or the order of first citation changes, which numbers the
/// IEEE list), or the .bib changed; makeglossaries only when a term is used for the first time or no longer used, a
/// term's pages change (the Terms list prints them), or the glossary changed; another pass only when something numbered or listed changed: a label added, removed or
/// renumbered (sections, appendix sections, figures, tables, equations, listings, hypotheses), or the contents, list
/// of figures or tables, or the PDF's bookmarks. Prose alone is one pass.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Needs {
    pub biber: Vec<String>,
    pub glossaries: Vec<String>,
    pub rerun: Vec<String>,
}

impl Needs {
    pub fn nothing(&self) -> bool {
        self.biber.is_empty() && self.glossaries.is_empty() && self.rerun.is_empty()
    }

    pub fn add(&mut self, other: Needs) {
        self.biber.extend(other.biber);
        self.glossaries.extend(other.glossaries);
        self.rerun.extend(other.rerun);
    }
}

/// The generated files a pass reads and rewrites, by extension.
pub const GENERATED: [&str; 7] = ["aux", "bcf", "glo", "toc", "lof", "lot", "out"];

/// Some names for the log: the first six, then how many more.
fn some(names: &[&String]) -> String {
    let mut s = names.iter().take(6).map(|n| n.as_str()).collect::<Vec<_>>().join(", ");
    if names.len() > 6 {
        s += &format!(" and {} more", names.len() - 6);
    }
    s
}

/// A label's value without its page (`{2.1}{14}{Title}{section.2.1}{}` → its groups but the second): a label that only
/// moved to another page prints the same everywhere but in a \pageref.
fn label_number(v: &str) -> Vec<&str> {
    let mut groups = Vec::new();
    let mut i = 0;
    while let Some((g, next)) = braced(v, i) {
        groups.push(g);
        i = next;
    }
    if groups.len() > 1 {
        groups.remove(1);
    }
    groups
}

/// What one pass leaves to do: its generated files before it (`before`, by extension) and after it. `pagerefs`: the
/// chapters print a label's page (\pageref), so a label that moved page needs another pass too.
pub fn after_pass(before: &BTreeMap<&str, String>, after: &BTreeMap<&str, String>, pagerefs: bool) -> Needs {
    let get = |m: &BTreeMap<&str, String>, ext: &str| m.get(ext).cloned().unwrap_or_default();
    let mut n = Needs::default();
    let (a, b) = (bcf_keys(&get(before, "bcf")), bcf_keys(&get(after, "bcf")));
    let added: Vec<&String> = b.iter().filter(|k| !a.contains(k)).collect();
    let removed: Vec<&String> = a.iter().filter(|k| !b.contains(k)).collect();
    if !added.is_empty() {
        n.biber.push(format!("cited for the first time: {}", some(&added)));
    }
    if !removed.is_empty() {
        n.biber.push(format!("no longer cited: {}", some(&removed)));
    }
    if added.is_empty() && removed.is_empty() && a != b {
        n.biber.push("the order of first citation changed (it numbers the list)".into());
    }
    let (a, b) = (glo_keys(&get(before, "glo")), glo_keys(&get(after, "glo")));
    let first: Vec<&String> = b.difference(&a).collect();
    let gone: Vec<&String> = a.difference(&b).collect();
    if !first.is_empty() {
        n.glossaries.push(format!("terms used for the first time: {}", some(&first)));
    }
    if !gone.is_empty() {
        n.glossaries.push(format!("terms no longer used: {}", some(&gone)));
    }
    if first.is_empty() && gone.is_empty() && get(before, "glo") != get(after, "glo") {
        // the Terms list prints each term's pages (the correctness suite's scenario 10: a term moved to the next page)
        n.glossaries.push("a term's pages changed".into());
    }
    let (a, b) = (labels(&get(before, "aux")), labels(&get(after, "aux")));
    let added: Vec<&String> = b.keys().filter(|k| !a.contains_key(*k)).collect();
    let removed: Vec<&String> = a.keys().filter(|k| !b.contains_key(*k)).collect();
    let moved: Vec<&String> = b.iter().filter(|(k, v)| a.get(*k).is_some_and(|w| if pagerefs { w != *v } else { label_number(w) != label_number(v) })).map(|(k, _)| k).collect();
    if !added.is_empty() {
        n.rerun.push(format!("labels added: {}", some(&added)));
    }
    if !removed.is_empty() {
        n.rerun.push(format!("labels removed: {}", some(&removed)));
    }
    if !moved.is_empty() {
        n.rerun.push(format!("labels renumbered: {}", some(&moved)));
    }
    for (ext, what) in [("toc", "the contents"), ("lof", "the list of figures"), ("lot", "the list of tables"), ("out", "the PDF's bookmarks")] {
        if get(before, ext) != get(after, ext) {
            n.rerun.push(format!("{what} changed"));
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_pass_leaves() {
        let m = |v: &[(&'static str, &str)]| v.iter().map(|(k, t)| (*k, t.to_string())).collect::<BTreeMap<&str, String>>();
        let bcf = |keys: &[&str]| keys.iter().enumerate().map(|(i, k)| format!("    <bcf:citekey order=\"{}\">{k}</bcf:citekey>\n", i + 1)).collect::<String>();
        let aux = "\\newlabel{sec:a}{{1}{3}{A}{section.1}{}}\n\\newlabel{tab:b}{{2}{5}{B}{table.2}{}}\n";
        let glo = "\\glossaryentry{fnrate?\\glossentry{fnrate}|setentrycounter[]{page}\\glsnumberformat}{3}\n";
        let before = m(&[("bcf", &bcf(&["a", "b"])), ("aux", aux), ("glo", glo), ("toc", "x")]);
        // prose: the same keys, terms and labels; a label only moved page
        let moved_page = aux.replace("{1}{3}", "{1}{4}");
        let after = m(&[("bcf", &bcf(&["a", "b"])), ("aux", &moved_page), ("glo", glo), ("toc", "x")]);
        assert!(after_pass(&before, &after, false).nothing());
        assert_eq!(after_pass(&before, &after, true).rerun, ["labels renumbered: sec:a"]);
        let glo_moved = glo.replace("}{3}", "}{4}");
        let after = m(&[("bcf", &bcf(&["a", "b"])), ("aux", aux), ("glo", &glo_moved), ("toc", "x")]);
        assert_eq!(after_pass(&before, &after, false).glossaries, ["a term's pages changed"]);
        // a key cited for the first time, one no longer cited, a new order
        let after = m(&[("bcf", &bcf(&["a", "b", "c"])), ("aux", aux), ("glo", glo), ("toc", "x")]);
        assert_eq!(after_pass(&before, &after, false), Needs { biber: vec!["cited for the first time: c".into()], ..Needs::default() });
        let after = m(&[("bcf", &bcf(&["a"])), ("aux", aux), ("glo", glo), ("toc", "x")]);
        assert_eq!(after_pass(&before, &after, false).biber, ["no longer cited: b"]);
        let after = m(&[("bcf", &bcf(&["b", "a"])), ("aux", aux), ("glo", glo), ("toc", "x")]);
        assert_eq!(after_pass(&before, &after, false).biber.len(), 1);
        // a table added (a label and the list of tables), a term no longer used
        let after = m(&[("bcf", &bcf(&["a", "b"])), ("aux", &format!("{aux}\\newlabel{{tab:c}}{{{{3}}{{6}}{{C}}{{table.3}}{{}}}}\n")), ("glo", ""), ("toc", "x"), ("lot", "C")]);
        let n = after_pass(&before, &after, false);
        assert_eq!(n.rerun, ["labels added: tab:c", "the list of tables changed"]);
        assert_eq!(n.glossaries, ["terms no longer used: fnrate"]);
        assert!(n.biber.is_empty());
    }

    #[test]
    fn citations() {
        let tex = "As \\citet{a} says [\\cite[p.~2]{b, c}] and \\cites[see][]{d}[p. 3]{a,e}. % \\cite{z}\n\\parencite{f}\\nocite{g}";
        assert_eq!(cite_keys(tex), ["a", "b", "c", "d", "e", "f", "g"]);
        let main = "\\input{0-preamble}\n% \\input{2-old}\n\\input{3-intro}\n\\input{4-review}\n";
        let files: BTreeMap<String, String> = [("3-intro".to_string(), "\\cite{b}\\cite{a}".to_string()), ("4-review".to_string(), "\\cite{a}\\cite{c}".to_string())].into();
        assert_eq!(inputs(main), ["0-preamble", "3-intro", "4-review"]);
        assert_eq!(citation_order(main, &files), ["b", "a", "c"]);
        // a chapter left out of the PDF cites nothing
        let mut out = files.clone();
        out.insert("3-intro".into(), "\\endinput% left out\n\\cite{b}\\cite{a}".into());
        assert_eq!(citation_order(main, &out), ["a", "c"]);
        assert_eq!(read_part("% \\endinput\nText\n\\endinput\nMore"), "% \\endinput\nText\n\\endinput\n");
        assert_eq!(read_part("No end\n"), "No end\n");
        let k =|v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(cite_change(&k(&["a", "b"]), &k(&["a", "b"])), CiteChange::Same);
        assert_eq!(cite_change(&k(&["a", "b"]), &k(&["a", "b", "c"])), CiteChange::Appended);
        assert_eq!(cite_change(&k(&["a", "b", "c"]), &k(&["a", "b"])), CiteChange::Appended);
        assert_eq!(cite_change(&k(&["a", "b"]), &k(&["c", "a", "b"])), CiteChange::Renumbered);
        assert_eq!(cite_change(&k(&["a", "b", "c"]), &k(&["a", "c"])), CiteChange::Renumbered);
        assert_eq!(cite_change(&k(&["a", "b"]), &k(&["b", "a"])), CiteChange::Renumbered);
    }

    #[test]
    fn bcf() {
        let bcf = "<bcf:section number=\"0\">\n    <bcf:citekey order=\"1\" intorder=\"1\">a</bcf:citekey>\n    <bcf:citekey order=\"2\" intorder=\"1\">b</bcf:citekey>\n</bcf:section>\n";
        assert_eq!(bcf_keys(bcf), ["a", "b"]);
        let new = bcf_with_keys(bcf, &["c".to_string(), "a".to_string(), "b".to_string()]).unwrap();
        assert_eq!(new, "<bcf:section number=\"0\">\n    <bcf:citekey order=\"1\" intorder=\"1\">c</bcf:citekey>\n    <bcf:citekey order=\"2\" intorder=\"1\">a</bcf:citekey>\n    <bcf:citekey order=\"3\" intorder=\"1\">b</bcf:citekey>\n</bcf:section>\n");
        assert_eq!(bcf_keys(&new), ["c", "a", "b"]);
    }

    #[test]
    fn labels_and_refs() {
        let tex = "\\section{A}\\label{sec:a}\nSee Section~\\ref{sec:b} and \\eqref{eq:x}, \\cref{a,b}. % \\ref{no}\n";
        assert_eq!(label_defs(tex).into_iter().collect::<Vec<_>>(), ["sec:a"]);
        assert_eq!(label_refs(tex).into_iter().collect::<Vec<_>>(), ["a", "b", "eq:x", "sec:b"]);
        let aux = "\\relax\n\\newlabel{sec:a}{{1}{3}{A}{section.1}{}}\n\\newlabel{x}{{2}{4}}\n";
        assert_eq!(labels(aux)["sec:a"], "{1}{3}{A}{section.1}{}");
    }

    #[test]
    fn glossary() {
        let g = "\\term{FNRATE}{FNRATE}{Attack Presentation Classification Error Rate}{The share {of} attacks accepted.}\n\\term[known good presentation]{knowngood}{Known Good Presentation}{}{A presentation.}\n";
        let e = glossary_entries(g, "term");
        assert_eq!(e.len(), 2);
        assert_eq!(e["FNRATE"].description, "The share {of} attacks accepted.");
        assert_eq!(e["knowngood"].text.as_deref(), Some("known good presentation"));
        let g2 = g.replace("The share {of} attacks", "The share of intrusion attempts").replace("[known good presentation]", "[genuine presentation]") + "\\term{W1}{$W_1$}{Wasserstein}{A distance.}\n";
        let c = glossary_change(&e, &glossary_entries(&g2, "term"));
        assert_eq!(c, GlossaryChange { printed: vec!["knowngood".into()], described: vec!["FNRATE".into()], added: vec!["W1".into()], removed: vec![] });
        assert_eq!(gls_keys("\\gls{FNRATE} and \\Glspl{NIT}, \\glslink{knowngood}{x}"), ["FNRATE", "NIT", "knowngood"].iter().map(|s| s.to_string()).collect());
        let glo = "\\glossaryentry{FNRATE?\\glossentry{FNRATE}|setentrycounter[]{page}\"\\glsnumberformat}{2}\n\\glossaryentry{NIT?\\glossentry{NIT}|setentrycounter[]{page}\"\\glsnumberformat}{5}\n";
        assert_eq!(glo_keys(glo).len(), 2);
        let used: BTreeSet<String> = ["FNRATE".to_string()].into();
        let pages: BTreeSet<String> = ["5".to_string()].into();
        let merged = glo_merge(glo, "\\glossaryentry{W1?\\glossentry{W1}|setentrycounter[]{page}\"\\glsnumberformat}{5}\n", &used, &pages);
        assert_eq!(merged.lines().count(), 2, "{merged}");
        assert!(merged.contains("{FNRATE}") && merged.contains("{W1}") && !merged.contains("{NIT}"));
        let aux = "\\relax\n\\@newglossary{main}{glg}{gls}{glo}\n\\@istfilename{main.ist}\n\\@glsorder{word}\n\\newlabel{x}{y}\n";
        assert_eq!(gls_aux(aux, "fp-gls"), "\\@newglossary{main}{glg}{gls}{glo}\n\\@istfilename{fp-gls.ist}\n\\@glsorder{word}\n");
    }

    #[test]
    fn lists() {
        let toc = "\\contentsline {section}{\\numberline {1}Introduction}{1}{section.1}%\n\\contentsline {subsection}{\\numberline {1.1}Aims}{2}{subsection.1.1}%\n";
        let aux = "\\relax\n\\@writefile{toc}{\\contentsline {subsection}{\\numberline {1.1}Aims and Scope}{2}{subsection.1.1}\\protected@file@percent }\n\\@writefile{lof}{\\contentsline {figure}{x}{2}{figure.1}}\n";
        assert!(list_changed(toc, aux, "toc"));
        let m = list_merge(toc, aux, "toc").unwrap();
        assert!(m.contains("Aims and Scope") && m.contains("Introduction") && m.lines().count() == 2);
        assert_eq!(list_merge(toc, aux, "lof"), None, "figure.1 is not in the contents: added");
        assert!(!list_changed(toc, "\\@writefile{toc}{\\contentsline {section}{\\numberline {1}Introduction}{1}{section.1}}\n", "toc"));
    }

    #[test]
    fn changes() {
        assert_eq!(changed_lines("a\nb\nc\nd\n", "a\nb\nX\nd\n"), Some((3, 3)));
        assert_eq!(changed_lines("a\nb\n", "a\nb\nc\n"), Some((3, 3)));
        let span = changed_span("a\nb\nc\nd\n", "a\nX\nY\nZ\nc\nd\n").unwrap();
        assert_eq!(span, (2, 2, 4));
        assert_eq!((new_line(1, span), new_line(3, span), new_line(4, span)), (1, 5, 6), "after the block: two lines later");
        // lines only added: the line at the insertion moves down with what follows
        let span = changed_span("a\nb\nc\n", "a\nX\nY\nb\nc\n").unwrap();
        assert_eq!(span, (2, 1, 3));
        assert_eq!((new_line(1, span), new_line(2, span), new_line(3, span)), (1, 4, 5));
        assert_eq!(changed_lines("a\nb\nc\n", "a\nc\n"), Some((2, 2)), "a line removed: the place where it was");
        assert_eq!(changed_lines("a\nb\nc\n", "a\nc\n"), Some((2, 2)));
        assert_eq!(changed_lines("a\n", "a\n"), None);
        let tex = "\\section{A}\n\\label{sec:a}\nText.\n\\begin{figure}\n\\caption{C}\n\\end{figure}\n";
        assert_eq!(lists_touched(tex, 1, 1, tex).into_iter().collect::<Vec<_>>(), ["toc"]);
        assert_eq!(lists_touched(tex, 5, 5, tex).into_iter().collect::<Vec<_>>(), ["lof"]);
        assert!(lists_touched(tex, 3, 3, tex).is_empty());
    }
}
