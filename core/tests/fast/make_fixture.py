"""Writes the fast-preview correctness fixture (tests/fast/fixture/): a small document in the plugin's notes that hits
every case the fast preview handles. Deterministic: the prose is made from a fixed vocabulary with a seeded generator,
so running it again gives the same files. Run it only to change the fixture: python make_fixture.py

The document: numeric biblatex in order of citation (sorting=none) with 24 sources; a glossary printed by \\printterms
with page lists; an appendix with labelled items (hypotheses, paragraph labels) and sub-appendices; references from the
body to the appendix and back; figures and tables at section boundaries; footnotes just before section starts; a
contents, a list of figures and a list of tables."""
import os
import random

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "fixture")
rng = random.Random(20261025)

NOUNS = ["model", "estimator", "dataset", "threshold", "instrument", "sensor", "protocol", "baseline", "signal", "sample",
         "error rate", "pipeline", "measure", "condition", "subject", "device", "network", "layer", "score", "trial"]
VERBS = ["reduces", "raises", "changes", "predicts", "limits", "explains", "follows", "matches", "shifts", "bounds"]
ADJS = ["aggregated", "small", "robust", "noisy", "stable", "held-out", "batched", "cached", "calibrated", "local"]
ADVS = ["clearly", "slightly", "often", "rarely", "under load", "in practice", "on average", "at first"]


def sentence():
    s = f"The {rng.choice(ADJS)} {rng.choice(NOUNS)} {rng.choice(VERBS)} the {rng.choice(NOUNS)} {rng.choice(ADVS)}"
    if rng.random() < 0.4:
        s += f", while the {rng.choice(ADJS)} {rng.choice(NOUNS)} {rng.choice(VERBS)} it"
    return s + "."


def para(n=5):
    return " ".join(sentence() for _ in range(n))


KEYS = [f"src{k:02d}" for k in range(1, 25)]
SURNAMES = ["Abbott", "Baker", "Chen", "Dubois", "Evans", "Fischer", "Garcia", "Hughes", "Ito", "Jensen", "Kowalski",
            "Lindqvist", "Moreau", "Nakamura", "Okafor", "Petrov", "Quinn", "Rossi", "Silva", "Tanaka", "Ueda",
            "Varga", "Weber", "Young"]


def cite(*ks):
    return "[" + "; ".join("@" + k for k in ks) + "]"


def write(name, text):
    path = os.path.join(OUT, name)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


PREAMBLE = r"""```latex
\usepackage[margin=1in]{geometry}
\usepackage[T1]{fontenc}
\usepackage{lmodern}
\usepackage{microtype}
\usepackage{amsmath}
\usepackage{booktabs}
\usepackage{tabularx}
\usepackage{xcolor}
\usepackage{tikz}
\usetikzlibrary{positioning,calc,decorations.pathreplacing}
\usepackage{placeins}
\newcolumntype{Y}{>{\raggedright\arraybackslash}X}

% Word counts: the plugin writes wordcounts.tex.
\makeatletter
\newcommand{\startwordcount}[1]{\par}
\newcommand{\stopwordcount}{\par}
\newcommand{\wordcountoff}{}
\def\wc@total{0}
\newcommand{\wc@set}[2]{%
  \expandafter\xdef\csname wc@val@#1\endcsname{#2}%
  \xdef\wc@total{\the\numexpr\wc@total+#2\relax}}
\InputIfFileExists{wordcounts.tex}{}{%
  \PackageWarningNoLine{preamble}{wordcounts.tex not found, so the word
    counts print as ??: convert the chapters (LaTeX plugin: Build all
    chapters)}}
\newcommand{\wordcount}[1]{\ifcsname wc@val@#1\endcsname\csname wc@val@#1\endcsname\else\textbf{??}\fi}
\newcommand{\printwordcount}[1]{{\noindent\small\textit{Word count: \wordcount{#1}}\par}}
\makeatother

\usepackage[style=numeric,sorting=none,backend=biber,natbib=true,maxcitenames=2]{biblatex}
\addbibresource{fixture.bib}
\usepackage[colorlinks=true,allcolors=blue,citecolor=black]{hyperref}
\usepackage[nopostdot,nogroupskip,docdef=true,undefaction=warn]{glossaries-extra}
\NewDocumentCommand{\term}{o m m m m}{%
  \IfNoValueTF{#1}{\termentry{#2}{#3}{#3}{#4}{#5}}{\termentry{#2}{#3}{#1}{#4}{#5}}}
\newcommand{\termentry}[5]{%
  \ifblank{#4}
    {\newglossaryentry{#1}{name={#2}, text={#3}, sort={#1}, description={#5}}}
    {\newglossaryentry{#1}{name={#2 (#4)}, text={#3}, sort={#1}, description={#5}}}}
\makeglossaries

\newcommand{\rqref}[1]{\hyperlink{rq#1}{RQ#1}}
\newcommand{\hypref}[1]{\hyperlink{hyp#1}{H#1}}
\newcommand{\startappendices}{%
  \setcounter{subsection}{0}%
  \renewcommand{\thesubsection}{\Alph{subsection}}%
  \renewcommand{\theHsubsection}{appendix.\Alph{subsection}}}
\newcommand{\subappendix}{%
  \addtocounter{subsection}{-1}%
  \renewcommand{\thesubsection}{\Alph{subsection}2}%
  \renewcommand{\theHsubsection}{appendix.\Alph{subsection}2}}
\newcommand{\closesubappendix}{%
  \renewcommand{\thesubsection}{\Alph{subsection}}%
  \renewcommand{\theHsubsection}{appendix.\Alph{subsection}}}
\newcommand{\printterms}{%
  {\renewcommand*{\glossarysection}[2][]{}%
   \printglossary[type=main,style=altlist,nonumberlist=false]}}

\setlength{\parindent}{0pt}
\setlength{\parskip}{6pt plus 2pt minus 1pt}
```
"""

