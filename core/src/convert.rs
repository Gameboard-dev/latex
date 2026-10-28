//! A note to its LaTeX chapter: the rules of latex_sync.py's md_to_tex (zotsearch/tools), ported rule for rule, so that a
//! note gives the same .tex from either. Each line of a note becomes one or more lines of LaTeX, in order; nothing is
//! reflowed. The rules (README.md lists them with examples):
//!
//! - Properties: `tex`, `version`, `last_updated` give the first line, from the header template (Settings); the second
//!   is `% !TEX root = <main>.tex`. `wordcount: true` wraps each `# ` section in \startwordcount{key} ... \printwordcount{key}
//!   (key: its label without `sec:`) and ends the chapter with \stopwordcount. The title-page note's other properties
//!   become `\newcommand{\tpTitle}{...}` ... and \maketitlepage.
//! - Comments: `%% text %%` is a LaTeX comment line, `text %%c%%` an end-of-line comment, several lines one `%%` block;
//!   `%%[tag] ...%%` is for Obsidian only and left out; `___` is a divider, `% ====...`.
//! - Left out: callouts (`> [!info]-` and their `> ` lines), links to other notes (`[[EV-0012]]`: evidence, with a
//!   space before them and the commas that join a run of them), links to Zotero (`[❝](zotero://...)`).
//! - Blocks: ```latex is copied (with a warning outside the preamble), ```tikz followed by its `^fig--name` id and a
//!   `((<tag|Figure)) caption` line is a figure float, a block of code (```rust, ```python …) is a hand-typed listing
//!   whose code goes to the output's code folder (CodeFile; app.rs highlights it with Pygments, as latexmkrc does for
//!   a hand-made code/NAME.rs), numbered when a `^lst--name` id and a caption line follow it, any other fence is
//!   verbatim; `$$ ... $$` is \[ \] or, with an
//!   `^eq--name` id after it, a numbered equation labelled from the id; `^FloatBarrier` (and every argument-free
//!   command the preamble defines) is that command; `Listing: caption ^lst--name` is the hand-typed listing.
//! - Headings `#`..`####` are \section..\paragraph (` {-}` starred, still listed in the contents), with the `^sec--x`
//!   lines under them as labels (`:` written `--`, `_` written `---`); a trailing ` ^id` on a paragraph or list item is
//!   \phantomsection\label.
//! - Inline: links to headings and ids are \ref (a tie after Section, Table ...), `^rq1`/`^hyp2a` ids are \rqref/\hypref,
//!   glossary links are \gls/\Gls/\glspl/\glslink, Pandoc citations are biblatex (\cite, \citet/\Citet, \cites),
//!   `*x*` \emph, `_x_` \textit, `**x**` \textbf, `` `x` `` \texttt, coloured spans \textcolor, `^[x]` \footnote, Markdown
//!   links \href, curly quotes TeX quotes. Everything else is LaTeX already.
//! - Lists (itemize, enumerate, labelled lists `- **H2a** text ^hyp2a`), tables (booktabs; size and widths automatic,
//!   `%%[cols]` and `%%[size]` above to override), and the glossary note (`# heading` entries -> \term).
//!
//! Whatever a rule cannot represent is a warning, never dropped in silence. The plugin's settings replace what the
//! Python script fixed for one thesis (`Options`); their defaults give the same output.
//!
//! Python indexes strings by character, this by byte: every step over text moves a whole character (`next`), and
//! positions are only compared within the same string.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex_lite::Regex;

/// What the Python script fixed and the plugin's settings now set (app.rs fills it); `Default` is the author's setup.
#[derive(Clone, Debug)]
pub struct Options {
    /// The first line of every chapter: {vault}, {path} (the note's path in the vault), {note}, {version}, {updated}.
    pub header: String,
    pub vault: String,
    /// The notes' folder in the vault, for {path}.
    pub folder: String,
    /// The main file's name without .tex: `% !TEX root = main.tex`.
    pub main: String,
    /// The notes (names without .md) with a role of their own.
    pub preamble: String,
    pub glossary: String,
    pub title_page: String,
    /// What the preamble loads with \usetikzlibrary; a ```tikz block that loads more is warned about.
    pub tikz_libraries: String,
    /// The hand-typed listing: `\begin{<env>}[htbp] \<cmd>{name} \caption \label \end{<env>}`.
    pub listing_env: String,
    pub listing_cmd: String,
    /// The property that turns word counting on, and the three macros.
    pub wordcount: String,
    pub count_start: String,
    pub count_print: String,
    pub count_stop: String,
    /// The property the plugin stamps with the time of the last change (`last_updated`).
    pub updated: String,
    /// The commands a `^name` line gives besides those the preamble defines without arguments.
    pub commands: Vec<String>,
    /// A reference after one of these words is tied to it: `Section~\ref{...}`.
    pub ref_words: Vec<String>,
    /// A full stop after these does not end a sentence (so @key there is \citet, not \Citet).
    pub abbreviations: Vec<String>,
    /// Labelled lists' references: an id prefix, its command, its alias prefix (`^rq1` -> \rqref{1}, alias RQ1).
    pub lists: Vec<(String, String, String)>,
    /// The glossary entry command, \term[text]{key}{name}{long form}{description}.
    pub term: String,
    /// The title page: \newcommand{\<prefix>Field}{...} for each property, then \<make_title>.
    pub title_prefix: String,
    pub make_title: String,
    /// Tables: a row this long (in characters) fits the line; a longer column wraps; \scriptsize from this many columns.
    pub line_chars: usize,
    pub wrap_chars: usize,
    pub small_from: usize,
    /// Words an acronym's long form keeps in lower case (title case).
    pub small_words: Vec<String>,
}

/// A comma-separated setting as its items.
pub fn words(s: &str) -> Vec<String> {
    s.split(',').map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect()
}

impl Default for Options {
    fn default() -> Self {
        Options {
            header: DEFAULT_HEADER.into(),
            vault: "Vault".into(),
            folder: "LaTeX".into(),
            main: "main".into(),
            preamble: "0-preamble".into(),
            glossary: "glossary".into(),
            title_page: "1-title_page".into(),
            tikz_libraries: "positioning,calc,decorations.pathreplacing".into(),
            listing_env: "handtyped".into(),
            listing_cmd: "handtypedcode".into(),
            wordcount: "wordcount".into(),
            count_start: "startwordcount".into(),
            count_print: "printwordcount".into(),
            count_stop: "stopwordcount".into(),
            updated: "last_updated".into(),
            commands: words(DEFAULT_COMMANDS),
            ref_words: words(DEFAULT_REF_WORDS),
            abbreviations: words(DEFAULT_ABBREVIATIONS),
            lists: parse_lists(DEFAULT_LISTS),
            term: "term".into(),
            title_prefix: "tp".into(),
            make_title: "maketitlepage".into(),
            line_chars: 85,
            wrap_chars: 20,
            small_from: 6,
            small_words: words(DEFAULT_SMALL_WORDS),
        }
    }
}

pub const DEFAULT_HEADER: &str = "% Generated from {vault}/{path} (version {version}, last updated {updated}). Edit the note: this file is rewritten from it on every save.";
pub const DEFAULT_COMMANDS: &str = "FloatBarrier, clearpage, newpage, cleardoublepage";
pub const DEFAULT_REF_WORDS: &str = "Section, Sections, Appendix, Appendices, Table, Tables, Figure, Figures, Equation, Equations, Chapter, Chapters, Listing, Listings, Hypothesis, Hypotheses, Question, Questions";
pub const DEFAULT_ABBREVIATIONS: &str = "al., e.g., i.e., cf., vs., Fig., Figs., Eq., Eqs., No., p., pp., Sec., Ch., approx.";
pub const DEFAULT_LISTS: &str = "rq rqref RQ, hyp hypref H";
pub const DEFAULT_SMALL_WORDS: &str = "a, an, the, and, but, or, nor, of, in, on, at, to, for, by, with, as, per, via";

/// "rq rqref RQ, hyp hypref H" -> (id prefix, command, alias prefix) triples; an incomplete item is skipped.
pub fn parse_lists(s: &str) -> Vec<(String, String, String)> {
    s.split(',')
        .filter_map(|item| {
            let p: Vec<&str> = item.split_whitespace().collect();
            (p.len() == 3).then(|| (p[0].to_string(), p[1].trim_start_matches('\\').to_string(), p[2].to_string()))
        })
        .collect()
}

/// Python's `\s` (what str.isspace accepts: Unicode's white space and the separators \x1c-\x1f). regex-lite's classes
/// are ASCII only, so every pattern's `\s` and `\S` is written out as this class (`py`): a non-breaking space in a note
/// is white space to both converters. (`\d` and `\w` stay ASCII; the rules use them only on ids and numbers, and the
/// few Unicode word tests are written by hand: `is_word`.)
const PY_SPACE: &str = r"\t\n\x{0B}\x{0C}\r\x{1C}-\x{1F} \x{85}\x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}";

/// A pattern with Python's white space for `\s` and `\S`.
pub(crate) fn py(re: &str) -> String {
    let mut out = String::with_capacity(re.len());
    let mut chars = re.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out += &format!("[{PY_SPACE}]"),
            Some('S') => out += &format!("[^{PY_SPACE}]"),
            Some(n) => {
                out.push('\\');
                out.push(n);
            }
            None => out.push('\\'),
        }
    }
    out
}

macro_rules! rx {
    ($name:ident, $re:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new(&crate::convert::py($re)).unwrap());
    };
}
pub(crate) use rx;

