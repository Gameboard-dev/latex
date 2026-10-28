//! The SyncTeX file of the last build (`<main>.synctex.gz`, written by `-synctex=1`), read once per build: which input
//! line was typeset where in the PDF. With a note's LineMap (convert.rs) it says where a note line is in the PDF, and
//! which note line a place in the PDF shows (scroll.rs scrolls the panes).
//!
//! The format is SyncTeX's (<https://github.com/jlaurens/synctex>, `synctex_parser.c` is the reference reader, and
//! `man synctex` the record syntax): a preamble (`Input:tag:path`, `Unit:`, `Magnification:`, `X Offset:`, `Y Offset:`),
//! then the content, one sheet per `{page` ... `}page`, holding boxes (`[` `]` vertical, `(` `)` horizontal) and records
//! `<kind><tag>,<line>[,<column>]:<h>,<v>[:...]` with h and v from the top left of the page, in TeX's scaled points
//! times the unit. Only the leaves are read (`x` current point, `k` kern, `g` glue, `$` math, `h` `v` void boxes, `r`
//! rule): a box is tagged with the line where it was finished, which for a paragraph's lines is the blank line after it.
//! Many leaves are tagged with whatever line TeX was reading when they were made: the blank line after a paragraph (its
//! line ends, made when the paragraph is broken into lines) and, with LuaTeX, the line being read when the page was
//! shipped out (hundreds of records over the page, measured 2026-10-25). A line's own records are the most numerous on
//! its baselines, so each baseline belongs to the line most of its records name, a line is only where it wins a
//! baseline, and of those places the first in order after the line before it (`Synctex::new`).

use crate::convert::LineMap;

/// A place in the PDF: a page (from 1) and how far down it, in PDF points (bp) from the top of the page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pos {
    pub page: u32,
    pub v: f32,
}

impl Pos {
    /// Down the whole document, as if the pages were `page_h` high and stacked: to compare and interpolate.
    fn y(self, page_h: f32) -> f32 {
        (self.page as f32 - 1.0) * page_h + self.v
    }

    fn at_y(y: f32, page_h: f32) -> Pos {
        let page = (y / page_h).floor().max(0.0);
        Pos { page: page as u32 + 1, v: y - page * page_h }
    }

    fn before(self, other: Pos) -> bool {
        (self.page, self.v) < (other.page, other.v)
    }
}

/// How far above a line's baseline the top of the PDF pane is put, in bp: about a line of 11pt type and a little room.
pub const ABOVE: f32 = 14.0;
/// Scaled points per PDF point (65536 × 72.27 / 72): SyncTeX's unit factor.
const SP_PER_BP: f64 = 65781.76;

/// The index of a SyncTeX file, for the input files `keep` accepted (the chapters).
#[derive(Debug, Default)]
pub struct Synctex {
    /// The kept input files by file name (`3-introduction.tex`); a file is its index here.
    files: Vec<String>,
    /// For each file, its typeset lines in order, each with the place it was first typeset.
    lines: Vec<Vec<(u32, Pos)>>,
    /// For each page (from page 1), its baselines from the top, each with the line most of its records name: (v,
    /// file, line).
    pages: Vec<Vec<(f32, usize, u32)>>,
    /// How many pages the file has.
    pub sheets: u32,
}

/// `tag,line[,column]:h,v...` after a record's letter: (tag, line, h, v).
fn record(s: &str) -> Option<(u32, u32, i64, i64)> {
    let (link, rest) = s.split_once(':')?;
    let mut l = link.split(',');
    let (tag, line) = (l.next()?.parse().ok()?, l.next()?.parse().ok()?);
    let point = rest.split(':').next()?;
    let (h, v) = point.split_once(',')?;
    Some((tag, line, h.parse().ok()?, v.parse().ok()?))
}

