//! A small PDF reader and writer, enough to splice LuaTeX's PDFs (splice.rs): the objects of a file (from its cross-reference
//! tables or streams, object streams included, or by scanning the file when those are broken), and a new file written
//! from objects, with a classic cross-reference table. Streams are copied as they are (their filters kept); only object
//! streams, cross-reference streams and content streams that must be joined are inflated.
//!
//! The syntax is ISO 32000-1, section 7 (<https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf>):
//! 7.2 lexical conventions, 7.3 objects, 7.5 file structure (7.5.4 the cross-reference table, 7.5.7 object streams, 7.5.8
//! cross-reference streams), 7.4.4.4 the PNG predictors of FlateDecode. lopdf was measured (2026-10-25) at about 840 KB of
//! WebAssembly on its own, more than this plugin, so this reads only what LuaTeX writes and what a splice needs.

use std::collections::BTreeMap;

pub type Dict = Vec<(Vec<u8>, Obj)>;

/// A PDF object. Numbers keep their text when they are reals, so a file written back has the same numbers.
#[derive(Clone, Debug, PartialEq)]
pub enum Obj {
    Null,
    Bool(bool),
    Int(i64),
    Real(String),
    Name(Vec<u8>),
    Str(Vec<u8>),
    Array(Vec<Obj>),
    Dict(Dict),
    Ref(u32, u16),
    Stream(Dict, Vec<u8>),
}

impl Obj {
    pub fn dict(&self) -> Option<&Dict> {
        match self {
            Obj::Dict(d) | Obj::Stream(d, _) => Some(d),
            _ => None,
        }
    }

    pub fn dict_mut(&mut self) -> Option<&mut Dict> {
        match self {
            Obj::Dict(d) | Obj::Stream(d, _) => Some(d),
            _ => None,
        }
    }

