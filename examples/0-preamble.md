---
version: 1.0
last_updated: 2026-10-26 03:30
---
```latex
% =====================================================================
% Preamble for the dissertation. main.tex loads this file with \input{0-preamble}
% immediately after \documentclass, so the class options (11pt, a4paper) are set
% there and everything else is set here.
% =====================================================================


% ---------------------------------------------------------------------
% Page layout and fonts
% ---------------------------------------------------------------------

% The geometry package provides an interface to customize LaTeX page dimensions,
% including margins, paper size, and layout. Here we specify 1 inch page margins
% on all sides. The paper size (A4) comes from the \documentclass options in
% main.tex.
\usepackage[margin=1in]{geometry}

% fontenc selects the font encoding. T1 (8-bit) encoding contains accented
% characters as single glyphs rather than composing them from a letter plus an
% accent. This allows words with accents to hyphenate correctly, and allows
% characters like <, >, and | to print as themselves. It also makes text copied
% from the PDF come out correctly.
\usepackage[T1]{fontenc}

% LuaLaTeX reads the source as UTF-8 by itself, so inputenc is not loaded (it
% would only print a warning that it is ignored). The T1 fonts below, however,
% have no glyphs at the Unicode positions of the en and em dashes, so a dash
% typed directly, as in some titles exported from Zotero to the .bib file,
% would silently vanish from the PDF ("Missing character" in the log).
% newunicodechar maps each to the font's own dash. https://ctan.org/pkg/newunicodechar
\usepackage{newunicodechar}
\newunicodechar{–}{\textendash}
\newunicodechar{—}{\textemdash}
% The Unicode hyphen (U+2010), in three titles exported from Zotero, prints as
% the ordinary hyphen.
\newunicodechar{‐}{-}

% lmodern loads Latin Modern, an enhanced version of LaTeX's default Computer
% Modern font that is available in T1 encoding as a scalable vector font.
% Without it, T1 encoding can fall back to bitmap fonts that look blurry in PDF
% viewers.
\usepackage{lmodern}

% microtype makes small typographic refinements: it lets characters stick out
% slightly into the margin (character protrusion) and stretches or shrinks
% letter shapes very slightly (font expansion). The result is more even word
% spacing, fewer hyphenations, and fewer overfull lines, with no change to your
% source text. https://ctan.org/pkg/microtype
\usepackage{microtype}

% setspace controls line spacing (single, one-and-a-half, double) without
% disturbing the spacing of footnotes, captions, and floats. It provides the
% \onehalfspacing command used further down, as well as environments such as
% \begin{singlespace} for locally overriding the document-wide spacing.
% https://ctan.org/pkg/setspace
\usepackage{setspace}


% ---------------------------------------------------------------------
% Mathematics
% ---------------------------------------------------------------------

% AMS stands for "American Mathematical Society". amsmath provides enhanced
% mathematics typesetting capabilities beyond LaTeX's defaults. Specifically, it
% gives you access to math environments that allow you to create multi-line
% equations with fine-grained alignment control. The methodology uses its align
% environment for the regression model and \text, \operatorname and \tfrac
% throughout.
% https://mirror.ox.ac.uk/sites/ctan.org/macros/latex/required/amsmath/amsldoc.pdf
\usepackage{amsmath}

% amssymb (American Mathematical Society symbols) provides access to an extended
% library of mathematical symbols and operators that aren't available in
% standard LaTeX. Essentially, amssymb ensures they render correctly without
% undefined command errors.
\usepackage{amssymb}


% ---------------------------------------------------------------------
% Tables
% ---------------------------------------------------------------------

% Booktabs is a LaTeX package designed to enhance the quality of tables by
% providing commands that create aesthetically pleasing horizontal rules and
% improve spacing, making tables look more professional and easier to read. It
% supplies the \toprule, \midrule and \bottomrule used in every table.
% https://mirror.apps.cam.ac.uk/pub/tex-archive/macros/latex/contrib/booktabs/booktabs.pdf
\usepackage{booktabs}

% array extends the column definitions available in tabular
% environments. Its main use is the >{...} and <{...} syntax, which inserts
% commands before or after every cell in a column (e.g., >{\RaggedRight}p{4cm}),
% plus the m{width} and b{width} column types for vertically centred or
% bottom-aligned cells. It also provides \newcolumntype, used below to define
% the P column type.
\usepackage{array}

% ragged2e provides \RaggedRight, \RaggedLeft, and \Centering, improved versions
% of LaTeX's \raggedright, \raggedleft, and \centering. Unlike the standard
% commands, they still allow hyphenation and do not break paragraph indentation,
% so text in narrow table columns looks less jagged.
\usepackage{ragged2e}

% Narrow table cells are set ragged-right rather than justified, because
% justifying a few words across a narrow column leaves large gaps between them.
% P{width} replaces p{width}, with the same width but ragged-right text.
% \arraybackslash restores the meaning of \\ (end of row), which \RaggedRight
% would otherwise redefine.
\newcolumntype{P}[1]{>{\RaggedRight\arraybackslash}p{#1}}

% tabularx sets a table to a given width (here \linewidth) and shares the spare
% width among its X columns. Y is an X column set ragged-right, as P is for a
% fixed width; the converter gives a Markdown table's long columns this type
% when the table is too wide for its natural widths (latex_sync.py, Tables).
\usepackage{tabularx}
\newcolumntype{Y}{>{\RaggedRight\arraybackslash}X}


% ---------------------------------------------------------------------
% Figures, colour and code
% ---------------------------------------------------------------------

% xcolor provides named colours and colour mixing such as gray!60 (60% grey, 40%
% white). It is used for the shading of hand-typed code, the red placeholder in
% the executive summary, and by TikZ in the figures.
% https://ctan.org/pkg/xcolor
\usepackage{xcolor}

% TikZ draws diagrams directly in LaTeX, so figures are vector graphics in the
% document's own fonts rather than imported images. It draws the pipeline figure
% and the quantisation diagrams in the methodology. https://ctan.org/pkg/pgf
\usepackage{tikz}
% TikZ libraries loaded on top of the core package: positioning              --
%   places nodes relative to one another (e.g. right=6pt of a node) calc
%   -- coordinate arithmetic inside ( ... ) decorations.pathreplacing -- the
%   curly braces drawn beside the per-tensor/per-channel quantisation figure
\usetikzlibrary{positioning,calc,decorations.pathreplacing}

% placeins provides \FloatBarrier, which makes every figure still waiting to
% be placed appear before that point. The literature review uses it before
% each subsection, so a run of figures cannot drift past the next heading.
% https://ctan.org/pkg/placeins
\usepackage{placeins}

% Source code is highlighted by Pygments before LaTeX runs (no shell escape):
% the LaTeX plugin writes each block of code in a note to code/NAME.EXT and
% runs its Highlight command, which turns it into code/NAME.tex, a fancyvrb
% Verbatim block. code/pygments-style.tex defines the colours (\PY and the
% \PYZ.. characters) those blocks use; the plugin writes it (Pygments' default
% style) with the first block of code, and never overwrites it. It is read
% only if it exists, so a document without code builds without it.
% fancyvrb sets the verbatim block; framed shades it.
% https://ctan.org/pkg/fancyvrb  https://ctan.org/pkg/framed
\usepackage{fancyvrb}
\usepackage{framed}
\InputIfFileExists{code/pygments-style.tex}{}{}
\fvset{fontsize=\footnotesize}
% \handtypedcode{NAME} inserts the highlighted code/NAME.rs on a shaded ground
% with one continuous heavy grey rule down its left side. framed draws both,
% because fancyvrb's own left rule is drawn line by line and shows gaps.
\colorlet{shadecolor}{gray!8}
\newenvironment{handtypedbox}{%
  \def\FrameCommand{{\color{gray!60}\vrule width 2.5pt}%
    \fboxsep=6pt\colorbox{shadecolor}}%
  \MakeFramed{\advance\hsize-\width\FrameRestore}%
  % Verbatim is a list; drop its vertical spacing so the shading fits the code,
  % and remove the skip it leaves after its last line.
  \setlength{\topsep}{0pt}\setlength{\partopsep}{0pt}\setlength{\parskip}{0pt}}%
  {\par\vskip-\lastskip\endMakeFramed}
% A listing whose code/NAME.rs is missing prints a placeholder and a warning
% instead of stopping the build.
\newcommand{\handtypedcode}[1]{%
  \begin{handtypedbox}\InputIfFileExists{code/#1.tex}{}{%
    \PackageWarning{preamble}{code/#1.tex not found (is code/#1.rs missing?)}%
    \textit{[code/#1.rs is missing]}}\end{handtypedbox}}
% Hand-typed code must be identifiable as such: every hand-typed listing is
% shaded, ruled on the left and set in a "handtyped" float whose caption is
% prefixed "Hand-typed listing". Longer code goes in as a screenshot instead.
% newfloat is part of the caption bundle. https://ctan.org/pkg/newfloat
\usepackage{newfloat}
\DeclareFloatingEnvironment[name={Hand-typed listing},fileext=lol]{handtyped}


% ---------------------------------------------------------------------
% Word counts
%
% Each counted section sits between \startwordcount{<key>} and \stopwordcount,
% with \printwordcount{<key>} under its heading: the LaTeX plugin writes all
% three into a chapter whose note has the property wordcount: true. Whenever a
% note is converted, the plugin counts its chapter from the LaTeX and writes
% the counts to wordcounts.tex, beside this file, one line \wc@set{<key>}{<n>}
% per counted section (the file changes only when a count does). This preamble
% only reads that file, so the counts are right on the first run and a changed
% word never asks for a second one.
% \printwordcount{<key>} prints a section's count under its heading,
% \wordcount{<key>} the count alone, and \totalwordcount the sum over all
% counted sections.
%
% Counted (the plugin's rules, since 2026-10-27; the plugin's README lists
% them, and its Word-count rules setting changes them): running text,
% headings and footnotes, the labels of numbered and bulleted lists, the
% captions of figures and tables (not their "Figure 3:"), the text in a
% table's cells, and each citation as one word: everything from its [ to its
% ], inclusive, is one word whatever it holds ("[3, p. 27]" and "[3, 5, 9]"
% are one word each). A narrative citation is its authors' names plus one
% word for its brackets.
% Not counted: section numbers, footnote marks, inline and displayed maths,
% TikZ pictures (their drawing code and their labels), code listings, page
% numbers, and anything
% outside a \startwordcount ... \stopwordcount pair (the appendices, the
% bibliography). Any other passage can be left out by wrapping it in
% {\wordcountoff ...}, which the plugin reads and LaTeX ignores.
%
% A word is a run of characters between two spaces (or en or em dashes) that
% contains at least one letter or digit, so "one-to-one" is one word and a
% stray "--" or "(" is none; a word hyphenated across two lines counts once.
% ---------------------------------------------------------------------

\makeatletter
% The markers the plugin counts between. In LaTeX they only end the paragraph
% before them, as they did when LaTeX counted, so the layout is unchanged.
\newcommand{\startwordcount}[1]{\par}
\newcommand{\stopwordcount}{\par}
\newcommand{\wordcountoff}{}

% The counts: \wc@set{key}{n} stores n as \wc@val@key and adds it to the
% total. Without wordcounts.tex (before the first conversion) every count
% prints as ??.
\def\wc@total{0}
\newcommand{\wc@set}[2]{%
  \expandafter\xdef\csname wc@val@#1\endcsname{#2}%
  \xdef\wc@total{\the\numexpr\wc@total+#2\relax}}
\InputIfFileExists{wordcounts.tex}{}{%
  \PackageWarningNoLine{preamble}{wordcounts.tex not found, so the word
    counts print as ??: convert the chapters (LaTeX plugin: Build all
    chapters)}}

% A count with a comma between thousands: 14772 -> 14,772. \numexpr rounds,
% so (n - 500)/1000 is n/1000 rounded down.
\newcommand{\wc@format}[1]{%
  \ifnum#1<1000 \number#1\else
    \expandafter\wc@format\expandafter{\number\numexpr(#1-500)/1000\relax},%
    \expandafter\wc@three\expandafter{\number\numexpr#1-((#1-500)/1000)*1000\relax}%
  \fi}
\newcommand{\wc@three}[1]{\ifnum#1<100 0\fi\ifnum#1<10 0\fi#1}

% The count for one section (?? if wordcounts.tex has none), and the total.
\newcommand{\wordcount}[1]{%
  \ifcsname wc@val@#1\endcsname
    \wc@format{\csname wc@val@#1\endcsname}%
  \else\textbf{??}\fi}
\newcommand{\totalwordcount}{\wc@format{\wc@total}}

% The line printed under a section heading. It is a paragraph of its own, which
% would end the heading's hold on the text below it and let the page break
% between this line and the text (leaving the heading at the foot of a page),
% so it forbids a break on both sides and then renews that hold
% (\@afterheading, what \section ends with): the heading, this line and the
% first two lines of text stay on one page.
\newcommand{\printwordcount}[1]{%
  \nobreak{\noindent\small\textit{Word count: \wordcount{#1}}\par}%
  \nobreak\@afterheading}
\makeatother


% ---------------------------------------------------------------------
% Bibliography
% ---------------------------------------------------------------------

% Bibliography management using Biber compiler backend with numeric citation
% style (e.g., [1], [2]). Processes .bib files and formats references.
% backend=biber is the modern replacement for BibTeX. sorting=none maintains
% citation order as they appear in the document. style=numeric gives a generic
% numbered style similar to IEEE; for exact IEEE formatting, use style=ieee
% (requires the biblatex-ieee package). natbib=true provides natbib's commands
% (\citet, \citep, \citealp, \citeauthor), which the chapters use throughout:
% \citet{hinton2015} prints "Hinton et al. [12]" and \citep{hinton2015} prints
% "[12]". maxcitenames=2 shortens author lists of three or more to "First et
% al." when \citet or \citeauthor prints names in the text.
\usepackage[style=numeric,sorting=none,backend=biber,natbib=true,maxcitenames=2]{biblatex}

% Points to the .bib bibliography database file containing all reference
% metadata (exported from Zotero).
\addbibresource{My Library.bib}

% Long URLs and DOIs in the reference list otherwise break only at punctuation
% and can overrun the margin. These penalties let biblatex break them after any
% digit, upper-case or lower-case letter as well.
\setcounter{biburlnumpenalty}{9000}
\setcounter{biburlucpenalty}{9000}
\setcounter{biburllcpenalty}{9000}

% Citations are counted, each as ONE word ("[3, p. 27]" is one word; a
% narrative citation is the authors' names and one more): the LaTeX plugin
% counts them from the chapters (Word counts, above). To leave them out, give
% the citation commands the behaviour skip in the plugin's Word-count rules
% setting (\cite soom skip, \citet soom skip, and so on).

% Zotero exports standards (ISO/IEC documents) as @standard and statutes (GDPR,
% the Data Protection Act) as @legislation, entry types for which biblatex's
% standard styles have no layout. These lines format them as a report and a
% miscellaneous entry respectively, instead of failing.
\DeclareBibliographyAlias{standard}{report}
\DeclareBibliographyAlias{legislation}{misc}


% ---------------------------------------------------------------------
% Hyperlinks -- LOAD ORDER: after the packages above
% ---------------------------------------------------------------------

% Enables cross-references, hyperlinks, and PDF metadata for internal document
% navigation and external URLs. Makes citations and references hyperlinkable in
% the compiled PDF. colorlinks=true colours the link text instead of drawing
% boxes around it, and allcolors=blue makes every link type (internal, citation,
% URL) blue; citecolor=black then overrides that for citations, which stay
% clickable but print in black. hyperref redefines many internal commands, so it
% must be loaded after almost every other package.
\usepackage[colorlinks=true,allcolors=blue,citecolor=black]{hyperref}


% ---------------------------------------------------------------------
% Glossary: acronyms and defined terms -- LOAD ORDER: after hyperref
% ---------------------------------------------------------------------

% glossaries-extra manages one list of acronyms and defined terms, printed in
% the Terms and Definitions appendix. \gls{API} in the text prints "API" as
% a link to its entry, and each entry lists the pages that use it. It must come
% after hyperref so that those links are created.
% nopostdot   -- no full stop after each description
% nogroupskip -- no gap between letter groups in the list
% docdef=true -- entries may be defined inside the document, so that they sit
%                in the appendix (10-appendix.tex) where they are printed; an
%                entry used before its definition is read back from the
%                .glsdefs file, so a new entry needs one more LaTeX run (latexmk
%                reruns by itself)
% undefaction=warn -- on that first run, an entry not yet defined is a
%                warning, not an error, so that the build can go on to the run
%                that knows it; a key that is never defined stays a warning in
%                the log ("Glossary entry `...' has not been defined")
% https://ctan.org/pkg/glossaries-extra
\usepackage[nopostdot,nogroupskip,docdef=true,undefaction=warn]{glossaries-extra}

