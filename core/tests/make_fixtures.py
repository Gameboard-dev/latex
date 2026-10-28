"""tests/fixtures.json for python.rs: synthetic notes through latex_sync.md_to_tex (the script the plugin was ported from)
and, where they round-trip, through its LaTeX -> Markdown import and back.

    python -I make_fixtures.py SCRATCH_FOLDER [OUT]     (OUT: fixtures.json beside this file)

Writes only a preamble note into SCRATCH_FOLDER (latex_sync reads the preamble's commands from its notes folder) and the
JSON; never the real notes or their chapters.

The script was archived on 2026-10-25, when this plugin replaced it: it is imported from the archived copy,
archived/latex-sync-python/latex_sync.py (formerly zotsearch/tools/latex_sync.py)."""
import json, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[3] / "archived" / "latex-sync-python"))
import latex_sync as ls

SCRATCH = Path(sys.argv[1])
OUT = Path(sys.argv[2]) if len(sys.argv) > 2 else HERE / "fixtures.json"
SCRATCH.mkdir(parents=True, exist_ok=True)
PREAMBLE = ("---\ntex: 0-preamble.tex\n---\n```latex\n\\newcommand{\\startappendices}{\\appendix}\n\\newcommand{\\printterms}{\\printglossary}\n"
            "\\DeclareRobustCommand{\\subappendix}{x}\n\\newcommand{\\withargs}[1]{#1}\n```\n")
(SCRATCH / "0-preamble.md").write_text(PREAMBLE, encoding="utf-8", newline="\n")
ls.NOTES_DIR = SCRATCH
ls.TEX_DIR = SCRATCH / "unused"
ls._COMMANDS.clear()
# the script named its own vault in the header; the plugin's default vault name (Options::default) is "Vault"
VAULT = "Vault"
SCRIPT_VAULT = ls.re.compile(r"(?<=Generated from )\w+(?=/LaTeX/)")
ls.HEADER = SCRIPT_VAULT.sub(VAULT, ls.HEADER, count=1)
ls.HEADER_RX = ls.re.compile(SCRIPT_VAULT.sub(VAULT, ls.HEADER_RX.pattern, count=1))


class _Fixed(ls.datetime):
    """The import stamps last_updated with the time it ran: a fixed time keeps the fixtures the same on every run."""

    @classmethod
    def now(cls, tz=None):
        return cls(2026, 10, 25, 0, 52)


ls.datetime = _Fixed
# Words from the script's own project, replaced by neutral ones in the fixtures: a name in its synthetic chapter, and the
# example glossary link of a warning (the plugin's message names NetIDS).
NEUTRAL = {"quantize_static": "batch_static", r"quantize\_static": r"batch\_static"}
EXAMPLE_LINK = ls.re.compile(r"\[(\w+)\]\(glossary\.md#\1\)")


def neutral(s):
    for old, new in NEUTRAL.items():
        s = s.replace(old, new)
    return EXAMPLE_LINK.sub("[NetIDS](glossary.md#NetIDS)", s)

GLOSS = """---
tex: glossary.tex
---
%% A comment.%%

# FNRATE
**Long form:** signal connection classification error rate

The proportion of signals accepted.

# W1
**Long form:** Wasserstein-1 distance
**Printed as:** $W_1$

A distance between two distributions.

# Known Good Connection
^known-good

A connection with no attempt to interfere.
"""
cases = []


def run(note, md, notes):
    ls.collect_headings(notes)
    return ls.md_to_tex(note, md)


def case(name, note, md, others=(), rt=True):
    notes = {"0-preamble.md": PREAMBLE, note: md}
    for n, t in others:
        notes[n] = t
    try:
        tex, warn = run(note, md, notes)
    except Exception as e:
        cases.append({"name": name, "note": note, "md": md, "notes": notes, "error": f"{type(e).__name__}: {e}"})
        return
    cases.append({"name": name, "note": note, "md": md, "notes": notes, "tex": tex, "warnings": warn})
    if rt:
        texts = {n: run(n, t, notes)[0] for n, t in notes.items()}
        md2 = ls.TexToMd(note, tex, ls.build_index(texts), {}).run()
        notes2 = dict(notes)
        notes2[note] = md2
        tex2, warn2 = run(note, md2, notes2)
        cases.append({"name": name + " (round trip)", "note": note, "md": md2, "notes": notes2, "tex": tex2, "warnings": warn2})


