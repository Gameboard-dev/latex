//! Each rule on a line or two, with the author's settings (`Options::default()`) unless a test changes one. Equivalence
//! with latex_sync.py over many more cases is tests/python.rs.

use super::*;

const PRE: &str = "---\ntex: 0-preamble.tex\n---\n```latex\n\\newcommand{\\printterms}{x}\n```\n";

/// The chapter a note gives (without its two generated lines), and its warnings.
fn conv_with(note: &str, md: &str, o: &Options, others: &[(&str, &str)]) -> (Vec<String>, Vec<String>) {
    let notes: Vec<(&str, &str)> = [("0-preamble.md", PRE), (note, md)].into_iter().chain(others.iter().copied()).collect();
    let idx = Index::new(notes, o);
    let (tex, warn) = md_to_tex(note, md, &idx, o).unwrap();
    (tex.split('\n').skip(2).filter(|l| !l.is_empty()).map(String::from).collect(), warn)
}

fn conv(md: &str) -> Vec<String> {
    let (lines, warn) = conv_with("5-methodology.md", md, &Options::default(), &[]);
    assert!(warn.is_empty(), "{warn:?}");
    lines
}

fn one(line: &str) -> String {
    conv(&format!("{line}\n")).join("\n")
}

fn warnings(md: &str) -> Vec<String> {
    conv_with("5-methodology.md", md, &Options::default(), &[]).1
}

#[test]
fn labels_and_block_ids() {
    assert_eq!(block_id("sec:lit:compgen").unwrap(), "sec--lit--compgen");
    assert_eq!(label_of("tab--three---class").unwrap(), "tab:three_class");
    assert_eq!(label_of(&block_id("app:capture-eqs").unwrap()).unwrap(), "app:capture-eqs");
    assert!(label_of("a----b").is_err() && block_id("a::b").is_err());
}

#[test]
fn header_and_root() {
    let o = Options::default();
    let idx = Index::new([], &o);
    let tex = md_to_tex("3-introduction.md", "---\nversion: 2\nlast_updated: 2026-10-25 10:00\n---\nText.\n", &idx, &o).unwrap().0;
    let lines: Vec<&str> = tex.split('\n').collect();
    assert_eq!(lines[0], "% Generated from Vault/LaTeX/3-introduction.md (version 2, last updated 2026-10-25 10:00). Edit the note: this file is rewritten from it on every save.");
    assert_eq!(lines[1], "% !TEX root = main.tex");
    let o = Options { header: "% From {note} in {vault} ({version}, {updated})".into(), main: "thesis".into(), ..Options::default() };
    let tex = md_to_tex("a.md", "Text.\n", &idx, &o).unwrap().0;
    assert!(tex.starts_with("% From a.md in Vault (?, ?)\n% !TEX root = thesis.tex\n"), "{tex}");
}

#[test]
fn comments_and_dividers() {
    assert_eq!(one("%% a comment line%%"), "% a comment line");
    assert_eq!(one("%%[note] for Obsidian only%%"), "");
    assert_eq!(one("text %%c%%"), "text %c");
    assert_eq!(conv("___\n%% First line\nsecond\n\nfourth%%\n"), ["% ".to_string() + &"=".repeat(69), "% First line".into(), "% second".into(), "%".into(), "% fourth".into()]);
    assert_eq!(one("as error [[EV-0379]] %%[ev] R-770559a0%%. Next"), "as error. Next");
}

#[test]
fn left_out() {
    assert_eq!(one("rather than an average [[EV-0215]], [[EV-0308]], and it calls"), "rather than an average, and it calls");
    assert_eq!(one("a cite [@k1, p. 2] [❝](zotero://open-pdf/library/items/A?annotation=B) here"), "a cite \\cite[p.~2]{k1} here");
    assert_eq!(conv("> [!info]- Notes for the author\n> not in LaTeX\nAfter.\n"), ["After."]);
    assert_eq!(conv("> quoted\n"), ["\\begin{quote}", "quoted", "\\end{quote}"]);
}