    pub fn array(&self) -> Option<&Vec<Obj>> {
        match self {
            Obj::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn int(&self) -> Option<i64> {
        match self {
            Obj::Int(i) => Some(*i),
            Obj::Real(r) => r.parse::<f64>().ok().map(|f| f as i64),
            _ => None,
        }
    }

    pub fn num(&self) -> Option<f64> {
        match self {
            Obj::Int(i) => Some(*i as f64),
            Obj::Real(r) => r.parse().ok(),
            _ => None,
        }
    }

    pub fn name(&self) -> Option<&[u8]> {
        match self {
            Obj::Name(n) => Some(n),
            _ => None,
        }
    }

    pub fn reference(&self) -> Option<u32> {
        match self {
            Obj::Ref(n, _) => Some(*n),
            _ => None,
        }
    }
}

/// The value of `key` in a dictionary.
pub fn get<'a>(d: &'a Dict, key: &[u8]) -> Option<&'a Obj> {
    d.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Set `key`, replacing its value or adding it.
pub fn put(d: &mut Dict, key: &[u8], v: Obj) {
    match d.iter_mut().find(|(k, _)| k == key) {
        Some(e) => e.1 = v,
        None => d.push((key.to_vec(), v)),
    }
}

pub fn remove(d: &mut Dict, key: &[u8]) {
    d.retain(|(k, _)| k != key);
}

// ------------------------------------------------------------------ reading

fn white(c: u8) -> bool {
    matches!(c, 0 | 9 | 10 | 12 | 13 | 32)
}

fn delim(c: u8) -> bool {
    matches!(c, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

/// A position in a file's bytes, reading objects.
struct Lex<'a> {
    b: &'a [u8],
    i: usize,
}

impl Lex<'_> {
    fn skip(&mut self) {
        while self.i < self.b.len() {
            match self.b[self.i] {
                c if white(c) => self.i += 1,
                b'%' => {
                    while self.i < self.b.len() && !matches!(self.b[self.i], b'\r' | b'\n') {
                        self.i += 1;
                    }
                }
                _ => break,
            }
        }
    }

    /// A run of regular characters (a number or a keyword).
    fn word(&mut self) -> &[u8] {
        self.skip();
        let s = self.i;
        while self.i < self.b.len() && !white(self.b[self.i]) && !delim(self.b[self.i]) {
            self.i += 1;
        }
        &self.b[s..self.i]
    }

    fn peek_word(&mut self) -> Vec<u8> {
        let at = self.i;
        let w = self.word().to_vec();
        self.i = at;
        w
    }

    fn object(&mut self) -> Result<Obj, String> {
        self.skip();
        let Some(&c) = self.b.get(self.i) else { return Err("unexpected end of file".into()) };
        match c {
            b'/' => {
                self.i += 1;
                let s = self.i;
                while self.i < self.b.len() && !white(self.b[self.i]) && !delim(self.b[self.i]) {
                    self.i += 1;
                }
                Ok(Obj::Name(unescape_name(&self.b[s..self.i])))
            }
            b'(' => self.literal().map(Obj::Str),
            b'<' if self.b.get(self.i + 1) == Some(&b'<') => {
                self.i += 2;
                let mut d = Dict::new();
                loop {
                    self.skip();
                    if self.b[self.i..].starts_with(b">>") {
                        self.i += 2;
                        break;
                    }
                    let Obj::Name(k) = self.object()? else { return Err(format!("a dictionary key that is not a name at byte {}", self.i)) };
                    let v = self.object()?;
                    d.push((k, v));
                }
                // a stream follows its dictionary
                let at = self.i;
                if self.word() == b"stream" {
                    if self.b.get(self.i) == Some(&b'\r') {
                        self.i += 1;
                    }
                    if self.b.get(self.i) == Some(&b'\n') {
                        self.i += 1;
                    }
                    let start = self.i;
                    let len = get(&d, b"Length").and_then(Obj::int).map(|n| n as usize);
                    let end = match len {
                        Some(n) if start + n <= self.b.len() && after_data(self.b, start + n) => start + n,
                        _ => find(self.b, start, b"endstream").map(|e| trim_eol(self.b, start, e)).ok_or("a stream without endstream")?,
                    };
                    let data = self.b[start..end].to_vec();
                    self.i = find(self.b, end, b"endstream").map_or(end, |e| e + 9);
                    return Ok(Obj::Stream(d, data));
                }
                self.i = at;
                Ok(Obj::Dict(d))
            }
            b'<' => {
                self.i += 1;
                let s = self.i;
                while self.i < self.b.len() && self.b[self.i] != b'>' {
                    self.i += 1;
                }
                let hex: Vec<u8> = self.b[s..self.i].iter().copied().filter(u8::is_ascii_hexdigit).collect();
                self.i += 1;
                let digit = |c: u8| (c as char).to_digit(16).unwrap_or(0) as u8;
                Ok(Obj::Str(hex.chunks(2).map(|p| digit(p[0]) << 4 | p.get(1).map_or(0, |&c| digit(c))).collect()))
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                loop {
                    self.skip();
                    if self.b.get(self.i) == Some(&b']') {
                        self.i += 1;
                        break;
                    }
                    if self.i >= self.b.len() {
                        return Err("an array without ]".into());
                    }
                    a.push(self.object()?);
                }
                Ok(Obj::Array(a))
            }
            _ => {
                let w = self.word().to_vec();
                match w.as_slice() {
                    b"true" => Ok(Obj::Bool(true)),
                    b"false" => Ok(Obj::Bool(false)),
                    b"null" => Ok(Obj::Null),
                    _ => {
                        let text = std::str::from_utf8(&w).map_err(|_| "a malformed number")?;
                        if let Ok(n) = text.parse::<i64>() {
                            // `n g R` is a reference
                            let at = self.i;
                            let g = self.word().to_vec();
                            if let Ok(g) = std::str::from_utf8(&g).unwrap_or("").parse::<u16>() {
                                if self.word() == b"R" && n >= 0 {
                                    return Ok(Obj::Ref(n as u32, g));
                                }
                            }
                            self.i = at;
                            Ok(Obj::Int(n))
                        } else if text.parse::<f64>().is_ok() || text == "." || text == "-." {
                            Ok(Obj::Real(text.to_string()))
                        } else {
                            Err(format!("unexpected {text:?} at byte {}", self.i))
                        }
                    }
                }
            }
        }
    }

    fn literal(&mut self) -> Result<Vec<u8>, String> {
        self.i += 1;
        let (mut out, mut depth) = (Vec::new(), 1);
        while let Some(&c) = self.b.get(self.i) {
            self.i += 1;
            match c {
                b'(' => {
                    depth += 1;
                    out.push(c);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(out);
                    }
                    out.push(c);
                }
                b'\\' => {
                    let Some(&e) = self.b.get(self.i) else { break };
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'\r' => {
                            if self.b.get(self.i) == Some(&b'\n') {
                                self.i += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut v = (e - b'0') as u32;
                            for _ in 0..2 {
                                match self.b.get(self.i) {
                                    Some(&d @ b'0'..=b'7') => {
                                        v = v * 8 + (d - b'0') as u32;
                                        self.i += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(v as u8);
                        }
                        _ => out.push(e),
                    }
                }
                _ => out.push(c),
            }
        }
        Err("a string without )".into())
    }
}

/// Whether `endstream` follows (after an end of line) at `k`: the stream's /Length was right.
fn after_data(b: &[u8], k: usize) -> bool {
    let rest = &b[k.min(b.len())..];
    let s = rest.iter().position(|&c| !white(c)).unwrap_or(rest.len());
    rest[s..].starts_with(b"endstream")
}

fn trim_eol(b: &[u8], start: usize, mut e: usize) -> usize {
    if e > start && b[e - 1] == b'\n' {
        e -= 1;
    }
    if e > start && b[e - 1] == b'\r' {
        e -= 1;
    }
    e
}

fn find(b: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    b.get(from..)?.windows(pat.len()).position(|w| w == pat).map(|k| k + from)
}

fn rfind(b: &[u8], pat: &[u8]) -> Option<usize> {
    b.windows(pat.len()).rposition(|w| w == pat)
}

fn unescape_name(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut k = 0;
    while k < raw.len() {
        if raw[k] == b'#' && k + 3 <= raw.len() {
            if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&raw[k + 1..k + 3]).unwrap_or("zz"), 16) {
                out.push(v);
                k += 3;
                continue;
            }
        }
        out.push(raw[k]);
        k += 1;
    }
    out
}

/// A stream's data with its filters undone (FlateDecode only, with the PNG predictors).
pub fn decode(d: &Dict, data: &[u8]) -> Result<Vec<u8>, String> {
    let filters: Vec<&[u8]> = match get(d, b"Filter") {
        None => vec![],
        Some(Obj::Name(n)) => vec![n.as_slice()],
        Some(Obj::Array(a)) => a.iter().filter_map(Obj::name).collect(),
        Some(_) => return Err("a malformed /Filter".into()),
    };
    let mut out = data.to_vec();
    for f in filters {
        if f != b"FlateDecode" {
            return Err(format!("the filter {} is not supported", String::from_utf8_lossy(f)));
        }
        out = miniz_oxide::inflate::decompress_to_vec_zlib(&out).map_err(|e| format!("inflate: {e:?}"))?;
    }
    if let Some(p) = get(d, b"DecodeParms").and_then(Obj::dict) {
        let predictor = get(p, b"Predictor").and_then(Obj::int).unwrap_or(1);
        if predictor >= 10 {
            let cols = get(p, b"Columns").and_then(Obj::int).unwrap_or(1) as usize;
            out = unpredict(&out, cols)?;
        }
    }
    Ok(out)
}

/// A gzip file's content (RFC 1952: a header, raw deflate, a trailer): SyncTeX's .synctex.gz.
pub fn gunzip(b: &[u8]) -> Option<Vec<u8>> {
    if b.len() < 18 || b[0] != 0x1f || b[1] != 0x8b || b[2] != 8 {
        return None;
    }
    let flags = b[3];
    let mut i = 10;
    if flags & 4 != 0 {
        i += 2 + u16::from_le_bytes([b[i], b[i + 1]]) as usize; // FEXTRA
    }
    for bit in [8u8, 16] {
        // FNAME, FCOMMENT: zero-terminated
        if flags & bit != 0 {
            i += b.get(i..)?.iter().position(|&c| c == 0)? + 1;
        }
    }
    if flags & 2 != 0 {
        i += 2; // FHCRC
    }
    miniz_oxide::inflate::decompress_to_vec(b.get(i..b.len() - 8)?).ok()
}

/// PNG predictors, one byte of filter type before each row of `cols` bytes (ISO 32000-1, 7.4.4.4; RFC 2083, 6).
fn unpredict(data: &[u8], cols: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(data.len());
    let mut prev = vec![0u8; cols];
    for row in data.chunks(cols + 1) {
        if row.len() < cols + 1 {
            break;
        }
        let (t, r) = (row[0], &row[1..]);
        let mut cur = vec![0u8; cols];
        for k in 0..cols {
            let left = if k > 0 { cur[k - 1] } else { 0 };
            let (up, ul) = (prev[k], if k > 0 { prev[k - 1] } else { 0 });
            cur[k] = r[k].wrapping_add(match t {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((left as u16 + up as u16) / 2) as u8,
                4 => {
                    let p = left as i16 + up as i16 - ul as i16;
                    let (pa, pb, pc) = ((p - left as i16).abs(), (p - up as i16).abs(), (p - ul as i16).abs());
                    if pa <= pb && pa <= pc {
                        left
                    } else if pb <= pc {
                        up
                    } else {
                        ul
                    }
                }
                _ => return Err(format!("PNG predictor {t} is not one")),
            });
        }
        out.extend_from_slice(&cur);
        prev = cur;
    }
    Ok(out)
}

/// A PDF file's objects and trailer.
#[derive(Clone, Debug, Default)]
pub struct Pdf {
    pub objs: BTreeMap<u32, Obj>,
    pub trailer: Dict,
}

/// Where an object is: at a byte offset, or the k-th object of an object stream.
#[derive(Clone, Copy)]
enum At {
    Offset(usize),
    InStream(u32, usize),
}

impl Pdf {
    /// Reads a file: its cross-reference sections (the newest first, along /Prev), or, if those cannot be read, every
    /// `n g obj` in it.
    pub fn read(b: &[u8]) -> Result<Pdf, String> {
        if !b.starts_with(b"%PDF-") {
            return Err("not a PDF file".into());
        }
        match Self::from_xref(b) {
            Ok(p) => Ok(p),
            Err(_) => Self::by_scanning(b),
        }
    }

