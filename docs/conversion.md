# Conversion rules

How a note in the notes folder becomes its `.tex` file. The rules are fixed; the commands they write are those of the
example preamble ([`examples/0-preamble.md`](../examples/0-preamble.md)). The code is `core/src/convert.rs`, with its
tests in `core/src/convert/tests.rs`.

Each line of a note becomes one or more lines of LaTeX, in order. Nothing is reflowed: a paragraph is one line in both.
Every chapter starts with a comment naming its note, then `% !TEX root = main.tex`, so an editor such as LaTeX Workshop
compiles `main.tex` from any chapter.

## Contents

- [Properties](#properties)
- [Comments and left-out content](#comments-and-left-out-content)
- [Headings and labels](#headings-and-labels)
- [Cross-references and labelled lists](#cross-references-and-labelled-lists)
- [Citations](#citations)
- [Inline formatting and lists](#inline-formatting-and-lists)
- [Equations](#equations)
- [Figures](#figures)
- [Tables](#tables)
- [Code listings](#code-listings)
- [Commands without arguments](#commands-without-arguments)
- [LaTeX blocks](#latex-blocks)
- [The glossary note](#the-glossary-note)
- [Warnings](#warnings)
- [Notes with fixed names and chapter order](#notes-with-fixed-names-and-chapter-order)
- [Leaving chapters out of the PDF](#leaving-chapters-out-of-the-pdf)
- [Keys in data.json](#keys-in-datajson)

## Properties

| Property | Effect |
| --- | --- |
| `version`, `last_updated` | Written into the chapter's first line. With **Stamp the last change** on, `last_updated` is set to the current time whenever the note's text changes. |
| `wordcount: true` | Each `#` section is wrapped as `\startwordcount{key}` `\section{...}` `\label{...}` `\printwordcount{key}`, the key being the label without `sec:`; the chapter ends with `\stopwordcount`. See [Word counts](word-counts.md). |
| Any property of the title-page note | `title` becomes `\newcommand{\tpTitle}{...}`, `student_id` becomes `\tpStudentId`, and so on, followed by `\maketitlepage`, whose layout is in the preamble. A property whose name cannot form a command is reported. The example preamble prints `university`, `department`, `title`, `subtitle`, `author`, `student_id`, `degree`, `supervisor`, `module`, `word_count_note` and `date`, each only when it is set (`\iftpfield`): a missing or empty property prints nothing, its label ("Student ID:") included. The placeholder note lists all of them, empty. |

## Comments and left-out content

| In the note | Becomes |
| --- | --- |
| `%% text %%` on a line of its own | `% text` |
| `text %%c%%` | `text %c` |
| A `%% ... %%` block over several lines | one `%` line each |
| `%%[flag] ...%%`, `%%[note] ...%%` (a tag in brackets) | nothing: Obsidian-only comments |
| `___` | a divider, `% =====...` |
| Callouts (`> [!info]- Notes` and their `> ` lines) | nothing |
| Links to other notes (`[[Some note]]`) | nothing, together with the space before them and the commas that join a run of them |
| Links to Zotero (`[❝](zotero://...)`) | nothing |

## Headings and labels

- `#` to `####` become `\section` to `\paragraph`. A trailing ` {-}` gives the starred form, which is still listed in
  the table of contents.
- The `^id` lines directly under a heading are its labels. A block id is the label with `:` written `--` and `_`
  written `---`, since Obsidian ids allow neither: `^sec--intro--scope` is `\label{sec:intro:scope}`.
- `#### Title` followed directly by a line of text is `\paragraph{Title} text`.
- A paragraph or list item ending in ` ^id` gets `\phantomsection\label{...}`.

## Cross-references and labelled lists

| In the note | Becomes |
| --- | --- |
| `[[3-introduction#Scope\|Scope]]` (a link to a heading) | `\ref{sec:intro:scope}` (that heading's label) |
| `[[#^tab--results\|tab:results]]` (a link to a block id) | `\ref{tab:results}` |
| `Section [[...]]` | `Section~\ref{...}` |

The tie is written after *Section*, *Table*, *Figure*, *Equation*, *Appendix*, *Chapter*, *Listing*, *Hypothesis* or
*Question* and their plurals (the `refWords` key). The link still works in Obsidian; the PDF prints the number.

**Labelled lists.** A list whose items start with a bold label and end with an id, `- **H2a** text ^hyp2a`, becomes an
`enumerate` of `\item[\textbf{H2a}] \hypertarget{hyp2a}{}text`. A link `[[3-introduction#^hyp2a|H2a]]` becomes
`\hypref{2a}`, and an item `^rq1` is referenced as `\rqref{1}`. The prefixes and commands are the `lists` key. Write no
blank lines between the items.

## Citations

Citations use Pandoc's syntax, as the Zotero Cite Links plugin writes it, and become biblatex commands.

| In the note | Becomes |
| --- | --- |
| `[@key]` | `\cite{key}` |
| `[@key, p. 27]` | `\cite[p.~27]{key}` |
| `[@a; @b]` | `\cite{a, b}` |
| `[see @a, p. 2]` | `\cite[see][p.~2]{a}` |
| `@key` | `\citet{key}` (`\Citet` at the start of a sentence) |
| `@key [p. 4]` | `\citet[p.~4]{key}` |
| a prefix or locator on a key after the first | `\cites` |

## Inline formatting and lists

| In the note | Becomes |
| --- | --- |
| `*x*` | `\emph{x}` |
| `_x_` | `\textit{x}` |
| `**x**` | `\textbf{x}` |
| `` `x` `` | `\texttt{x}`, with `_ # % & $ { }` escaped |
| `<span style="color:red">x</span>` | `\textcolor{red}{x}` |
| `^[x]` | `\footnote{x}` |
| `[text](https://...)` | `\href{https://...}{text}` |
| curly quotes | TeX quotes |
| `- x` | an `itemize` item |
| `1. x` | an `enumerate` item (indentation is kept) |

Anything else, such as `\gls{API}` or `$x$`, is already LaTeX and is copied as it is. In prose, write `%` as `\%`.

## Equations

`$$ ... $$` becomes `\[ ... \]`, or the environment it holds (`align`, `gather` ...). An `^eq--name` id after the block,
between blank lines, makes it a numbered equation labelled `eq:name`. Do not write `\label` inside the block: Obsidian's
MathJax reports a label drawn twice as "multiply defined".

```markdown
$$
E = mc^2
$$

^eq--energy
```

## Figures

A ```` ```tikz ```` block, a blank line, the `^fig--name` id, a blank line and a caption line becomes a figure float
holding the picture, shrunk to the line width if it is wider.

````markdown
```tikz
\usetikzlibrary{positioning}
\begin{document}
\begin{tikzpicture}
  \node (a) {A};
  \node[right=of a] (b) {B};
  \draw[->] (a) -- (b);
\end{tikzpicture}
\end{document}
```

^fig--pipeline

((<tag|Figure)) The two stages of the pipeline.
````

- The `\usetikzlibrary` and `\begin{document}` lines are for Obsidian's TikZJax renderer; the converter writes the
  picture alone.
- A `%%[resize] \textwidth%%` line above the block, followed by a blank line, forces a width.
- A TikZ library the preamble does not load (the `tikzLibraries` key) is reported.
- The caption line starts with an arrow tag of the Tags for Markdown plugin, `((<tag|Figure))`, then the caption as
  ordinary Markdown. It must hold no other `|`, because that plugin rewrites such lines; write a glossary link there as
  a Markdown link, `[API](glossary.md#API)`.

## Tables

A Markdown table, a blank line, its `^tab--name` id, a blank line and `((<tag|Table)) The caption.` becomes a table
float with booktabs rules.

- The alignment row sets each column: `---` left, `:-:` centred, `--:` right.
- Size and widths are automatic: `\small`, or `\scriptsize` from six columns (`smallFrom`); natural widths when the
  widest row fits the line (`lineChars`), otherwise a `tabularx` whose long columns (`wrapChars`) wrap in a `Y` column,
  a ragged-right `X` column the preamble defines.
- `%%[cols] P{4cm}ccc%%` or `%%[size] footnotesize%%` above the table, then a blank line, overrides them. Obsidian draws
  a table only after a blank line.
- The one-line caption `Table: caption ^tab--name` is also read.
- In a table cell, write the pipe of a link alias as `\|`.

## Code listings

A code block (```` ```rust ````, ```` ```python ```` ...), a blank line, its `^lst--name` id, a blank line and a
`((<tag|Listing)) caption` line (or the one-line `Listing: caption ^lst--name`) becomes:

```latex
\begin{handtyped}[htbp]
\handtypedcode{name}
\caption{...}
\label{lst:name}
\end{handtyped}
```

- The code is written to `compiled/code/name.EXT` (`.rs`, `.py` ...). The **Highlight command** (Pygments by default)
  turns it into `code/name.tex`, which the listing inputs. It runs whenever the block's code changes. A code file
  edited by hand is backed up first.
- The highlighted code uses the macros `\PY` and `\PYZ..`, defined in `code/pygments-style.tex`. When a note has code
  and that file is missing, the plugin writes it (Pygments' default style, bundled with the plugin, so no
  `pygmentize -S` is needed). It never overwrites the file, which may hold another style
  (`pygmentize -S <style> -f latex`). The example preamble reads it with `\InputIfFileExists`, so a document without
  code builds without it.
- A block without a caption is set the same way but not numbered (`code/<note>-<n>.EXT`), with a warning.
- A block with no language, plain text, or a diagram or query that Obsidian draws (mermaid, dataview) stays verbatim.
- The one-line `Listing: caption ^lst--name` with no block above it is the listing of a hand-made `code/name.rs`.
- The environment and command names are the `listingEnv` and `listingCommand` keys.

## Commands without arguments

A line `^FloatBarrier` becomes `\FloatBarrier`; so do `^clearpage`, `^newpage`, `^cleardoublepage` and every command the
preamble defines without arguments (`^printterms`, `^startappendices` ...). Any other `^name` line is a block id.

## LaTeX blocks

A ```` ```latex ```` block is copied verbatim. Only the preamble note should hold one; in another note it is copied with
a warning.

## The glossary note

One entry per `#` heading. Under it, optionally `**Long form:** ...` and `**Printed as:** ...` (when the name needs
maths), then the description, and any `$$` equations, which join the description as `\[ ... \]`.

```markdown
# API

**Long form:** Application Programming Interface

The set of functions a library offers to other programs.

# Source Code

^source-code

The text of a program as its authors wrote it.
```

- Each entry becomes `\term[text]{key}{name}{long form}{description}` (the `termCommand` key; the preamble defines it).
- An acronym (`API`, `W1`: a capital letter after the first, or a digit) prints as written, with its long form in
  title case. A term (`Source Code`) prints in lower case in the text and may have a short id under its heading
  (`^source-code`) for links.
- In the text, `[[glossary#API|API]]` becomes `\gls{API}`; an alias with a capital first letter `\Gls`; an alias ending
  in `s` `\glspl`; any other alias `\glslink{key}{alias}`. `\gls` prints the short form only, so write the long form
  once by hand where the term is introduced.
- A link to an entry the glossary lacks is reported.

## Warnings

Whatever a rule cannot represent (an unknown heading in a link, a table without a caption, an embedded image, a
`==highlight==`, `<key>`-like text that Obsidian would read as HTML ...) is written as LaTeX as well as it can be and
reported in the log with the note's line number. Nothing is dropped silently.

## Notes with fixed names and chapter order

Four notes have fixed names; the plugin finds them by these names alone.

| Note | Holds | Place |
| --- | --- | --- |
| `0-preamble` | the preamble, as one ```` ```latex ```` block | always first |
| `1-title_page` | the title page, as the note's properties | always second |
| `glossary` | the glossary: one `#` heading per entry | after the last chapter |
| `appendix` | the appendices (not counted in the word count) | last |

**Apply ordering** makes any that is missing, with placeholders; a notes folder without a preamble gets the example
preamble, and the placeholder title page lists every property the example preamble prints, empty (all optional). Every other Markdown note of the folder is a chapter. Files that are not notes (a PDF, a picture) and a
`README` note are never part of the document.

The notes are numbered in the order of the document: `0-preamble`, `1-title_page`, the chapters (`2-introduction`,
`3-background` ...), then the glossary and the appendix (`9-glossary`, `10-appendix`). A note becomes the `.tex` of the
same name (`3-background.md` becomes `3-background.tex`). The numbers change only when you ask:

1. Under **Chapter ordering and inclusion** in the settings, drag a chapter or use its arrows. This records the order;
   no note is renamed. A new note without a number appears at the end of the list.
2. Right-click the notes folder and choose **Apply ordering** (it is on that folder's menu only). The plugin then:
   - renames the notes to that order through Obsidian, which rewrites every link to them;
   - numbers a new note and adds it to `main.tex` after the last chapter;
   - puts the glossary and appendix after the chapters;
   - deletes each old `.tex` it wrote, unless it was edited by hand;
   - puts `main.tex`'s `\input` lines in the new order.

The `\input` lines keep their places and the chapters fill them in order, so lines between them (`\pagenumbering`,
`\clearpage`) stay where they are. The rules are `core/src/order.rs`, with tests.

## Leaving chapters out of the PDF

Under **Chapter ordering and inclusion** each note except the preamble and the glossary has a toggle. A note left out
is remembered by its name without the number, so renumbering keeps it out.

- A chapter turned off is still converted on every save and its words are still counted, but its `.tex` starts with
  `\endinput% left out of the PDF ...` on its first line, so `main.tex`'s `\input` of it reads nothing. The rest of the
  chapter stays below, on the same lines. `main.tex` is not changed.
- Turning a chapter off or on rewrites its `.tex` and runs a full build after the quiet period, so its citations, labels
  and terms come or go with it. A chapter left out is not previewed.
- A reference to a label in a chapter left out prints `??`.

## Keys in data.json

The commands the rules write are those of the example preamble. A document that needs other names sets these keys in
the plugin's `data.json` (`.obsidian/plugins/latex/data.json`); they are not in the settings tab.

| Key | Default | Meaning |
| --- | --- | --- |
| `wordcountProperty` | `wordcount` | the property that turns counting on |
| `countStart`, `countPrint`, `countStop` | `startwordcount`, `printwordcount`, `stopwordcount` | the counter's macros |
| `countOff` | `wordcountoff` | text in its group, `{\wordcountoff ...}`, is not counted |
| `countsFile` | `wordcounts.tex` | in the output folder: one `\wc@set{key}{n}` line per counted section |
| `tikzLibraries` | `positioning,calc,decorations.pathreplacing` | the TikZ libraries the preamble loads |
| `listingEnv`, `listingCommand` | `handtyped`, `handtypedcode` | the listing float and its command |
| `termCommand` | `term` | the glossary entry command |
| `titlePrefix`, `makeTitle` | `tp`, `maketitlepage` | the title page's commands |
| `lists` | `rq rqref RQ, hyp hypref H` | labelled lists: id prefix, command, label prefix |
| `commands` | `FloatBarrier, clearpage, newpage, cleardoublepage` | commands without arguments, besides the preamble's |
| `lineChars`, `wrapChars`, `smallFrom` | 85, 20, 6 | table layout: a table whose widest row fits this many characters keeps its natural widths; a longer column wraps; this many columns or more are set in `\scriptsize` |
| `inlineMathWord` | `true` | inline maths counts as one word |
| `wordcountRules` | (none) | word-count rules added to or replacing the built-in ones ([Word counts](word-counts.md)) |
| `refWords` | Section, Table, Figure ... (singular and plural) | words after which a reference gets a tie |
| `abbreviations` | `al., e.g., i.e., cf., vs., Fig., ...` | a full stop after these does not end a sentence |
| `smallWords` | `a, an, the, and, of, ...` | kept lower case in title case |
