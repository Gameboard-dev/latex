//! The compiled PDF beside the chapter notes, and the two scrolled together. The PDF opens in a pane on the right when a
//! build succeeds or a chapter note is opened (focus stays in the note). With sync scrolling on, scrolling a chapter
//! note's editor scrolls the PDF to the same place, and scrolling the PDF with the pointer over it scrolls the note: the
//! note line at the top of the editor goes to its .tex line (convert::LineMap, from the note's latest conversion) and
//! to the place SyncTeX says it was typeset (synctex.rs, read once per build), interpolated through the part (a
//! paragraph, a table ...) by how far it is scrolled; the PDF's place goes back the same way.
//!
//! Only notes linked to the PDF take part: a note of the notes folder (not one left out) whose .tex is in the SyncTeX
//! file's inputs, and only the pane showing `<output>/<main>.pdf`. The pane a scroll comes from leads (an editor's own
//! scroller, so a note beside the chapter does nothing); a pane scrolled by the plugin ignores its own scroll events for
//! a moment (`ECHO_MS`), so the two never chase each other. A place in a chapter whose note is not open does nothing:
//! no note is opened.
//!
//! Obsidian's API: <https://docs.obsidian.md/Reference/TypeScript+API/Workspace/getLeavesOfType>,
//! <https://docs.obsidian.md/Reference/TypeScript+API/Workspace/on('file-open')>,
//! <https://docs.obsidian.md/Reference/TypeScript+API/Component/registerDomEvent>,
//! <https://docs.obsidian.md/Reference/TypeScript+API/WorkspaceLeaf/openFile> (`eState: {subpath: "#page=N"}`), the
//! editor's CodeMirror view, `MarkdownView.editor.cm` (<https://codemirror.net/docs/ref/#view.EditorView>: documentTop,
//! lineBlockAtHeight, lineBlockAt, scrollDOM). The PDF view's pdf.js viewer is not in Obsidian's API: it is looked for
//! by its shape (`pdfjs`), at `view.viewer.child.pdfViewer.pdfViewer` in Obsidian 1.x, pdf.js's PDFViewer
//! (<https://github.com/mozilla/pdf.js/blob/master/web/pdf_viewer.js>: scrollPageIntoView, getPageView, container,
//! pagesCount, _location). Without it the PDF only jumps to the page (`setEphemeralState`, or `openFile` with
//! `#page=N`), and the PDF does not lead. The SyncTeX file is unpacked here (pdf::gunzip), so it works in a browser too.

use std::collections::HashMap;
use std::rc::Rc;

use js_sys::{Array, Uint8Array};
use wasm_bindgen::prelude::*;

use crate::app::{with, App};
use crate::convert::{md_to_tex_mapped, Index, LineMap};
use crate::files::{self, node, vault_path, Place};
use crate::js::{self, call, f1, get, obj, set, str};
use crate::pipeline;
use crate::synctex::{self, Pos, Synctex};

/// Where the two panes meet, as a share of each pane's height from its top (user, 2026-10-27): the line at this height
/// in the note is put at this height in the PDF, and back. A share of each pane's own height, rather than a distance
/// in points or pixels, keeps the panes in step whatever the PDF's zoom, Obsidian's zoom, the font size and the tab
/// width (CodeMirror's line heights already follow the last three).
const ANCHOR: f64 = 0.2;

/// pdf.js's CSS pixels per PDF point at zoom 1 (PixelsPerInch.PDF_TO_CSS_UNITS: 96 / 72).
const CSS_PER_PT: f64 = 96.0 / 72.0;

/// The anchor's distance below the top of the PDF pane, in PDF points at the viewer's zoom.
fn pdf_anchor_pt(viewer: &JsValue) -> f64 {
    let scale = num(viewer, "currentScale").max(0.05);
    ANCHOR * num(&get(viewer, "container"), "clientHeight") / (scale * CSS_PER_PT)
}

/// The PDF pane scrolled so that its pages are centred across it (zooming in must not leave the right of the page cut
/// off; pdf.js's XYZ destination puts the page's left edge at the left).
fn centre_pdf(viewer: &JsValue) {
    let c = get(viewer, "container");
    let spare = num(&c, "scrollWidth") - num(&c, "clientWidth");
    if spare > 0.0 {
        set(&c, "scrollLeft", spare / 2.0);
    }
}