    fn from_xref(b: &[u8]) -> Result<Pdf, String> {
        let sx = rfind(b, b"startxref").ok_or("no startxref")?;
        let mut lx = Lex { b, i: sx + 9 };
        let mut next = std::str::from_utf8(lx.word()).ok().and_then(|w| w.parse::<usize>().ok());
        let mut at: BTreeMap<u32, At> = BTreeMap::new();
        let mut trailer: Option<Dict> = None;
        let mut seen = Vec::new();
        while let Some(off) = next.filter(|o| *o < b.len() && !seen.contains(o)) {
            seen.push(off);
            let mut lx = Lex { b, i: off };
            let tr = if lx.peek_word() == b"xref" {
                lx.word();
                loop {
                    if lx.peek_word() == b"trailer" {
                        lx.word();
                        break;
                    }
                    let first: u32 = std::str::from_utf8(lx.word()).ok().and_then(|x| x.parse().ok()).ok_or("a malformed xref section")?;
                    let count: u32 = std::str::from_utf8(lx.word()).ok().and_then(|x| x.parse().ok()).ok_or("a malformed xref section")?;
                    for k in 0..count {
                        let o = std::str::from_utf8(lx.word()).ok().and_then(|x| x.parse::<usize>().ok()).ok_or("a malformed xref entry")?;
                        let _g = lx.word().to_vec();
                        let kind = lx.word().to_vec();
                        if kind == b"n" {
                            at.entry(first + k).or_insert(At::Offset(o));
                        }
                    }
                }
                match lx.object()? {
                    Obj::Dict(d) => d,
                    _ => return Err("a trailer that is not a dictionary".into()),
                }
            } else {
                // a cross-reference stream: `n g obj << /Type /XRef ... >> stream`
                lx.word();
                lx.word();
                lx.word();
                let Obj::Stream(d, data) = lx.object()? else { return Err("startxref points at no xref".into()) };
                let rows = decode(&d, &data)?;
                let w: Vec<usize> = get(&d, b"W").and_then(Obj::array).ok_or("an xref stream without /W")?.iter().map(|x| x.int().unwrap_or(0) as usize).collect();
                let size = get(&d, b"Size").and_then(Obj::int).unwrap_or(0);
                let index: Vec<i64> = get(&d, b"Index").and_then(Obj::array).map_or(vec![0, size], |a| a.iter().filter_map(Obj::int).collect());
                let width: usize = w.iter().sum();
                let field = |row: &[u8], k: usize| -> usize {
                    let s: usize = w[..k].iter().sum();
                    row[s..s + w[k]].iter().fold(0, |v, &c| v << 8 | c as usize)
                };
                let mut rows = rows.chunks(width.max(1));
                for pair in index.chunks(2) {
                    for k in 0..pair.get(1).copied().unwrap_or(0) {
                        let Some(row) = rows.next() else { break };
                        let n = (pair[0] + k) as u32;
                        let kind = if w[0] == 0 { 1 } else { field(row, 0) };
                        match kind {
                            1 => _ = at.entry(n).or_insert(At::Offset(field(row, 1))),
                            2 => _ = at.entry(n).or_insert(At::InStream(field(row, 1) as u32, field(row, 2))),
                            _ => {}
                        }
                    }
                }
                d
            };
            next = get(&tr, b"Prev").and_then(Obj::int).map(|p| p as usize);
            if trailer.is_none() {
                trailer = Some(tr);
            }
        }
        let mut trailer = trailer.ok_or("no trailer")?;
        for k in [b"Prev".as_slice(), b"Index", b"W", b"Filter", b"DecodeParms", b"Length", b"Type", b"XRefStm"] {
            remove(&mut trailer, k);
        }
        let mut pdf = Pdf { objs: BTreeMap::new(), trailer };
        let mut streams: BTreeMap<u32, Vec<(u32, usize)>> = BTreeMap::new();
        for (&n, &a) in &at {
            match a {
                At::Offset(o) => {
                    let mut lx = Lex { b, i: o };
                    let (num, _g, kw) = (lx.word().to_vec(), lx.word().to_vec(), lx.word().to_vec());
                    if kw != b"obj" || std::str::from_utf8(&num).ok().and_then(|x| x.parse::<u32>().ok()) != Some(n) {
                        return Err(format!("object {n} is not at its offset"));
                    }
                    pdf.objs.insert(n, lx.object()?);
                }
                At::InStream(s, k) => streams.entry(s).or_default().push((n, k)),
            }
        }
        for (s, members) in streams {
            let Some(Obj::Stream(d, data)) = pdf.objs.get(&s).cloned() else { return Err(format!("object stream {s} is missing")) };
            for (n, o) in object_stream(&d, &data)? {
                if members.iter().any(|(m, _)| *m == n) {
                    pdf.objs.entry(n).or_insert(o);
                }
            }
        }
        pdf.objs.retain(|_, o| !matches!(o, Obj::Stream(d, _) if matches!(get(d, b"Type"), Some(Obj::Name(t)) if t == b"ObjStm" || t == b"XRef")));
        Ok(pdf)
    }