def one(name, line, others=(), note="5-methodology.md"):
    case(name, note, "---\ntex: 5-methodology.tex\n---\n" + line + "\n", others, rt=False)


# the synthetic chapter of test_latex_sync.py, imported as a note by TexToMd
TEST = (HERE.parents[3] / "archived" / "latex-sync-python" / "test_latex_sync.py").read_text(encoding="utf-8")
src = neutral(TEST.split('TEX = r"""', 1)[1].split('"""', 1)[0])
md = ls.TexToMd("5-methodology.md", src, ls.build_index({"5-methodology.md": src}), {}).run()
case("synthetic chapter", "5-methodology.md", md)
extra = md.replace("This is *an* experiment", "%%[flag] check this%%\nThis is *an* [[EV-0001]] experiment") \
          .replace("[@oates2006, p. 127]", "[@oates2006, p. 127] [❝](zotero://open-pdf/library/items/ABCD1234?annotation=EFGH5678)")
extra = extra.replace("# Research Design\n^sec--design\n", "# Research Design\n^sec--design\n> [!info]- Notes for the author\n> not in LaTeX\n")
case("Obsidian-only content", "5-methodology.md", extra, rt=False)

LINES = [
    "[see @a2020, p. 2]", "@a2020 found it. @b2021 did not.", "as Smith et al. @b2021 say",
    "A link [[#^sec--design]] and a [site](https://example.org).", "A note^[with *emphasis*].",
    "%% a comment line%%", "%%[note] for Obsidian only%%",
    "rather than an average [[EV-0215]], [[EV-0308]], and it calls", "as error [[EV-0379]] %%[ev] R-770559a0%%. Next",
    "smaller (Figure~[[#^sec--design|sec:design]]) [[EV-0300]], [[EV-0229]], [[EV-0136]].",
    "as [[3-introduction#^hyp2a|H2a]] and [[#^rq1|RQ1]] ask", "As @oates2006 [[EV-0260]] showed.",
    "See [a site](https://example.org/#top).",
    # beyond test_latex_sync.py: every inline rule and warning
    "[@a; @b, p. 3] and [@a, p. 2; @b, p. 3] and [see @a; also @b] and [@a, pp. 4-5]",
    "@Key2020 [p. 4] says, and @k2 [[EV-1]] and @k3 [see](https://x.org) and e-mail a@b.org",
    "Text with $a_1 \\$ b$ maths, `code_x {y}` and \\% and \\textbf{x} and \\\\ end",
    "*a **b** c* and **bold *it* x** and _it_ and snake_case_word and _ space and * star and ** open",
    "Quotes “double” and ‘single’ and it’s, with a non\u00a0breaking space and ünïcödé and 🙂 emoji.",
    "A <span style=\"color:red\">red</span> and <span style=\"color: blue;\">blue</span> and </span> stray",
    "<span style=\"color:green\">never closed", "a ==highlight== and ~~strike~~ text",
    "embed ![[image.png]] here and ![[never closed", "a %% mid %% comment and more",
    "unclosed %% comment", "unclosed %%[tag] comment", "end %%c%%   ", "trailing space %%[ev] x%%y and %%[ev] z%% w",
    "See Section [[#Missing heading|x]] and [[other#Nope]] and [[#^bad----id|x]] and [[#^rq|Q1]]",
    "As Table [[#^tab--x|tab:x]] and Tables [[#^tab--y]] and XTable [[#^tab--z]] and Hypotheses [[#^hyp|H1]].",
    "[[#^rq|RQ1]], [[#^hyp|H2a]] and [[3-introduction#^rq12|RQ12]] and [[#^rqx|x]]",
    "a [[Some note]] b [[Some note|alias]] c [[EV-1]],[[EV-2]] d [[EV-3]] , [[EV-4]]",
    "a footnote^[with [brackets] inside] and ^[unclosed and ^ caret",
    "[a [nested] bracket] and [plain] and [@bad key] and [x](y) and [unclosed",
    "@a2020. @b2021? @c2021! e.g. @d2020 and (@e2020) and “@f2020” and Fig. @g2020",
    "Paragraph with id ^sec--para",
    "x [link](note.md#Heading) and [g](glossary.md#Nope) and [h](https://a.b/c#d) and [z](zotero://select/x)",
    "  indented paragraph with \\gls{x} and  double  spaces  ",
    # white space beyond ASCII, which Python's \s matches
    "[@a1; @b2] and [@a1, p. 2] and [see @a1] and text ^sec--nbsp",
    "%% x%%　",
    "a ápp. 2 and (p. 3) and pp. 4 and _p. 5",
]
for k, l in enumerate(LINES):
    one(f"inline {k}: {l[:40]}", l, others=[("glossary.md", GLOSS)] if k > 12 else ())
