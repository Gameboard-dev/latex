# Word counts

The plugin counts the words of each counted section itself, from the LaTeX it has just written, so the counts need no
particular engine and are right on the first LaTeX run. The counter is `core/src/wordcount.rs`.

## How it works

- A note with the property `wordcount: true` has each `#` section counted. The converter wraps the section in
  `\startwordcount{key}` ... `\stopwordcount` and puts `\printwordcount{key}` under its heading; the key is the
  section's label without `sec:`.
- Whenever a note is converted, the plugin counts its chapter and writes every counted section's count to
  `wordcounts.tex` in the output folder, one `\wc@set{key}{n}` line each. It writes the file only when a count changes,
  so a changed word never asks for a second LaTeX run.
- The status bar's tooltip shows the counts.
- Every chapter is counted again from its `.tex` when the `.bib` changes (it is read before each conversion), when the
  glossary note is converted, when the plugin starts, and when the word-count rules change.

## The preamble

Define the three macros so that LaTeX ignores them (`\startwordcount` and `\stopwordcount` as `\par`, `\wordcountoff` as
nothing), read `wordcounts.tex` with `\makeatletter` in force, and print `\wc@val@<key>` where a count should appear.
The example preamble ([`examples/0-preamble.md`](../examples/0-preamble.md), section "Word counts") does this and
defines `\wordcount{key}`, `\totalwordcount` and `\printwordcount{key}`.

## What counts

- A word is a run of characters between two spaces, or en or em dashes (`--`, `---`, `–`, `—`), that holds a letter or a
  digit. Any character beyond ASCII counts as a letter. `one-to-one` is one word; a stray `--` or `(` is none. A tie
  `~` is a space; a thin space `\,` is not, so `372\,ms` is one word.
- Only text between the start and stop macros counts, and nothing in the group of the off macro
  (`{\wordcountoff ...}`). The appendix is left out by having no `wordcount` property.
- Running text, headings, footnotes, captions (not their "Figure 3:") and the text in table cells count (`&` separates
  words).
- Inline maths (`$x$`, `\(x\)`) is one word (the `inlineMathWord` key; off, it counts nothing).
- Section numbers, footnote marks, displayed maths, TikZ pictures and code (`verbatim` blocks and listings) do not
  count. To count a picture's node and label text instead (`\node {text}`, `node {text}` on a path,
  `label=above:{text}`, each label a paragraph of its own), add the rule `tikzpicture labels`.
- A citation's brackets, with all they hold, are one word (`[3, p. 27]`). A narrative citation (`\citet`) counts the
  authors' family names from the `.bib` (`Smith`, `Smith and Jones` or `Smith et al.`), then its one-word brackets.
  The editors stand in where there are no authors, and the title where there are no names.
- A glossary entry (`\gls`) counts its text from the glossary; `\glspl` adds an `s`. An entry printed as maths counts as
  inline maths.
- A reference (`\ref`, `\eqref`, `\rqref`, `\hypref`) is one word, run into the characters around it.
- An enumerate item's number and an itemize item's bullet are one word each; a description item without a label is
  none. A numbered row of `align`, `gather` or `multline` is one word; an `equation`'s number is none.

Two differences from the typeset page: a word hyphenated across a page break counts once, and a `\ref` to a missing
label counts as one word where LaTeX prints `??`.

## Rules

Every command and environment the counter meets in counted text needs a rule. The built-in table covers everything the
converter writes and the common LaTeX a note may hold. The `wordcountRules` key in the plugin's `data.json` adds rules
and overrides built-in ones, one per line, `%` starting a comment:

```
\name [arguments] behaviour          a command
name [arguments] behaviour           an environment
```

The arguments are xparse's letters, as in `\NewDocumentCommand`: `s` an optional star, `o` an optional `[argument]`,
`m` a mandatory `{argument}`. A trailing `+` repeats the `o` and `m` letters while another argument follows, as in
`\cites[p.~2]{a}[see]{b}`.

A command's behaviour is one of:

| Behaviour | Meaning |
| --- | --- |
| `text` | its last mandatory argument is running text (`text=2`: the second argument) |
| `heading` | the same, as a paragraph of its own (`\section`) |
| `runin` | the same, then a space (`\paragraph`) |
| `footnote` | the argument counts as a paragraph of its own; the mark is not a word |
| `one` | one word, whatever it holds, separate from the word before it (a citation) |
| `names` | the authors' names from the `.bib`, then one word (a narrative citation) |
| `authors` | the names only (`\citeauthor`) |
| `value` | one word, run into the characters around it (`\ref`) |
| `glossary`, `glossary-plural` | the glossary entry's text (with an `s`) |
| `skip` | nothing that counts; the words around it run on (`\label`, `\%`, `\,`) |
| `space` | a space between words (`\quad`, `\\`) |
| `math` | typeset as maths: no word, and the word before it ends (`\url`) |
| `item` | a list item: its `[label]` is text, or else the list's own label |
| `par` | ends the paragraph |
| `start`, `print`, `stop`, `off` | the counter's macros |