    /// Every `n g obj` in the file, the last one of a number winning, and the trailer of the last `trailer` (or the
    /// cross-reference stream that has /Root).
    fn by_scanning(b: &[u8]) -> Result<Pdf, String> {
        let mut pdf = Pdf::default();
        let mut i = 0;
        while let Some(k) = find(b, i, b" obj") {
            i = k + 4;
            // back over `n g`
            let line_start = b[..k].iter().rposition(|&c| c == b'\n' || c == b'\r').map_or(0, |p| p + 1);
            let head = String::from_utf8_lossy(&b[line_start..k]).to_string();
            let mut parts = head.split_whitespace().rev();
            let (Some(_g), Some(n)) = (parts.next(), parts.next()) else { continue };
            let Ok(n) = n.parse::<u32>() else { continue };
            let mut lx = Lex { b, i: k + 4 };
            if let Ok(o) = lx.object() {
                if let Obj::Stream(d, data) = &o {
                    if matches!(get(d, b"Type"), Some(Obj::Name(t)) if t == b"ObjStm") {
                        for (m, x) in object_stream(d, data).unwrap_or_default() {
                            pdf.objs.insert(m, x);
                        }
                        continue;
                    }
                    if matches!(get(d, b"Type"), Some(Obj::Name(t)) if t == b"XRef") {
                        pdf.trailer = d.iter().filter(|(k, _)| [b"Root".as_slice(), b"Info", b"ID"].contains(&k.as_slice())).cloned().collect();
                        continue;
                    }
                }
                pdf.objs.insert(n, o);
            }
        }
        if let Some(t) = rfind(b, b"trailer") {
            if let Ok(Obj::Dict(d)) = (Lex { b, i: t + 7 }).object() {
                pdf.trailer = d;
            }
        }
        if get(&pdf.trailer, b"Root").is_none() {
            return Err("no /Root".into());
        }
        Ok(pdf)
    }