/// How long a pane's scroll events are taken as the echo of the plugin's own scrolling, in ms.
const ECHO_MS: f64 = 150.0;
/// A page's height when the viewer cannot say (A4, in bp): only for interpolating across a page break.
const PAGE_H: f32 = 842.0;
/// How often, and how far apart, the plugin looks for a newly opened PDF's pages before placing it (in ms).
const READY_TRIES: u32 = 30;
const READY_MS: f64 = 150.0;

/// Which pane scrolled last and leads the next frame.
enum Lead {
    Note(JsValue),
    Pdf,
}

/// What sync scrolling knows and is doing.
pub struct Sync {
    /// The last build's SyncTeX file (None: none read, or none to read).
    pub index: Option<Rc<Synctex>>,
    /// Each chapter note's line map, from its latest conversion (or converted in memory when first needed).
    pub maps: HashMap<String, Rc<LineMap>>,
    /// The pointer is over the PDF pane: the PDF leads.
    pointer_pdf: bool,
    /// Until when each pane's scroll events are the echo of the plugin's own scroll (performance.now()).
    quiet_pdf: f64,
    quiet_note: f64,
    lead: Option<Lead>,
    /// A frame is asked for; scroll events until then only change who leads.
    frame: bool,
    /// The page the PDF was last sent to, when it can only jump to pages.
    page: u32,
    /// The glossary note's entries (the line of each # heading, and its key), for the note's text they were read from;
    /// and the entry the PDF was last sent to.
    gloss: Option<(String, Rc<Vec<(u32, String)>>)>,
    gloss_at: String,
    /// The property that marks an element whose events this load of the plugin listens to (a new one each load: an
    /// element keeps its properties when the plugin is turned off and on).
    mark: String,
    /// When this plugin last wrote the shown PDF itself (performance.now()): its own copy is placed by `built`.
    published: f64,
}

impl Sync {
    pub fn new() -> Sync {
        Sync { index: None, maps: HashMap::new(), pointer_pdf: false, quiet_pdf: 0.0, quiet_note: 0.0, lead: None, frame: false, page: 0, gloss: None, gloss_at: String::new(), mark: format!("__latexSync{}", js::now() as u64), published: f64::NEG_INFINITY }
    }
}

/// How long after this plugin's own write of the shown PDF a change to it is taken as that write (ms).
const OWN_WRITE_MS: f64 = 5000.0;

fn num(v: &JsValue, k: &str) -> f64 {
    get(v, k).as_f64().unwrap_or(0.0)
}

impl App {
    fn workspace(&self) -> JsValue {
        get(&get(&self.plugin, "app"), "workspace")
    }

    /// The PDF the pane shows (`<main>.shown.pdf`, written by `publish`), as a vault path; None when the output
    /// folder is outside the vault (Obsidian cannot show it).
    pub(crate) fn pdf_path(&self) -> Option<String> {
        match self.out().join(&format!("{}.shown.pdf", self.settings.main())) {
            Place::Vault(p) => Some(p),
            Place::Disk(_) => None,
        }
    }

    /// A note linked to the PDF, from its vault path: (note name, .tex name). A note of the notes folder, not left out,
    /// whose .tex is in the SyncTeX file's inputs; with no SyncTeX file read yet, `strict` false accepts any note of
    /// the folder.
    pub(crate) fn chapter(&self, path: &str, strict: bool) -> Option<(String, String)> {
        let name = self.note_name(path)?;
        let tex = pipeline::tex_name(&name);
        match &self.sync.index {
            Some(st) => _ = st.file(&tex)?,
            None if strict => return None,
            None => {}
        }
        Some((name, tex))
    }

    /// A note's line map: from its latest conversion, or converted now in memory (nothing is written).
    fn map_for(&mut self, name: &str) -> Option<Rc<LineMap>> {
        if let Some(m) = self.sync.maps.get(name) {
            return Some(m.clone());
        }
        let o = self.options();
        let idx = Index::new(self.notes.iter().map(|(n, t)| (n.as_str(), t.as_str())), &o);
        let (_, _, map) = md_to_tex_mapped(name, self.notes.get(name)?, &idx, &o).ok()?;
        let map = Rc::new(map);
        self.sync.maps.insert(name.to_string(), map.clone());
        Some(map)
    }

    /// For a scrolled editor's note: the SyncTeX index, the note's .tex and its line map.
    fn locate(&mut self, path: &str) -> Option<(Rc<Synctex>, String, Rc<LineMap>)> {
        let (name, tex) = self.chapter(path, true)?;
        Some((self.sync.index.clone()?, tex, self.map_for(&name)?))
    }