const CITE_KEY: &str = r"[A-Za-z][A-Za-z0-9_:.\-]*[A-Za-z0-9]"; // what Zotero Cite Links recognises after @
rx!(CITE_AT, &format!("^{CITE_KEY}"));
rx!(CITE_ITEM, &format!(r"^(?P<pre>[^@]*?)\s*-?@(?P<key>{CITE_KEY})(?:,\s*(?P<loc>.*))?$"));
rx!(LABEL_RX, r"^[A-Za-z0-9]+(?:[-:_][A-Za-z0-9]+)*$");
rx!(ID_RX, r"^[A-Za-z0-9]+(?:-{1,3}[A-Za-z0-9]+)*$");
rx!(DASHES, r"-+");
rx!(LIST_RX, r"^(\s*)(-|\d+\.) (.*)$");
rx!(ID_LINE, r"^\^([A-Za-z0-9-]+)$");
rx!(TRAIL_ID, r"\s\^([A-Za-z0-9-]+)$");
rx!(COMMENT_LINE, r"^(\s*)%%(.*)%%\s*$");
rx!(TAG, r"^\[[A-Za-z]+\]");
rx!(TAG_COMMENT, r"^%%\[[A-Za-z]+\]");
// "((<tag|Figure)) The design ..." under a table's or figure's id: an arrow tag (Tags for Markdown; any label, optional
// background and text colours) then the caption; the older "^caption - ..." and "^caption: ..." are also read
rx!(CAPTION_LINE, r"^(?:\(\(<?tag[|/][^|/)]+(?:[|/][^|/)]*){0,2}\)\)|\^caption(?:\s+-|:))\s+(.*?\S)\s*$");
rx!(CAPTION_RX, r"^(Table|Figure|Listing):\s+(.*?\S)(?:\s+\^([A-Za-z0-9-]+))?\s*$"); // "Table: caption ^tab--name"
rx!(HTML_LIKE, r"^/?[A-Za-z][A-Za-z ]*>"); // after "<": <key>, <long form>
rx!(CODE_SPAN, r"`[^`]*`");
rx!(COMMAND_LINE, r"^\^([A-Za-z]+)$"); // "^FloatBarrier": a command with no arguments, on a line of its own
rx!(DEFINED, r"\\(?:newcommand|DeclareRobustCommand)\*?\{\\([A-Za-z]+)\}\{");
rx!(HINT_RX, r"^%%\[(cols|size|resize)\]\s*(.*?)\s*%%$");
rx!(TABLE_SIZES, r"^(?:normalsize|small|footnotesize|scriptsize|tiny)$");
rx!(SEP_CELL, r"^\s*:?-+:?\s*$");
rx!(LABELLED_ITEM, r"^- \*\*([^*]+)\*\*\s+(.*\S)\s\^([A-Za-z0-9-]+)$"); // "- **H2a** text ^hyp2a"
rx!(FIELD_RX, r"^\*\*(Long form|Printed as):\*\*\s*(.*?)\s*$");
rx!(HEADING, r"^(#{1,6}) (.*?)( \{-\})?$");
rx!(HEADING4, r"^(#{1,4}) (.*?)( \{-\})?$");
rx!(ENTRY, r"^# (.+?)\s*$");
rx!(UNNUMBERED, r"\s*\{-\}$");
rx!(TIKZ_ONLY, r"^(?:\\(usepackage|usetikzlibrary)(\[[^\]]*\])?\{[^}]*\}|\\(begin|end)\{document\})$");
rx!(TIKZ_LIB, r"^\\usetikzlibrary\{([^}]*)\}$");
rx!(CMD_NAME, r"^\\[A-Za-z@]+");
rx!(CLOSERS, r"['”)\]]+$");
rx!(SPAN_OPEN, r#"^<span style="color:\s*([A-Za-z!0-9]+);?">"#);
rx!(EV_COMMA, r"^,\s*");
rx!(EV_NEXT, r"^\[\[[^\]#]*\]\]");
rx!(URL_SCHEME, r"^[A-Za-z][A-Za-z0-9+.-]*:");
rx!(LABEL_ALIAS, r"^[A-Za-z]+:[A-Za-z0-9:_.\-]+$");
rx!(CELL_CITE, r"\[[^\]]*@[^\]]*\]");
rx!(CELL_CMD_ARG, r"\\[A-Za-z]+\*?(?:\[[^\]]*\])?\{([^{}]*)\}");
rx!(CELL_MARKUP, r"\\[A-Za-z]+\*?|[*_`$\\]");
rx!(TT_SPECIAL, r"([_#%&$\{\}])");
rx!(NOT_KEY_CHARS, r"[^a-z0-9]+");

const DIVIDER_FILL: usize = 69;
/// Every figure's picture is shrunk to the line width if it is wider, and left at its own size otherwise (a size has
/// no place in the note). \width is the picture's natural width inside \resizebox (graphicx).
pub const FIT: &str = r"\ifdim\width>\linewidth\linewidth\else\width\fi";

pub type Result<T> = std::result::Result<T, String>;

// ------------------------------------------------------------------ small helpers

/// The index after the character at `i`.
fn next(s: &str, i: usize) -> usize {
    i + s[i..].chars().next().map_or(1, char::len_utf8)
}

/// After a backslash at `i` and the character it escapes (Python's `i += 2`), at most the end.
fn skip2(s: &str, i: usize) -> usize {
    if i + 1 >= s.len() {
        s.len()
    } else {
        next(s, i + 1)
    }
}

fn find(s: &str, pat: &str, from: usize) -> Option<usize> {
    s.get(from..)?.find(pat).map(|p| p + from)
}

fn char_at(s: &str, i: usize) -> Option<char> {
    s.get(i..)?.chars().next()
}

/// Python's `\w`: a letter, a digit (of any script) or `_`.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `s` starts with `word` and a word boundary follows (Python's `word\b`, Unicode).
fn starts_with_word(s: &str, word: &str) -> bool {
    s.strip_prefix(word).is_some_and(|r| r.chars().next().is_none_or(|c| !is_word(c)))
}

/// "p. 27" -> "p.~27", "pp. " -> "pp.~" after a word boundary (latex_sync.py's `\b(pp?)\. `).
fn tie_pages(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let boundary = s[..i].chars().next_back().is_none_or(|c| !is_word(c));
        let hit = ["pp. ", "p. "].into_iter().find(|p| boundary && s[i..].starts_with(p));
        match hit {
            Some(p) => {
                out += &p.replace(". ", ".~");
                i += p.len();
            }
            None => {
                let e = next(s, i);
                out += &s[i..e];
                i = e;
            }
        }
    }
    out
}

/// A labelled list's number, `2a` of `^hyp2a` (latex_sync.py's `\d\w*`).
fn is_list_number(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|d| d.is_ascii_digit()) && c.all(is_word)
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn strip_hyphens(s: &str) -> &str {
    s.trim_matches('-')
}

/// Python's repr of a string, as its warnings quote text.
fn repr(s: &str) -> String {
    let q = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut out = String::from(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

/// A label as an Obsidian block id: `:` -> `--`, `_` -> `---` (sec:lit:compgen <-> ^sec--lit--compgen).
pub fn block_id(label: &str) -> Result<String> {
    if !LABEL_RX.is_match(label) {
        return Err(format!("label {} cannot be an Obsidian block id (letters, digits and single - : _ between them)", repr(label)));
    }
    Ok(label.replace('_', "---").replace(':', "--"))
}

/// The label a block id stands for: runs of one, two, three dashes are `-`, `:`, `_`.
pub fn label_of(bid: &str) -> Result<String> {
    if !ID_RX.is_match(bid) {
        return Err(format!("block id ^{bid} is not one this script writes (runs of one to three dashes)"));
    }
    Ok(DASHES.replace_all(bid, |c: &regex_lite::Captures| ["-", ":", "_"][c[0].len() - 1]).into_owned())
}

/// The glossary key of an entry: its id's letters and digits when it has one (^known-good -> knowngood), otherwise its
/// heading's, in lower case for a term (Domain Divergence -> domaindivergence) and as written for an acronym (NetIDS).
pub fn gloss_key(heading: &str, bid: Option<&str>) -> String {
    if let Some(b) = bid.filter(|b| !b.is_empty()) {
        return b.chars().filter(char::is_ascii_alphanumeric).collect();
    }
    let key: String = heading.chars().filter(char::is_ascii_alphanumeric).collect();
    if is_acronym(heading) {
        key
    } else {
        key.to_lowercase()
    }
}

/// FNRATE, NetIDS, W1: one word with a capital after its first letter, or a digit.
pub fn is_acronym(heading: &str) -> bool {
    !heading.contains(' ') && (heading.chars().skip(1).any(|c| c.is_ascii_uppercase()) || heading.chars().any(|c| c.is_ascii_digit()))
}

/// How the text prints an entry: an acronym as written, a term in lower case (Known Good Presentation -> known good ...).
fn in_text(heading: &str) -> String {
    if is_acronym(heading) {
        heading.to_string()
    } else {
        heading.to_lowercase()
    }
}

/// Python's str.islower: some cased character and no upper-case one.
fn is_lower(s: &str) -> bool {
    s.chars().any(char::is_lowercase) && !s.chars().any(char::is_uppercase)
}

/// The long form of an acronym in title case (False Acceptance Rate); small words stay lower case except first, and a
/// word with a capital or a LaTeX command is left as it is.
pub fn title_case(s: &str, small: &[String]) -> String {
    let word = |w: &str, first: bool| -> String {
        if !first && small.iter().any(|x| *x == w.to_lowercase()) {
            return w.to_lowercase();
        }
        w.split('-').map(|p| if !p.is_empty() && is_lower(p) { cap(p) } else { p.to_string() }).collect::<Vec<_>>().join("-")
    };
    s.split(' ').enumerate().map(|(k, w)| if w.contains('\\') || w.contains('$') { w.to_string() } else { word(w, k == 0) }).collect::<Vec<_>>().join(" ")
}

fn escape_tt(s: &str) -> String {
    TT_SPECIAL.replace_all(s, "\\$1").into_owned()
}

fn camel(key: &str) -> String {
    key.split('_').map(cap).collect()
}

/// A property's value without its YAML quotes (a double-quoted value is a JSON string).
pub fn unquote(v: &str) -> Result<String> {
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        return serde_json::from_str::<String>(v).map_err(|e| format!("the property value {v} is not a valid quoted string ({e})"));
    }
    if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        return Ok(v[1..v.len() - 1].replace("''", "'"));
    }
    Ok(v.to_string())
}

/// The word-count key of a counted section: its label without the "sec:" prefix (sec:litreview -> litreview).
fn count_key(label: &str) -> String {
    label.strip_prefix("sec:").unwrap_or(label).replace(':', "-")
}

/// A note's properties, in order: (key, value as written).
pub type Props = Vec<(String, String)>;

