//! The PDF shown during a fast preview: the last full build's PDF with the preview's pages in place of the edited
//! regions (pipeline::fast::plan says which), so that the bibliography, the glossary and every other page are there.
//!
//! - Pages are copied from the preview with everything they use, under new object numbers.
//! - A region's first page, when it starts below the top, is joined: the full build's page above the cut and the
//!   preview's below it, each drawn as a form XObject inside a clipping rectangle (ISO 32000-1, 8.10 form XObjects,
//!   8.5.4 clipping); its links come from the part they lie in. Its last page is joined the other way round.
//! - Named destinations (12.3.2.3) and the links that use them: the name tree is rebuilt from the full build's names
//!   on pages that stay and the preview's names on its pages, each pointing at the page of the result; a link whose name
//!   is in neither, or a destination given as a page that is gone, is reported.
//! - The page tree is one flat node; page labels (12.4.2) are made again from each page's own; objects nothing uses
//!   any more are dropped.
//! - Pages that are not current get a line at the top saying so (and the manifest says which and why).

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::pdf::{self, get, put, Dict, Obj, Pdf};
use crate::pipeline::fast::{Plan, Replace};

/// A part of a joined page: (source page, from, to, moved down by), in bp from the page's top.
type Band4 = (Part, f64, f64, f64);

/// Where a page of the result comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Main(u32),
    Preview(u32),
    /// Parts of pages, top to bottom: (source page, from, to, moved down by), in bp from the page's top.
    Joined(Vec<(Part, f64, f64, f64)>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Part {
    Main(u32),
    Preview(u32),
}

/// The spliced PDF, where each of its pages comes from, what is stale (result pages, from 0, end excluded) and what was
/// found wrong.
#[derive(Debug, Default)]
pub struct Spliced {
    pub pdf: Vec<u8>,
    pub pages: Vec<Source>,
    pub stale: Vec<((u32, u32), String)>,
    pub problems: Vec<String>,
}

/// Which pages the result has, in order (pure: the splice's arithmetic, tested on its own).
pub fn layout(plan: &Plan, main_pages: u32) -> Vec<Source> {
    let mut reps: Vec<&Replace> = plan.replace.iter().collect();
    reps.sort_by_key(|r| r.main.0);
    let mut out = Vec::new();
    let mut m = 0;
    for r in reps {
        let (c, d) = r.preview;
        let n = d.saturating_sub(c);
        // a region starting on the page where the one before ended (joined there): that page gets three bands, the
        // one before above its cut, the full build's between, this one below its start
        let shared = r.main.0 + 1 == m && r.top.is_some() && r.top_page.is_none();
        let mut skip = 0;
        if shared {
            if let Some(Source::Joined(parts)) = out.last_mut() {
                if let (Some(last), Some(t)) = (parts.last_mut().filter(|p| p.0 == Part::Main(r.main.0) && p.1 < r.top.unwrap_or(0.0)), r.top) {
                    last.2 = t;
                    let bottom = r.bottom.filter(|_| n == 1);
                    parts.push((Part::Preview(c), t, bottom.unwrap_or(f64::INFINITY), r.shift));
                    if let Some(b) = bottom {
                        parts.push((Part::Main(r.main.1 - 1), b, f64::INFINITY, 0.0));
                    }
                    skip = 1;
                }
            }
        }
        if r.main.0 < m && skip == 0 {
            continue; // overlapping replacements: the first wins
        }
        out.extend((m..r.main.0).map(Source::Main));
        for (k, j) in (c..d).enumerate().skip(skip) {
            let first = k == 0;
            let last = k as u32 + 1 == n;
            let mut parts = Vec::new();
            let (top, bottom) = (r.top.filter(|_| first), r.bottom.filter(|_| last));
            if let Some(t) = top {
                parts.push((r.top_page.map_or(Part::Main(r.main.0), Part::Preview), 0.0, t, 0.0));
            }
            let dy = if first && top.is_some() { r.shift } else { 0.0 };
            let foot = r.foot.filter(|_| dy > 0.0 && bottom.is_none());
            parts.push((Part::Preview(j), top.unwrap_or(0.0), foot.or(bottom).unwrap_or(f64::INFINITY), dy));
            if let Some(f) = foot {
                parts.push((Part::Main(r.main.0), f, f64::INFINITY, 0.0));
            }
            if let Some(b) = bottom {
                parts.push((Part::Main(r.main.1 - 1), b, f64::INFINITY, 0.0));
            }
            out.push(if parts.len() == 1 { Source::Preview(j) } else { Source::Joined(parts) });
        }
        m = r.main.1;
        // the full build's page after a kept tail: only what lies below the region's end
        if let Some((c, bottom)) = r.crop.filter(|_| m < main_pages) {
            out.push(Source::Joined(vec![(Part::Main(m), c, bottom, 0.0)]));
            m += 1;
        }
    }
    out.extend((m..main_pages).map(Source::Main));
    out
}

/// A band of a page of the result: (from the preview, source page from 1, from, to, moved down by), in bp from the top.
pub type Band = (bool, u32, f32, f32, f32);

/// Each page of the result as bands of the source pages, for its SyncTeX index (synctex::Synctex::spliced): (from the
/// preview, page from 1, from, to, moved down by) in bp from the page's top.
pub fn bands(pages: &[Source]) -> Vec<Vec<Band>> {
    let part = |p: &Part| match p {
        Part::Main(i) => (false, i + 1),
        Part::Preview(j) => (true, j + 1),
    };
    pages
        .iter()
        .map(|s| match s {
            Source::Main(i) => vec![(false, i + 1, f32::NEG_INFINITY, f32::INFINITY, 0.0)],
            Source::Preview(j) => vec![(true, j + 1, f32::NEG_INFINITY, f32::INFINITY, 0.0)],
            Source::Joined(parts) => parts.iter().map(|(p, a, b, dy)| (part(p).0, part(p).1, if *a <= 0.0 { f32::NEG_INFINITY } else { *a as f32 }, *b as f32, *dy as f32)).collect(),
        })
        .collect()
}

/// A page of either PDF, with its inherited attributes.
struct Pages {
    main: Vec<(u32, Dict)>,
    preview: Vec<(u32, Dict)>,
}

/// Copies objects of the preview into the result under new numbers, once each.
struct Importer<'a> {
    from: &'a Pdf,
    map: HashMap<u32, u32>,
    /// The preview's pages that are not in the result, and its page tree nodes: references to them become null.
    void: HashSet<u32>,
}