MAIN = r"""% The fast-preview fixture's main file (hand-written, as the author's main.tex is).
\documentclass[11pt,a4paper]{article}

\input{0-preamble}
\input{glossary}

\begin{document}

\pagenumbering{roman}
\input{2-summary}
\clearpage

\pdfbookmark[1]{Contents}{toc}
\begingroup
\hypersetup{linkcolor=black}
\tableofcontents
\endgroup
\clearpage

\listoffigures

\listoftables
\clearpage

\pagenumbering{arabic}
\input{3-introduction}
\input{4-review}
\input{5-method}
\input{9-appendix}

\printbibliography
\end{document}
"""

GLOSSARY = """# FNRATE
**Long form:** Signal Connection Classification Error Rate

The share of intrusion attempts accepted as known good.

# FPRATE
**Long form:** Known Good Connection Classification Error Rate

The share of known good connections rejected as signals.

# Known Good Connection
^known-good

A connection made in the intended manner, with no attempt to deceive.

# NIT
**Long form:** Network Intrusion Instrument

The object used in an signal, such as a printed photograph.

# Domain Shift
^shift

A change between the conditions of training and of use.

# EER
**Long form:** Equal Error Rate

The error rate at the threshold where the two error rates are equal.
"""


def gl(key, alias=None):
    return f"[[glossary#{key}|{alias or key}]]"


def tikz(label):
    return f"""```tikz
\\usetikzlibrary{{positioning}}
\\begin{{document}}
\\begin{{tikzpicture}}[node distance=2cm]
\\node[draw] (a) {{{label} input}};
\\node[draw, right=of a] (b) {{model}};
\\node[draw, right=of b] (c) {{score}};
\\draw[->] (a) -- (b);
\\draw[->] (b) -- (c);
\\end{{tikzpicture}}
\\end{{document}}
```"""


def table(label, caption, rows=4):
    lines = ["| Condition | Value | Note |", "|---|:-:|---|"]
    for k in range(rows):
        lines.append(f"| {rng.choice(ADJS)} {rng.choice(NOUNS)} | {rng.randint(1, 99)}.{rng.randint(0, 9)} | {rng.choice(ADVS)} |")
    return "\n".join(lines) + f"\n\n^tab--{label}\n\n((<tag|Table)) {caption}"


def figure(label, caption):
    return tikz(label) + f"\n\n^fig--{label}\n\n((<tag|Figure)) {caption}"


SUMMARY = f"""# Summary {{-}}
^sec--summary

{para(6)}

{para(5)}
"""

c = iter(KEYS)
k1, k2, k3, k4, k5, k6 = (next(c) for _ in range(6))
INTRO = f"""---
wordcount: true
---
# Introduction
^sec--intro

{para(4)} {cite(k1)} {para(3)} The {gl('FNRATE')} and the {gl('FPRATE')} are reported together {cite(k2, k3)}.

{para(5)} As @{k4} show, {sentence().lower()}

## Background
^sec--intro--background

{para(6)} {cite(k5)}

{para(5)} The data used are described in Appendix [[9-appendix#Data Tables|Data Tables]], and item [[9-appendix#^app--item-a|app:item-a]] lists the first.

{para(4)} A note on scope.^[{para(1)}]

## Research Questions
^sec--intro--questions

{para(3)}

- **RQ1** Does the {rng.choice(ADJS)} model keep its {gl('EER')} under {gl('^shift', 'domain shift')}? ^rq1
- **RQ2** Does a {gl('NIT')} change the threshold? ^rq2

{para(5)} {cite(k6)}

{figure('pipeline', 'The pipeline from input to score.')}

## Contribution
^sec--intro--contribution

{para(6)}

{para(5)} {cite(k1, k3)}
"""