#[test]
fn citations() {
    assert_eq!(one("[@a1; @b2]"), r"\cite{a1, b2}");
    assert_eq!(one("[@a1;@b2]"), r"\cite{a1,b2}");
    assert_eq!(one("[@a; @b]"), "[@a; @b]"); // a key has two characters at least
    assert_eq!(one("[see @a2020, p. 2]"), r"\cite[see][p.~2]{a2020}");
    assert_eq!(one("@a2020 found it. @b2021 did not."), r"\Citet{a2020} found it. \Citet{b2021} did not.");
    assert_eq!(one("as Smith et al. @b2021 say"), r"as Smith et al. \citet{b2021} say");
    assert_eq!(one("so @k1 [pp. 4-5] says"), r"so \citet[pp.~4-5]{k1} says");
    assert_eq!(conv_with("x.md", "x [@a1, p. 2; @b2, p. 3]\n", &Options::default(), &[]).0, [r"x \cites[p.~2]{a1}[p.~3]{b2}"]);
    assert!(warnings("[@a1, p. 2; @b2]\n")[0].ends_with(r"written as \cites"));
    assert_eq!(one("mail a@b.org"), "mail a@b.org");
}

#[test]
fn references() {
    let md = "# Research Design\n^sec--design\n\nSee Section [[#Research Design|Research Design]], Table [[#^tab--x|tab:x]] and [[#^sec--design|any]].\n";
    assert_eq!(conv(md)[2], r"See Section~\ref{sec:design}, Table~\ref{tab:x} and \ref{sec:design}.");
    assert_eq!(one("as [[3-introduction#^hyp2a|H2a]] and [[#^rq1|RQ1]] and [[#^rq|RQ3]] ask"), r"as \hypref{2a} and \rqref{1} and \rqref{3} ask");
    // the reference words and the labelled lists are settings
    let o = Options { ref_words: words("Abschnitt"), lists: parse_lists("fr frage F"), ..Options::default() };
    let (l, _) = conv_with("x.md", "Siehe Abschnitt [[#^sec--a]] und Section [[#^sec--a]], [[#^fr2|F2]].\n", &o, &[]);
    assert_eq!(l, [r"Siehe Abschnitt~\ref{sec:a} und Section \ref{sec:a}, \frage{2}."]);
    assert!(warnings("[[#Nope|x]]\n")[0].contains("no labelled heading 'Nope' in this note"));
}

#[test]
fn inline_markup() {
    assert_eq!(one("*a* _b_ **c** `d_e{}` ^[f *g*]"), r"\emph{a} \textit{b} \textbf{c} \texttt{d\_e\{\}} \footnote{f \emph{g}}");
    assert_eq!(one("snake_case_word and $a_1 * b_2$"), "snake_case_word and $a_1 * b_2$");
    assert_eq!(one("<span style=\"color:red\">x</span>"), r"\textcolor{red}{x}");
    assert_eq!(one("“q” ‘s’ it’s"), "``q'' `s' it's");
    assert_eq!(one("[site](https://example.org/#top)"), r"\href{https://example.org/#top}{site}");
    assert!(warnings("a ==mark==\n")[0].contains("highlight or strike-through"));
}

#[test]
fn unclosed_markdown_link_is_text() {
    // latex_sync.py never returns from this line (its index goes back to 0); here it is text, with a warning
    let (l, w) = conv_with("x.md", "[text](no closing parenthesis\n", &Options::default(), &[]);
    assert_eq!(l, ["[text](no closing parenthesis"]);
    assert!(w[0].contains("no closing parenthesis"));
}

#[test]
fn headings_and_word_counts() {
    let md = "---\nwordcount: true\n---\n# Research Philosophy\n^sec--philosophy\n\nText.\n## Sub {-}\n#### Aim.\nThe aim.\n";
    assert_eq!(conv(md), [
        r"\startwordcount{philosophy}", r"\section{Research Philosophy}", r"\label{sec:philosophy}", r"\printwordcount{philosophy}", "Text.",
        r"\subsection*{Sub}", r"\phantomsection\addcontentsline{toc}{subsection}{Sub}", r"\paragraph{Aim.} The aim.", r"\stopwordcount",
    ]);
    let o = Options { wordcount: "counted".into(), count_start: "wcstart".into(), count_print: "wcprint".into(), count_stop: "wcstop".into(), ..Options::default() };
    let (l, _) = conv_with("x.md", "---\ncounted: true\n---\n# A\n^sec--a\n", &o, &[]);
    assert_eq!(l, [r"\wcstart{a}", r"\section{A}", r"\label{sec:a}", r"\wcprint{a}", r"\wcstop"]);
}