% Every entry, acronym or term, is defined in the same way (the converter
% writes these lines into 9-glossary.tex from LaTeX/9-glossary.md):
%   \term[text]{key}{name}{long form}{description}
% The key is what \gls takes, and it is case-sensitive: \gls{API} finds the
% entry whose key is API. Keys of acronyms are written as the acronym is
% (API, HTTP), keys of terms in lower case (groundtruth). The
% list prints the name (Ground Truth), the long form in parentheses
% (empty for a term that is not an abbreviation), and the description; the text
% prints the optional [text] (ground truth), or the name where it is
% not given, at every use. Entries are sorted by key.
\NewDocumentCommand{\term}{o m m m m}{%
  \IfNoValueTF{#1}{\termentry{#2}{#3}{#3}{#4}{#5}}{\termentry{#2}{#3}{#1}{#4}{#5}}}
% \termentry{key}{name}{text}{long form}{description}
\newcommand{\termentry}[5]{%
  \ifblank{#4}
    {\newglossaryentry{#1}{name={#2}, text={#3}, sort={#1}, description={#5}}}
    {\newglossaryentry{#1}{name={#2 (#4)}, text={#3}, sort={#1}, description={#5}}}}

% Glossary terms are links like cross-references, so hyperref would colour them
% blue too. \glstextformat wraps the text inside each \gls link, so setting it
% in black keeps the terms clickable without colouring them, while \ref links
% stay blue.
\renewcommand*{\glstextformat}[1]{\textcolor{black}{#1}}

% Opens the files that makeglossaries reads to build the sorted list.
\makeglossaries

% Links to the research questions and hypotheses listed in the introduction.
% \rqref{1} prints "RQ1" and \hypref{2a} prints "H2a", each as a link to the
% anchor that \hypertarget sets at the start of that item (3-introduction.tex).
\newcommand{\rqref}[1]{\hyperlink{rq#1}{RQ#1}}
\newcommand{\hypref}[1]{\hyperlink{hyp#1}{H#1}}

% The title page. Its text comes from the properties of 1-title_page.md: the
% converter writes one \newcommand{\tp<Field>}{...} per property (title ->
% \tpTitle, student_id -> \tpStudentId) and then \maketitlepage. The fields
% are university, department, title, subtitle, author, student_id, degree,
% supervisor, module, word_count_note and date, and every one is optional:
% \iftpfield{Field}{text} sets the text only when the property exists and has
% a value, so a missing or empty one prints nothing, its label included.
% (\ifcsvoid, from etoolbox, is true for an undefined or empty command.) The
% word count prints once a chapter is counted. The page is numbered A in the
% PDF's internal page labels (and shows no number), so that it does not share
% a label with page i or page 1 and hyperref does not warn about duplicate
% page destinations.
% https://ctan.org/pkg/etoolbox
\usepackage{etoolbox}
\newcommand{\iftpfield}[2]{\ifcsvoid{tp#1}{}{#2}}
\makeatletter
\newcommand{\maketitlepage}{%
  \pagenumbering{Alph}%
  \begin{titlepage}
  \centering
  \onehalfspacing
  \iftpfield{University}{{\large \tpUniversity\par}}%
  \iftpfield{Department}{{\large \tpDepartment\par}}%
  \vspace*{\stretch{1}}
  \iftpfield{Title}{{\LARGE\bfseries \tpTitle\par}}%
  \iftpfield{Subtitle}{\vspace{0.5em}{\Large \tpSubtitle\par}}%
  \vspace*{\stretch{1}}
  \iftpfield{Author}{{\large \tpAuthor\par}}%
  \iftpfield{StudentId}{{\large Student ID: \tpStudentId\par}}%
  \iftpfield{Degree}{\vspace{2em}%
    A dissertation submitted in partial fulfilment of the requirements for the
    degree of\par
    {\large \tpDegree\par}}%
  \vspace{2em}
  \iftpfield{Supervisor}{Supervisor: \tpSupervisor\par}%
  \iftpfield{Module}{Module: \tpModule\par}%
  \vspace*{\stretch{1}}
  \ifnum\wc@total>0
    Word count: \totalwordcount\iftpfield{WordCountNote}{\ \tpWordCountNote}\par
  \else
    \iftpfield{WordCountNote}{\tpWordCountNote\par}%
  \fi
  \iftpfield{Date}{\vspace{0.5em}\tpDate\par}%
  \end{titlepage}}
\makeatother

% The appendices. Each is a subsection lettered A, B, C, ... so that \ref gives
% "Appendix A"; \theHsubsection gives hyperref anchors that do not clash with
% the subsections of earlier sections. In the note: ^startappendices.
\newcommand{\startappendices}{%
  \setcounter{subsection}{0}%
  \renewcommand{\thesubsection}{\Alph{subsection}}%
  \renewcommand{\theHsubsection}{appendix.\Alph{subsection}}}
% An appendix printed "A2" that takes no letter of its own, so that the letters
% after it stay as the chapters cite them: ^subappendix before its heading,
% ^closesubappendix after its content.
\newcommand{\subappendix}{%
  \addtocounter{subsection}{-1}%
  \renewcommand{\thesubsection}{\Alph{subsection}2}%
  \renewcommand{\theHsubsection}{appendix.\Alph{subsection}2}}
\newcommand{\closesubappendix}{%
  \renewcommand{\thesubsection}{\Alph{subsection}}%
  \renewcommand{\theHsubsection}{appendix.\Alph{subsection}}}
% The list of terms, without the glossary's own heading, so that it sits
% directly under the appendix heading. In the note: ^printterms.
\newcommand{\printterms}{%
  {\renewcommand*{\glossarysection}[2][]{}%
   \printglossary[type=main,style=altlist,nonumberlist=false]}}


% ---------------------------------------------------------------------
% Paragraph and heading layout
% ---------------------------------------------------------------------

% Sets 1.5x line spacing for the whole document using the setspace package.
\onehalfspacing

% Removes paragraph indent on line 1 that LaTeX normally applies to new
% paragraphs.
\setlength{\parindent}{0pt}

% Adds vertical spacing between paragraphs. LaTeX's "rubber length" (glue)
% syntax defines a flexible spacing value instead of a fixed one, with the
% natural/preferred value first, followed by the stretchability and
% shrinkability value, to adjust as needed for better page layout.
\setlength{\parskip}{6pt plus 2pt minus 1pt}

% Controls how many heading levels get numbers (1=section, 2=subsection,
% 3=subsubsection).
\setcounter{secnumdepth}{2}

% A section or subsection starts on a new page unless there is room on this one
% for the heading and a few lines of text under it, so a heading is never left
% near the foot of a page with one line of text or none. \Needspace (needspace
% package) ends the page early when the room is short. A heading that follows
% another directly (\if@nobreak) is skipped: the first heading has already asked
% for room, and a page break must not come between the two.
% https://ctan.org/pkg/needspace
\usepackage{needspace}
\makeatletter
\newcommand{\headingneedspace}{\if@nobreak\else\Needspace{6\baselineskip}\fi}
\AddToHook{cmd/section/before}{\headingneedspace}
\AddToHook{cmd/subsection/before}{\headingneedspace}
\makeatother


% ---------------------------------------------------------------------
% Line-breaking tolerances
% ---------------------------------------------------------------------

% When TeX cannot fit a line within its normal limits, allow up to 2em of extra
% inter-word space before letting the line run into the margin.
\setlength{\emergencystretch}{2em}

% Do not report lines that overflow the margin by less than half a point. Such
% overflows are invisible and come mostly from microtype's protrusion.
\setlength{\hfuzz}{0.5pt}

% Do not report mildly loose lines (badness up to 3000, the default being 1000).
% Ragged-right text in narrow table cells produces many of these.
\hbadness=3000
```