/// A note's properties (in order; a repeated key keeps its first place and its last value) and its lines after them.
pub fn split_front(md: &str) -> Result<(Props, Vec<&str>)> {
    let mut lines: Vec<&str> = md.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let mut props: Vec<(String, String)> = Vec::new();
    if lines.first() == Some(&"---") {
        let end = lines.iter().skip(1).position(|l| *l == "---").map(|p| p + 1).ok_or("the properties have no closing ---")?;
        for l in &lines[1..end] {
            let (k, v) = l.split_once(':').unwrap_or((l, ""));
            let (k, v) = (k.trim().to_string(), v.trim().to_string());
            match props.iter_mut().find(|(x, _)| *x == k) {
                Some(p) => p.1 = v,
                None => props.push((k, v)),
            }
        }
        lines.drain(..=end);
    }
    Ok((props, lines))
}

pub fn prop<'a>(props: &'a [(String, String)], key: &str) -> Option<&'a str> {
    props.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

// ------------------------------------------------------------------ the index of every note

/// What a note needs from the others: headings with labels (for [[note#Heading]] links), the glossary's entries, and
/// the commands the preamble defines without arguments.
#[derive(Default, Debug)]
pub struct Index {
    /// (note name without .md, heading text) -> label.
    headings: HashMap<(String, String), String>,
    /// A link target in the glossary note ("FNRATE", "^known-good") -> its entry's heading, in the note's order.
    glossary: Vec<(String, String)>,
    commands: HashSet<String>,
}

impl Index {
    /// `notes`: (file name, text) of every note in the folder.
    pub fn new<'a>(notes: impl IntoIterator<Item = (&'a str, &'a str)>, o: &Options) -> Index {
        let mut idx = Index { commands: o.commands.iter().cloned().collect(), ..Index::default() };
        for (name, text) in notes {
            let text = text.replace("\r\n", "\n");
            let stem = name.strip_suffix(".md").unwrap_or(name);
            let lines: Vec<&str> = text.split('\n').collect();
            if stem == o.glossary {
                idx.glossary_entries(&lines);
            }
            if stem == o.preamble {
                idx.commands.extend(DEFINED.captures_iter(&text).map(|c| c[1].to_string()));
            }
            for (k, l) in lines.iter().enumerate() {
                if let (Some(m), Some(next)) = (HEADING4.captures(l), lines.get(k + 1)) {
                    if next.len() > 1 && next.starts_with('^') && next[1..].bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                        if let Ok(label) = label_of(&next[1..]) {
                            idx.headings.insert((stem.to_string(), m[2].to_string()), label);
                        }
                    }
                }
            }
        }
        idx
    }

    /// The glossary's "# heading" lines and the "^id" line under each. latex_sync.py reads them with one multi-line
    /// pattern, `^# (.+?)\s*$(?:\n\^([A-Za-z0-9-]+)$)?`, whose `\s*` also runs over whitespace-only lines: the id may
    /// follow blank lines, so it does here.
    fn glossary_entries(&mut self, lines: &[&str]) {
        for (k, l) in lines.iter().enumerate() {
            let Some(rest) = l.strip_prefix("# ").filter(|r| !r.is_empty()) else { continue };
            let heading = match rest.trim_end() {
                "" => &rest[..next(rest, 0)],
                h => h,
            };
            self.glossary.push((heading.to_string(), heading.to_string()));
            let mut m = k;
            while m + 1 < lines.len() && lines[m + 1].trim().is_empty() {
                m += 1;
            }
            if let Some(id) = lines.get(m + 1).and_then(|x| ID_LINE.captures(x)) {
                self.glossary.push((format!("^{}", &id[1]), heading.to_string()));
            }
        }
    }
}

// ------------------------------------------------------------------ inline

struct Inline<'a> {
    warn: &'a mut Vec<String>,
    /// "note.md:line", the place every warning names.
    at: String,
    stem: &'a str,
    idx: &'a Index,
    o: &'a Options,
}