impl Importer<'_> {
    fn obj(&mut self, out: &mut Pdf, o: &Obj) -> Obj {
        match o {
            Obj::Ref(n, _) => match self.id(out, *n) {
                Some(m) => Obj::Ref(m, 0),
                None => Obj::Null,
            },
            Obj::Array(a) => Obj::Array(a.iter().map(|x| self.obj(out, x)).collect()),
            Obj::Dict(d) => Obj::Dict(self.dict(out, d)),
            Obj::Stream(d, s) => Obj::Stream(self.dict(out, d), s.clone()),
            x => x.clone(),
        }
    }

    fn dict(&mut self, out: &mut Pdf, d: &Dict) -> Dict {
        d.iter().filter(|(k, _)| k != b"Parent").map(|(k, v)| (k.clone(), self.obj(out, v))).collect()
    }

    fn id(&mut self, out: &mut Pdf, n: u32) -> Option<u32> {
        if self.void.contains(&n) {
            return None;
        }
        if let Some(&m) = self.map.get(&n) {
            return Some(m);
        }
        let m = out.add(Obj::Null); // reserved first: objects may refer to each other
        self.map.insert(n, m);
        let o = self.from.get(n).cloned().unwrap_or(Obj::Null);
        let o = self.obj(out, &o);
        out.objs.insert(m, o);
        Some(m)
    }
}

/// The page's content as one stream: its data and the filter keys to keep (a single stream as it is; several joined,
/// inflated, and deflated again).
fn content(p: &Pdf, page: &Dict) -> (Vec<u8>, Dict) {
    let streams: Vec<&Obj> = match get(page, b"Contents").map(|c| p.resolve(c)) {
        Some(Obj::Array(a)) => a.iter().map(|x| p.resolve(x)).collect(),
        Some(s @ Obj::Stream(..)) => vec![s],
        _ => vec![],
    };
    if let [Obj::Stream(d, data)] = streams.as_slice() {
        let keep: Dict = d.iter().filter(|(k, _)| k == b"Filter" || k == b"DecodeParms").cloned().collect();
        return (data.clone(), keep);
    }
    let mut all = Vec::new();
    for s in streams {
        if let Obj::Stream(d, data) = s {
            all.extend_from_slice(&pdf::decode(d, data).unwrap_or_default());
            all.push(b'\n');
        }
    }
    (miniz_oxide::deflate::compress_to_vec_zlib(&all, 6), vec![(b"Filter".to_vec(), Obj::Name(b"FlateDecode".to_vec()))])
}

fn media_box(page: &Dict) -> [f64; 4] {
    let a: Vec<f64> = get(page, b"MediaBox").and_then(Obj::array).map(|a| a.iter().filter_map(Obj::num).collect()).unwrap_or_default();
    if a.len() == 4 {
        [a[0], a[1], a[2], a[3]]
    } else {
        [0.0, 0.0, 595.276, 841.89]
    }
}

fn real(f: f64) -> Obj {
    Obj::Real(format!("{f:.4}"))
}

/// The y of a link's rectangle's middle, in bp from the page's top.
fn annot_y(p: &Pdf, a: &Obj, height: f64, y0: f64) -> Option<f64> {
    let r: Vec<f64> = get(p.resolve(a).dict()?, b"Rect").map(|r| p.resolve(r)).and_then(Obj::array)?.iter().filter_map(Obj::num).collect();
    (r.len() == 4).then(|| height - ((r[1] + r[3]) / 2.0 - y0))
}

/// A destination's page object and y (in bp from the bottom), from `[page /XYZ x y z]`, `[page /FitH y]` ... or `<< /D [...] >>`.
fn dest_target(p: &Pdf, d: &Obj) -> Option<(u32, Option<f64>)> {
    let d = p.resolve(d);
    let a = match d {
        Obj::Array(a) => a,
        Obj::Dict(x) => return dest_target(p, get(x, b"D")?),
        _ => return None,
    };
    let page = a.first()?.reference()?;
    let y = match a.get(1).and_then(Obj::name) {
        Some(b"XYZ") => a.get(3).and_then(Obj::num),
        Some(b"FitH") | Some(b"FitBH") => a.get(2).and_then(Obj::num),
        Some(b"FitR") => a.get(5).and_then(Obj::num),
        _ => None,
    };
    Some((page, y))
}