one("bad paragraph id", "Paragraph with bad id ^a----b")

case("heading link and tie", "5-methodology.md", "---\ntex: 5-methodology.tex\n---\n# Research Design\n^sec--design\n\n"
     "as in [[#^sec--design|any alias]] and Appendix [[#Research Design|Research Design]]\n", rt=False)

counted = "---\ntex: 5-methodology.tex\nwordcount: true\n---\n# Research Philosophy\n^sec--philosophy\n\nText.\n\n# Research Methodology\n^sec--methodology\n\nMore.\n"
case("word counts", "5-methodology.md", counted)
case("word count without label", "5-methodology.md", "---\nwordcount: \"true\"\n---\n# Unlabelled Section: Two!\n\nText.\n## Sub\n^sec--sub\n", rt=False)
table = """---
tex: 3-introduction.tex
---
Before.

| Fruit | Colour | Season | Price |
| --- | :-: | :-: | --: |
| Apple (dessert, home grown) | red | autumn | low |
| Lemon (imported, unwaxed) | yellow | winter | high [@doe2024] |

Table: The fruit of the season, adapted from @doe2024.

^tab--three---class

After.
"""
case("table", "3-introduction.md", table)
wide = table.replace("| Apple (dessert, home grown) |", "| Apple: a dessert apple grown in an orchard and picked by hand in October |")
case("wide table", "3-introduction.md", wide)
case("hinted table", "3-introduction.md", table.replace("\n| Fruit |", "\n%%[cols] P{4.4cm}ccc%%\n\n| Fruit |"))
case("sized table", "3-introduction.md", table.replace("\n| Fruit |", "\n%%[size] footnotesize%%\n\n| Fruit |"))
case("bad size", "3-introduction.md", table.replace("\n| Fruit |", "\n%%[size] huge%%\n\n| Fruit |"), rt=False)
case("tag caption", "3-introduction.md", table.replace("Table: The fruit of the season, adapted from @doe2024.\n\n^tab--three---class",
                                                    "^tab--three---class\n\n((<tag|Table)) The fruit, with \\| and [FNRATE](glossary.md#FNRATE) $x|y$."),
     others=[("glossary.md", GLOSS)])
case("caption with pipe", "3-introduction.md", table.replace("Table: The fruit of the season, adapted from @doe2024.\n\n^tab--three---class",
                                                          "^tab--three---class\n\n((<tag|Table)) The [[glossary#FNRATE|FNRATE]]."), others=[("glossary.md", GLOSS)], rt=False)
case("table without caption or alignment", "5-methodology.md", "| a | b |\n| c | d | e |\n\nText.\n", rt=False)
case("six columns", "5-methodology.md", "| a | b | c | d | e | f |\n| - | - | - | - | - | - |\n| 1 | 2 | 3 | 4 | 5 |\n\n^tab--six\n\n((<tag|Caption|#000000|#ffffff)) Six.\n")
case("table caption without id", "5-methodology.md", "| a |\n| --- |\n| 1 |\n\n((<tag|Table)) No id.\n", rt=False)