impl Synctex {
    /// Read a SyncTeX file's text, keeping the input files whose file name `keep` accepts. Err when it is not one.
    pub fn new(text: &str, keep: impl Fn(&str) -> bool) -> Result<Synctex, String> {
        if !text.starts_with("SyncTeX Version:") {
            return Err("not a SyncTeX file".into());
        }
        // the preamble's fields are integers (synctex_parser.c reads them so)
        let (mut unit, mut magnification, mut y_offset) = (1i64, 1000i64, 0i64);
        let mut st = Synctex::default();
        let mut tags: Vec<Option<usize>> = Vec::new(); // input tag -> kept file
        let (mut page, mut content) = (0usize, false);
        let mut found: Vec<Vec<(f32, usize, u32)>> = Vec::new(); // each page's records: (v, file, line)
        let mut scale = 0.0f64;
        let mut offset = 0.0f64;
        for l in text.lines() {
            let Some(&kind) = l.as_bytes().first() else { continue };
            if !content {
                // the preamble (and Input lines, which come anywhere)
                let int = |v: &str, or: i64| v.trim().parse().unwrap_or(or);
                if let Some(v) = l.strip_prefix("Unit:") {
                    unit = int(v, 1);
                } else if let Some(v) = l.strip_prefix("Magnification:") {
                    magnification = int(v, 1000);
                } else if let Some(v) = l.strip_prefix("Y Offset:") {
                    y_offset = int(v, 0);
                } else if l.starts_with("Content:") {
                    content = true;
                    // synctex_parser.c: unit = pre_unit / 65781.76 × magnification / 1000; offset = pre_offset × pre_unit / 65781.76
                    let unit = if unit > 0 { unit as f64 } else { 8192.0 };
                    let magnification = if magnification > 0 { magnification as f64 } else { 1000.0 };
                    scale = unit / SP_PER_BP * magnification / 1000.0;
                    offset = y_offset as f64 * unit / SP_PER_BP;
                }
            }
            match kind {
                b'I' => {
                    let Some((tag, path)) = l.strip_prefix("Input:").and_then(|r| r.split_once(':')) else { continue };
                    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
                    let (Ok(tag), true) = (tag.parse::<usize>(), keep(name)) else { continue };
                    let f = st.files.iter().position(|x| x == name).unwrap_or_else(|| {
                        st.files.push(name.to_string());
                        st.files.len() - 1
                    });
                    if tags.len() <= tag {
                        tags.resize(tag + 1, None);
                    }
                    tags[tag] = Some(f);
                }
                b'{' if content => {
                    page = l[1..].trim().parse().unwrap_or(0);
                    if found.len() < page {
                        found.resize(page, Vec::new());
                    }
                }
                b'}' if content => page = 0,
                b'x' | b'k' | b'g' | b'$' | b'h' | b'v' | b'r' if content && page > 0 => {
                    let Some((tag, line, _h, v)) = record(&l[1..]) else { continue };
                    let Some(&Some(f)) = tags.get(tag as usize) else { continue };
                    found[page - 1].push(((v as f64 * scale + offset) as f32, f, line));
                }
                b'P' if l.starts_with("Postamble:") => break,
                _ => {}
            }
        }
        st.sheets = found.len() as u32;
        // each page's baselines (records within 1 bp of the first below it) and the line that wins each, the earlier
        // line on a tie
        for mut recs in found {
            recs.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
            let mut base = Vec::new();
            let mut i = 0;
            while i < recs.len() {
                let v = recs[i].0;
                let n = recs[i..].iter().take_while(|r| r.0 - v <= 1.0).count();
                let mut votes: Vec<((usize, u32), usize)> = Vec::new();
                for r in &recs[i..i + n] {
                    match votes.iter_mut().find(|(x, _)| *x == (r.1, r.2)) {
                        Some(c) => c.1 += 1,
                        None => votes.push(((r.1, r.2), 1)),
                    }
                }
                let ((f, line), _) = votes.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0 .1.cmp(&a.0 .1))).unwrap_or(((0, 0), 0));
                // a lone record is no line of text (the page's head and foot boxes, tagged at shipout)
                if n > 1 {
                    base.push((v, f, line));
                }
                i += n;
            }
            st.pages.push(base);
        }
        st.place_lines();
        Ok(st)
    }

    /// Each line's place, from the baselines it wins on the pages.
    fn place_lines(&mut self) {
        let mut won: Vec<Vec<(u32, Pos)>> = vec![Vec::new(); self.files.len()];
        for (k, base) in self.pages.iter().enumerate() {
            for &(v, f, line) in base {
                won[f].push((line, Pos { page: k as u32 + 1, v }));
            }
        }
        self.lines.clear();
        // a line's place: of the baselines it wins, the first at or after the line before it's (a stray win comes before
        // the line's text), or its first when there is none after (a float placed above the text before it)
        for w in won {
            // the baselines were won page by page from the top, so each line's places are in order already
            let mut by_line: Vec<Vec<Pos>> = vec![Vec::new(); w.iter().map(|x| x.0 as usize + 1).max().unwrap_or(0)];
            for (line, p) in w {
                by_line[line as usize].push(p);
            }
            let mut lines: Vec<(u32, Pos)> = Vec::new();
            for (line, places) in by_line.iter().enumerate().filter(|(_, p)| !p.is_empty()) {
                let prev = lines.last().map(|l| l.1);
                let pick = prev.and_then(|p| places.iter().copied().find(|x| !x.before(p))).unwrap_or(places[0]);
                lines.push((line as u32, pick));
            }
            self.lines.push(lines);
        }
    }

    /// The index of a spliced PDF (splice.rs): each of its pages made of bands of the full build's (`main`) or the
    /// preview's pages, `(from preview, page from 1, from, to)` in bp from the page's top.
    pub fn spliced(main: &Synctex, preview: &Synctex, pages: &[Vec<crate::splice::Band>]) -> Synctex {
        let mut st = Synctex { files: main.files.clone(), ..Default::default() };
        let pmap: Vec<usize> = preview
            .files
            .iter()
            .map(|f| {
                st.files.iter().position(|x| x == f).unwrap_or_else(|| {
                    st.files.push(f.clone());
                    st.files.len() - 1
                })
            })
            .collect();
        for bands in pages {
            let mut base = Vec::new();
            for &(from_preview, page, a, b, dy) in bands {
                let src = if from_preview { preview } else { main };
                for &(v, f, line) in src.pages.get(page as usize - 1).map(Vec::as_slice).unwrap_or(&[]) {
                    if v + dy >= a && v + dy < b {
                        base.push((v + dy, if from_preview { pmap[f] } else { f }, line));
                    }
                }
            }
            base.sort_by(|x, y| x.0.total_cmp(&y.0));
            st.pages.push(base);
        }
        st.sheets = st.pages.len() as u32;
        st.place_lines();
        st
    }

    /// The kept file of this name.
    /// The typeset lines of a kept file, in order (the tests sample them).
    pub fn typeset_lines(&self, f: usize) -> Vec<u32> {
        self.lines.get(f).map(|l| l.iter().map(|x| x.0).collect()).unwrap_or_default()
    }

    pub fn file(&self, name: &str) -> Option<usize> {
        self.files.iter().position(|x| x == name)
    }

    /// Where .tex line `t` of a file is: the first line from it on that was typeset and is not one `skip` names (a
    /// blank line or a comment, whose few records belong to the paragraph before it), or the file's last.
    pub fn line_pos(&self, f: usize, t: u32, skip: impl Fn(u32) -> bool) -> Option<Pos> {
        let lines = self.lines.get(f)?;
        let k = lines.partition_point(|(l, _)| *l < t);
        lines[k..].iter().find(|(l, _)| !skip(*l)).or(lines.last()).map(|(_, p)| *p)
    }

    /// The input line shown at a place: the line of the first baseline at or below it on its page, or of the next
    /// page's first. (file name, line).
    pub fn at(&self, p: Pos) -> Option<(&str, u32)> {
        let on = |page: u32, v: f32| -> Option<(usize, u32)> {
            let base = self.pages.get((page as usize).checked_sub(1)?)?;
            base.get(base.partition_point(|b| b.0 < v - 1.0)).map(|b| (b.1, b.2))
        };
        let (f, line) = on(p.page, p.v).or_else(|| on(p.page + 1, f32::MIN))?;
        Some((self.files[f].as_str(), line))
    }
}