    /// The glossary note's entries, if `path` is the glossary note: each `#` heading's line (from 1) and the key its
    /// `\term` has in the converted glossary (headings and entries come in the same order).
    fn glossary_entries(&mut self, path: &str) -> Option<Rc<Vec<(u32, String)>>> {
        let name = self.note_name(path)?;
        if name.trim_end_matches(".md") != self.glossary_note() {
            return None;
        }
        let md = self.notes.get(&name)?.clone();
        if let Some((text, e)) = &self.sync.gloss {
            if *text == md {
                return Some(e.clone());
            }
        }
        let o = self.options();
        let idx = Index::new(self.notes.iter().map(|(n, t)| (n.as_str(), t.as_str())), &o);
        let (tex, _, _) = md_to_tex_mapped(&name, &md, &idx, &o).ok()?;
        let cmd = format!("\\{}", o.term.trim().trim_start_matches('\\'));
        let keys = tex.lines().filter_map(|l| {
            let rest = l.strip_prefix(&cmd)?;
            let rest = if rest.starts_with('[') { &rest[rest.find(']')? + 1..] } else { rest };
            Some(rest.strip_prefix('{')?.split('}').next()?.to_string())
        });
        let heads = md.lines().enumerate().filter(|(_, l)| l.starts_with("# ")).map(|(i, _)| i as u32 + 1);
        let e = Rc::new(heads.zip(keys).collect::<Vec<_>>());
        self.sync.gloss = Some((md, e.clone()));
        Some(e)
    }

    /// For a place in the PDF: the chapter note's vault path, and the part of it shown there (first and last line, how
    /// far through).
    fn locate_pdf(&mut self, p: Pos, page_h: f32) -> Option<(String, u32, u32, f32)> {
        let st = self.sync.index.clone()?;
        let (file, t) = st.at(p)?;
        let name = self.notes.keys().find(|n| pipeline::tex_name(n) == file).cloned()?;
        let map = self.map_for(&name)?;
        let (first, last, frac) = synctex::reverse(&st, file, &map, t, p, page_h)?;
        let folder = vault_path(&self.settings.notes_folder);
        Some((if folder.is_empty() { name } else { format!("{folder}/{name}") }, first, last, frac))
    }
}

// ----- the panes

fn leaves(workspace: &JsValue, kind: &str) -> Vec<JsValue> {
    Array::from(&call!(workspace, getLeavesOfType, kind)).iter().collect()
}

fn path_of(leaf: &JsValue) -> String {
    str(&get(&get(&get(leaf, "view"), "file"), "path"))
}

/// The pane showing the compiled PDF, if one does: never another PDF.
fn pdf_leaf() -> Option<JsValue> {
    let (workspace, path) = with(|a| Some((a.workspace(), a.pdf_path()?))).flatten()?;
    leaves(&workspace, "pdf").into_iter().find(|l| path_of(l) == path)
}

/// The pane showing a note that was active most recently (WorkspaceLeaf.activeTime, undocumented; the first without it).
fn note_leaf(workspace: &JsValue, path: &str) -> Option<JsValue> {
    leaves(workspace, "markdown").into_iter().filter(|l| path_of(l) == path).max_by(|a, b| num(a, "activeTime").total_cmp(&num(b, "activeTime")))
}

/// The pdf.js viewer in Obsidian's PDF view, found by its shape: an object with scrollPageIntoView and a container.
fn pdfjs(view: &JsValue) -> Option<JsValue> {
    let viewer = get(view, "viewer");
    let child = get(&viewer, "child");
    [get(&get(&child, "pdfViewer"), "pdfViewer"), get(&child, "pdfViewer"), get(&viewer, "pdfViewer"), get(view, "pdfViewer")]
        .into_iter()
        .find(|v| get(v, "scrollPageIntoView").is_function() && get(v, "container").is_object())
}

/// A page's view box in PDF points, [x0, y0, x1, y1] (pdf.js's PDFPageView.viewport.viewBox).
fn view_box(viewer: &JsValue, page: u32) -> Option<[f64; 4]> {
    let b = Array::from(&get(&get(&call!(viewer, getPageView, page.saturating_sub(1)), "viewport"), "viewBox"));
    let v: Vec<f64> = b.iter().filter_map(|x| x.as_f64()).collect();
    (v.len() == 4 && v[3] > v[1]).then(|| [v[0], v[1], v[2], v[3]])
}