    pub fn get(&self, n: u32) -> Option<&Obj> {
        self.objs.get(&n)
    }

    /// An object, through a reference if it is one.
    pub fn resolve<'a>(&'a self, o: &'a Obj) -> &'a Obj {
        match o {
            Obj::Ref(n, _) => self.objs.get(n).unwrap_or(&Obj::Null),
            _ => o,
        }
    }

    pub fn catalog(&self) -> Option<&Dict> {
        get(&self.trailer, b"Root").map(|r| self.resolve(r)).and_then(Obj::dict)
    }

    pub fn root_id(&self) -> Option<u32> {
        get(&self.trailer, b"Root").and_then(Obj::reference)
    }

    /// The pages in order (their object numbers), with the attributes they inherit from the page tree (Resources,
    /// MediaBox, CropBox, Rotate) where they lack their own.
    pub fn pages(&self) -> Vec<(u32, Dict)> {
        let mut out = Vec::new();
        let Some(root) = self.catalog().and_then(|c| get(c, b"Pages")).and_then(Obj::reference) else { return out };
        self.walk(root, &Dict::new(), &mut out, 0);
        out
    }

    fn walk(&self, n: u32, inherited: &Dict, out: &mut Vec<(u32, Dict)>, depth: usize) {
        let Some(d) = self.get(n).and_then(Obj::dict) else { return };
        if depth > 64 {
            return;
        }
        let mut inh = inherited.clone();
        for k in [b"Resources".as_slice(), b"MediaBox", b"CropBox", b"Rotate"] {
            if let Some(v) = get(d, k) {
                put(&mut inh, k, v.clone());
            }
        }
        match get(d, b"Type").and_then(Obj::name) {
            Some(b"Pages") => {
                for kid in get(d, b"Kids").map(|k| self.resolve(k)).and_then(Obj::array).cloned().unwrap_or_default() {
                    if let Some(k) = kid.reference() {
                        self.walk(k, &inh, out, depth + 1);
                    }
                }
            }
            _ => {
                let mut page = d.clone();
                for (k, v) in inh {
                    if get(&page, &k).is_none() {
                        page.push((k, v));
                    }
                }
                out.push((n, page));
            }
        }
    }