/// Every name of a name tree (12.3.2.3 name dictionaries, 7.9.6 name trees), with its value.
fn name_tree(p: &Pdf, node: &Obj, out: &mut Vec<(Vec<u8>, Obj)>, depth: usize) {
    let Some(d) = p.resolve(node).dict() else { return };
    if depth > 32 {
        return;
    }
    if let Some(names) = get(d, b"Names").map(|n| p.resolve(n)).and_then(Obj::array) {
        for pair in names.chunks(2) {
            if let (Obj::Str(k), Some(v)) = (p.resolve(&pair[0]), pair.get(1)) {
                out.push((k.clone(), v.clone()));
            }
        }
    }
    for kid in get(d, b"Kids").map(|k| p.resolve(k)).and_then(Obj::array).cloned().unwrap_or_default() {
        name_tree(p, &kid, out, depth + 1);
    }
}

fn dests(p: &Pdf) -> Vec<(Vec<u8>, Obj)> {
    let mut out = Vec::new();
    if let Some(t) = p.catalog().and_then(|c| get(c, b"Names")).map(|n| p.resolve(n)).and_then(Obj::dict).and_then(|n| get(n, b"Dests")) {
        name_tree(p, t, &mut out, 0);
    }
    out
}

/// A page label: (style, prefix, number).
type Label = (Option<Vec<u8>>, Vec<u8>, i64);

/// Each page's label from the catalog's /PageLabels number tree.
fn labels(p: &Pdf, n: usize) -> Vec<Option<Label>> {
    let mut ranges: Vec<(usize, Label)> = Vec::new();
    if let Some(t) = p.catalog().and_then(|c| get(c, b"PageLabels")) {
        let mut flat = Vec::new();
        number_tree(p, t, &mut flat, 0);
        for (k, v) in flat {
            let Some(d) = p.resolve(&v).dict() else { continue };
            let style = get(d, b"S").and_then(Obj::name).map(<[u8]>::to_vec);
            let prefix = match get(d, b"P").map(|x| p.resolve(x)) {
                Some(Obj::Str(s)) => s.clone(),
                _ => vec![],
            };
            ranges.push((k as usize, (style, prefix, get(d, b"St").and_then(Obj::int).unwrap_or(1))));
        }
    }
    ranges.sort_by_key(|r| r.0);
    (0..n).map(|i| ranges.iter().rev().find(|r| r.0 <= i).map(|(k, l)| (l.0.clone(), l.1.clone(), l.2 + (i - k) as i64))).collect()
}

fn number_tree(p: &Pdf, node: &Obj, out: &mut Vec<(i64, Obj)>, depth: usize) {
    let Some(d) = p.resolve(node).dict() else { return };
    if depth > 32 {
        return;
    }
    if let Some(nums) = get(d, b"Nums").map(|n| p.resolve(n)).and_then(Obj::array) {
        for pair in nums.chunks(2) {
            if let (Some(k), Some(v)) = (pair[0].int(), pair.get(1)) {
                out.push((k, v.clone()));
            }
        }
    }
    for kid in get(d, b"Kids").map(|k| p.resolve(k)).and_then(Obj::array).cloned().unwrap_or_default() {
        number_tree(p, &kid, out, depth + 1);
    }
}

/// The /PageLabels number tree for the result's labels: a range wherever the style or prefix changes or the numbers
/// do not follow on.
fn encode_labels(ls: &[Option<Label>]) -> Obj {
    let mut nums = Vec::new();
    let mut prev: Option<&Label> = None;
    for (i, l) in ls.iter().enumerate() {
        let Some(l) = l else { continue };
        let follows = prev.is_some_and(|p| p.0 == l.0 && p.1 == l.1 && p.2 + 1 == l.2);
        if !follows {
            let mut d = Dict::new();
            if let Some(s) = &l.0 {
                put(&mut d, b"S", Obj::Name(s.clone()));
            }
            if !l.1.is_empty() {
                put(&mut d, b"P", Obj::Str(l.1.clone()));
            }
            if l.2 != 1 {
                put(&mut d, b"St", Obj::Int(l.2));
            }
            nums.push(Obj::Int(i as i64));
            nums.push(Obj::Dict(d));
        }
        prev = Some(l);
    }
    Obj::Dict(vec![(b"Nums".to_vec(), Obj::Array(nums))])
}

/// The line drawn at the top of a page that is not current yet.
fn stale_stamp(mb: [f64; 4], why: &str) -> Vec<u8> {
    let text: String = format!("Preview: {why} until the full build").chars().filter(|c| c.is_ascii() && *c != '(' && *c != ')' && *c != '\\').collect();
    format!("q 0.8 0 0 rg BT /FPStale 8 Tf {:.2} {:.2} Td ({text}) Tj ET Q\n", mb[0] + 36.0, mb[3] - 18.0).into_bytes()
}