impl Inline<'_> {
    fn w(&mut self, msg: String) {
        self.warn.push(format!("{}: {msg}", self.at));
    }

    fn locator(s: &str) -> String {
        tie_pages(s.trim())
    }

    fn sentence_start(&self, out: &[String]) -> bool {
        let joined = out.concat();
        let before = joined.trim_end();
        if before.is_empty() {
            return true;
        }
        let tail = CLOSERS.replace(before, "");
        if !tail.ends_with(['.', '?', '!']) {
            return false;
        }
        let last = tail.split_whitespace().last().unwrap_or("");
        !(self.o.abbreviations.iter().any(|a| last.ends_with(a.as_str())) || tail.ends_with('\\'))
    }

    /// [@a; @b, p. 2] -> \cite[p.~2]{a, b}; per-key prefixes or locators -> \cites.
    fn bracket_cite(&mut self, content: &str) -> Option<String> {
        let (mut cites, mut seps) = (Vec::new(), Vec::new());
        let mut last = 0;
        for m in SEMI.captures_iter(content) {
            let all = m.get(0).unwrap();
            cites.push(&content[last..all.start()]);
            seps.push(m.get(1).unwrap().as_str());
            last = all.end();
        }
        cites.push(&content[last..]);
        let mut parsed: Vec<(String, String, Option<String>)> = Vec::new();
        for (k, it) in cites.iter().enumerate() {
            let m = CITE_ITEM.captures(if k > 0 { it.trim() } else { it })?;
            parsed.push((m["pre"].trim().to_string(), m["key"].to_string(), m.name("loc").map(|l| l.as_str().to_string())));
        }
        let nonempty = |l: &Option<String>| l.as_deref().is_some_and(|l| !l.is_empty());
        if parsed[1..].iter().any(|(p, _, _)| !p.is_empty()) || parsed[..parsed.len() - 1].iter().any(|(_, _, l)| nonempty(l)) {
            self.w(format!("[{content}]: a prefix or locator on a key other than the first or last; written as \\cites"));
            let mut s = String::from("\\cites");
            for (p, key, l) in &parsed {
                if !p.is_empty() {
                    s += &format!("[{p}]");
                }
                if nonempty(l) {
                    s += &format!("[{}]", Self::locator(l.as_deref().unwrap()));
                } else if !p.is_empty() {
                    s += "[]";
                }
                s += &format!("{{{key}}}");
            }
            return Some(s);
        }
        let (pre, post) = (&parsed[0].0, parsed[parsed.len() - 1].2.clone().filter(|l| !l.is_empty()));
        let mut keys = parsed[0].1.clone();
        for (sep, (_, key, _)) in seps.iter().zip(&parsed[1..]) {
            keys += &format!(",{sep}{key}");
        }
        let opt = if !pre.is_empty() {
            format!("[{pre}][{}]", post.as_deref().map(Self::locator).unwrap_or_default())
        } else {
            post.as_deref().map(|p| format!("[{}]", Self::locator(p))).unwrap_or_default()
        };
        Some(format!("\\cite{opt}{{{keys}}}"))
    }

    /// Index of the closing delimiter for the one opening at `i`, skipping maths, code and escapes.
    fn matching(s: &str, i: usize, delim: &str) -> Option<usize> {
        let (b, n) = (s.as_bytes(), s.len());
        let mut k = i + delim.len();
        while k < n {
            match b[k] {
                b'\\' => {
                    k = skip2(s, k);
                    continue;
                }
                b'$' | b'`' => {
                    k = find(s, if b[k] == b'$' { "$" } else { "`" }, k + 1).map_or(n, |e| e + 1);
                    continue;
                }
                _ => {}
            }
            if s[k..].starts_with(delim) {
                if delim == "*" && s[k..].starts_with("**") {
                    k = Self::matching(s, k, "**")? + 2;
                    continue;
                }
                if delim == "_" && k + 1 < n && b[k + 1].is_ascii_alphanumeric() {
                    k += 1;
                    continue;
                }
                return Some(k);
            }
            k = next(s, k);
        }
        None
    }

    fn bracket_end(s: &str, i: usize) -> Option<usize> {
        let (b, n) = (s.as_bytes(), s.len());
        let (mut depth, mut k) = (0i32, i);
        while k < n {
            match b[k] {
                b'\\' => {
                    k = skip2(s, k);
                    continue;
                }
                b'$' => {
                    k = find(s, "$", k + 1).map_or(n, |e| e + 1);
                    continue;
                }
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(k);
                    }
                }
                _ => {}
            }
            k = next(s, k);
        }
        None
    }

    /// Whether the text so far ends with "Section " (one of the reference words, then a space): the space becomes ~.
    fn ref_word_before(&self, done: &str) -> bool {
        let Some(body) = done.strip_suffix(' ') else { return false };
        self.o.ref_words.iter().any(|w| body.strip_suffix(w.as_str()).is_some_and(|before| before.chars().next_back().is_none_or(|c| !is_word(c))))
    }

    fn is_reference(&self, res: &str) -> bool {
        let cmd = |c: &str| res.starts_with(&format!("\\{c}{{"));
        cmd("ref") || self.o.lists.iter().any(|(_, c, _)| cmd(c))
    }

    /// A wikilink's target: a heading's or block's \ref, a labelled list's reference, a glossary entry's \gls, or ""
    /// (a link to another note, left out).
    fn link(&mut self, inner: &str) -> String {
        let (target, alias) = inner.split_once('|').unwrap_or((inner, ""));
        let (note, sub) = target.split_once('#').unwrap_or((target, ""));
        if sub.is_empty() {
            return String::new(); // a link to another note: evidence, left out
        }
        let o = self.o;
        if note == o.glossary {
            // [[glossary#FNRATE|FNRATE]], [[glossary#^known-good|known good presentation]] -> \gls
            let target = sub.trim();
            let (key, heading) = if let Some(id) = target.strip_prefix('^') {
                // an entry's id; matched by its letters and digits, as the key is
                let key = gloss_key("", Some(id));
                let found = self.idx.glossary.iter().find(|(t, _)| t.starts_with('^') && gloss_key("", Some(&t[1..])) == key).map(|(_, h)| h.clone());
                if !self.idx.glossary.is_empty() && found.is_none() {
                    self.w(format!("[[{inner}]]: no entry {} in {}.md", repr(target), o.glossary));
                }
                (key, found.unwrap_or_else(|| id.to_string()))
            } else {
                if !self.idx.glossary.is_empty() && !self.idx.glossary.iter().any(|(t, _)| t == target) {
                    self.w(format!("[[{inner}]]: no entry {} in {}.md", repr(target), o.glossary));
                }
                (gloss_key(target, None), target.to_string())
            };
            let (base, alias) = (in_text(&heading), alias.trim());
            // later forms win, so that a heading already capitalised (FNRATE) gives \gls, not \Gls
            let form = [(base.clone(), "gls"), (cap(&base), "Gls"), (format!("{base}s"), "glspl"), (format!("{}s", cap(&base)), "Glspl")]
                .into_iter()
                .find(|(f, _)| f == alias)
                .map(|(_, c)| c);
            return match (alias.is_empty(), form) {
                (true, _) => format!("\\gls{{{key}}}"),
                (_, Some(c)) => format!("\\{c}{{{key}}}"),
                _ => format!("\\glslink{{{key}}}{{{}}}", self.conv(alias)),
            };
        }
        if let Some(bid) = sub.strip_prefix('^') {
            // one research question's or hypothesis's own id: ^rq1, ^hyp2a
            for (prefix, cmd, _) in &o.lists {
                if let Some(n) = bid.strip_prefix(prefix.as_str()).filter(|n| is_list_number(n)) {
                    return format!("\\{cmd}{{{n}}}");
                }
            }
            // the list's own id, ^rq or ^hyp, with the number in the alias (RQ1, H2a): the older form
            if let Some((_, cmd, _)) = o.lists.iter().find(|(p, _, _)| p == bid) {
                let a = alias.trim();
                let n = o.lists.iter().find_map(|(_, _, ap)| a.strip_prefix(ap.as_str()).filter(|n| !n.is_empty() && n.chars().all(is_word)));
                return match n {
                    Some(n) => format!("\\{cmd}{{{n}}}"),
                    None => {
                        let forms: Vec<String> = o.lists.iter().map(|(_, _, ap)| format!("{ap}n")).collect();
                        self.w(format!("[[{inner}]]: a link to a labelled list needs the alias {}", forms.join(" or ")));
                        String::new()
                    }
                };
            }
            if LABEL_ALIAS.is_match(alias.trim()) {
                return format!("\\ref{{{}}}", alias.trim());
            }
            return match label_of(bid) {
                Ok(l) => format!("\\ref{{{l}}}"),
                Err(e) => {
                    self.w(e);
                    String::new()
                }
            };
        }
        let heading = UNNUMBERED.replace(sub.trim(), "").into_owned();
        let stem = if note.is_empty() { self.stem } else { note };
        match self.idx.headings.get(&(stem.to_string(), heading)) {
            Some(label) => format!("\\ref{{{label}}}"),
            None => {
                self.w(format!("[[{inner}]]: no labelled heading {} in {}", repr(sub), if note.is_empty() { "this note" } else { note }));
                String::new()
            }
        }
    }

    /// The text before a removed link loses the space it had (out's last piece, as the Python's list does).
    fn drop_space(out: &mut [String]) {
        if let Some(last) = out.last_mut().filter(|l| l.ends_with(' ')) {
            last.pop();
        }
    }

    /// "Section [[...]]" -> Section~\ref{...}: the space before a reference after a reference word is a tie.
    fn tie(&self, out: &mut Vec<String>, res: &str) {
        if self.is_reference(res) {
            let done = out.concat();
            if self.ref_word_before(&done) {
                *out = vec![format!("{}~", &done[..done.len() - 1])];
            }
        }
    }

    fn conv(&mut self, text: &str) -> String {
        let mut s = text.to_string();
        let mut out: Vec<String> = Vec::new();
        let mut spans: Vec<&str> = Vec::new();
        let mut i = 0;
        while i < s.len() {
            let n = s.len();
            let b = s.as_bytes();
            let c = b[i];
            let rest = &s[i..];
            if c == b'\\' {
                let step = CMD_NAME.find(rest).map_or_else(|| skip2(&s, i) - i, |m| m.end());
                out.push(s[i..i + step].to_string());
                i += step;
            } else if c == b'$' {
                let mut e = i + 1;
                while e < n && b[e] != b'$' {
                    e = if b[e] == b'\\' { skip2(&s, e) } else { next(&s, e) };
                }
                let end = (e + 1).min(n);
                out.push(s[i..end].to_string());
                i = end;
            } else if c == b'`' {
                match find(&s, "`", i + 1) {
                    None => {
                        out.push("`".into());
                        i += 1;
                    }
                    Some(e) => {
                        out.push(format!("\\texttt{{{}}}", escape_tt(&s[i + 1..e])));
                        i = e + 1;
                    }
                }
            } else if rest.starts_with("%%") {
                let tag = TAG_COMMENT.is_match(rest);
                let e = find(&s, "%%", i + 2);
                if tag {
                    let Some(e) = e else {
                        self.w("an unclosed %%[tag] comment".into());
                        break;
                    };
                    if out.last().is_some_and(|l| l.ends_with(' ')) && char_at(&s, e + 2).is_none_or(|c| !c.is_alphanumeric()) {
                        Self::drop_space(&mut out);
                    }
                    i = e + 2;
                    continue;
                }
                let r = s.trim_end();
                if r.ends_with("%%") && r.len() >= i + 4 {
                    out.push(format!("%{}", &r[i + 2..r.len() - 2]));
                    break;
                }
                let Some(e) = e else {
                    self.w("an unclosed %% comment; written as a LaTeX comment to the end of the line".into());
                    out.push(format!("%{}", &s[i + 2..]));
                    break;
                };
                self.w("a %% comment in the middle of a line is moved to the end of the line (LaTeX comments run to the end)".into());
                s = format!("{}{} %%{}%%", &s[..i], &s[e + 2..], &s[i + 2..e]);
            } else if rest.starts_with("![[") {
                let e = find(&s, "]]", i);
                let shown = match e {
                    Some(e) => s[i..e + 2].to_string(),
                    None => s[i..1.max(i)].to_string(),
                };
                self.w(format!("embedded note or image {} is left out (no LaTeX equivalent)", repr(&shown)));
                i = e.map_or(n, |e| e + 2);
            } else if rest.starts_with("[[") {
                let Some(e) = find(&s, "]]", i) else {
                    out.push("[".into());
                    i += 1;
                    continue;
                };
                let res = self.link(&s[i + 2..e]);
                if res.is_empty() {
                    Self::drop_space(&mut out);
                }
                self.tie(&mut out, &res);
                let empty = res.is_empty();
                out.push(res);
                i = e + 2;
                if empty {
                    // a run of evidence links "[[EV-1]], [[EV-2]]" goes as a whole, with the commas between them
                    if let Some(m) = EV_COMMA.find(&s[i..]) {
                        if EV_NEXT.is_match(&s[i + m.end()..]) {
                            i += m.end();
                        }
                    }
                }
            } else if rest.starts_with("^[") {
                match Self::matching(&s, i + 1, "]") {
                    None => {
                        out.push("^".into());
                        i += 1;
                    }
                    Some(e) => {
                        let inner = s[i + 2..e].to_string();
                        out.push(format!("\\footnote{{{}}}", self.conv(&inner)));
                        i = e + 1;
                    }
                }
            } else if c == b'[' {
                let Some(e) = Self::bracket_end(&s, i) else {
                    out.push("[".into());
                    i += 1;
                    continue;
                };
                let content = s[i + 1..e].to_string();
                if e + 1 < n && b[e + 1] == b'(' {
                    // (latex_sync.py loops for ever on a "[text](" with no closing parenthesis; here it is text)
                    let Some(u) = find(&s, ")", e + 2) else {
                        self.w(format!("a Markdown link {} has no closing parenthesis; copied as text", repr(&s[i..])));
                        out.push("[".into());
                        i += 1;
                        continue;
                    };
                    let url = s[e + 2..u].to_string();
                    if url.starts_with("zotero:") {
                        Self::drop_space(&mut out);
                    } else if url.contains('#') && !URL_SCHEME.is_match(&url) {
                        // a Markdown link to a note's heading or block, [NetIDS](glossary.md#NetIDS): read as the
                        // wikilink [[glossary#NetIDS|NetIDS]] (it has no "|", which the Tags plugin needs on a caption line)
                        let (target, sub) = url.split_once('#').unwrap();
                        let note = percent(target);
                        let note = note.strip_suffix(".md").unwrap_or(&note);
                        let res = self.link(&format!("{note}#{}|{content}", percent(sub)));
                        self.tie(&mut out, &res);
                        out.push(res);
                    } else {
                        out.push(format!("\\href{{{url}}}{{{}}}", self.conv(&content)));
                    }
                    i = u + 1;
                    continue;
                }
                match content.contains('@').then(|| self.bracket_cite(&content)).flatten() {
                    Some(cite) => {
                        out.push(cite);
                        i = e + 1;
                    }
                    None => {
                        out.push("[".into());
                        i += 1;
                    }
                }
            } else if c == b'@' && s[..i].chars().next_back().is_none_or(|p| !(p.is_ascii_alphanumeric() || "_@.".contains(p))) && CITE_AT.is_match(&s[i + 1..]) {
                let key = CITE_AT.find(&s[i + 1..]).unwrap().as_str().to_string();
                let (mut j, mut loc) = (i + 1 + key.len(), String::new());
                if s[j..].starts_with(" [") && !s[j..].starts_with(" [[") {
                    if let Some(e) = Self::bracket_end(&s, j + 1).filter(|&e| e > 0) {
                        if !s[j + 2..e].contains('@') && !s[e + 1..].starts_with('(') {
                            loc = format!("[{}]", Self::locator(&s[j + 2..e]));
                            j = e + 1;
                        }
                    }
                }
                let cmd = if self.sentence_start(&out) { "\\Citet" } else { "\\citet" };
                out.push(format!("{cmd}{loc}{{{key}}}"));
                i = j;
            } else if rest.starts_with("<span style=\"color:") {
                match SPAN_OPEN.captures(rest) {
                    None => {
                        out.push("<".into());
                        i += 1;
                    }
                    Some(m) => {
                        out.push(format!("\\textcolor{{{}}}{{", &m[1]));
                        spans.push("}");
                        i += m[0].len();
                    }
                }
            } else if rest.starts_with("</span>") && !spans.is_empty() {
                out.push(spans.pop().unwrap().into());
                i += "</span>".len();
            } else if rest.starts_with("**") {
                match Self::matching(&s, i, "**") {
                    None => {
                        out.push("**".into());
                        i += 2;
                    }
                    Some(e) => {
                        let inner = s[i + 2..e].to_string();
                        out.push(format!("\\textbf{{{}}}", self.conv(&inner)));
                        i = e + 2;
                    }
                }
            } else if c == b'*' {
                match Self::matching(&s, i, "*").filter(|&e| e != i + 1) {
                    None => {
                        out.push("*".into());
                        i += 1;
                    }
                    Some(e) => {
                        let inner = s[i + 1..e].to_string();
                        out.push(format!("\\emph{{{}}}", self.conv(&inner)));
                        i = e + 1;
                    }
                }
            } else if c == b'_' && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'\\')) && i + 1 < n && b[i + 1] != b' ' {
                match Self::matching(&s, i, "_") {
                    None => {
                        out.push("_".into());
                        i += 1;
                    }
                    Some(e) => {
                        let inner = s[i + 1..e].to_string();
                        out.push(format!("\\textit{{{}}}", self.conv(&inner)));
                        i = e + 1;
                    }
                }
            } else {
                let ch = char_at(&s, i).unwrap();
                let quote = match ch {
                    '“' => Some("``"),
                    '”' => Some("''"),
                    '‘' => Some("`"),
                    '’' => Some("'"),
                    _ => None,
                };
                if let Some(q) = quote {
                    out.push(q.into());
                } else {
                    if rest.starts_with("==") || rest.starts_with("~~") {
                        self.w(format!("Markdown {} (highlight or strike-through) has no LaTeX rule; copied as it is", &rest[..2]));
                    }
                    out.push(ch.to_string());
                }
                i = next(&s, i);
            }
        }
        if !spans.is_empty() {
            self.w("an unclosed <span>".into());
            out.extend(spans.iter().map(|s| s.to_string()));
        }
        out.concat()
    }
}

