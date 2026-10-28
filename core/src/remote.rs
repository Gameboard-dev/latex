//! What the computer's build did, shown on a device that does not build (user, 2026-10-28): Ignis in a browser, or a
//! phone with the vault synced. The desktop app writes `<output>/latex-status.json` when a build or a preview starts and
//! when it ends (only for an output folder in the vault, the one place other devices see). A device that cannot compile
//! reads it once at start and again whenever it changes (Obsidian's vault events; Ignis passes them on from its file
//! watcher, which must therefore watch this file: Ignis's SETUP.md, IGNORED_PATHS), and shows it in the status bar:
//! "built on the PC at 23:28 (preview)". The PDF pane follows by itself: Obsidian's PDF view loads `<main>.shown.pdf`
//! again when the file changes, and the plugin then places it at the note (scroll::shown_elsewhere).
//!
//! The file is JSON (`Report`), written whole each time, one write after another (the newest wins).

use std::cell::RefCell;

use serde::{Deserialize, Serialize};

use crate::app::{with, Status};
use crate::files::{self, Place};
use crate::js;

/// The file's name in the output folder.
pub const FILE: &str = "latex-status.json";

/// A build still "building" after this long (ms) is taken as cut off (Obsidian closed on the computer mid-build).
pub const STALE_MS: f64 = 15.0 * 60.0 * 1000.0;

/// One start or end of a build or a preview on the computer.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Report {
    /// "building", "built", "failed", "stopped", or "skipped" (a preview that did not run: the full build follows).
    pub state: String,
    /// "preview" (fast preview, spliced into the PDF) or "full" (the whole document).
    pub kind: String,
    /// The computer's local time, "2026-10-28 23:28:36".
    pub at: String,
    /// The same moment in ms since 1970 (Date.now()), for the age of a build that never reported its end.
    pub when: f64,
    pub errors: usize,
    pub undefined: usize,
    pub seconds: f64,
    /// The build's summary, or the preview's steps, for the tooltip.
    pub summary: String,
}

impl Report {
    pub fn new(state: &str, kind: &str) -> Report {
        Report { state: state.into(), kind: kind.into(), ..Report::default() }
    }
}