/// The spliced PDF. `stale`: the result's pages that are not current, with why (pipeline::fast::shifted).
pub fn splice(main: &Pdf, preview: &Pdf, plan: &Plan, stale: &[((u32, u32), String)], placeholders: &HashSet<Vec<u8>>) -> Result<Spliced, String> {
    let pages = Pages { main: main.pages(), preview: preview.pages() };
    if pages.main.is_empty() {
        return Err("the full build's PDF has no pages".into());
    }
    let sources = layout(plan, pages.main.len() as u32);
    let mut out = main.clone();
    let mut problems = Vec::new();
    // the preview's pages in the result, and everything else of its page tree, which nothing may point at
    let used: HashSet<u32> = sources
        .iter()
        .flat_map(|s| match s {
            Source::Preview(j) => vec![*j],
            Source::Joined(parts) => parts.iter().filter_map(|(p, _, _, _)| if let Part::Preview(j) = p { Some(*j) } else { None }).collect(),
            Source::Main(_) => vec![],
        })
        .collect();
    let mut void: HashSet<u32> = preview.objs.iter().filter(|(_, o)| matches!(o.dict().and_then(|d| get(d, b"Type")), Some(Obj::Name(t)) if t == b"Pages")).map(|(n, _)| *n).collect();
    void.extend(pages.preview.iter().enumerate().filter(|(j, _)| !used.contains(&(*j as u32))).map(|(_, (n, _))| *n));
    let mut imp = Importer { from: preview, map: HashMap::new(), void };
    // the preview's whole pages first, so that links and destinations pointing at them are mapped
    let mut preview_page_ids: HashMap<u32, u32> = HashMap::new();
    for s in &sources {
        if let Source::Preview(j) = s {
            let (n, d) = &pages.preview[*j as usize];
            let m = out.add(Obj::Null);
            imp.map.insert(*n, m);
            preview_page_ids.insert(*j, m);
            let d = imp.dict(&mut out, d);
            out.objs.insert(m, Obj::Dict(d));
        }
    }
    // the result's pages
    let mut kids = Vec::new();
    let mut main_ids: HashMap<u32, u32> = HashMap::new(); // a full-build page -> the result's page showing it (whole)
    let mut joined_parts: Vec<(u32, Vec<Band4>)> = Vec::new();
    for s in &sources {
        let id = match s {
            Source::Main(i) => {
                let (n, d) = &pages.main[*i as usize];
                out.objs.insert(*n, Obj::Dict(d.clone())); // inherited attributes pushed down: the tree is flattened
                main_ids.insert(*n, *n);
                *n
            }
            Source::Preview(j) => preview_page_ids[j],
            Source::Joined(parts) => {
                let id = join(&mut out, main, &pages, &mut imp, parts);
                joined_parts.push((id, parts.clone()));
                id
            }
        };
        kids.push(id);
    }
    // the page tree: one node
    let tree = out.add(Obj::Null);
    for &k in &kids {
        if let Some(d) = out.objs.get_mut(&k).and_then(Obj::dict_mut) {
            put(d, b"Parent", Obj::Ref(tree, 0));
        }
    }
    out.objs.insert(tree, Obj::Dict(vec![(b"Type".to_vec(), Obj::Name(b"Pages".to_vec())), (b"Kids".to_vec(), Obj::Array(kids.iter().map(|&k| Obj::Ref(k, 0)).collect())), (b"Count".to_vec(), Obj::Int(kids.len() as i64))]));
    // named destinations: the full build's on pages that stay (or on their part of a joined page), the preview's on its
    let height_of = |page: &Dict| {
        let mb = media_box(page);
        (mb[3] - mb[1], mb[1])
    };
    let keep_main = |page: u32, y: Option<f64>| -> Option<(u32, f64)> {
        if let Some(&id) = main_ids.get(&page) {
            return Some((id, 0.0));
        }
        let i = pages.main.iter().position(|(n, _)| *n == page)? as u32;
        let (h, y0) = height_of(&pages.main[i as usize].1);
        joined_parts.iter().find_map(|(id, parts)| parts.iter().any(|(p, a, b, _)| *p == Part::Main(i) && y.is_none_or(|y| (h - (y - y0)) >= *a - 1.0 && (h - (y - y0)) < *b + 1.0)).then_some((*id, 0.0)))
    };
    let keep_preview = |page: u32, y: Option<f64>| -> Option<(u32, f64)> {
        let j = pages.preview.iter().position(|(n, _)| *n == page)? as u32;
        if let Some(&id) = preview_page_ids.get(&j) {
            return Some((id, 0.0));
        }
        let (h, y0) = height_of(&pages.preview[j as usize].1);
        joined_parts.iter().find_map(|(id, parts)| parts.iter().find(|(p, a, b, dy)| *p == Part::Preview(j) && y.is_none_or(|y| (h - (y - y0)) + dy >= *a - 1.0 && (h - (y - y0)) + dy < *b + 1.0)).map(|(_, _, _, dy)| (*id, *dy)))
    };
    let mut names: BTreeMap<Vec<u8>, Obj> = BTreeMap::new();
    for (k, v) in dests(main) {
        if let Some((page, y)) = dest_target(main, &v) {
            if let Some((id, dy)) = keep_main(page, y) {
                names.insert(k, retarget(main.resolve(&v), id, dy));
            }
        }
    }
    let mut duplicates = Vec::new();
    for (k, v) in dests(preview).into_iter().filter(|(k, _)| !placeholders.contains(k)) {
        if let Some((page, y)) = dest_target(preview, &v) {
            // a destination without a place (LuaTeX's stand-in for a name it did not have) never replaces a real one
            if y.is_none() && names.contains_key(&k) {
                continue;
            }
            if let Some((id, dy)) = keep_preview(page, y) {
                if names.insert(k.clone(), retarget(preview.resolve(&v), id, dy)).is_some() {
                    duplicates.push(String::from_utf8_lossy(&k).to_string());
                }
            }
        }
    }
    // a destination on a preview page that is not in the result (a region's last page, when the region did not end as
    // the full build did) points at the full build's page that stayed there (stale): near, rather than nowhere
    for r in &plan.replace {
        let (Some((pn, _)), Some((mn, _))) = (pages.preview.get(r.preview.1 as usize), pages.main.get(r.main.1 as usize)) else { continue };
        let Some(&id) = main_ids.get(mn).filter(|_| !used.contains(&r.preview.1)) else { continue };
        for (k, v) in dests(preview).into_iter().filter(|(k, _)| !placeholders.contains(k)) {
            if !names.contains_key(&k) && dest_target(preview, &v).is_some_and(|(page, _)| page == *pn) {
                names.insert(k, retarget(preview.resolve(&v), id, 0.0));
            }
        }
    }
    if !duplicates.is_empty() {
        problems.push(format!("{} destinations are both in the preview and on the full build's pages that stay (the preview's are used): {}", duplicates.len(), duplicates.iter().take(8).cloned().collect::<Vec<_>>().join(", ")));
    }
    let tree_obj = Obj::Dict(vec![(b"Names".to_vec(), Obj::Array(names.iter().flat_map(|(k, v)| [Obj::Str(k.clone()), v.clone()]).collect()))]);
    let dests_id = out.add(tree_obj);
    let root = out.root_id().ok_or("the full build's PDF has no catalog")?;
    let mut cat = out.get(root).and_then(Obj::dict).cloned().ok_or("the catalog is not a dictionary")?;
    let mut name_dict = get(&cat, b"Names").map(|n| main.resolve(n)).and_then(Obj::dict).cloned().unwrap_or_default();
    put(&mut name_dict, b"Dests", Obj::Ref(dests_id, 0));
    put(&mut cat, b"Names", Obj::Dict(name_dict));
    put(&mut cat, b"Pages", Obj::Ref(tree, 0));
    // page labels: each page's own
    let (ml, pl) = (labels(main, pages.main.len()), labels(preview, pages.preview.len()));
    let ls: Vec<Option<Label>> = sources
        .iter()
        .map(|s| match s {
            Source::Main(i) => ml[*i as usize].clone(),
            Source::Preview(j) => pl[*j as usize].clone(),
            // the page number is printed at the bottom: the label of the part at the bottom
            Source::Joined(parts) => match parts.last().map(|p| p.0) {
                Some(Part::Main(i)) => ml[i as usize].clone(),
                Some(Part::Preview(j)) => pl[j as usize].clone(),
                None => None,
            },
        })
        .collect();
    if main.catalog().and_then(|c| get(c, b"PageLabels")).is_some() {
        put(&mut cat, b"PageLabels", encode_labels(&ls));
    }
    out.objs.insert(root, Obj::Dict(cat));
    // links: a named destination must be in the tree; a destination given as a page must be a page of the result
    let page_set: HashSet<u32> = kids.iter().copied().collect();
    let mut dangling = BTreeMap::new();
    for &k in &kids {
        let annots = out.get(k).and_then(Obj::dict).and_then(|d| get(d, b"Annots")).map(|a| out.resolve(a).clone());
        for a in annots.and_then(|a| a.array().cloned()).unwrap_or_default() {
            let Some(ad) = out.resolve(&a).dict() else { continue };
            let d = get(ad, b"Dest").or_else(|| get(ad, b"A").map(|x| out.resolve(x)).and_then(Obj::dict).filter(|x| get(x, b"S").and_then(Obj::name) == Some(b"GoTo")).and_then(|x| get(x, b"D")));
            match d.map(|d| out.resolve(d)) {
                Some(Obj::Str(n)) | Some(Obj::Name(n)) if !names.contains_key(n) => *dangling.entry(String::from_utf8_lossy(n).to_string()).or_insert(0) += 1,
                Some(Obj::Array(x)) if x.first().and_then(Obj::reference).is_some_and(|p| !page_set.contains(&p)) => *dangling.entry("(a page that is gone)".to_string()).or_insert(0) += 1,
                _ => {}
            }
        }
    }
    if !dangling.is_empty() {
        problems.push(format!("links to destinations the PDF does not have: {}", dangling.iter().map(|(k, n)| format!("{k} ({n})")).collect::<Vec<_>>().join(", ")));
    }
    // stale pages: a line at the top
    for ((a, b), why) in stale {
        for k in *a..(*b).min(kids.len() as u32) {
            stamp(&mut out, kids[k as usize], why);
        }
    }
    collect_garbage(&mut out);
    Ok(Spliced { pdf: out.write(), pages: sources, stale: stale.to_vec(), problems })
}