fn page_height(viewer: Option<&JsValue>) -> f32 {
    viewer.and_then(|v| view_box(v, 1)).map_or(PAGE_H, |b| (b[3] - b[1]) as f32)
}

/// Where the top of the PDF pane is: the page and how far down it (bp), from the pages' positions in the scrolled
/// container, or pdf.js's own record of it (`_location`, its top in PDF space, y upwards).
fn location(viewer: &JsValue) -> Option<(u32, f32)> {
    let container = get(viewer, "container");
    // the anchor's height in the pane, not its top
    let top = num(&container, "scrollTop") + ANCHOR * num(&container, "clientHeight");
    let n = num(viewer, "pagesCount") as u32;
    for i in 0..n {
        let pv = call!(viewer, getPageView, i);
        let div = get(&pv, "div");
        let (y0, h) = (num(&div, "offsetTop") + num(&div, "clientTop"), num(&div, "clientHeight"));
        if !div.is_object() || h <= 0.0 {
            break;
        }
        if top < y0 + h || i + 1 == n {
            let point = Array::from(&call!(get(&pv, "viewport"), convertToPdfPoint, 0.0, top - y0));
            let (y, b) = (point.get(1).as_f64()?, view_box(viewer, i + 1)?);
            return Some((i + 1, (b[3] - y) as f32));
        }
    }
    let loc = get(viewer, "_location");
    let page = num(&loc, "pageNumber") as u32;
    let b = view_box(viewer, page)?;
    (page > 0).then(|| (page, (b[3] - get(&loc, "top").as_f64().unwrap_or(b[3])) as f32))
}

/// An editor's CodeMirror view and its scroller.
fn editor(leaf: &JsValue) -> Option<(JsValue, JsValue)> {
    let cm = get(&get(&get(leaf, "view"), "editor"), "cm");
    let scroller = get(&cm, "scrollDOM");
    (cm.is_object() && scroller.is_object() && get(&cm, "lineBlockAtHeight").is_function()).then_some((cm, scroller))
}

/// How far down the document the top of the editor is: CodeMirror's heights, from the top of the document (the
/// properties and the inline title above it are in the scroller too).
fn top_height(cm: &JsValue, scroller: &JsValue) -> f64 {
    num(&call!(scroller, getBoundingClientRect), "top") - num(cm, "documentTop")
}

/// The top and bottom of note lines first to last in the editor (document heights).
fn span(cm: &JsValue, first: u32, last: u32) -> Option<(f64, f64)> {
    let doc = get(&get(cm, "state"), "doc");
    let n = num(&doc, "lines") as u32;
    if n == 0 {
        return None;
    }
    let (a, b) = (call!(doc, line, first.clamp(1, n)), call!(doc, line, last.clamp(first.clamp(1, n), n)));
    let top = num(&call!(cm, lineBlockAt, get(&a, "from")), "top");
    let end = call!(cm, lineBlockAt, get(&b, "to"));
    Some((top, num(&end, "top") + num(&end, "height")))
}

/// Where the line at the top of a chapter note's editor is in the PDF.
fn note_place(leaf: &JsValue, page_h: f32) -> Option<Pos> {
    let (cm, scroller) = editor(leaf)?;
    let path = path_of(leaf);
    let h = (top_height(&cm, &scroller) + ANCHOR * num(&scroller, "clientHeight")).max(0.0);
    let block = call!(cm, lineBlockAtHeight, h);
    let line = num(&call!(get(&get(&cm, "state"), "doc"), lineAt, get(&block, "from")), "number") as u32;
    let (st, tex, map) = with(|a| a.locate(&path)).flatten()?;
    // a part that prints nothing (a comment block) gives way to the next that prints, from its top
    let (first, last, _, moved) = map.printed_part(line)?;
    let (top, bottom) = span(&cm, first, last)?;
    let frac = if moved || bottom <= top { 0.0 } else { ((h - top) / (bottom - top)) as f32 };
    synctex::forward(&st, &tex, &map, first, frac, page_h)
}