An environment's behaviour is `count` (running text), `skip`, `math`, `math-rows` (each numbered row is one word),
`math-once` (its number, if any, is one word), `list` (an `\item` without a label is one word), `list-plain` (it is
none), `verbatim` (counted line by line, as it stands) or `labels` (a TikZ picture: only its nodes' and labels' text).

Example:

```
% emphasis is not counted
\emph m skip
% the preamble's own citation command
\mycite oom one
\keyword m text
\todo om skip
itemize list-plain
myenv skip
```

The built-in table, in the same syntax, is `DEFAULT_RULES` in `core/src/wordcount.rs`. In short:

| Behaviour | Commands and environments |
| --- | --- |
| `heading`, `runin` | `\section` `\subsection` `\subsubsection` (`som`); `\paragraph` `\subparagraph` |
| `text` | `\emph` `\textit` `\textbf` `\texttt` `\textsc` `\textsf` `\textrm` `\textup` `\textmd` `\textsl` `\textnormal` `\mbox` (`m`); `\textcolor` `\href` `\glslink` (`omm`); `\hypertarget` (`mm`); the accents `\c` `\v` `\u` `\H` `\r` `\k` `\d` `\b` `\t` |
| `footnote` | `\footnote` (`om`) |
| `one` | `\cite` `\Cite` `\citep` `\parencite` `\Parencite` `\autocite` `\citealp` (`soom`); `\cites` `\parencites` (`oom+`) |
| `names`, `authors` | `\citet` `\Citet` `\textcite` `\Textcite` (`soom`); `\textcites` (`oom+`); `\citeauthor` `\Citeauthor` |
| `value` | `\ref` `\pageref` (`sm`); `\eqref` `\rqref` `\hypref`; `\citeyear`; the letters `\i` `\j` `\l` `\L` `\o` `\O` `\ss` `\ae` `\AE` `\oe` `\OE` `\aa` `\AA` |
| `glossary` | `\gls` `\Gls`; `glossary-plural`: `\glspl` `\Glspl` (`om`) |
| `math` | `\url` |
| `space` | `\\` (`so`), `\newline`, `\linebreak`, `\quad`, `\qquad`, `\hspace`, `\hfill`, `\textendash`, `\textemdash` |
| `par` | `\par`, `\vspace`, `\smallskip`, `\medskip`, `\bigskip` |
| `skip` | `\label` `\nocite` `\phantomsection` `\addcontentsline` `\FloatBarrier` `\clearpage` `\newpage` `\cleardoublepage` `\noindent` `\indent` `\centering` `\raggedright` `\raggedleft`, the size and font switches (`\small`, `\bfseries` ...), `\ldots` `\dots` `\protect` `\relax` `\nobreak` `\enspace` `\thinspace`, and `\%` `\&` `\#` `\$` `\_` `\{` `\}` `\,` `\;` `\:` `\!` `\/` `\-` `\@` `\'` `` \` `` `\^` `\"` `\~` `\=` `\.` |
| `item` | `\item` (`o`) |
| floats | `text`: `\caption` (`som`), `\resizebox` (`mmm`), `\scalebox` (`om`), `\multicolumn` (`mmm`), `\multirow` (`omm`); `skip`: `\includegraphics` (`om`), `\toprule` `\midrule` `\bottomrule` `\addlinespace` (`o`), `\hline`, `\cline` (`m`), `\endhead` `\endfirsthead` `\endfoot` `\endlastfoot` |
| environments | `count`: `figure` `table` (and starred; `o`), `tabular` (`m`), `tabular*` `tabularx` (`mm`), `longtable` (`om`), `center` `flushleft` `flushright` `quote` `quotation`; `skip`: `tikzpicture` `handtyped` `Verbatim` `lstlisting` `minted` `verbatim`; `math`: `equation` `equation*` `displaymath` `align*` `gather*` `flalign*` `alignat*` `multline*`; `math-rows`: `align` `gather` `flalign` `alignat`; `math-once`: `multline`; `list`: `enumerate` `itemize`; `list-plain`: `description` |

## Reported in the log

With the note and its line:

- a command or environment without a rule, in counted text (its arguments are then counted as text; the message names
  the rule to add);
- a citation key missing from the `.bib`;
- a glossary key missing from the glossary;
- a line of `wordcountRules` that is not a rule;
- a word-count key used in two chapters.

## Tests

`core/tests/wordcount.rs` checks the counter against reference counts on the documents in `core/tests/wordcount/`.