/// How many parts the lookups step over at most, looking for the next or the previous place.
const STEPS: usize = 64;

/// Where the part beginning at .tex line `t` is typeset.
fn place(st: &Synctex, f: usize, map: &LineMap, t: u32) -> Option<Pos> {
    st.line_pos(f, t, |l| map.empty.binary_search(&l).is_ok())
}

/// The place of the first part after the one ending at note line `last` that is typeset below `a`, within two pages
/// of it: what a position inside the part is interpolated towards. None when the next part placed elsewhere is above
/// it (a float).
fn next_place(st: &Synctex, f: usize, map: &LineMap, mut last: u32, a: Pos, page_h: f32) -> Option<Pos> {
    for _ in 0..STEPS {
        let (_, l, t) = map.part(last + 1).filter(|p| p.0 > last)?;
        let b = place(st, f, map, t)?;
        if b.y(page_h) > a.y(page_h) {
            return (b.y(page_h) - a.y(page_h) < 2.0 * page_h).then_some(b);
        }
        if b.y(page_h) < a.y(page_h) {
            return None;
        }
        last = l;
    }
    None
}

/// Where note line `line` (from 1) of the chapter `file` is in the PDF, `frac` (0 to 1) of the way through its part:
/// between the place where the part is typeset and where the next part is, on pages `page_h` high.
pub fn forward(st: &Synctex, file: &str, map: &LineMap, line: u32, frac: f32, page_h: f32) -> Option<Pos> {
    let f = st.file(file)?;
    let (_, last, t) = map.part(line)?;
    let a = place(st, f, map, t)?;
    Some(match next_place(st, f, map, last, a, page_h) {
        Some(b) => Pos::at_y(a.y(page_h) + frac.clamp(0.0, 1.0) * (b.y(page_h) - a.y(page_h)), page_h),
        None => a,
    })
}