rx!(SEMI, r";(\s*)");

/// Percent-decoding, as urllib.parse.unquote: [W1](glossary.md#%5Eknown-good).
fn percent(s: &str) -> String {
    percent_encoding::percent_decode_str(s).decode_utf8_lossy().into_owned()
}

// ------------------------------------------------------------------ blocks

fn is_md_prose(l: &str) -> bool {
    !l.trim().is_empty() && !(["```", "$$", "#", "|", ">", "%%", "^"].iter().any(|p| l.starts_with(p)) || LIST_RX.is_match(l))
}

fn split_row(row: &str) -> Vec<String> {
    let s = row.trim();
    let (mut cells, mut cur) = (Vec::new(), String::new());
    let mut k = next(s, 0).min(s.len());
    while k < s.len() {
        if s[k..].starts_with("\\|") {
            cur.push('|');
            k += 2;
            continue;
        }
        if s.as_bytes()[k] == b'|' {
            cells.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(char_at(s, k).unwrap());
        }
        k = next(s, k);
    }
    cells
}

/// Roughly how many characters a Markdown cell prints.
fn cell_length(cell: &str) -> usize {
    let s = CELL_CITE.replace_all(cell, "[1]");
    let s = CELL_CMD_ARG.replace_all(&s, "$1");
    let s = CELL_MARKUP.replace_all(&s, "");
    s.trim().chars().count()
}

fn align_of(sep: &str) -> &'static str {
    let s = sep.trim();
    if s.starts_with(':') && s.ends_with(':') {
        "c"
    } else if s.ends_with(':') {
        "r"
    } else {
        "l"
    }
}

/// Natural widths when the table fits the line; otherwise every long column wraps (Y, ragged right) and shares the spare width.
fn auto_spec(rows: &[Vec<String>], aligns: &[&str], o: &Options) -> String {
    let widths: Vec<usize> = (0..aligns.len()).map(|k| rows.iter().filter_map(|r| r.get(k)).map(|c| cell_length(c)).max().unwrap_or(0)).collect();
    if widths.iter().sum::<usize>() + 3 * widths.len() <= o.line_chars {
        return aligns.concat();
    }
    widths.iter().zip(aligns).map(|(&w, &a)| if w > o.wrap_chars { "Y" } else { a }).collect()
}

/// \small, or \scriptsize for a table of many columns, which would not fit the width at \small.
fn auto_size(ncols: usize, o: &Options) -> &'static str {
    if ncols >= o.small_from {
        "scriptsize"
    } else {
        "small"
    }
}

pub fn table_tex(head: &[String], body: &[Vec<String>], spec: &str, caption: Option<&str>, label: Option<&str>, size: &str) -> Vec<String> {
    let env = if spec.contains(['X', 'Y']) { "tabularx" } else { "tabular" };
    let width = if env == "tabularx" { "{\\linewidth}" } else { "" };
    let mut inner = vec![format!("\\begin{{{env}}}{width}{{@{{}}{spec}@{{}}}}"), "\\toprule".into(), format!("{} \\\\", head.join(" & ")), "\\midrule".into()];
    inner.extend(body.iter().map(|r| format!("{} \\\\", r.join(" & "))));
    inner.extend(["\\bottomrule".into(), format!("\\end{{{env}}}")]);
    let Some(caption) = caption else { return inner };
    let mut out = vec!["\\begin{table}[htbp]".to_string(), "\\centering".into(), format!("\\caption{{{caption}}}")];
    out.extend(label.map(|l| format!("\\label{{{l}}}")));
    out.push(format!("\\{size}"));
    out.extend(inner);
    out.push("\\end{table}".into());
    out
}

pub fn figure_tex(pic: &[String], caption: &str, label: Option<&str>, resize: Option<&str>) -> Vec<String> {
    let mut out = vec!["\\begin{figure}[htbp]".to_string(), "\\centering".into(), format!("\\resizebox{{{}}}{{!}}{{%", resize.filter(|r| !r.is_empty()).unwrap_or(FIT))];
    out.extend(pic.iter().cloned());
    out.extend(["}".into(), format!("\\caption{{{caption}}}")]);
    out.extend(label.map(|l| format!("\\label{{{l}}}")));
    out.push("\\end{figure}".into());
    out
}

fn listing_tex(code: &str, caption: &str, label: Option<&str>, o: &Options) -> Vec<String> {
    let (env, cmd) = (&o.listing_env, &o.listing_cmd);
    let mut out = vec![format!("\\begin{{{env}}}[htbp]"), format!("\\{cmd}{{{code}}}"), format!("\\caption{{{caption}}}")];
    out.extend(label.map(|l| format!("\\label{{{l}}}")));
    out.push(format!("\\end{{{env}}}"));
    out
}

/// A note's block of code, for the output's code folder: `name` is the file (`bench.rs`), `lang` the fence's language,
/// as Pygments' lexer.
#[derive(Clone, Debug, PartialEq)]
pub struct CodeFile {
    pub name: String,
    pub lang: String,
    pub text: String,
}

/// The file extension for a fence's language when the block is code to highlight; None for blocks that are not code
/// (no language, plain text, diagrams and queries Obsidian draws, which LaTeX cannot).
fn code_ext(lang: &str) -> Option<String> {
    const NOT_CODE: [&str; 12] = ["", "text", "txt", "plain", "plaintext", "mermaid", "dataview", "dataviewjs", "query", "math", "latex", "tex"];
    let lang = lang.to_lowercase();
    if NOT_CODE.contains(&lang.as_str()) || lang == "tikz" || !lang.chars().all(|c| c.is_ascii_alphanumeric() || "+#-_".contains(c)) {
        return None;
    }
    let ext = match lang.as_str() {
        "python" | "py" => "py",
        "rust" | "rs" => "rs",
        "javascript" | "js" => "js",
        "typescript" | "ts" => "ts",
        "bash" | "sh" | "shell" | "zsh" => "sh",
        "powershell" | "ps1" | "pwsh" => "ps1",
        "c++" | "cpp" => "cpp",
        "c#" | "csharp" | "cs" => "cs",
        "matlab" => "m",
        "markdown" | "md" => "md",
        "yaml" | "yml" => "yaml",
        other => return Some(other.replace(['+', '#'], "")),
    };
    Some(ext.to_string())
}

/// The "%%[cols] ...%%", "%%[size] ...%%" and "%%[resize] ...%%" lines above line i, separated from it (and from each
/// other) only by blank lines: Obsidian draws a table only after a blank line, never directly under a comment.
fn hints_above(lines: &[&str], i: usize) -> HashMap<String, String> {
    let mut hints = HashMap::new();
    let mut k = i;
    while k > 0 {
        let l = lines[k - 1];
        if l.trim().is_empty() {
            k -= 1;
        } else if let Some(h) = HINT_RX.captures(l.trim()) {
            hints.insert(h[1].to_string(), h[2].to_string());
            k -= 1;
        } else {
            break;
        }
    }
    hints
}

/// From line j (after a table or a figure): blank lines, then its id and a caption line, or the older one-line
/// "<kind>: caption ^id" -> (caption, id, index after it and one blank line).
fn read_caption(lines: &[&str], j: usize, kind: &str) -> Option<(String, Option<String>, usize)> {
    let blank = |k: usize| lines[k].trim().is_empty();
    let mut k = j;
    while k < lines.len() && blank(k) {
        k += 1;
    }
    let prefix = match kind {
        "Table" => Some("tab--"),
        "Figure" => Some("fig--"),
        "Listing" => Some("lst--"),
        _ => None,
    };
    if let (Some(prefix), true) = (prefix, k < lines.len()) {
        // "^tab--name", a blank line, "((<tag|Table)) ..." (or the caption line alone)
        let (mut bid, mut m) = (None, k);
        if let Some(idm) = ID_LINE.captures(lines[k]).filter(|c| c[1].starts_with(prefix)) {
            (bid, m) = (Some(idm[1].to_string()), k + 1);
            while m < lines.len() && blank(m) {
                m += 1;
            }
        }
        if let Some(cap) = lines.get(m).and_then(|l| CAPTION_LINE.captures(l)) {
            let mut end = m + 1;
            if end < lines.len() && blank(end) {
                end += 1;
            }
            return Some((cap[1].to_string(), bid, end));
        }
    }
    let cm = lines.get(k).and_then(|l| CAPTION_RX.captures(l)).filter(|c| &c[1] == kind)?;
    let (text, mut bid, mut end) = (cm[2].to_string(), cm.get(3).map(|m| m.as_str().to_string()), k + 1);
    if bid.is_none() {
        let mut m = end;
        while m < lines.len() && blank(m) {
            m += 1;
        }
        if m < lines.len() && ID_LINE.is_match(lines[m]) {
            (bid, end) = (Some(lines[m][1..].to_string()), m + 1);
        }
    }
    if end < lines.len() && blank(end) {
        end += 1;
    }
    Some((text, bid, end))
}