    pub fn max_id(&self) -> u32 {
        self.objs.keys().next_back().copied().unwrap_or(0)
    }

    pub fn add(&mut self, o: Obj) -> u32 {
        let n = self.max_id() + 1;
        self.objs.insert(n, o);
        n
    }

    /// The file: a header, the objects in order, a cross-reference table and the trailer.
    pub fn write(&self) -> Vec<u8> {
        let mut out: Vec<u8> = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
        let size = self.max_id() + 1;
        let mut offsets = vec![None; size as usize];
        for (&n, o) in &self.objs {
            offsets[n as usize] = Some(out.len());
            out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
            write_obj(&mut out, o);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
        for (k, o) in offsets.iter().enumerate() {
            match o {
                Some(off) => out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes()),
                None => out.extend_from_slice(format!("0000000000 {} f \n", if k == 0 { 65535 } else { 0 }).as_bytes()),
            }
        }
        let mut trailer: Dict = self.trailer.iter().filter(|(k, _)| [b"Root".as_slice(), b"Info", b"ID"].contains(&k.as_slice())).cloned().collect();
        put(&mut trailer, b"Size", Obj::Int(size as i64));
        out.extend_from_slice(b"trailer\n");
        write_obj(&mut out, &Obj::Dict(trailer));
        out.extend_from_slice(format!("\nstartxref\n{xref}\n%%EOF\n").as_bytes());
        out
    }
}

/// The objects of an object stream: `/N` pairs of number and offset, then the objects from `/First`.
fn object_stream(d: &Dict, data: &[u8]) -> Result<Vec<(u32, Obj)>, String> {
    let body = decode(d, data)?;
    let n = get(d, b"N").and_then(Obj::int).unwrap_or(0) as usize;
    let first = get(d, b"First").and_then(Obj::int).unwrap_or(0) as usize;
    let mut lx = Lex { b: &body, i: 0 };
    let mut heads = Vec::new();
    for _ in 0..n {
        let num = std::str::from_utf8(lx.word()).ok().and_then(|x| x.parse::<u32>().ok()).ok_or("a malformed object stream")?;
        let off = std::str::from_utf8(lx.word()).ok().and_then(|x| x.parse::<usize>().ok()).ok_or("a malformed object stream")?;
        heads.push((num, off));
    }
    let mut out = Vec::new();
    for (num, off) in heads {
        let mut lx = Lex { b: &body, i: first + off };
        out.push((num, lx.object()?));
    }
    Ok(out)
}

// ------------------------------------------------------------------ writing

pub fn write_obj(out: &mut Vec<u8>, o: &Obj) {
    match o {
        Obj::Null => out.extend_from_slice(b"null"),
        Obj::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Obj::Int(i) => out.extend_from_slice(i.to_string().as_bytes()),
        Obj::Real(r) => out.extend_from_slice(r.as_bytes()),
        Obj::Name(n) => write_name(out, n),
        Obj::Str(s) => write_str(out, s),
        Obj::Array(a) => {
            out.push(b'[');
            for (k, x) in a.iter().enumerate() {
                if k > 0 {
                    out.push(b' ');
                }
                write_obj(out, x);
            }
            out.push(b']');
        }
        Obj::Dict(d) => write_dict(out, d),
        Obj::Ref(n, g) => out.extend_from_slice(format!("{n} {g} R").as_bytes()),
        Obj::Stream(d, data) => {
            let mut d = d.clone();
            put(&mut d, b"Length", Obj::Int(data.len() as i64));
            write_dict(out, &d);
            out.extend_from_slice(b"\nstream\n");
            out.extend_from_slice(data);
            out.extend_from_slice(b"\nendstream");
        }
    }
}

fn write_dict(out: &mut Vec<u8>, d: &Dict) {
    out.extend_from_slice(b"<<");
    for (k, v) in d {
        write_name(out, k);
        out.push(b' ');
        write_obj(out, v);
        out.push(b'\n');
    }
    out.extend_from_slice(b">>");
}

fn write_name(out: &mut Vec<u8>, n: &[u8]) {
    out.push(b'/');
    for &c in n {
        if c <= b' ' || c >= 127 || delim(c) || c == b'#' {
            out.extend_from_slice(format!("#{c:02X}").as_bytes());
        } else {
            out.push(c);
        }
    }
}

fn write_str(out: &mut Vec<u8>, s: &[u8]) {
    out.push(b'(');
    for &c in s {
        match c {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(c);
            }
            b' '..=b'~' => out.push(c),
            _ => out.extend_from_slice(format!("\\{c:03o}").as_bytes()),
        }
    }
    out.push(b')');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Obj {
        Lex { b: s.as_bytes(), i: 0 }.object().unwrap()
    }