more = [next(c) for _ in range(10)]
REVIEW = f"""---
wordcount: true
---
# Literature Review
^sec--lit

{para(5)} {cite(more[0])} {para(2)} {cite(more[1], more[2])}

## Estimators
^sec--lit--estimators

{para(6)} {cite(more[3])}

{para(6)} As @{more[4]} argue, {sentence().lower()} {cite(more[5])}

{table('estimators', 'Estimators compared in the review.')}

## Aggregation
^sec--lit--aggregation

{para(5)} {cite(more[6])}

{para(6)} The {gl('^known-good', 'known good connection')} rate falls {cite(more[7])}.

{para(4)} A remark at the end of the section.^[{para(1)}]

## Gap
^sec--lit--gap

{para(6)} {cite(more[8], more[9])} See Section [[3-introduction#Research Questions|Research Questions]] and [[3-introduction#^rq1|RQ1]].

{para(6)}

{figure('gap', 'Where the gap lies.')}

## Summary of the Review
^sec--lit--summary

{para(5)}
"""

rest = list(c)
METHOD = f"""---
wordcount: true
---
# Methodology
^sec--method

{para(5)} {cite(rest[0])}

## Design
^sec--method--design

{para(6)} The divergence is

$$
D = \\sum_{{i}} p_i \\log \\frac{{p_i}}{{q_i}}
$$

^eq--divergence

{para(4)} Equation [[#^eq--divergence|eq:divergence]] is computed per condition, as hypothesis [[9-appendix#^hyp1|H1]] states.

{table('design', 'The design of the experiment.', 6)}

## Data
^sec--method--data

{para(6)} {cite(rest[1])} The tables in Appendix [[9-appendix#Data Tables|Data Tables]] give the counts.

{para(6)} {cite(rest[2])}

{para(5)}

## Measures
^sec--method--measures

{para(5)} The {gl('FNRATE')} is computed as in item [[9-appendix#^app--item-b|app:item-b]] {cite(rest[3])}.

{para(6)}

{figure('measures', 'The measures and their thresholds.')}

## Analysis
^sec--method--analysis

{para(6)} {cite(rest[4], rest[5])}

{para(5)} {cite(rest[6])}
"""

APPENDIX = f"""# Appendices
^sec--appendix

^startappendices

## Data Tables
^app--data

{para(4)} The method is in Section [[5-method#Design|Design]] and the questions in Section [[3-introduction#Research Questions|Research Questions]].

{table('counts', 'Counts per condition.', 8)}

{para(5)} This is item A. ^app--item-a

{para(4)} This is item B. ^app--item-b

## Hypotheses
^app--hypotheses

{para(3)}

- **H1** The {rng.choice(ADJS)} model keeps its {gl('FNRATE')} at every level. ^hyp1
- **H2** The {gl('FPRATE')} rises with aggregation. ^hyp2

{para(4)}

^subappendix

## Hypotheses in Detail
^app--hypotheses-detail

{para(5)} {cite(rest[7])}

^closesubappendix

## Procedures
^app--procedures

{para(6)}

{para(5)}

## Terms and Definitions
^app--terms

Every term used in the body is listed here with the pages where it is used.

^printterms
"""


def bib():
    out = []
    for k, key in enumerate(KEYS):
        a = SURNAMES[k]
        b = SURNAMES[(k * 7 + 3) % len(SURNAMES)]
        authors = f"{a}, Alex" if k % 3 == 0 else f"{a}, Alex and {b}, Sam" if k % 3 == 1 else f"{a}, Alex and {b}, Sam and {SURNAMES[(k + 5) % 24]}, Kim"
        title = f"On the {rng.choice(ADJS)} {rng.choice(NOUNS)} and its {rng.choice(NOUNS)}"
        out.append(f"@article{{{key},\n  author = {{{authors}}},\n  title = {{{title}}},\n  journal = {{Journal of {rng.choice(ADJS).title()} Systems}},\n  year = {{{2010 + k % 15}}},\n  volume = {{{k + 1}}},\n  pages = {{{k * 10 + 1}--{k * 10 + 9}}}\n}}\n")
    # two sources not cited yet: the scenarios cite them
    for key, a in [("new01", "Archer"), ("new02", "Zimmer")]:
        out.append(f"@article{{{key},\n  author = {{{a}, Robin}},\n  title = {{A source cited later}},\n  journal = {{Journal of Tests}},\n  year = {{2025}}\n}}\n")
    return "\n".join(out)


write("0-preamble.md", PREAMBLE)
write("glossary.md", GLOSSARY)
write("2-summary.md", SUMMARY)
write("3-introduction.md", INTRO)
write("4-review.md", REVIEW)
write("5-method.md", METHOD)
write("9-appendix.md", APPENDIX)
write("compiled/main.tex", MAIN)
write("compiled/fixture.bib", bib())
print("written", OUT)