/// A "Listing: caption ^lst--name" line at i -> (caption, label, index after it).
fn listing(lines: &[&str], i: usize) -> Result<Option<(String, String, usize)>> {
    let Some((caption, Some(bid), end)) = read_caption(lines, i, "Listing") else { return Ok(None) };
    let lab = label_of(&bid)?;
    Ok(lab.starts_with("lst:").then_some((caption, lab, end)))
}

/// A %% block over several lines -> % lines: the first keeps its text as written after %%, each later line is "% text"
/// (a bare % when empty); an opening or closing line holding only %% gives no line; a [tag] block is left out.
fn comment_block_tex(first: &str, middle: &[&str], last: &str) -> Vec<String> {
    if TAG.is_match(first.trim()) {
        return Vec::new();
    }
    let mut out: Vec<String> = if first.trim().is_empty() { Vec::new() } else { vec![format!("%{first}")] };
    let rest = middle.iter().copied().chain((!last.trim().is_empty()).then_some(last));
    out.extend(rest.map(|x| if x.trim().is_empty() { "%".to_string() } else { format!("% {x}") }));
    out
}

fn divider() -> String {
    format!("% {}", "=".repeat(DIVIDER_FILL))
}

/// One note's conversion: its warnings and where its lines are.
struct Md<'a> {
    note: &'a str,
    stem: &'a str,
    warn: Vec<String>,
    /// The note's blocks of code, for the output's code folder.
    code: Vec<CodeFile>,
    idx: &'a Index,
    o: &'a Options,
    /// From an index into the lines after the properties to the note's own line number, less one (-1 when the note
    /// does not end with a newline, as in latex_sync.py).
    offset: i64,
}

impl Md<'_> {
    fn inline(&mut self, s: &str, k: usize) -> String {
        let at = format!("{}:{}", self.note, self.line(k));
        Inline { warn: &mut self.warn, at, stem: self.stem, idx: self.idx, o: self.o }.conv(s)
    }

    fn w(&mut self, msg: String) {
        self.warn.push(msg);
    }

    fn line(&self, k: usize) -> i64 {
        k as i64 + 1 + self.offset
    }
}

/// Where each line of a note went in its chapter: `tex[k]` is the .tex line (from 1) where the part of the chapter
/// that note line k + 1 belongs to begins. A part is a paragraph, a heading with its labels, a list item, a table row,
/// an equation, a figure, a comment block ...; a line that gives nothing (a callout, an id, a tagged comment) maps to
/// where the next output goes, and the properties to line 1. Scrolling a note and its PDF together (scroll.rs) reads it
/// with the SyncTeX file of the last build (synctex.rs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineMap {
    pub tex: Vec<u32>,
    /// The chapter's lines that print nothing (blank, or a comment), in order: TeX tags the line ends of a paragraph
    /// with the blank line after it, so they are not where anything begins.
    pub empty: Vec<u32>,
}

impl LineMap {
    /// The part around note line `line` (from 1): its first and last note lines and its first .tex line. A part is the
    /// run of neighbouring note lines that map to the same .tex line.
    pub fn part(&self, line: u32) -> Option<(u32, u32, u32)> {
        let k = (line.max(1) as usize - 1).min(self.tex.len().checked_sub(1)?);
        let t = self.tex[k];
        let first = self.tex[..k].iter().rposition(|&x| x != t).map_or(0, |p| p + 1);
        let last = self.tex[k..].iter().position(|&x| x != t).map_or(self.tex.len(), |p| p + k) - 1;
        Some((first as u32 + 1, last as u32 + 1, t))
    }

    /// The part around note line `line`, or, when that part prints nothing (a comment block, blank lines), the next
    /// part that prints, and whether it moved on (ToDo 1, user 2026-10-27: scrolling over a long comment block left the
    /// PDF at the wrong place; the comment takes no room in the PDF, so the PDF stays at the paragraph after it).
    pub fn printed_part(&self, line: u32) -> Option<(u32, u32, u32, bool)> {
        let prints = |t: u32| self.empty.binary_search(&t).is_err();
        let (mut first, mut last, mut t) = self.part(line)?;
        let mut moved = false;
        while !prints(t) && (last as usize) < self.tex.len() {
            (first, last, t) = self.part(last + 1)?;
            moved = true;
        }
        Some((first, last, t, moved))
    }

    /// The part a .tex line belongs to: the one that begins nearest before it (the first, on a tie). Parts are mostly
    /// in the same order in the note and the chapter, not always: a table's caption line follows its rows in the note
    /// and comes before them in the chapter.
    pub fn part_of_tex(&self, t: u32) -> Option<(u32, u32, u32)> {
        let best = self.tex.iter().copied().filter(|&x| x <= t).max().or_else(|| self.tex.iter().copied().min())?;
        self.part(self.tex.iter().position(|&x| x == best)? as u32 + 1)
    }
}

/// A note -> its chapter's LaTeX and the warnings; Err when it cannot be converted at all (the .tex is then kept).
pub fn md_to_tex(note: &str, md: &str, idx: &Index, o: &Options) -> Result<(String, Vec<String>)> {
    md_to_tex_mapped(note, md, idx, o).map(|(tex, warn, _)| (tex, warn))
}

/// From the index into `out` where each line's part begins to .tex lines: an entry of `out` may hold several lines (a
/// starred heading and its contents line).
fn tex_lines(out: &[String], front: usize, at: &[usize]) -> LineMap {
    let (mut start, mut empty) = (Vec::with_capacity(out.len() + 1), Vec::new());
    let mut line = 1u32;
    for x in out {
        start.push(line);
        for l in x.split('\n') {
            if l.trim().is_empty() || l.trim_start().starts_with('%') {
                empty.push(line);
            }
            line += 1;
        }
    }
    let last = line.saturating_sub(1).max(1);
    start.push(last); // where the next output would go, past the end: the last line
    let mut tex = vec![1; front];
    tex.extend(at.iter().map(|&k| start.get(k).copied().unwrap_or(last).min(last)));
    LineMap { tex, empty }
}

/// md_to_tex, and where each note line went (LineMap). The LaTeX is md_to_tex's, byte for byte.
pub fn md_to_tex_mapped(note: &str, md: &str, idx: &Index, o: &Options) -> Result<(String, Vec<String>, LineMap)> {
    md_to_tex_code(note, md, idx, o).map(|(tex, warn, map, _)| (tex, warn, map))
}

