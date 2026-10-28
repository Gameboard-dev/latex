---
tex: 3-fixture.tex
version: 1.0
wordcount: true
---
# Text and Emphasis
^sec--text

%% P1.1 | a paragraph label comment, not counted %%
A plain paragraph of running text with a one-to-one match, a state-of-the-art estimator, numbers such as 27, 3.5 and 1,000, a share of 50\% and 7\,\%, a (bracketed aside) and a sentence that ends here. Words separated only by a tie: Mr.~Smith and 372\,ms, and a thin space in 10\,000 people.

Emphasis takes *emphasised words*, _italic words_, **bold words** and ***bold italic words***, `code_name--with-dashes` and `two words`, <span style="color:red">red words here</span>, and a [link with text](https://example.org/a-b_c) to a page.

Quotes: “curly double quotes”, ‘curly single quotes’, it’s, the users’ choice, "straight double" and 'straight single' marks.

Dashes: pages 4--5, an en dash – here, an em dash — there, joined---up, spaced -- out, a lone - hyphen, and well-known self-evident co-operation.

Characters: café, naïve, Łukasz, Ångström, a×b, 5 × 6, x ≈ y, and an ellipsis… then on.

Very long words that may be hyphenated at the ends of lines: internationalisation, characterisation, incomprehensibilities, counterrevolutionaries, institutionalisation, electroencephalographically, uncharacteristically, telecommunications, interdisciplinarity, compartmentalisation, misunderstandings, disproportionately, representativeness, overgeneralisation, incontrovertibility.

%% A comment line with several words that are never counted %%
A line with an end comment %%c not counted%%
and its continuation.

## Citations
^sec--text--cites

A citation [@one]. One with a page [@one, p. 27]. A range [@two, pp. 4–5]. Two keys [@one; @two]. Three keys [@one; @two; @three]. A prefix [see @two, p. 2]. A locator on the first key [@one, p. 3; @two]. A prefix in the middle [@one; see @two, p. 4; @three]. Attached to a word[@one] and before a stop [@three].

As @one argues, and as @two [p. 4] shows, the claim holds. @three agree. @corp warns. @prefix replies. @edited collects. @accent compare. @particle notes. @others found it.

Raw forms: \citet{one,two} and \citep{three} and \citeauthor{two} and \textcite{three} and \parencite[p.~9]{one} and \Citet{accent}.

## Glossary and References
^sec--text--refs

Glossary terms: [[glossary#FNRATE|FNRATE]], [[glossary#FNRATE|FNRATEs]], [[glossary#^known-good|known good connection]], [[glossary#^known-good|Known good connection]], [[glossary#^known-good|known good connections]], [[glossary#W1|W1]], [[glossary#W1|W1s]], [[glossary#^shift|domain shift]], and [[glossary#FNRATE|the signal error]] once.

References: Section [[#Citations|Citations]], Sections [[#Citations|Citations]] and [[#Lists and Footnotes|Lists and Footnotes]], Table [[#^tab--demo|tab:demo]], Figure [[#^fig--demo|fig:demo]], Equation [[#^eq--demo|eq:demo]], Listing [[#^lst--bench|lst:bench]], [[3-fixture#^rq1|RQ1]] and [[3-fixture#^hyp2a|H2a]], and (Section [[#Citations|Citations]]) in brackets.

## Lists and Footnotes
^sec--text--lists

A sentence with a footnote^[A footnote of five words.] and another one^[A second footnote, citing @one with *emphasis*.] in it.

- A bullet item of six words
- A second bullet item
	- A nested bullet item
		- A third-level bullet
			- A fourth-level bullet

1. A numbered item
2. A second numbered item
	1. A nested numbered item
		1. A third-level numbered item
			1. A fourth-level numbered item

The labelled list:

- **RQ1** Does the first question hold? ^rq1
- **H2a** The second hypothesis, in *italics*. ^hyp2a

A paragraph ending in a label. ^sec--text--anchor

#### A Run-in Heading
Its text follows on the same line.

# Maths, Floats and Code
^sec--floats

Inline maths $x^2 + y$ in a sentence, an $n$-fold rise, a 10$\times$ speed-up, $\alpha$-level tests, a value of $5$\% and $p<0.05$.

$$
a = b + c
$$

The display above has no number.

$$
E = m c^2
$$

^eq--demo

The equation above is numbered.

$$
\begin{align}
  a &= b + c \\
  d &= e \nonumber \\
  f &= g \tag{T}
\end{align}
$$

$$
\begin{align*}
  a &= b \\
  c &= d
\end{align*}
$$

$$
\begin{gather}
  a = b \\
  c = d
\end{gather}
$$

$$
\begin{multline}
  a + b + c \\
  = d + e
\end{multline}
$$

$$
\begin{equation*}
  x = y
\end{equation*}
$$

A table follows.

| Column one | Column two |
|---|:-:|
| table words | more table words |
| a [[glossary#FNRATE\|FNRATE]] cell | \cite{one} |

^tab--demo

((<tag|Table)) A table caption of several words, citing [@one].

```tikz
\begin{document}
\begin{tikzpicture}
\node (a) {picture words};
\end{tikzpicture}
\end{document}
```

^fig--demo

((<tag|Figure)) A figure caption of several words.

Listing: A listing caption of several words ^lst--bench

```
verbatim code block words
```

^FloatBarrier

{\wordcountoff These words are left out of the count.} Counted again after the group.

## Unnumbered Heading {-}
^sec--floats--unnumbered

Words under an unnumbered heading.

___

A paragraph after a divider.