#[test]
fn commands_lists_and_ids() {
    assert_eq!(conv("^FloatBarrier\n^printterms\n^other\n"), [r"\FloatBarrier", r"\printterms"]);
    assert_eq!(conv("- a\n- b ^sec--b\n1. c\n"), [r"\begin{itemize}", r"\item a", r"\item \phantomsection\label{sec:b}b", r"\end{itemize}", r"\begin{enumerate}", r"\item c", r"\end{enumerate}"]);
    assert_eq!(conv("- **H2a** _Null:_ no. ^hyp2a\n\n^hyp\n"), [r"\begin{enumerate}", r"  \item[\textbf{H2a}] \hypertarget{hyp2a}{}\textit{Null:} no.", r"\end{enumerate}"]);
    assert_eq!(conv("A paragraph. ^sec--para\n"), [r"\phantomsection\label{sec:para}", "A paragraph."]);
}

#[test]
fn equations() {
    assert_eq!(conv("$$\ny = x\n$$\n\n^eq--model\n\n$$ z $$\n"), [r"\begin{equation}", "y = x", r"  \label{eq:model}", r"\end{equation}", r"\[", "z", r"\]"]);
}

#[test]
fn tables() {
    let md = "| A | B |\n| --- | --: |\n| 1 | 2 |\n\n^tab--t\n\n((<tag|Table)) Cap.\n";
    assert_eq!(conv(md), [r"\begin{table}[htbp]", r"\centering", r"\caption{Cap.}", r"\label{tab:t}", r"\small", r"\begin{tabular}{@{}lr@{}}", r"\toprule", r"A & B \\", r"\midrule", r"1 & 2 \\", r"\bottomrule", r"\end{tabular}", r"\end{table}"]);
    let long = format!("| {} | B |\n| --- | --- |\n\n^tab--t\n\n((<tag|Table)) Cap.\n", "x".repeat(90));
    assert!(conv(&long).contains(&r"\begin{tabularx}{\linewidth}{@{}Yl@{}}".to_string()));
    let o = Options { line_chars: 200, ..Options::default() };
    assert!(conv_with("x.md", &long, &o, &[]).0.contains(&r"\begin{tabular}{@{}ll@{}}".to_string()));
    assert!(conv(&format!("%%[cols] p{{3cm}}c%%\n\n{md}")).contains(&r"\begin{tabular}{@{}p{3cm}c@{}}".to_string()));
    assert!(warnings("| a |\n| --- |\n")[0].contains("bare tabular"));
}

#[test]
fn figures_and_listings() {
    let md = "```tikz\n\\usetikzlibrary{calc}\n\\begin{document}\n\\begin{tikzpicture}\n\\end{tikzpicture}\n\\end{document}\n```\n\n^fig--f\n\n((<tag|Figure)) Cap.\n";
    assert_eq!(conv(md), [r"\begin{figure}[htbp]", r"\centering", &format!("\\resizebox{{{FIT}}}{{!}}{{%"), r"\begin{tikzpicture}", r"\end{tikzpicture}", "}", r"\caption{Cap.}", r"\label{fig:f}", r"\end{figure}"]);
    assert!(warnings(&md.replace("{calc}", "{calc,arrows.meta}"))[0].contains("only for TikZJax"));
    let o = Options { tikz_libraries: "calc, arrows.meta".into(), ..Options::default() };
    assert!(conv_with("x.md", &md.replace("{calc}", "{calc,arrows.meta}"), &o, &[]).1.is_empty());
    let o = Options { listing_env: "listing".into(), listing_cmd: "inputcode".into(), ..Options::default() };
    let (l, _) = conv_with("x.md", "Listing: Core. ^lst--bench\n", &o, &[]);
    assert_eq!(l, [r"\begin{listing}[htbp]", r"\inputcode{bench}", r"\caption{Core.}", r"\label{lst:bench}", r"\end{listing}"]);
}

#[test]
fn title_page() {
    let md = "---\ntex: 1-title_page.tex\nversion: 1\ntitle: \"A: b\"\nstudent_id: 7\ntags:\n  - x\n---\n";
    let (l, w) = conv_with("1-title_page.md", md, &Options::default(), &[]);
    assert_eq!(l, [r"\newcommand{\tpTitle}{A: b}", r"\newcommand{\tpStudentId}{7}", r"\maketitlepage"]);
    // a YAML list's item is not a field (latex_sync.py would write \newcommand{\tp- x}{})
    assert!(w[0].contains("cannot name a LaTeX command"), "{w:?}");
    let o = Options { title_page: "cover".into(), title_prefix: "cv".into(), make_title: "makecover".into(), ..Options::default() };
    assert_eq!(conv_with("cover.md", "---\nname: N\n---\n", &o, &[]).0, [r"\newcommand{\cvName}{N}", r"\makecover"]);
}