/// A destination pointing at the result's page `id`, moved down by `dy` (bp).
fn retarget(d: &Obj, id: u32, dy: f64) -> Obj {
    match d {
        Obj::Array(a) => {
            let mut a = a.clone();
            if !a.is_empty() {
                a[0] = Obj::Ref(id, 0);
            }
            // the y of /XYZ x y z, /FitH y, /FitBH y
            let k = match a.get(1).and_then(Obj::name) {
                Some(b"XYZ") => 3,
                Some(b"FitH") | Some(b"FitBH") => 2,
                _ => 0,
            };
            if let Some(y) = a.get(k).filter(|_| k > 0 && dy != 0.0).and_then(Obj::num) {
                a[k] = real(y - dy);
            }
            Obj::Array(a)
        }
        Obj::Dict(x) => {
            let mut x = x.clone();
            if let Some(inner) = get(&x, b"D").cloned() {
                put(&mut x, b"D", retarget(&inner, id, dy));
            }
            Obj::Dict(x)
        }
        o => o.clone(),
    }
}

/// A joined page: each part's page drawn as a form XObject, clipped to its band; the links of each band.
fn join(out: &mut Pdf, main: &Pdf, pages: &Pages, imp: &mut Importer, parts: &[(Part, f64, f64, f64)]) -> u32 {
    let first = match parts[0].0 {
        Part::Main(i) => pages.main[i as usize].1.clone(),
        Part::Preview(j) => pages.preview[j as usize].1.clone(),
    };
    let mb = media_box(&first);
    let (w, h) = (mb[2] - mb[0], mb[3] - mb[1]);
    let mut xobjects = Dict::new();
    let mut ops = Vec::new();
    let mut annots = Vec::new();
    for (k, (part, a, b, dy)) in parts.iter().enumerate() {
        let (src, page, is_main) = match part {
            Part::Main(i) => (main, &pages.main[*i as usize].1, true),
            Part::Preview(j) => (imp.from, &pages.preview[*j as usize].1, false),
        };
        let (data, filters) = content(src, page);
        let mut fd: Dict = vec![(b"Type".to_vec(), Obj::Name(b"XObject".to_vec())), (b"Subtype".to_vec(), Obj::Name(b"Form".to_vec())), (b"BBox".to_vec(), Obj::Array(mb.iter().map(|&x| real(x)).collect()))];
        fd.extend(filters);
        if let Some(r) = get(page, b"Resources") {
            let r = if is_main { r.clone() } else { imp.obj(out, r) };
            put(&mut fd, b"Resources", r);
        }
        if let Some(g) = get(page, b"Group") {
            let g = if is_main { g.clone() } else { imp.obj(out, g) };
            put(&mut fd, b"Group", g);
        }
        let form = out.add(Obj::Stream(fd, data));
        let name = format!("FP{k}");
        xobjects.push((name.as_bytes().to_vec(), Obj::Ref(form, 0)));
        // the band: from a to b below the page's top
        let (y_top, y_bottom) = (mb[3] - a.max(0.0), mb[3] - b.min(h));
        ops.extend(format!("q {:.4} {:.4} {:.4} {:.4} re W n 1 0 0 1 0 {:.4} cm /{name} Do Q\n", mb[0], y_bottom, w, (y_top - y_bottom).max(0.0), -dy).into_bytes());
        for an in get(page, b"Annots").map(|x| src.resolve(x)).and_then(Obj::array).cloned().unwrap_or_default() {
            let Some(y) = annot_y(src, &an, h, mb[1]) else { continue };
            if y + dy >= *a && y + dy < *b {
                let an = if is_main { an.clone() } else { imp.obj(out, &an) };
                annots.push(if *dy == 0.0 { an } else { moved(out, an, *dy) });
            }
        }
    }
    let contents = out.add(Obj::Stream(vec![], ops));
    let mut d: Dict = vec![
        (b"Type".to_vec(), Obj::Name(b"Page".to_vec())),
        (b"MediaBox".to_vec(), Obj::Array(mb.iter().map(|&x| real(x)).collect())),
        (b"Resources".to_vec(), Obj::Dict(vec![(b"XObject".to_vec(), Obj::Dict(xobjects))])),
        (b"Contents".to_vec(), Obj::Ref(contents, 0)),
    ];
    if !annots.is_empty() {
        d.push((b"Annots".to_vec(), Obj::Array(annots)));
    }
    let id = out.add(Obj::Dict(d.clone()));
    // the links' /P is this page
    for a in d.iter().find(|(k, _)| k == b"Annots").and_then(|(_, v)| v.array()).cloned().unwrap_or_default() {
        if let Some(n) = a.reference() {
            if let Some(ad) = out.objs.get_mut(&n).and_then(Obj::dict_mut) {
                if get(ad, b"P").is_some() {
                    put(ad, b"P", Obj::Ref(id, 0));
                }
            }
        }
    }
    id
}

