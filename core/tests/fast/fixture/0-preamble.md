```latex
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