labelled = """---
tex: 3-introduction.tex
---
- **RQ1** Does increasing the ratio increase \\gls{FNRATE}? ^rq1
%% This is deliberately imprecise.%%
- **H2a** _Null:_ It does not. _Alternative:_ It does. (RQ1) ^hyp2a

^rq

After.
"""
case("labelled list", "3-introduction.md", labelled)
case("labelled list with blanks and tags", "3-introduction.md", "- **RQ1** One ^rq1\n\n%%[flag] x%%\n  %% indented%%\n- **RQ2** Two ^rq2\n^rq\nText.\n", rt=False)
case("bold item without id", "5-methodology.md", "---\ntex: 5-methodology.tex\n---\n- **Bold** but no id\n")
case("lists", "5-methodology.md", "- one\n- two ^sec--two\n1. three\n2. four\n  - nested\n\n10. ten\n-not a list\n", rt=False)
eq = "---\ntex: 5-methodology.tex\n---\n$$\n  y = x .\n$$\n\n^eq--model\n\n$$\n\\begin{align}\n  a &= b\n\\end{align}\n$$\n\n^eq--al\n\n$$\nz\n$$\nEnd.\n"
case("equations", "5-methodology.md", eq)
case("equation forms", "5-methodology.md", "$$ x = 1 $$\n$$x$$\n\n$$\nx \\label{eq:own}\n$$\n\n^eq--own\n\n$$\nunclosed\n", rt=False)
fig = ("---\ntex: 4-literature_review.tex\n---\n```tikz\n\\usetikzlibrary{positioning,calc,decorations.pathreplacing}\n\\begin{document}\n"
       "\\begin{tikzpicture}\n  \\draw (0,0) -- (1,1);\n\\end{tikzpicture}\n\\end{document}\n```\n\n^fig--line\n\n((<tag|Figure)) A line [@oates2006].\n\nAfter.\n")
case("figure", "4-literature_review.md", fig)
for k, old in enumerate(("^fig--line\n\n((<tag|Caption|#000000|#ffffff)) A line [@oates2006].", "^fig--line\n\n^caption - A line [@oates2006].",
                         "^fig--line\n\n^caption: A line [@oates2006].", "Figure: A line [@oates2006]. ^fig--line", "Figure: A line.\n\n^fig--line")):
    case(f"figure caption form {k}", "4-literature_review.md", fig.replace("^fig--line\n\n((<tag|Figure)) A line [@oates2006].", old), rt=False)
case("resized figure", "4-literature_review.md", fig.replace("```tikz", "%%[resize] \\textwidth%%\n\n```tikz", 1))
case("tikz extras", "4-literature_review.md", "```tikz\n\\usepackage{pgfplots}\n\\usetikzlibrary{arrows.meta, calc}\n\\begin{document}\n\\begin{tikzpicture}\n"
     "\\end{tikzpicture}\n\\end{document}\n```\n\n((<tag|Figure)) No id.\n\n```tikz\n\\begin{tikzpicture}\n\\end{tikzpicture}\n```\nText.\n", rt=False)
case("fences", "5-methodology.md", "```python\nprint(1)\n```\n```latex\n\\foo\n```\n```\nunclosed\n", rt=False)
misc = ("---\ntex: 9-appendices.tex\n---\n# Appendices {-}\n^app--appendices\n%% a rule%%\n^FloatBarrier\n\nText.\n\n"
        "Listing: The core of the harness. ^lst--bench\n\nEnd.\n")
case("commands, starred heading, listing", "9-appendix.md", misc)
case("preamble commands", "9-appendix.md", "^startappendices\n^printterms\n^subappendix\n^withargs\n^unknown\n\n^blockid\n\n## Starred {-}\n#### Para {-}\n"
     "Text after.\n#### Labelled\n^sec--lab\nText.\n##### Five\n###### Six\n####### Seven\n#### Title\n\n#hashtag\n", rt=False)
case("listing forms", "9-appendix.md", "Listing: no id\n\nListing: wrong ^tab--x\n\nListing: later\n\n^lst--later\n\nText.\n", rt=False)
title = """---
tex: 1-title_page.tex
version: 1.0
title: "A Sample Title:"
student_id: "000000000"
date: "[Month Year]"
quote: 'it''s'
---
%%[flag] a note for the author%%
"""
case("title page", "1-title_page.md", title)
case("html placeholders", "9-appendix.md", "---\ntex: 9-appendices.tex\n---\n%% one \\term{<key>}{<name>}%%\nText <span style=\"color:red\">x</span> and `<code>`.\n"
     "```\n<inside fence>\n```\n<spanx> and <b>\n", rt=False)