#[test]
fn glossary() {
    let g = "# FNRATE\n**Long form:** attack presentation classification error rate\n\nAttacks accepted.\n\n# Known Good Presentation\n^known-good\n\nNo attack.\n";
    let (l, _) = conv_with("glossary.md", g, &Options::default(), &[]);
    assert_eq!(l, [r"\term{FNRATE}{FNRATE}{Attack Presentation Classification Error Rate}{Attacks accepted.}", r"\term[known good presentation]{knowngood}{Known Good Presentation}{}{No attack.}"]);
    let (l, w) = conv_with("x.md", "[[glossary#FNRATE|FNRATEs]], [[glossary#^known-good|Known good presentation]], [[glossary#FNRATE|the rate]], [[glossary#APCRR|APCRR]]\n", &Options::default(), &[("glossary.md", g)]);
    assert_eq!(l, [r"\glspl{FNRATE}, \Gls{knowngood}, \glslink{FNRATE}{the rate}, \gls{APCRR}"]);
    assert!(w[0].contains("no entry 'APCRR' in glossary.md"));
    let o = Options { glossary: "terms".into(), term: "entry".into(), ..Options::default() };
    assert_eq!(conv_with("terms.md", "# AUC\nArea.\n", &o, &[]).0, [r"\entry{AUC}{AUC}{}{Area.}"]);
    assert_eq!(title_case("area under the curve of x-ray $y$", &Options::default().small_words), "Area Under the Curve of X-Ray $y$");
}

#[test]
fn line_endings_and_errors() {
    assert_eq!(conv("---\ntex: x\n---\r\nA.\r\nB.\r\n"), ["A.", "B."]);
    let o = Options::default();
    let idx = Index::new([], &o);
    assert!(md_to_tex("x.md", "---\ntex: x\n", &idx, &o).is_err());
    assert!(md_to_tex("x.md", "# H\n^a----b\n", &idx, &o).is_err());
}

/// Each note line to the .tex line where its part begins: (note line, the text of that .tex line).
fn mapped(note: &str, md: &str) -> Vec<(usize, String)> {
    let o = Options::default();
    let idx = Index::new([("0-preamble.md", PRE), (note, md)], &o);
    let (tex, _, map) = md_to_tex_mapped(note, md, &idx, &o).unwrap();
    assert_eq!(tex, md_to_tex(note, md, &idx, &o).unwrap().0, "the LaTeX is md_to_tex's");
    let lines: Vec<&str> = tex.split('\n').collect();
    assert_eq!(map.tex.len(), md.split('\n').count() - usize::from(md.ends_with('\n')), "one entry per note line");
    let empty: Vec<u32> = (1..=lines.len() as u32 - 1).filter(|&t| lines[t as usize - 1].trim().is_empty() || lines[t as usize - 1].starts_with('%')).collect();
    assert_eq!(map.empty, empty);
    map.tex.iter().enumerate().map(|(k, &t)| (k + 1, lines[t as usize - 1].to_string())).collect()
}