/// md_to_tex_mapped, and the note's blocks of code for the output's code folder.
pub fn md_to_tex_code(note: &str, md: &str, idx: &Index, o: &Options) -> Result<(String, Vec<String>, LineMap, Vec<CodeFile>)> {
    let md = md.replace("\r\n", "\n");
    let (props, lines) = split_front(&md)?;
    let stem = note.strip_suffix(".md").unwrap_or(note);
    let offset = md.split('\n').count() as i64 - lines.len() as i64 - 1;
    let mut c = Md { note, stem, warn: Vec::new(), code: Vec::new(), idx, o, offset };
    let path = if o.folder.is_empty() { note.to_string() } else { format!("{}/{note}", o.folder.trim_end_matches('/')) };
    let header = o
        .header
        .replace("{vault}", &o.vault)
        .replace("{path}", &path)
        .replace("{note}", note)
        .replace("{version}", prop(&props, "version").unwrap_or("?"))
        .replace("{updated}", prop(&props, &o.updated).unwrap_or("?"));
    let mut out: Vec<String> = vec![header, format!("% !TEX root = {}.tex", o.main)];
    let counted = unquote(prop(&props, &o.wordcount).unwrap_or(""))?.to_lowercase() == "true";
    // the note's lines before `lines` (the properties), and for each of `lines` the index into `out` of its part
    let front = md.split('\n').count() - usize::from(md.ends_with('\n')) - lines.len();
    const UNSET: usize = usize::MAX;
    let mut at = vec![UNSET; lines.len()];
    if stem == o.glossary {
        out.extend(glossary_tex(&lines, &mut c)?);
        // the entries are printed where the preamble's \printterms is, not here: every line is the chapter's third
        let map = tex_lines(&out, front, &vec![2; lines.len()]);
        return Ok((out.join("\n") + "\n", c.warn, map, c.code));
    }
    let l_ = &lines;

    // the Tags plugin treats a line with "|" outside its tags as a table row and rewrites the editor's text in place,
    // which can turn "((<tag|Table))" into "Table" when the line is edited
    for (k, l) in l_.iter().enumerate() {
        if let Some(cap) = CAPTION_LINE.captures(l) {
            if l.starts_with("((") && cap[1].contains('|') {
                c.w(format!("{note}:{}: a caption line holds \"|\" (the Tags plugin then rewrites it in live preview); write a glossary link as [NetIDS](glossary.md#NetIDS)", c.line(k)));
            }
        }
    }
    // Obsidian reads "<key>" outside code as an HTML tag; left unclosed, it hides the rest of the note (comments included)
    let mut fence = false;
    for (k, x) in l_.iter().enumerate() {
        if x.starts_with("```") {
            fence = !fence;
        } else if !fence {
            let bare = CODE_SPAN.replace_all(x, "");
            let tag = bare.match_indices('<').find_map(|(p, _)| {
                let after = &bare[p + 1..];
                (!starts_with_word(after.strip_prefix('/').unwrap_or(after), "span")).then(|| HTML_LIKE.find(after).map(|m| &bare[p..p + 1 + m.end()])).flatten()
            });
            if let Some(tag) = tag {
                c.w(format!("{note}:{}: {tag} is read by Obsidian as an HTML tag and hides what follows; write it without angle brackets", c.line(k)));
            }
        }
    }

    let n = l_.len();
    let mut i = 0;
    let (mut part, mut part_out) = (0, out.len()); // the part being converted: its first line and where its output begins
    while i < n {
        // the lines of the part before this one that no rule placed more finely begin where its output began
        at[part..i].iter_mut().filter(|k| **k == UNSET).for_each(|k| *k = part_out);
        (part, part_out) = (i, out.len());
        let l = l_[i];
        if l.starts_with("```") {
            let lang = l.strip_prefix("```").unwrap_or(l).trim();
            let mut j = i + 1;
            while j < n && l_[j].trim() != "```" {
                j += 1;
            }
            let raw = &l_[i + 1..j];
            let mut body: Vec<String> = raw.iter().map(|x| x.to_string()).collect();
            if lang == "tikz" {
                body.retain(|x| !TIKZ_ONLY.is_match(x.trim()));
                let libs: HashSet<&str> = o.tikz_libraries.split(',').map(str::trim).collect();
                for x in raw {
                    let extra = TIKZ_LIB.captures(x.trim()).is_some_and(|lib| lib[1].replace(' ', "").split(',').any(|l| !libs.contains(l)));
                    if extra || x.trim().starts_with("\\usepackage") {
                        c.w(format!("{note}:{}: {} is only for TikZJax; load it in the preamble for LaTeX too", i + 1, x.trim()));
                    }
                }
                // a picture followed by its ^fig--name id and a "((<tag|Figure))" caption line is a figure float
                if let Some((caption, bid, end)) = read_caption(l_, j + 1, "Figure") {
                    if bid.is_none() {
                        c.w(format!("{note}:{}: a figure with a caption but no ^fig--name id has no label to refer to", i + 1));
                    }
                    let caption = c.inline(&caption, j);
                    let label = bid.as_deref().map(label_of).transpose()?;
                    let hints = hints_above(l_, i);
                    out.extend(figure_tex(&body, &caption, label.as_deref(), hints.get("resize").map(String::as_str)));
                    i = end;
                    continue;
                }
            } else if let Some(ext) = code_ext(lang) {
                // code: its file in the code folder, highlighted before LaTeX runs (app.rs), set as the hand-typed
                // listing; numbered when its ^lst--name id and a caption line follow
                let caption = read_caption(l_, j + 1, "Listing");
                let label = caption.as_ref().and_then(|(_, bid, _)| bid.as_deref()).map(label_of).transpose()?;
                let name = match label.as_deref().and_then(|l| l.strip_prefix("lst:")) {
                    Some(n) => n.replace(':', "-"),
                    None => format!("{stem}-{}", c.code.len() + 1),
                };
                c.code.push(CodeFile { name: format!("{name}.{ext}"), lang: lang.to_lowercase(), text: raw.join("\n") + "\n" });
                match caption {
                    Some((caption, _, end)) => {
                        if label.is_none() {
                            c.w(format!("{note}:{}: a listing with a caption but no ^lst--name id has no label to refer to", i + 1));
                        }
                        let caption = c.inline(&caption, j);
                        out.extend(listing_tex(&name, &caption, label.as_deref(), o));
                        i = end;
                    }
                    None => {
                        c.w(format!("{note}:{}: a ```{lang} block without a caption is set as code but not numbered; give it a ^lst--name id and a ((<tag|Listing)) caption line", i + 1));
                        out.push(format!("\\{}{{{name}}}", o.listing_cmd));
                        i = j + 1;
                    }
                }
                continue;
            } else if lang != "latex" && lang != "tex" {
                c.w(format!("{note}:{}: a ```{lang} block is written as verbatim", i + 1));
                body.insert(0, "\\begin{verbatim}".into());
                body.push("\\end{verbatim}".into());
            } else if stem != o.preamble {
                // raw LaTeX only in the preamble
                c.w(format!("{note}:{}: a ```latex block in a chapter note (copied as it is); give it a Markdown form: a label on a paragraph is a trailing ^sec--name, a comment a %% %% line", i + 1));
            }
            out.extend(body);
            i = j + 1;
        } else if l.trim() == "$$" || (l.starts_with("$$") && l.trim_end().ends_with("$$") && l.trim().chars().count() > 4) {
            if l.trim() != "$$" {
                let t = l.trim();
                out.extend(["\\[".to_string(), t[2..t.len() - 2].trim().to_string(), "\\]".into()]);
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j < n && l_[j].trim() != "$$" {
                j += 1;
            }
            let mut body: Vec<String> = l_[i + 1..j].iter().map(|x| x.to_string()).collect();
            let mut env = body.first().is_some_and(|b| b.trim_start().starts_with("\\begin{"));
            // the id after the block is its label, written into the LaTeX only (a \label in the note makes Obsidian's
            // MathJax report it "multiply defined" whenever the block is drawn again): a block without an environment
            // becomes a numbered equation
            let nxt = &l_[(j + 1).min(n)..(j + 4).min(n)];
            if nxt.len() >= 2 && nxt[0].is_empty() && ID_LINE.is_match(nxt[1]) && (nxt.len() == 2 || nxt[2].is_empty()) && !body.iter().any(|x| x.contains("\\label{")) {
                let lab = format!("  \\label{{{}}}", label_of(&nxt[1][1..])?);
                if env {
                    let last = body.pop().unwrap();
                    body.extend([lab, last]);
                } else {
                    body.insert(0, "\\begin{equation}".into());
                    body.extend([lab, "\\end{equation}".into()]);
                }
                env = true;
            }
            if env {
                out.extend(body);
            } else {
                out.push("\\[".into());
                out.extend(body);
                out.push("\\]".into());
            }
            i = j + 1;
        } else if let Some(cmd) = COMMAND_LINE.captures(l).filter(|m| idx.commands.contains(&m[1])) {
            out.push(format!("\\{}", &cmd[1])); // ^FloatBarrier -> \FloatBarrier
            i += 1;
        } else if ID_LINE.is_match(l) && i > 0 && l_[i - 1].is_empty() && (i + 1 >= n || l_[i + 1].is_empty()) {
            if out.last().is_some_and(|x| x.is_empty()) {
                out.pop();
                part_out = out.len(); // the id names the block above; its lines go where the next output goes
            }
            i += 2;
        } else if ID_LINE.is_match(l) {
            i += 1; // any other id on a line of its own names the block above it in Obsidian; nothing to print
        } else if let Some(m) = HEADING.captures(l) {
            let level = m[1].len();
            if level > 4 {
                c.w(format!("{note}:{}: heading level {level} has no LaTeX rule; written as \\paragraph", i + 1));
            }
            let starred = m.get(3).is_some();
            let base = ["section", "subsection", "subsubsection", "paragraph"][level.min(4) - 1];
            let cmd = format!("{base}{}", if starred { "*" } else { "" });
            let mut head = format!("\\{cmd}{{{}}}", c.inline(&m[2], i));
            if starred {
                // an unnumbered heading is still listed in the contents, as in Pandoc
                head += &format!("\n\\phantomsection\\addcontentsline{{toc}}{{{base}}}{{{}}}", c.inline(&m[2], i));
            }
            let mut j = i + 1;
            let mut labels = Vec::new();
            while j < n && ID_LINE.is_match(l_[j]) {
                labels.push(format!("\\label{{{}}}", label_of(&l_[j][1..])?));
                j += 1;
            }
            if level >= 4 && labels.is_empty() && j < n && is_md_prose(l_[j]) {
                let text = c.inline(l_[j], j);
                out.push(format!("{head} {text}"));
                j += 1;
            } else if counted && level == 1 {
                if labels.is_empty() {
                    c.w(format!("{note}:{}: a counted section needs a label (a ^sec--name line) to name its word count", i + 1));
                }
                let key = match labels.first() {
                    Some(l) => count_key(&l[7..l.len() - 1]),
                    None => strip_hyphens(&NOT_KEY_CHARS.replace_all(&m[2].to_lowercase(), "-")).to_string(),
                };
                out.push(format!("\\{}{{{key}}}", o.count_start));
                out.push(head);
                out.extend(labels);
                out.push(format!("\\{}{{{key}}}", o.count_print));
            } else {
                out.push(head);
                out.extend(labels);
            }
            i = j;
        } else if LABELLED_ITEM.is_match(l) {
            // a list whose items start with a bold label and end with an id: \item[\textbf{label}] with a \hypertarget
            // anchor; blank lines and comments between its items stay inside it
            let (mut j, mut block) = (i, Vec::new());
            while j < n {
                if let Some(im) = LABELLED_ITEM.captures(l_[j]) {
                    let (lab, text) = (c.inline(&im[1], j), c.inline(&im[2], j));
                    at[j] = out.len() + 1 + block.len(); // after \begin{enumerate}
                    block.push(format!("  \\item[\\textbf{{{lab}}}] \\hypertarget{{{}}}{{}}{text}", label_of(&im[3])?));
                    j += 1;
                    continue;
                }
                let mut k = j;
                while k < n && (l_[k].trim().is_empty() || COMMENT_LINE.is_match(l_[k])) {
                    k += 1;
                }
                if k == j || k >= n || !LABELLED_ITEM.is_match(l_[k]) {
                    break;
                }
                for x in &l_[j..k] {
                    match COMMENT_LINE.captures(x) {
                        None => block.push(String::new()),
                        Some(cm) if !TAG.is_match(&cm[2]) => block.push(format!("{}%{}", &cm[1], &cm[2])),
                        Some(_) => {}
                    }
                }
                j = k;
            }
            out.push("\\begin{enumerate}".into());
            out.extend(block);
            out.push("\\end{enumerate}".into());
            if j < n && ID_LINE.is_match(l_[j]) {
                j += 1; // the list's own id (^rq) directly under it, as Obsidian writes it
            }
            i = j;
        } else if LIST_RX.is_match(l) {
            let mut j = i;
            while j < n && LIST_RX.is_match(l_[j]) {
                j += 1;
            }
            let mut env: Option<&str> = None;
            for (k, item) in l_.iter().enumerate().take(j).skip(i) {
                let m = LIST_RX.captures(item).unwrap();
                let e = if &m[2] == "-" { "itemize" } else { "enumerate" };
                if env != Some(e) {
                    if let Some(old) = env {
                        out.push(format!("\\end{{{old}}}"));
                    }
                    out.push(format!("\\begin{{{e}}}"));
                    env = Some(e);
                }
                let mut text = &m[3];
                let mut prefix = String::new();
                if let Some(pid) = TRAIL_ID.captures(text) {
                    prefix = format!("\\phantomsection\\label{{{}}}", label_of(&pid[1])?);
                    text = &text[..pid.get(0).unwrap().start()];
                }
                let text = c.inline(text, k);
                at[k] = out.len();
                out.push(format!("{}\\item {prefix}{text}", &m[1]));
            }
            out.push(format!("\\end{{{}}}", env.unwrap()));
            i = j;
        } else if l.starts_with('|') {
            let mut j = i;
            while j < n && l_[j].starts_with('|') {
                j += 1;
            }
            let rows: Vec<Vec<String>> = l_[i..j].iter().map(|x| split_row(x)).collect();
            let head = rows[0].clone();
            let (aligns, body): (Vec<&str>, &[Vec<String>]) = if rows.len() < 2 || !rows[1].iter().all(|x| SEP_CELL.is_match(x)) || rows[1].len() != head.len() {
                c.w(format!("{note}:{}: a table needs its alignment row (| --- | :-: |) under the header row", i + 1));
                (vec!["l"; head.len()], &rows[1..])
            } else {
                (rows[1].iter().map(|x| align_of(x)).collect(), &rows[2..])
            };
            let body: Vec<Vec<String>> = body.iter().map(|r| (0..head.len()).map(|k| r.get(k).cloned().unwrap_or_default()).collect()).collect();
            let hints = hints_above(l_, i); // "%%[cols] spec%%" and "%%[size] scriptsize%%"
            let all: Vec<Vec<String>> = std::iter::once(head.clone()).chain(body.iter().cloned()).collect();
            let spec = hints.get("cols").filter(|x| !x.is_empty()).cloned().unwrap_or_else(|| auto_spec(&all, &aligns, o));
            let mut size = hints.get("size").cloned().unwrap_or_else(|| auto_size(head.len(), o).to_string());
            if !TABLE_SIZES.is_match(&size) {
                c.w(format!("{note}:{}: [size] {size} is not a font size (normalsize|small|footnotesize|scriptsize|tiny); the automatic size is used", i + 1));
                size = auto_size(head.len(), o).to_string();
            }
            // after the table: its ^tab--name id and a "((<tag|Table))" caption line
            let (mut caption, mut label) = (None, None);
            if let Some((cap, bid, end)) = read_caption(l_, j, "Table") {
                caption = Some(c.inline(&cap, j));
                label = bid.as_deref().map(label_of).transpose()?;
                j = end;
                if label.is_none() {
                    c.w(format!("{note}:{}: a table with a caption but no ^tab--name id has no label to refer to", i + 1));
                }
            } else {
                c.w(format!("{note}:{}: a table without a caption line is written as a bare tabular, not a float", i + 1));
            }
            let head: Vec<String> = head.iter().map(|x| c.inline(x, i)).collect();
            let body: Vec<Vec<String>> = body.iter().map(|r| r.iter().map(|x| c.inline(x, i)).collect()).collect();
            let first = out.len();
            out.extend(table_tex(&head, &body, &spec, caption.as_deref(), label.as_deref(), &size));
            // each row to its row: the header after \toprule, the alignment row to \midrule and the body after it (a
            // row further on when the alignment row is missing)
            if let Some(h) = out[first..].iter().position(|x| x == "\\toprule").map(|p| first + p + 1) {
                let aligned = rows.len() >= 2 && rows[1].len() == rows[0].len() && rows[1].iter().all(|x| SEP_CELL.is_match(x));
                for (d, k) in (i..i + rows.len()).enumerate() {
                    at[k] = h + d + usize::from(!aligned && d > 0);
                }
            }
            i = j;
        } else if l.starts_with('>') {
            let mut j = i;
            while j < n && l_[j].starts_with('>') {
                j += 1;
            }
            if !l.strip_prefix('>').unwrap_or(l).trim_start().starts_with("[!") {
                out.push("\\begin{quote}".into());
                for (k, x) in l_[i..j].iter().enumerate() {
                    let q = c.inline(x[1..].trim(), i + k);
                    at[i + k] = out.len();
                    out.push(q);
                }
                out.push("\\end{quote}".into());
            }
            i = j;
        } else if let Some(m) = COMMENT_LINE.captures(l) {
            if !TAG.is_match(&m[2]) {
                out.push(format!("{}%{}", &m[1], &m[2]));
            }
            i += 1;
            if HINT_RX.is_match(l.trim()) {
                // the blank lines between a [cols]/[size]/[resize] line and its table or figure
                while i < n && l_[i].trim().is_empty() {
                    i += 1;
                }
            }
        } else if l.trim() == "___" {
            out.push(divider()); // a divider: a rule of = signs in a LaTeX comment
            i += 1;
        } else if l.trim().starts_with("%%") {
            let mut j = i + 1;
            while j < n && !l_[j].contains("%%") {
                j += 1;
            }
            let last = if j < n { l_[j].split("%%").next().unwrap_or("") } else { "" };
            out.extend(comment_block_tex(&l.trim_start()[2..], &l_[i + 1..j.min(n)], last));
            i = j + 1;
        } else if l.trim().is_empty() {
            out.push(String::new());
            i += 1;
        } else if let Some((caption, lab, end)) = listing(l_, i)? {
            // "Listing: caption ^lst--bench" -> the hand-typed listing of code/bench.rs
            let caption = c.inline(&caption, i);
            out.extend(listing_tex(&lab[4..], &caption, Some(&lab), o));
            i = end;
        } else {
            let mut l = l;
            if let Some(pid) = TRAIL_ID.captures(l) {
                out.push(format!("\\phantomsection\\label{{{}}}", label_of(&pid[1])?));
                l = &l[..pid.get(0).unwrap().start()];
            }
            let text = c.inline(l, i);
            out.push(text);
            i += 1;
        }
    }
    if stem == o.title_page {
        let not_fields = ["tex", "version", o.updated.as_str(), o.wordcount.as_str(), "aliases", "tags", "cssclasses"];
        let mut fields = Vec::new();
        for (k, v) in props.iter().filter(|(k, _)| !not_fields.contains(&k.as_str())) {
            if !k.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') || k.is_empty() {
                c.w(format!("{note}: the property {} cannot name a LaTeX command; left out of the title page", repr(k)));
                continue;
            }
            let v = unquote(v)?;
            fields.push(format!("\\newcommand{{\\{}{}}}{{{}}}", o.title_prefix, camel(k), c.inline(&v, 0)));
        }
        if !fields.is_empty() {
            out.extend(fields);
            out.push(format!("\\{}", o.make_title));
        }
    }
    at[part..].iter_mut().filter(|k| **k == UNSET).for_each(|k| *k = part_out);
    if counted {
        out.push(format!("\\{}", o.count_stop));
    }
    let map = tex_lines(&out, front, &at);
    Ok((out.join("\n") + "\n", c.warn, map, c.code))
}