/// The PDF pane to a place: the line's baseline at the anchor's height in the pane, the pages centred across it.
/// Without pdf.js, the page only, and only when it changes.
fn scroll_pdf(leaf: &JsValue, p: Pos) {
    let view = get(leaf, "view");
    if let Some(viewer) = pdfjs(&view) {
        let Some(b) = view_box(&viewer, p.page) else { return };
        with(|a| a.sync.quiet_pdf = js::now() + ECHO_MS);
        let dest = Array::of5(&JsValue::NULL, &obj! { name: "XYZ" }, &JsValue::from(b[0]), &JsValue::from(b[3] - (p.v as f64 - pdf_anchor_pt(&viewer))), &JsValue::NULL);
        call!(viewer, scrollPageIntoView, obj! { pageNumber: p.page, destArray: dest, allowNegativeOffset: true });
        centre_pdf(&viewer);
        return;
    }
    if with(|a| std::mem::replace(&mut a.sync.page, p.page) != p.page) != Some(true) {
        return;
    }
    let state = obj! { subpath: format!("#page={}", p.page) };
    if get(&view, "setEphemeralState").is_function() {
        call!(view, setEphemeralState, state);
    } else {
        call!(leaf, openFile, get(&view, "file"), obj! { active: false, eState: state });
    }
}

/// A chapter note's editor scrolled: the PDF to the same place. The glossary note is not typeset where it is written
/// (its entries are printed in the Terms list), so it follows by entry: the entry being read, at its place in the list
/// (the PDF's named destination glo:<key>, which hyperref gives every entry printed).
fn note_to_pdf(leaf: &JsValue) {
    let Some(pdf) = pdf_leaf() else { return };
    let page_h = page_height(pdfjs(&get(&pdf, "view")).as_ref());
    if let Some(p) = note_place(leaf, page_h) {
        scroll_pdf(&pdf, p);
    } else if let Some(entries) = with(|a| a.glossary_entries(&path_of(leaf))).flatten() {
        let Some((cm, scroller)) = editor(leaf) else { return };
        let h = (top_height(&cm, &scroller) + ANCHOR * num(&scroller, "clientHeight")).max(0.0);
        let block = call!(cm, lineBlockAtHeight, h);
        let line = num(&call!(get(&get(&cm, "state"), "doc"), lineAt, get(&block, "from")), "number") as u32;
        let Some((_, key)) = entries.iter().rev().find(|(l, _)| *l <= line).or(entries.first()) else { return };
        if with(|a| std::mem::replace(&mut a.sync.gloss_at, key.clone()) == *key) == Some(true) {
            return; // still the same entry
        }
        js::spawn(entry_to_pdf(pdf, key.clone()));
    }
}

/// The PDF pane to a glossary entry's place in the Terms list (an entry the text does not use is not printed: nothing
/// to go to).
async fn entry_to_pdf(pdf: JsValue, key: String) {
    let Some(viewer) = pdfjs(&get(&pdf, "view")) else { return };
    let doc = get(&viewer, "pdfDocument");
    if !doc.is_object() {
        return;
    }
    let Ok(dest) = js::wait(call!(doc, getDestination, format!("glo:{key}"))).await else { return };
    if !Array::is_array(&dest) {
        return;
    }
    let dest = Array::from(&dest);
    let Ok(index) = js::wait(call!(doc, getPageIndex, dest.get(0))).await else { return };
    let Some(index) = index.as_f64() else { return };
    let (x, y) = (dest.get(2).as_f64().unwrap_or(0.0), dest.get(3).as_f64().unwrap_or(0.0));
    with(|a| a.sync.quiet_pdf = js::now() + ECHO_MS);
    // the entry's top at the anchor's height in the pane, as a chapter's line would be
    let to = Array::of5(&JsValue::NULL, &obj! { name: "XYZ" }, &JsValue::from(x), &JsValue::from(y + pdf_anchor_pt(&viewer)), &JsValue::NULL);
    call!(viewer, scrollPageIntoView, obj! { pageNumber: index + 1.0, destArray: to, allowNegativeOffset: true });
    centre_pdf(&viewer);
}

/// The PDF scrolled under the pointer: the chapter note shown there, if a pane shows it, to the same place.
fn pdf_to_note() {
    let Some(pdf) = pdf_leaf() else { return };
    let Some(viewer) = pdfjs(&get(&pdf, "view")) else { return };
    let Some((page, v)) = location(&viewer) else { return };
    let page_h = page_height(Some(&viewer));
    let Some((path, first, last, frac, workspace)) = with(|a| a.locate_pdf(Pos { page, v }, page_h).map(|(p, f, l, r)| (p, f, l, r, a.workspace()))).flatten() else { return };
    let Some(leaf) = note_leaf(&workspace, &path) else { return };
    let Some((cm, scroller)) = editor(&leaf) else { return };
    let Some((top, bottom)) = span(&cm, first, last) else { return };
    // the scroller's scrollTop for a document height: the document's own top within the scrolled content, plus it
    let offset = num(&cm, "documentTop") - num(&call!(scroller, getBoundingClientRect), "top") + num(&scroller, "scrollTop");
    with(|a| a.sync.quiet_note = js::now() + ECHO_MS);
    // the line at the anchor's height in the editor, as in the PDF
    set(&scroller, "scrollTop", offset + top + frac as f64 * (bottom - top) - ANCHOR * num(&scroller, "clientHeight"));
}