/// "23:28" for a moment of `today` ("2026-10-28"); "2026-10-27 23:28" for another day; `at` itself if it is not a time
/// ("?" when there is none).
fn time_of(at: &str, today: &str) -> String {
    match (at.get(..10), at.get(11..16)) {
        (Some(day), Some(hm)) if day == today => hm.to_string(),
        (Some(day), Some(hm)) => format!("{day} {hm}"),
        _ if at.trim().is_empty() => "?".into(),
        _ => at.to_string(),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The status bar's text and tooltip for a report, at `now_ms` (Date.now()) on `today` ("2026-10-28").
pub fn text(r: &Report, now_ms: f64, today: &str) -> (String, String) {
    let at = time_of(&r.at, today);
    let preview = r.kind == "preview";
    let counts = format!("{}, {}", plural(r.errors, "error", "errors"), plural(r.undefined, "undefined ref", "undefined refs"));
    let text = match r.state.as_str() {
        "building" if r.when > 0.0 && now_ms - r.when > STALE_MS => format!("the PC's {} of {at} did not finish", if preview { "preview" } else { "build" }),
        "building" if preview => format!("previewing on the PC… (started {at})"),
        "building" => format!("building on the PC… (started {at})"),
        "built" if preview => format!("built on the PC at {at} (preview)"),
        "built" => format!("built on the PC at {at} ({counts})"),
        "failed" => format!("build failed on the PC at {at} ({}) — see log", plural(r.errors, "error", "errors")),
        "stopped" => format!("build stopped on the PC at {at}"),
        "skipped" => format!("preview skipped on the PC at {at}; the full build follows"),
        other => format!("the PC: {other} at {at}"),
    };
    let mut tip = format!("What Obsidian on the computer last did with the document ({FILE} in the output folder).");
    if r.seconds > 0.0 {
        tip += &format!("\nIt took {:.1} s.", r.seconds);
    }
    let summary: Vec<&str> = r.summary.lines().filter(|l| !l.trim().is_empty()).take(6).collect();
    if !summary.is_empty() {
        tip += &format!("\n{}", summary.join("\n"));
    }
    (text, tip)
}

thread_local! {
    /// The newest report not yet written, and whether a write is under way.
    static QUEUE: RefCell<(Option<(Place, String)>, bool)> = const { RefCell::new((None, false)) };
}

/// The report written to `place` after any write still under way; a newer one replaces one still waiting.
pub fn write_later(place: Place, text: String) {
    let start = QUEUE.with(|q| {
        let mut q = q.borrow_mut();
        q.0 = Some((place, text));
        !std::mem::replace(&mut q.1, true)
    });
    if start {
        js::spawn(flush());
    }
}

async fn flush() {
    loop {
        let next = QUEUE.with(|q| {
            let mut q = q.borrow_mut();
            let next = q.0.take();
            if next.is_none() {
                q.1 = false;
            }
            next
        });
        let Some((place, text)) = next else { break };
        if let Err(e) = files::write(&place, &text).await {
            with(|a| a.log(&format!("{FILE} not written: {e}")));
        }
    }
}

/// The report read and shown, on a device that does not build (at start, and when the file changed). A build this
/// device asked of Ignis keeps the status bar while it runs.
pub async fn read() {
    let Some((place, here)) = with(|a| (a.out().join(FILE), !a.can_compile())) else { return };
    if !here {
        return;
    }
    let Some(text) = files::read(&place).await else { return };
    let Ok(r) = serde_json::from_str::<Report>(&text) else { return };
    with(|a| {
        if !a.build.running {
            a.set_status(Status::Remote(r));
        }
    });
}

/// A file of the vault changed (from a timer): on a device that does not build, the report is read again, and a new
/// `<main>.shown.pdf` is placed at the note once Obsidian has loaded it.
pub fn changed(path: &str) {
    let Some((report, pdf, here)) = with(|a| (a.out().join(FILE), a.pdf_path(), !a.can_compile())) else { return };
    if !here {
        return;
    }
    if report == Place::Vault(path.to_string()) {
        js::spawn(read());
    } else if pdf.as_deref() == Some(path) {
        js::spawn(crate::scroll::shown_elsewhere());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(state: &str, kind: &str) -> Report {
        Report { at: "2026-10-28 23:28:36".into(), when: 1_000_000.0, ..Report::new(state, kind) }
    }

    #[test]
    fn status_texts() {
        let today = "2026-10-28";
        let now = 1_000_000.0 + 5_000.0;
        assert_eq!(text(&r("building", "full"), now, today).0, "building on the PC… (started 23:28)");
        assert_eq!(text(&r("building", "preview"), now, today).0, "previewing on the PC… (started 23:28)");
        assert_eq!(text(&r("built", "preview"), now, today).0, "built on the PC at 23:28 (preview)");
        let full = Report { errors: 0, undefined: 10, ..r("built", "full") };
        assert_eq!(text(&full, now, today).0, "built on the PC at 23:28 (0 errors, 10 undefined refs)");
        let one = Report { errors: 1, undefined: 1, ..r("built", "full") };
        assert_eq!(text(&one, now, today).0, "built on the PC at 23:28 (1 error, 1 undefined ref)");
        let failed = Report { errors: 3, ..r("failed", "full") };
        assert_eq!(text(&failed, now, today).0, "build failed on the PC at 23:28 (3 errors) — see log");
        assert_eq!(text(&r("stopped", "full"), now, today).0, "build stopped on the PC at 23:28");
        assert_eq!(text(&r("skipped", "preview"), now, today).0, "preview skipped on the PC at 23:28; the full build follows");
        // another day: the date too
        assert_eq!(text(&r("built", "preview"), now, "2026-10-29").0, "built on the PC at 2026-10-28 23:28 (preview)");
        // a build that never reported its end
        assert_eq!(text(&r("building", "full"), 1_000_000.0 + STALE_MS + 1.0, today).0, "the PC's build of 23:28 did not finish");
    }

    #[test]
    fn tooltip_has_the_summary() {
        let rep = Report { seconds: 2.86, summary: "Compiled in 3 s: ok; 0 errors\n\n  steps: LuaLaTeX 2.1 s".into(), ..r("built", "full") };
        let (_, tip) = text(&rep, 0.0, "2026-10-28");
        assert!(tip.contains("It took 2.9 s."), "{tip}");
        assert!(tip.ends_with("Compiled in 3 s: ok; 0 errors\n  steps: LuaLaTeX 2.1 s"), "{tip}");
    }

    #[test]
    fn json_round_trip_and_missing_fields() {
        let rep = Report { errors: 2, seconds: 30.5, summary: "x".into(), ..r("failed", "full") };
        let back: Report = serde_json::from_str(&serde_json::to_string(&rep).unwrap()).unwrap();
        assert_eq!(back, rep);
        // an older or hand-written file with fields missing still reads
        let partial: Report = serde_json::from_str(r#"{"state":"built","kind":"preview"}"#).unwrap();
        assert_eq!(partial, Report::new("built", "preview"));
        assert_eq!(text(&partial, 0.0, "2026-10-28").0, "built on the PC at ? (preview)");
    }
}
