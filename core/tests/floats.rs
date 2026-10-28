//! Since 2026-10-27 (the author's rule): captions and table cells count; code (verbatim blocks, listings) and TikZ
//! pictures do not, unless a rule says `tikzpicture labels`, which counts the text of a picture's nodes and labels.

use latex_core::wordcount::{count_chapter, tikz_labels, Context, Glossary, Macros, Names, Rules};

#[test]
fn floats_and_code() {
    let tex = r"\startwordcount{x}
Text here.
\begin{figure}[htbp]
\centering
\resizebox{\ifdim\width>\linewidth\linewidth\else\width\fi}{!}{%
\begin{tikzpicture}[font=\small,node distance=0.55cm, every node/.style={draw}]
  \node[st] (a) at (0,0) {load IDSNet\\now};
  \draw (a) -- node[above] {two words} (b);
  \node[label=above:{lab el}] (c) {};
\end{tikzpicture}}
\caption{A caption.}
\label{fig:x}
\end{figure}
\begin{table}[htbp]
\centering
\caption{Cap two.}
\small
\begin{tabularx}{\linewidth}{@{}YY@{}}
\toprule
A & B \\
\midrule
one two&three \\
\bottomrule
\end{tabularx}
\end{table}
\begin{verbatim}
code words here
\end{verbatim}
\stopwordcount
";
    let rules = Rules::default();
    let c = count_chapter(tex, &Context { rules: &rules, names: &Names::default(), glossary: &Glossary::default() });
    // text 2, picture 0, captions 2 + 2, cells 5, code 0
    assert_eq!(c.sections, vec![("x".to_string(), 11)], "{:?}", c.warnings);
    assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    // with `tikzpicture labels`, the picture's labels count too: 3 + 2 + 2
    let rules = Rules::new("tikzpicture labels", &Macros::default()).0;
    let c = count_chapter(tex, &Context { rules: &rules, names: &Names::default(), glossary: &Glossary::default() });
    assert_eq!(c.sections, vec![("x".to_string(), 18)], "{:?}", c.warnings);
}

#[test]
fn labels() {
    let body = r"[node distance=1cm, every node/.style={draw}, b/.style={text width=2cm}]
  \node[b] (p) at (0,0) {first};
  \node (q) [right=of p] {second};
  \draw (p) -- node[midway, above] {third} (q);
  \node[label={[red]below:fourth}] {};
  % \node {not this}
  \foreach \x in {1,2} \fill (\x,0) circle (1pt);";
    let got: Vec<&str> = tikz_labels(body).into_iter().map(|(a, b)| &body[a..b]).collect();
    assert_eq!(got, ["first", "second", "third", "fourth", ""]);
}