// ----- events

/// A scroll event from either pane: the leading pane's place is followed once per frame.
fn scrolled(lead: Lead) {
    let go = with(|a| {
        let (s, now) = (&mut a.sync, js::now());
        let leads = match lead {
            Lead::Note(_) => !s.pointer_pdf && now >= s.quiet_note,
            Lead::Pdf => s.pointer_pdf && now >= s.quiet_pdf,
        };
        if !a.settings.sync_scroll || !leads {
            return false;
        }
        s.lead = Some(lead);
        !std::mem::replace(&mut s.frame, true)
    });
    if go == Some(true) {
        js::frame(|| match with(|a| {
            a.sync.frame = false;
            a.sync.lead.take()
        })
        .flatten()
        {
            Some(Lead::Note(leaf)) => note_to_pdf(&leaf),
            Some(Lead::Pdf) => pdf_to_note(),
            None => {}
        });
    }
}

/// Listen to the editors' scrollers and the PDF pane (each element once per load of the plugin): run after the layout
/// changes, from a timer (Obsidian fires its events inside calls made from within `with`).
pub fn attach() {
    let Some((plugin, workspace, mark, on)) = with(|a| (a.plugin.clone(), a.workspace(), a.sync.mark.clone(), a.settings.sync_scroll)) else { return };
    if !on {
        return;
    }
    let listen = |el: &JsValue, events: &[(&str, JsValue)]| {
        if el.is_object() && !get(el, &mark).is_truthy() {
            set(el, &mark, true);
            for (event, f) in events {
                call!(plugin, registerDomEvent, el.clone(), *event, f.clone());
            }
        }
    };
    for leaf in leaves(&workspace, "markdown") {
        if let Some((_, scroller)) = editor(&leaf) {
            listen(&scroller, &[("scroll", f1(move |_| {
                scrolled(Lead::Note(leaf.clone()));
                JsValue::UNDEFINED
            }))]);
        }
    }
    let Some(pdf) = pdf_leaf() else { return };
    let view = get(&pdf, "view");
    let pointer = |over: bool| {
        f1(move |_| {
            with(|a| a.sync.pointer_pdf = over);
            JsValue::UNDEFINED
        })
    };
    listen(&get(&view, "containerEl"), &[("mouseenter", pointer(true)), ("mouseleave", pointer(false))]);
    if let Some(viewer) = pdfjs(&view) {
        listen(&get(&viewer, "container"), &[("scroll", f1(|_| {
            scrolled(Lead::Pdf);
            JsValue::UNDEFINED
        }))]);
    }
}

/// Once a newly opened (or redrawn) PDF has its pages, the note's place in it; without pdf.js, the page jump after the
/// last try.
fn follow(leaf: JsValue, tries: u32) {
    js::timeout(READY_MS, move || {
        let ready = pdf_leaf().and_then(|l| pdfjs(&get(&l, "view"))).is_some_and(|v| num(&v, "pagesCount") > 0.0);
        if ready || tries == 0 {
            attach();
            note_to_pdf(&leaf);
        } else {
            follow(leaf, tries - 1);
        }
    });
}

/// The PDF beside the notes: opened on the right (focus stays where it is) unless a pane shows it; with sync on, at
/// the place of `note` (a chapter note's pane; the active one, or the chapter shown most recently, when None). `open`
/// false only places a PDF already shown. <https://docs.obsidian.md/Reference/TypeScript+API/Workspace/getLeaf>.
fn show_pdf(note: Option<JsValue>, open: bool) {
    let Some((app, workspace, path, sync)) = with(|a| Some((get(&a.plugin, "app"), a.workspace(), a.pdf_path()?, a.settings.sync_scroll))).flatten() else { return };
    let file = call!(get(&app, "vault"), getAbstractFileByPath, path.as_str());
    if file.is_null() || file.is_undefined() {
        return;
    }
    let note = note.or_else(|| chapter_leaf(&workspace)).filter(|_| sync);
    if pdf_leaf().is_some() {
        if let Some(n) = note {
            follow(n, READY_TRIES);
        }
        return;
    }
    if !open {
        return;
    }
    let page = note.as_ref().and_then(|n| note_place(n, PAGE_H)).map(|p| p.page);
    let state = match page {
        Some(n) => {
            with(|a| a.sync.page = n);
            obj! { active: false, eState: obj! { subpath: format!("#page={n}") } }
        }
        None => obj! { active: false },
    };
    let leaf = call!(workspace, getLeaf, "split", "vertical");
    call!(leaf, openFile, file, state);
    if let Some(n) = note {
        follow(n, READY_TRIES);
    }
}