    #[test]
    fn objects() {
        assert_eq!(parse("<< /S /GoTo /D (section.2) >>"), Obj::Dict(vec![(b"S".to_vec(), Obj::Name(b"GoTo".to_vec())), (b"D".to_vec(), Obj::Str(b"section.2".to_vec()))]));
        assert_eq!(parse("[1 0 R 2 -3.5 /XYZ null]"), Obj::Array(vec![Obj::Ref(1, 0), Obj::Int(2), Obj::Real("-3.5".into()), Obj::Name(b"XYZ".to_vec()), Obj::Null]));
        assert_eq!(parse(r"(a\(b\)\\c\101\
d)"), Obj::Str(b"a(b)\\cAd".to_vec()));
        assert_eq!(parse("<FEFF0041>"), Obj::Str(vec![0xfe, 0xff, 0, 0x41]));
        assert_eq!(parse("/A#20B"), Obj::Name(b"A B".to_vec()));
        let s = parse("<< /Length 5 >>\nstream\nhello\nendstream");
        assert_eq!(s, Obj::Stream(vec![(b"Length".to_vec(), Obj::Int(5))], b"hello".to_vec()));
        // a wrong /Length: the data runs to endstream
        assert_eq!(parse("<< /Length 99 >>\nstream\nhi\nendstream"), Obj::Stream(vec![(b"Length".to_vec(), Obj::Int(99))], b"hi".to_vec()));
    }