/// The glossary note -> one \term[text]{key}{name}{long form}{description} per "# heading" entry; comments are kept.
#[allow(clippy::unnecessary_unwrap)] // the branches keep latex_sync.py's order, so the entry is tested where it tests it
fn glossary_tex(lines: &[&str], c: &mut Md) -> Result<Vec<String>> {
    #[derive(Default)]
    struct Entry {
        heading: String,
        at: usize,
        desc: String,
        id: Option<String>,
        long: Option<String>,
        printed: Option<String>,
    }
    let (note, o) = (c.note, c.o);
    let mut out: Vec<String> = Vec::new();
    let mut entry: Option<Entry> = None;
    let (mut in_math, mut maths): (bool, Vec<String>) = (false, Vec::new()); // a $$ block in an entry: its lines

    let flush = |entry: &mut Option<Entry>, c: &mut Md, out: &mut Vec<String>| {
        let Some(e) = entry.take() else { return };
        if e.desc.is_empty() {
            c.w(format!("{note}: the entry {} has no description", repr(&e.heading)));
        }
        let name = match e.printed.as_deref().filter(|p| !p.is_empty()) {
            Some(p) => p.to_string(),
            None => c.inline(&e.heading, e.at),
        };
        let acronym = is_acronym(&e.heading);
        let mut long = e.long.clone().unwrap_or_default();
        if acronym {
            long = title_case(&long, &o.small_words);
        }
        let text = if acronym || e.printed.is_some() { String::new() } else { format!("[{}]", in_text(&name)) };
        out.push(format!("\\{}{text}{{{}}}{{{name}}}{{{long}}}{{{}}}", o.term, gloss_key(&e.heading, e.id.as_deref()), e.desc));
    };

    let mut block: Option<Vec<String>> = None; // a %% block over several lines: its lines so far
    for (k, l) in lines.iter().enumerate() {
        if let Some(b) = block.as_mut() {
            if l.contains("%%") {
                let middle: Vec<&str> = b[1..].iter().map(String::as_str).collect();
                out.extend(comment_block_tex(&b[0], &middle, l.split("%%").next().unwrap_or("")));
                block = None;
            } else {
                b.push(l.to_string());
            }
            continue;
        }
        let id = entry.as_ref().filter(|e| e.desc.is_empty() && e.long.is_none()).and_then(|_| ID_LINE.captures(l));
        if let Some(m) = ENTRY.captures(l) {
            flush(&mut entry, c, &mut out);
            entry = Some(Entry { heading: m[1].to_string(), at: k, ..Entry::default() });
        } else if let Some(id) = id {
            entry.as_mut().unwrap().id = Some(id[1].to_string()); // ^known-good under the heading: the short name links use
        } else if let Some(cm) = COMMENT_LINE.captures(l) {
            if !TAG.is_match(&cm[2]) {
                out.push(format!("{}%{}", &cm[1], &cm[2]));
            }
        } else if l.trim().starts_with("%%") && !in_math {
            block = Some(vec![l.trim_start()[2..].to_string()]);
        } else if l.trim().is_empty() || (in_math && l.trim() != "$$" && entry.is_none()) {
            continue;
        } else if l.trim() == "$$" && entry.is_some() {
            in_math = !in_math;
            if !in_math {
                let e = entry.as_mut().unwrap();
                let m = format!("\\[ {} \\]", maths.join(" "));
                e.desc = if e.desc.is_empty() { m } else { format!("{} {m}", e.desc) };
                maths.clear();
            }
        } else if in_math {
            maths.push(l.trim().to_string());
        } else if entry.is_none() {
            c.w(format!("{note}:{}: text before the first entry is left out", c.line(k)));
        } else if let Some(f) = FIELD_RX.captures(l) {
            if &f[1] == "Long form" {
                let v = c.inline(&f[2], k);
                entry.as_mut().unwrap().long = Some(v);
            } else {
                entry.as_mut().unwrap().printed = Some(f[2].to_string());
            }
        } else {
            let d = c.inline(l.trim(), k);
            let e = entry.as_mut().unwrap();
            e.desc = if e.desc.is_empty() { d } else { format!("{} {d}", e.desc) };
        }
    }
    flush(&mut entry, c, &mut out);
    Ok(out)
}

#[cfg(test)]
mod tests;