/// The pane of a chapter note to place the PDF at: the active editor's, or else the chapter shown most recently.
fn chapter_leaf(workspace: &JsValue) -> Option<JsValue> {
    let linked = |l: &JsValue| l.is_object() && with(|a| a.chapter(&path_of(l), true).is_some()) == Some(true);
    let active = get(&call!(workspace, getActiveViewOfType, with(|a| get(&a.obs, "MarkdownView")).unwrap_or_default()), "leaf");
    if linked(&active) {
        return Some(active);
    }
    leaves(workspace, "markdown").into_iter().filter(linked).max_by(|a, b| num(a, "activeTime").total_cmp(&num(b, "activeTime")))
}

/// A note was opened (Workspace 'file-open', from a timer): a chapter note shows the PDF beside it (the setting "Show
/// the PDF beside the notes"), and with sync on, the PDF goes to its place.
pub fn opened(path: String) {
    attach();
    let Some((open, sync, workspace, linked)) = with(|a| (a.settings.open_pdf, a.settings.sync_scroll, a.workspace(), a.chapter(&path, false).is_some())) else { return };
    if !linked || !(open || sync) {
        return;
    }
    let note = note_leaf(&workspace, &path);
    if note.is_some() {
        show_pdf(note, open);
    }
}

/// A build succeeded (from a timer): the SyncTeX file read again (with sync on), then the PDF shown or placed.
/// The built PDF copied to the one the pane shows, `<main>.shown.pdf`, in one write (ToDo 2, user 2026-10-27: latexmk's
/// passes rewrite `<main>.pdf` several times, and Obsidian redrew the pane after each; the shown copy changes once per
/// finished build or preview). Same pages, so SyncTeX still places them.
pub async fn publish() {
    let Some((ad, from, to)) = with(|a| {
        let main = a.settings.main();
        match (a.out().join(&format!("{main}.pdf")), a.out().join(&format!("{main}.shown.pdf"))) {
            (Place::Vault(f), Place::Vault(t)) => Some((a.adapter.clone(), f, t)),
            _ => None,
        }
    })
    .flatten() else { return };
    if let Ok(bytes) = js::wait(call!(ad, readBinary, from.as_str())).await {
        with(|a| a.sync.published = js::now());
        _ = js::wait(call!(ad, writeBinary, to.as_str(), bytes)).await;
    }
}

/// The shown PDF changed, written by another device's build (the computer's, seen from Ignis in a browser; remote.rs):
/// Obsidian's PDF view loads the new copy by itself; with sync on, the SyncTeX file is read again and the PDF placed at
/// the note once its pages are there, as after a build here (`built`). A pane is not opened for it.
pub async fn shown_elsewhere() {
    let Some((own, sync)) = with(|a| (js::now() - a.sync.published < OWN_WRITE_MS, a.settings.sync_scroll)) else { return };
    if own || !sync {
        return;
    }
    js::sleep(READY_MS).await; // the PDF view starts loading the new copy first
    load().await;
    show_pdf(None, false);
}

pub async fn built() {
    publish().await;
    let Some((open, sync)) = with(|a| (a.settings.open_pdf, a.settings.sync_scroll)) else { return };
    if sync {
        load().await;
    }
    if open || sync {
        show_pdf(None, open);
    }
}

/// Sync scrolling turned on, or the plugin loaded with it on: the last build's SyncTeX file, and the listeners.
pub async fn enable() {
    if with(|a| a.settings.sync_scroll) == Some(true) {
        load().await;
        attach();
    }
}