    #[test]
    fn round_trip() {
        for src in ["<< /S /GoTo /D (section.2) >>", "[1 0 R (a\\(b) <00ff> /N#20x 1.50 true]"] {
            let o = parse(src);
            let mut out = Vec::new();
            write_obj(&mut out, &o);
            assert_eq!(parse(std::str::from_utf8(&out).unwrap()), o);
        }
    }

    /// A file with a classic xref, written and read back.
    #[test]
    fn files() {
        let mut p = Pdf::default();
        let pages = 2;
        p.objs.insert(1, Obj::Dict(vec![(b"Type".to_vec(), Obj::Name(b"Catalog".to_vec())), (b"Pages".to_vec(), Obj::Ref(pages, 0))]));
        p.objs.insert(2, Obj::Dict(vec![(b"Type".to_vec(), Obj::Name(b"Pages".to_vec())), (b"Kids".to_vec(), Obj::Array(vec![Obj::Ref(3, 0)])), (b"Count".to_vec(), Obj::Int(1)), (b"MediaBox".to_vec(), Obj::Array(vec![Obj::Int(0), Obj::Int(0), Obj::Int(595), Obj::Int(842)]))]));
        p.objs.insert(3, Obj::Dict(vec![(b"Type".to_vec(), Obj::Name(b"Page".to_vec())), (b"Parent".to_vec(), Obj::Ref(2, 0)), (b"Contents".to_vec(), Obj::Ref(5, 0))]));
        p.objs.insert(5, Obj::Stream(vec![(b"Length".to_vec(), Obj::Int(5))], b"BT ET".to_vec()));
        p.trailer = vec![(b"Root".to_vec(), Obj::Ref(1, 0))];
        let bytes = p.write();
        let q = Pdf::read(&bytes).unwrap();
        assert_eq!(q.objs, p.objs);
        let pages = q.pages();
        assert_eq!(pages.len(), 1);
        assert!(get(&pages[0].1, b"MediaBox").is_some(), "the inherited MediaBox");
        // broken offsets: read by scanning
        let mut broken = bytes.clone();
        let x = rfind(&broken, b"startxref").unwrap();
        broken.truncate(x);
        broken.extend_from_slice(b"startxref\n99999999\n%%EOF\n");
        assert_eq!(Pdf::read(&broken).unwrap().objs, p.objs);
    }

    /// PDF_ROUNDTRIP=<file.pdf>: read, written to <file>.rt.pdf, read again: the same objects and pages.
    #[test]
    fn real_file() {
        let Ok(path) = std::env::var("PDF_ROUNDTRIP") else { return };
        let p = Pdf::read(&std::fs::read(&path).unwrap()).unwrap();
        let out = p.write();
        std::fs::write(format!("{path}.rt.pdf"), &out).unwrap();
        let q = Pdf::read(&out).unwrap();
        assert_eq!(p.objs.len(), q.objs.len());
        assert_eq!(p.pages().len(), q.pages().len());
        eprintln!("{path}: {} objects, {} pages", p.objs.len(), p.pages().len());
    }

    #[test]
    fn predictors() {
        // two rows of 2 bytes, Up predictor
        assert_eq!(unpredict(&[2, 1, 2, 2, 1, 1], 2).unwrap(), vec![1, 2, 2, 3]);
        assert_eq!(unpredict(&[1, 1, 1], 2).unwrap(), vec![1, 2]);
    }
}