/// The note position a place in the PDF shows, `at` having found .tex line `t` of the chapter `file` there: the part's
/// first and last note lines and how far through it (0 to 1). The inverse of `forward`.
pub fn reverse(st: &Synctex, file: &str, map: &LineMap, t: u32, p: Pos, page_h: f32) -> Option<(u32, u32, f32)> {
    let f = st.file(file)?;
    let (mut first, mut last, t0) = map.part_of_tex(t)?;
    let mut a = place(st, f, map, t0)?;
    // the part's text begins below the place (the place is in the space after the parts before it, or on a line end
    // of the paragraph before a blank line): the nearest part before it that begins at or above the place
    for _ in 0..STEPS {
        if a.y(page_h) <= p.y(page_h) || first <= 1 {
            break;
        }
        let Some((f0, l0, t1)) = map.part(first - 1) else { break };
        let Some(a0) = place(st, f, map, t1) else { break };
        (first, last, a) = (f0, l0, a0);
    }
    let frac = match next_place(st, f, map, last, a, page_h) {
        Some(b) => ((p.y(page_h) - a.y(page_h)) / (b.y(page_h) - a.y(page_h))).clamp(0.0, 1.0),
        None => 0.0,
    };
    Some((first, last, frac))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two pages; chapter a.tex (tag 2) lines 3, 4, 6, stray records of line 6 at the top of page 1 and a lone record
    /// of line 7 (no line of text).
    const FILE: &str = "SyncTeX Version:1\nInput:1:/x/main.tex\nInput:2:C:\\x\\./a.tex\nOutput:pdf\nMagnification:1000\nUnit:1\nX Offset:0\nY Offset:0\nContent:\n!100\n{1\n[1,5:0,0:100,100,0\n(2,6:4736286,4736286:0,0,0\nx2,6:4736286,4736286\ng2,6:4836286,4736286\n)\nk2,7:0,7000000:5\n(2,4:4736286,6579000:0,0,0\nx2,3:4736286,6579000\ng2,3:5000000,6579000\nx2,6:5100000,6579000\n)\n(2,5:4736286,8000000:0,0,0\nk2,4:4736286,8000000:-100\nx2,4:4736286,8000000\nx1,9:0,9000000\n)\n]\n}1\n{2\nx2,6:4736286,4736286\ng2,6:5000000,4736286\nx2,6:4736286,9473000\nx2,6:5736286,9473000\n}2\nInput:3:/x/main.aux\nPostamble:\nCount:9\n";

    fn st() -> Synctex {
        Synctex::new(FILE, |n| n == "a.tex").unwrap()
    }

    #[test]
    fn parses_inputs_pages_and_units() {
        let st = st();
        assert_eq!(st.files, ["a.tex"]);
        assert_eq!(st.sheets, 2);
        // 4736286 sp is 1 in, 72 bp
        let p = st.line_pos(0, 3, |_| false).unwrap();
        assert_eq!(p.page, 1);
        assert!((p.v - 100.013).abs() < 0.01, "{p:?}"); // 6579000 / 65781.76
        assert_eq!(st.line_pos(0, 1, |_| false), st.line_pos(0, 3, |_| false)); // before the first typeset line: the first
        assert_eq!(st.line_pos(0, 5, |_| false), st.line_pos(0, 6, |_| false)); // nothing typeset: the next
        assert!(Synctex::new("Not one", |_| true).is_err());
        assert!(st.pages[0].iter().all(|r| r.1 == 0)); // main.tex's records left out
    }

    #[test]
    fn stray_records_and_majority() {
        let st = st();
        // line 6 is first at the top of page 1 (a stray record) and at 100 bp, but in order it comes after line 4
        // (page 1, 121.6 bp): page 2
        assert_eq!(st.line_pos(0, 6, |_| false).unwrap().page, 2);
        assert_eq!(st.line_pos(0, 3, |l| l == 3), st.line_pos(0, 4, |_| false)); // line 3 skipped (a blank line)
        assert!(st.pages[0].iter().all(|b| b.2 != 7)); // the lone record is no baseline
        // at 100 bp the baseline holds two records of line 3 and one of line 6: line 3
        assert_eq!(st.at(Pos { page: 1, v: 99.5 }), Some(("a.tex", 3)));
        assert_eq!(st.at(Pos { page: 1, v: 110.0 }), Some(("a.tex", 4)));
        // below the last line of page 1: the next page's first
        assert_eq!(st.at(Pos { page: 1, v: 500.0 }), Some(("a.tex", 6)));
        assert_eq!(st.at(Pos { page: 3, v: 0.0 }), None);
    }

    #[test]
    fn forward_and_back() {
        let st = st();
        // note lines 1-2 properties, 3 -> tex 3, 4-5 -> tex 4 (a part of two lines), 6 -> tex 6
        let map = LineMap { tex: vec![1, 1, 3, 4, 4, 6], empty: vec![5] };
        let a = forward(&st, "a.tex", &map, 3, 0.0, 842.0).unwrap();
        let b = forward(&st, "a.tex", &map, 4, 0.0, 842.0).unwrap();
        let mid = forward(&st, "a.tex", &map, 3, 0.5, 842.0).unwrap();
        assert!((mid.v - (a.v + b.v) / 2.0).abs() < 0.01);
        for (line, frac) in [(3, 0.0), (3, 0.5), (4, 0.0), (6, 0.0)] {
            let p = forward(&st, "a.tex", &map, line, frac, 842.0).unwrap();
            let (file, t) = st.at(p).unwrap();
            let (first, _, f) = reverse(&st, file, &map, t, p, 842.0).unwrap();
            assert_eq!((first, (f * 100.0).round()), (map.part(line).unwrap().0, frac * 100.0), "line {line} frac {frac}");
        }
        assert_eq!(forward(&st, "b.tex", &map, 3, 0.0, 842.0), None);
    }
}