/// A link moved down by `dy` (bp): a copy with its rectangle moved.
fn moved(out: &mut Pdf, an: Obj, dy: f64) -> Obj {
    let (n, mut d) = match &an {
        Obj::Ref(n, _) => (Some(*n), out.get(*n).and_then(Obj::dict).cloned().unwrap_or_default()),
        Obj::Dict(d) => (None, d.clone()),
        _ => return an,
    };
    if let Some(r) = get(&d, b"Rect").and_then(Obj::array).cloned() {
        let r: Vec<Obj> = r.iter().enumerate().map(|(k, x)| if k % 2 == 1 { x.num().map_or(x.clone(), |v| real(v - dy)) } else { x.clone() }).collect();
        put(&mut d, b"Rect", Obj::Array(r));
    }
    match n {
        Some(n) => {
            out.objs.insert(n, Obj::Dict(d));
            an
        }
        None => Obj::Dict(d),
    }
}

/// The stale line added to a page: a content stream after its own, and Helvetica in its resources.
fn stamp(out: &mut Pdf, page: u32, why: &str) {
    let Some(d) = out.get(page).and_then(Obj::dict).cloned() else { return };
    let mb = media_box(&d);
    let s = out.add(Obj::Stream(vec![], stale_stamp(mb, why)));
    let mut contents = match get(&d, b"Contents").map(|c| out.resolve(c).clone()) {
        Some(Obj::Array(a)) => a,
        Some(Obj::Stream(..)) => vec![get(&d, b"Contents").cloned().unwrap_or(Obj::Null)],
        _ => vec![],
    };
    // the page's own content in a group of its own, so that its graphics state does not reach the line
    let open = out.add(Obj::Stream(vec![], b"q\n".to_vec()));
    let close = out.add(Obj::Stream(vec![], b"\nQ\n".to_vec()));
    contents.insert(0, Obj::Ref(open, 0));
    contents.push(Obj::Ref(close, 0));
    contents.push(Obj::Ref(s, 0));
    let mut res = get(&d, b"Resources").map(|r| out.resolve(r).clone()).and_then(|r| r.dict().cloned()).unwrap_or_default();
    let mut fonts = get(&res, b"Font").map(|f| out.resolve(f).clone()).and_then(|f| f.dict().cloned()).unwrap_or_default();
    put(&mut fonts, b"FPStale", Obj::Dict(vec![(b"Type".to_vec(), Obj::Name(b"Font".to_vec())), (b"Subtype".to_vec(), Obj::Name(b"Type1".to_vec())), (b"BaseFont".to_vec(), Obj::Name(b"Helvetica".to_vec()))]));
    put(&mut res, b"Font", Obj::Dict(fonts));
    let mut d = d;
    put(&mut d, b"Contents", Obj::Array(contents));
    put(&mut d, b"Resources", Obj::Dict(res));
    out.objs.insert(page, Obj::Dict(d));
}