case("glossary", "glossary.md", GLOSS)
case("glossary links", "5-methodology.md", "---\ntex: 5-methodology.tex\n---\nThe [[glossary#FNRATE|FNRATE]] and [[glossary#W1|W1]], [[glossary#^known-good|Known good connection]] "
     "and [[glossary#FNRATE|FNRATEs]] and [[glossary#FNRATE|the error]] and [[glossary#^knowngood|known good connections]] and [[glossary#FNRATE]] "
     "and [[glossary#APCR|APCR]] and [[glossary#^nope|x]] and [The FNRATE](glossary.md#FNRATE), [Known good connection](glossary#%5Eknown-good) and [W1](glossary.md#W1).\n",
     others=[("glossary.md", GLOSS)], rt=False)
case("glossary links without a glossary", "5-methodology.md", "---\ntex: 5-methodology.tex\n---\nThe [[glossary#FNRATE|FNRATE]] and [[glossary#^x-y|x y]].\n", rt=False)
EQ = GLOSS.replace("The proportion of signals accepted.\n",
                   "The proportion of signals accepted:\n\n$$\n\\mathrm{FNRATE}_s = \\frac{1}{N_s}\\sum_{i=1}^{N_s}(1-\\mathrm{Res}_i)\n$$\n\n"
                   "where $\\mathrm{Res}_i = 1$ for an signal decision.\n")
case("glossary equations", "glossary.md", EQ)
case("glossary edge cases", "glossary.md", "---\ntex: glossary.tex\n---\nText first.\n%% block\nmore\nend%%\n\n# Empty Entry\n\n# Spaced  \n\n\n^spaced-id\n\n"
     "**Printed as:** \n\nDesc with [[glossary#FNRATE|FNRATE]].\n# Area Under the Curve of it\n**Long form:** x\n# AUC\n"
     "**Long form:** area under the receiver-operating characteristic curve of the \\emph{x} and $y$\n\nA.\n$$\nstray\n$$\n", rt=False)
cm = "---\ntex: 5-methodology.tex\n---\n___\n%% First line\nsecond line\n\nfourth%%\n%%[flag] for Obsidian%%\nText.\n"
case("comment blocks", "5-methodology.md", cm)
case("comment block forms", "5-methodology.md", "%%\nonly middle\n%%\n%%[tag] tagged\nblock%%\n  %% indented%%\n%% unclosed\nto the end\n", rt=False)
case("non-ASCII white space in blocks", "5-methodology.md", "| a |\n| --- |\n| 1 |\n\nTable: Cap. ^tab--n\n\n"
     "- **RQ1** One ^rq1\n\n%%[cols] l%%\n\n# Heading {-}\n**x**\n", rt=False)
case("quotes and callouts", "5-methodology.md", "> quoted *text*\n> second\n\n> [!info]- Notes\n> hidden\nAfter.\n", rt=False)
case("no trailing newline", "5-methodology.md", "---\ntex: x.tex\n---\n| a |\n| --- |\n| 1 |", rt=False)
case("no properties", "5-methodology.md", "Just text, [[#Nope]].\n", rt=False)
case("unclosed properties", "5-methodology.md", "---\ntex: x\nText.\n", rt=False)
case("bad heading label", "5-methodology.md", "# H\n^a----b\n", rt=False)
case("bad quoted property", "5-methodology.md", "---\nwordcount: \"tr\\qe\"\n---\nText.\n", rt=False)
case("heading references across notes", "5-methodology.md",
     "See Section [[3-introduction#Aims {-}|Aims]] and Chapter [[3-introduction#Aims|Aims]] and [[#Local|L]].\n## Local\n^sec--local\n",
     others=[("3-introduction.md", "# Aims {-}\n^sec--aims\n")], rt=False)

for c in cases:
    if "warnings" in c:
        c["warnings"] = [neutral(w) for w in c["warnings"]]
OUT.write_text(json.dumps(cases, indent=1, ensure_ascii=False), encoding="utf-8", newline="\n")
print(len(cases), "cases;", sum("error" in c for c in cases), "errors")