#[test]
fn line_map() {
    let md = "---\nversion: 1\nwordcount: true\n---\n# Intro\n^sec--intro\n\nA paragraph.\n> [!info]- Notes\n> left out\n%%[note] left out%%\n- one\n- two\n1. three\n\n$$\ny = x\n$$\n\n^eq--m\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n^tab--t\n\n((<tag|Table)) Cap.\n%% a\nb %%\n> quoted\n> twice\n- **H1** Yes. ^hyp1\n- **H2** No. ^hyp2\n```tikz\n\\begin{tikzpicture}\n\\end{tikzpicture}\n```\n\n^fig--f\n\n((<tag|Figure)) Fig.\n## Sub {-}\nLast. ^sec--last\n";
    let m = mapped("3-introduction.md", md);
    let want: &[(usize, &str)] = &[
        (1, "% Generated from"), (4, "% Generated from"), // the properties: the first line
        (5, r"\startwordcount{intro}"), (6, r"\startwordcount{intro}"), // a counted heading and its label
        (7, ""), (8, "A paragraph."),
        (9, r"\begin{itemize}"), (11, r"\begin{itemize}"), // a callout and a tagged comment: where the next output goes
        (12, r"\item one"), (13, r"\item two"), (14, r"\item three"),
        (16, r"\begin{equation}"), (18, r"\begin{equation}"), (20, r"\begin{table}[htbp]"), // the block; its id gives nothing
        (22, r"A & B \\"), (23, r"\midrule"), (24, r"1 & 2 \\"), // each row to its row
        (26, r"\begin{table}[htbp]"), (28, r"\begin{table}[htbp]"), // the id and caption to the float
        (29, "% a"), (30, "% a"), // a comment block
        (31, "quoted"), (32, "twice"),
        (33, r"  \item[\textbf{H1}]"), (34, r"  \item[\textbf{H2}]"),
        (35, r"\begin{figure}[htbp]"), (42, r"\begin{figure}[htbp]"), // the block, its id and its caption
        (43, r"\subsection*{Sub}"), (44, r"\phantomsection\label{sec:last}"),
    ];
    for (line, text) in want {
        assert!(m[line - 1].1.starts_with(text), "note line {line}: {:?}, not {text:?}", m[line - 1].1);
    }
    // the starred heading's two lines are one entry of the output: the paragraph after it is two .tex lines on
    let o = Options::default();
    let idx = Index::new([], &o);
    let (tex, _, map) = md_to_tex_mapped("x.md", "## Sub {-}\nLast.\n", &idx, &o).unwrap();
    assert_eq!(tex.split('\n').nth(map.tex[1] as usize - 1), Some("Last."));
    // the glossary: every line is the chapter's third (its entries are printed where \printterms is)
    let (_, _, map) = md_to_tex_mapped("glossary.md", "# AUC\nArea.\n", &idx, &o).unwrap();
    assert_eq!(map.tex, [3, 3]);
}

#[test]
fn line_map_parts() {
    let map = LineMap { tex: vec![1, 1, 3, 4, 4, 9, 6, 10], empty: vec![] };
    assert_eq!(map.part(1), Some((1, 2, 1)));
    assert_eq!(map.part(5), Some((4, 5, 4)));
    assert_eq!(map.part(99), Some((8, 8, 10)));
    // a table's caption line after its rows: .tex line 7 belongs to the part that begins at 6, not to the one at 9
    // that comes before it in the note
    assert_eq!(map.part_of_tex(7), Some((7, 7, 6)));
    assert_eq!(map.part_of_tex(4), Some((4, 5, 4)));
    assert_eq!(LineMap::default().part(1), None);
}

#[test]
fn code_blocks_are_listings_with_their_code_in_the_code_folder() {
    let o = Options::default();
    let md = "Text.\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\n^lst--demo\n\n((<tag|Listing)) A small program.\n\n```python\nprint(1)\n```\n\n```text\nplain\n```\n";
    let notes = [("0-preamble.md", PRE), ("5-methodology.md", md)];
    let idx = Index::new(notes, &o);
    let (tex, warn, _, code) = md_to_tex_code("5-methodology.md", md, &idx, &o).unwrap();
    assert!(tex.contains("\\begin{handtyped}[htbp]\n\\handtypedcode{demo}\n\\caption{A small program.}\n\\label{lst:demo}\n\\end{handtyped}"), "{tex}");
    assert!(tex.contains("\\handtypedcode{5-methodology-2}"), "an uncaptioned block: set, not numbered");
    assert!(tex.contains("\\begin{verbatim}\nplain\n\\end{verbatim}"), "plain text stays verbatim");
    assert_eq!(code, vec![
        CodeFile { name: "demo.rs".into(), lang: "rust".into(), text: "fn main() {\n    println!(\"hi\");\n}\n".into() },
        CodeFile { name: "5-methodology-2.py".into(), lang: "python".into(), text: "print(1)\n".into() },
    ]);
    assert_eq!(warn.len(), 2, "the uncaptioned block and the verbatim one: {warn:?}");
    assert!(warn[0].contains("without a caption"));
    // the older one-line caption works after a block too
    let md = "```rust\nlet x = 1;\n```\n\nListing: One line. ^lst--one\n";
    let idx = Index::new([("0-preamble.md", PRE), ("x.md", md)], &o);
    let (tex, warn, _, code) = md_to_tex_code("x.md", md, &idx, &o).unwrap();
    assert!(tex.contains("\\handtypedcode{one}\n\\caption{One line.}\n\\label{lst:one}") && warn.is_empty() && code[0].name == "one.rs", "{tex} {warn:?}");
}