/// The last build's SyncTeX file into the index. What keeps it from being read (no file, no desktop app, chapters not
/// in it) is one line in the log; sync scrolling then does nothing.
async fn load() {
    let Some((out, main, keep, base, outside)) = with(|a| {
        let keep: Vec<String> = a.notes.keys().map(|n| pipeline::tex_name(n)).collect();
        (a.out(), a.settings.main(), keep, a.base(), a.pdf_path().is_none())
    }) else {
        return;
    };
    let result = async {
        if outside {
            return Err("the output folder is outside the vault, where Obsidian cannot show the PDF".to_string());
        }
        let (gz, plain) = (out.join(&format!("{main}.synctex.gz")), out.join(&format!("{main}.synctex")));
        let text = if files::exists(&gz).await {
            let bytes = read_binary(&gz, base.as_deref()).await?;
            unpack(&bytes).ok_or(format!("{main}.synctex.gz could not be unpacked"))?
        } else if files::exists(&plain).await {
            files::read(&plain).await.ok_or(format!("{main}.synctex could not be read"))?
        } else {
            return Err(format!("no {main}.synctex.gz in the output folder (the compile command needs -synctex=1)"));
        };
        let st = Synctex::new(&text, |f| keep.iter().any(|k| k == f)).map_err(|e| format!("{main}.synctex.gz: {e}"))?;
        let missing: Vec<&str> = keep.iter().map(String::as_str).filter(|k| st.file(k).is_none()).collect();
        Ok((st, missing))
    }
    .await;
    with(|a| match result {
        Ok((st, missing)) => {
            let main = a.settings.main();
            a.sync.index = Some(Rc::new(st));
            if !missing.is_empty() {
                a.log(&format!("Sync scrolling: not in {main}.synctex.gz (not \\input by {main}.tex, or nothing typeset): {}", missing.join(", ")));
            }
        }
        Err(e) => {
            a.sync.index = None;
            a.log(&format!("Sync scrolling: {e}; the PDF is not synced"));
        }
    });
}

/// A .synctex.gz's text, unpacked here rather than by Node's zlib, so that it is read in the browser too (Ignis).
fn unpack(bytes: &JsValue) -> Option<String> {
    let b = Uint8Array::new(bytes).to_vec();
    crate::pdf::gunzip(&b).map(|t| String::from_utf8_lossy(&t).into_owned())
}

/// A file's bytes (a Uint8Array): Obsidian's adapter for the vault, Node's fs elsewhere.
async fn read_binary(p: &Place, base: Option<&str>) -> Result<JsValue, String> {
    let bytes = match p {
        Place::Vault(v) => {
            let adapter = with(|a| a.adapter.clone()).ok_or("the plugin is unloaded")?;
            js::wait(call!(adapter, readBinary, v.as_str())).await
        }
        Place::Disk(_) => {
            let fs = node("fs").ok_or("a folder outside the vault needs the desktop app")?;
            js::wait(call!(get(&fs, "promises"), readFile, p.absolute(base).unwrap_or_default())).await
        }
    }
    .map_err(|e| files::reason(&e))?;
    Ok(Uint8Array::new(&bytes).into())
}

/// A preview was spliced into the PDF (from a timer): the index made from the last full build's SyncTeX file and the
/// preview's, page by page (splice::bands), then the PDF shown.
pub async fn spliced(bands: Vec<Vec<crate::splice::Band>>) {
    let Some((open, sync)) = with(|a| (a.settings.open_pdf, a.settings.sync_scroll)) else { return };
    if sync {
        let (m, p) = (read_index(crate::preview::FULL).await, read_index(crate::preview::JOB).await);
        with(|a| match (m, p) {
            (Ok(m), Ok(p)) => a.sync.index = Some(Rc::new(Synctex::spliced(&m, &p, &bands))),
            (Err(e), _) | (_, Err(e)) => a.log(&format!("Sync scrolling: {e}; the preview is not synced")),
        });
    }
    if open || sync {
        show_pdf(None, open);
    }
}

/// `<stem>.synctex.gz` of the output folder, indexed for the chapters.
async fn read_index(stem: &str) -> Result<Synctex, String> {
    let (out, keep, base) = with(|a| {
        (a.out(), a.notes.keys().map(|n| pipeline::tex_name(n)).collect::<Vec<String>>(), a.base())
    })
    .ok_or("the plugin is unloaded")?;
    let gz = out.join(&format!("{stem}.synctex.gz"));
    let bytes = read_binary(&gz, base.as_deref()).await?;
    let text = unpack(&bytes).ok_or(format!("{stem}.synctex.gz could not be unpacked"))?;
    Synctex::new(&text, |f| keep.iter().any(|k| k == f)).map_err(|e| format!("{stem}.synctex.gz: {e}"))
}