/// Drops the objects that the catalog and the document information no longer reach.
fn collect_garbage(p: &mut Pdf) {
    let mut seen: HashSet<u32> = HashSet::new();
    let mut todo: Vec<u32> = [b"Root".as_slice(), b"Info"].iter().filter_map(|k| get(&p.trailer, k)).filter_map(Obj::reference).collect();
    fn refs(o: &Obj, out: &mut Vec<u32>) {
        match o {
            Obj::Ref(n, _) => out.push(*n),
            Obj::Array(a) => a.iter().for_each(|x| refs(x, out)),
            Obj::Dict(d) | Obj::Stream(d, _) => d.iter().for_each(|(_, v)| refs(v, out)),
            _ => {}
        }
    }
    while let Some(n) = todo.pop() {
        if seen.insert(n) {
            if let Some(o) = p.objs.get(&n) {
                refs(o, &mut todo);
            }
        }
    }
    p.objs.retain(|n, _| seen.contains(n));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(main: (u32, u32), preview: (u32, u32), top: Option<f64>, bottom: Option<f64>) -> Replace {
        Replace { main, preview, top, bottom, shift: 0.0, tail: false, crop: None, top_page: None, foot: None }
    }

    #[test]
    fn layouts() {
        let plan = Plan { replace: vec![rep((2, 4), (0, 3), Some(300.0), Some(500.0))], ..Default::default() };
        let l = layout(&plan, 6);
        assert_eq!(l.len(), 7);
        assert_eq!(&l[..2], [Source::Main(0), Source::Main(1)]);
        assert_eq!(l[2], Source::Joined(vec![(Part::Main(2), 0.0, 300.0, 0.0), (Part::Preview(0), 300.0, f64::INFINITY, 0.0)]));
        assert_eq!(l[3], Source::Preview(1));
        assert_eq!(l[4], Source::Joined(vec![(Part::Preview(2), 0.0, 500.0, 0.0), (Part::Main(3), 500.0, f64::INFINITY, 0.0)]));
        assert_eq!(&l[5..], [Source::Main(4), Source::Main(5)]);
        // a region within one page: three bands
        let plan = Plan { replace: vec![rep((1, 2), (0, 1), Some(100.0), Some(400.0))], ..Default::default() };
        assert_eq!(layout(&plan, 3)[1], Source::Joined(vec![(Part::Main(1), 0.0, 100.0, 0.0), (Part::Preview(0), 100.0, 400.0, 0.0), (Part::Main(1), 400.0, f64::INFINITY, 0.0)]));
        // two regions sharing a page: the one before above its end, the full build's between, the next below its start
        let plan = Plan { replace: vec![rep((1, 3), (0, 2), None, Some(300.0)), rep((2, 4), (2, 4), Some(500.0), None)], ..Default::default() };
        let l = layout(&plan, 5);
        assert_eq!(l[2], Source::Joined(vec![(Part::Preview(1), 0.0, 300.0, 0.0), (Part::Main(2), 300.0, 500.0, 0.0), (Part::Preview(2), 500.0, f64::INFINITY, 0.0)]));
        assert_eq!(&l[3..], [Source::Preview(3), Source::Main(4)]);
        // a kept tail: the full build's next page shows only what lies below the region's end
        let mut r = rep((1, 2), (0, 2), None, None);
        r.tail = true;
        r.crop = Some((250.0, 700.0));
        let l = layout(&Plan { replace: vec![r], ..Default::default() }, 4);
        assert_eq!(l, [Source::Main(0), Source::Preview(0), Source::Preview(1), Source::Joined(vec![(Part::Main(2), 250.0, 700.0, 0.0)]), Source::Main(3)]);
        // two regions, the bibliography at the end
        let plan = Plan { replace: vec![rep((8, 10), (3, 6), None, None), rep((1, 2), (0, 1), None, None)], ..Default::default() };
        let l = layout(&plan, 10);
        assert_eq!(l, [Source::Main(0), Source::Preview(0), Source::Main(2), Source::Main(3), Source::Main(4), Source::Main(5), Source::Main(6), Source::Main(7), Source::Preview(3), Source::Preview(4), Source::Preview(5)]);
    }

    #[test]
    fn page_labels() {
        let ls = vec![Some((Some(b"r".to_vec()), vec![], 1)), Some((Some(b"r".to_vec()), vec![], 2)), Some((Some(b"D".to_vec()), vec![], 1)), Some((Some(b"D".to_vec()), vec![], 2)), Some((Some(b"D".to_vec()), vec![], 2))];
        let Obj::Dict(d) = encode_labels(&ls) else { panic!() };
        let nums = get(&d, b"Nums").unwrap().array().unwrap();
        assert_eq!(nums.len(), 6, "{nums:?}"); // ranges at 0, 2 and 4 (a repeated 2)
        assert_eq!(nums[4], Obj::Int(4));
    }

    /// A small PDF of `n` pages, each with a named destination `p<k>` and a link to the next page's.
    pub fn sample(n: u32, tag: &str) -> Pdf {
        let mut p = Pdf::default();
        let mut kids = Vec::new();
        let mut names = Vec::new();
        for k in 0..n {
            let id = 10 + k * 3;
            let content = id + 1;
            let link = id + 2;
            p.objs.insert(content, Obj::Stream(vec![], format!("BT /F1 12 Tf 72 700 Td ({tag} page {k}) Tj ET").into_bytes()));
            p.objs.insert(link, Obj::Dict(vec![(b"Type".to_vec(), Obj::Name(b"Annot".to_vec())), (b"Subtype".to_vec(), Obj::Name(b"Link".to_vec())), (b"Rect".to_vec(), Obj::Array(vec![Obj::Int(72), Obj::Int(100), Obj::Int(200), Obj::Int(112)])), (b"A".to_vec(), Obj::Dict(vec![(b"S".to_vec(), Obj::Name(b"GoTo".to_vec())), (b"D".to_vec(), Obj::Str(format!("p{}", (k + 1) % n).into_bytes()))]))]));
            p.objs.insert(id, Obj::Dict(vec![(b"Type".to_vec(), Obj::Name(b"Page".to_vec())), (b"Parent".to_vec(), Obj::Ref(2, 0)), (b"Contents".to_vec(), Obj::Ref(content, 0)), (b"Annots".to_vec(), Obj::Array(vec![Obj::Ref(link, 0)]))]));
            kids.push(Obj::Ref(id, 0));
            names.push(Obj::Str(format!("p{k}").into_bytes()));
            names.push(Obj::Array(vec![Obj::Ref(id, 0), Obj::Name(b"XYZ".to_vec()), Obj::Int(72), Obj::Int(720), Obj::Null]));
        }
        p.objs.insert(1, Obj::Dict(vec![(b"Type".to_vec(), Obj::Name(b"Catalog".to_vec())), (b"Pages".to_vec(), Obj::Ref(2, 0)), (b"Names".to_vec(), Obj::Dict(vec![(b"Dests".to_vec(), Obj::Ref(3, 0))]))]));
        p.objs.insert(2, Obj::Dict(vec![(b"Type".to_vec(), Obj::Name(b"Pages".to_vec())), (b"Kids".to_vec(), Obj::Array(kids)), (b"Count".to_vec(), Obj::Int(n as i64)), (b"MediaBox".to_vec(), Obj::Array(vec![Obj::Int(0), Obj::Int(0), Obj::Int(595), Obj::Int(842)]))]));
        p.objs.insert(3, Obj::Dict(vec![(b"Names".to_vec(), Obj::Array(names))]));
        p.trailer = vec![(b"Root".to_vec(), Obj::Ref(1, 0))];
        p
    }

    #[test]
    fn splices() {
        let main = sample(5, "main");
        let preview = sample(2, "preview");
        // pages 1..3 of the full build give way to the preview's two pages, the first joined at 400 bp
        let plan = Plan { replace: vec![rep((1, 3), (0, 2), Some(400.0), None)], ..Default::default() };
        let s = splice(&main, &preview, &plan, &[((3, 4), "page numbers updating".into())], &HashSet::new()).unwrap();
        let out = Pdf::read(&s.pdf).unwrap();
        let pages = out.pages();
        assert_eq!(pages.len(), 5);
        // names: p0 (main page 0), p1 (the joined page: main's p1 is at y 720, above the cut, kept), p3 and p4 from the
        // full build; the preview's p0 lies above the cut on the joined page (dropped), its p1 on its own page
        let names: Vec<(String, u32)> = dests(&out).into_iter().map(|(k, v)| (String::from_utf8(k).unwrap(), dest_target(&out, &v).unwrap().0)).collect();
        let at = |n: &str| names.iter().find(|(k, _)| k == n).map(|(_, p)| pages.iter().position(|(id, _)| id == p).unwrap());
        assert_eq!(at("p0"), Some(0));
        assert_eq!(at("p1"), Some(2), "the preview's p1 replaces the full build's (which was on a replaced page)");
        assert_eq!(at("p3"), Some(3));
        assert!(at("p2").is_none(), "the full build's page 2 is gone");
        assert_eq!(s.problems.len(), 1, "{:?}", s.problems); // p1 in both
        assert!(s.problems[0].contains("p1"));
        // the full build's page 1 links to p2, which was only on its page 2, now replaced: a dangling link
        let plan2 = Plan { replace: vec![rep((2, 3), (0, 1), None, None)], ..Default::default() };
        let s2 = splice(&main, &preview, &plan2, &[], &HashSet::new()).unwrap();
        assert!(s2.problems.iter().any(|p| p.contains("links to destinations the PDF does not have: p2 (1)")), "{:?}", s2.problems);
        // the joined page draws both parts, each clipped
        let joined = &pages[1].1;
        let xo = get(get(joined, b"Resources").unwrap().dict().unwrap(), b"XObject").unwrap().dict().unwrap();
        assert_eq!(xo.len(), 2);
        // the stale page has its line
        let last = &pages[3].1;
        assert!(get(last, b"Contents").unwrap().array().unwrap().len() >= 3);
        // nothing unreachable is left
        assert!(out.objs.len() < main.objs.len() + preview.objs.len() + 20);
    }
}
