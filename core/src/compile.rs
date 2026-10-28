//! Compiling: the compile command's steps run one after another in the output folder (Node's child_process, desktop
//! only), their output streamed into the build log and read line by line for the status bar's stage (pipeline::Stage).
//! A new build cancels one still running (its whole process tree, then the files a stopped run may leave half-written),
//! or waits for it, as the settings say.
//!
//! References: <https://nodejs.org/api/child_process.html#child_processspawncommand-args-options> (spawn, its 'close'
//! and 'error' events), <https://nodejs.org/api/os.html#ossetprioritypid-priority> (below-normal priority, inherited by
//! the processes latexmk starts), `taskkill /T` (<https://learn.microsoft.com/windows-server/administration/windows-commands/taskkill>)
//! and process groups (`detached`, `process.kill(-pid)`) to stop a tree, <https://nodejs.org/api/fs.html#fscreatewritestreampath-options>.

use js_sys::{Array, Promise};
use wasm_bindgen::prelude::*;

use crate::app::{with, Status};
use crate::files::{self, node};
use crate::js::{self, call, f1, f2, get, obj, set, str};
use crate::pipeline::{self, Stage};
use crate::remote::Report;

/// The build in progress, if any.
#[derive(Default)]
pub struct Build {
    /// Incremented by every build; a run whose number is no longer current stops.
    pub generation: u32,
    pub running: bool,
    /// Started while one was running and not to cancel it: run again when it ends.
    pub again: bool,
    pub stage: Stage,
    pub since: f64,
    /// The next build runs the whole recipe (latexmk), whatever changed: Compile now, Build all chapters and compile.
    pub whole: bool,
    /// The step's process and the promise of its exit (resolved once, however often it is awaited).
    pub(crate) child: Option<(JsValue, JsValue)>,
}

pub fn windows() -> bool {
    str(&get(&get(&js::global(), "process"), "platform")) == "win32"
}

/// The environment a TeX program runs in: Obsidian's, with the TeX programs folder (this device's setting, else the
/// usual places that exist) after its PATH, so a TeX installed after Obsidian started is found (user, 2026-10-27).
pub fn tex_env() -> JsValue {
    let process = get(&js::global(), "process");
    let env = get(&process, "env");
    let copy = call!(get(&js::global(), "Object"), assign, js::empty(), env.clone());
    // the variable's own name (Windows: "Path"), so that the child does not get two
    let keys: js_sys::Array = call!(get(&js::global(), "Object"), keys, copy.clone()).into();
    let name = keys.iter().map(|k| str(&k)).find(|k| k.eq_ignore_ascii_case("path")).unwrap_or_else(|| "PATH".into());
    let var = |k: &str| str(&get(&env, k));
    let folders = match with(|a| a.device.tex_folder.clone()).unwrap_or_default() {
        f if !f.trim().is_empty() => vec![f],
        _ => {
            let fs = node("fs");
            crate::device::usual_folders(windows(), &var("HOME"), &var("APPDATA"), &var("LOCALAPPDATA"))
                .into_iter()
                .filter(|f| fs.as_ref().is_some_and(|fs| call!(fs, existsSync, f.as_str()).is_truthy()))
                .collect()
        }
    };
    let sep = if windows() { ';' } else { ':' };
    set(&copy, &name, crate::device::with_tex_path(&str(&get(&copy, &name)), &folders, sep));
    copy
}

/// A promise of the process's end: its exit code (null when it was killed), or the error that kept it from starting.
pub fn exit_of(child: &JsValue) -> JsValue {
    let c = child.clone();
    Promise::new(&mut |resolve, _| {
        let r = resolve.clone();
        call!(c, on, "close", f2(move |code, _signal| {
            _ = r.call1(&JsValue::NULL, &code);
            JsValue::UNDEFINED
        }));
        call!(c, on, "error", f1(move |e| {
            _ = resolve.call1(&JsValue::NULL, &e);
            JsValue::UNDEFINED
        }));
    })
    .into()
}

/// Start a build now (the quiet period is over, or a command asked).
pub fn start() {
    if !with(|a| a.can_compile()).unwrap_or(false) {
        // a browser: Ignis builds, when that is set up and on (ignis.rs)
        match with(|a| (a.ignis_ok, a.build.running)) {
            Some((true, false)) => js::spawn(crate::ignis::build()),
            Some((true, true)) => {}
            _ => _ = with(|a| a.set_status(Status::NoCompiler)),
        }
        return;
    }
    let Some((running, cancel)) = with(|a| (a.build.running, a.settings.cancel_running)) else { return };
    if running && !cancel {
        with(|a| a.build.again = true);
        return;
    }
    let Some((gen, old)) = with(|a| {
        a.cancel_quiet(); // a build asked for now replaces the one the quiet period would start
        a.build.generation += 1;
        a.build.running = true;
        a.build.again = false;
        (a.build.generation, a.build.child.take())
    }) else {
        return;
    };
    let ended = old.map(|(child, exit)| kill(&child, exit));
    js::spawn(async move {
        if let Some(ended) = ended {
            ended.await;
            clean().await;
        }
        run(gen).await;
    });
}

/// The quiet period after a save ended: the full build now, unless fast preview has put every change into the PDF with
/// every page current (user, 2026-10-27: no half-minute LuaLaTeX run after every save when the PDF is already right);
/// then it waits until the notes have been idle for the setting Full build when idle. A preview still to come or
/// running decides when it ends.
pub fn quiet_ended() {
    let Some((fast, previewing, current, idle, debug)) = with(|a| (a.settings.fast() && a.can_compile(), a.preview.running || a.preview.pending || a.preview.timer.is_some(), a.preview.current, a.settings.idle_build_seconds, a.settings.debug_log)) else { return };
    if fast && previewing {
        with(|a| a.preview.decide = true);
        return;
    }
    if fast && current && idle > 0.0 {
        let ms = idle * 1000.0;
        with(|a| {
            if debug {
                a.log(&format!("debug: the PDF shown is current; the full build waits until the notes have been idle for {idle} s"));
            }
            a.cancel_quiet();
            // in the quiet period's slot, so that the next save cancels it
            a.quiet = Some(js::timeout(ms, || {
                with(|a| a.quiet = None);
                start();
            }));
            a.set_status(crate::app::Status::Previewed { until: js::now() + ms });
        });
        return;
    }
    if debug {
        with(|a| a.log(&format!("debug: full build now ({})", if !fast { "fast preview is off" } else if idle <= 0.0 { "Full build when idle is 0" } else { "the last preview left pages to update, or none ran" })));
    }
    start();
}

/// A build of the whole recipe (latexmk) at once, whatever changed since the last.
pub fn start_whole() {
    with(|a| a.build.whole = true);
    start();
}

/// Stop the build in progress (the command "Stop the build", and unloading: the kill starts at once, before the plugin
/// is gone).
pub fn cancel() {
    stop_worker();
    let old = with(|a| {
        a.build.generation += 1;
        a.build.running = false;
        a.build.again = false;
        a.build.child.take()
    })
    .flatten();
    if let Some((child, exit)) = old {
        let ended = kill(&child, exit);
        js::spawn(async move {
            ended.await;
            clean().await;
            with(|a| {
                a.log("Build stopped.");
                a.report(Report::new("stopped", "full"));
                a.set_status(Status::Stopped);
            });
        });
    }
}

/// Kill a step's process and everything it started, now; the future ends when it has ended (so its files are closed).
fn kill(child: &JsValue, exit: JsValue) -> impl std::future::Future<Output = ()> {
    let pid = get(child, "pid").as_f64();
    let killer = match (pid, windows()) {
        (Some(pid), true) => node("child_process").map(|cp| {
            let args: Array = ["/pid".to_string(), pid.to_string(), "/T".into(), "/F".into()].iter().map(JsValue::from).collect();
            exit_of(&call!(cp, spawn, "taskkill", args, obj! { windowsHide: true, stdio: "ignore" }))
        }),
        (Some(pid), false) => {
            call!(get(&js::global(), "process"), kill, -pid, "SIGKILL"); // the group: the process was started detached
            None
        }
        (None, _) => None,
    };
    async move {
        if let Some(k) = killer {
            _ = js::wait(k).await;
        }
        _ = js::wait(exit).await;
    }
}

/// After a cancellation: the main file's scratch files, which latexmk makes again from scratch.
async fn clean() {
    let Some((out, main)) = with(|a| (a.out(), a.settings.main())) else { return };
    for ext in pipeline::SCRATCH_EXTS {
        files::remove(&out.join(&format!("{main}{ext}"))).await;
    }
}

fn current(gen: u32) -> bool {
    with(|a| a.build.generation == gen).unwrap_or(false)
}

async fn run(gen: u32) {
    let Some((out, s, base)) = with(|a| (a.out(), a.settings.clone(), a.base())) else { return };
    // an .aux cut off part way (a build killed while writing it, e.g. when Obsidian closed) stops every later run at
    // \begin{document}: the scratch files go, and latexmk makes them again
    if let Some(aux) = files::read(&out.join(&format!("{}.aux", s.main()))).await {
        if pipeline::aux_truncated(&aux) {
            with(|a| a.log(&format!("{}.aux was cut off by an earlier build: removed with the other scratch files, so this build starts afresh", s.main())));
            clean().await;
        }
    }
    let (Some(dir), Some(cp), Some(fs)) = (out.absolute(base.as_deref()), node("child_process"), node("fs")) else {
        return finish(gen, Err("this device cannot run the compile command".into()));
    };
    if s.fast() {
        return run_fast(gen, out, s, dir, fs, base).await;
    }
    let sep = if windows() { "\\" } else { "/" };
    // this device's command (device.rs), each step's program checked before anything runs
    let Some(device) = with(|a| a.device.clone()) else { return };
    let steps = match pipeline::steps(&device.compile_command(&s.engine), &dir, &s.main(), sep).and_then(|st| st.iter().try_for_each(|step| device.allows(step)).map(|()| st)) {
        Ok(st) => st,
        Err(e) => return finish(gen, Err(e)),
    };
    let build_log = out.join(&s.build_log).absolute(base.as_deref()).unwrap_or_default();
    let stream = call!(fs, createWriteStream, build_log.as_str());
    let t0 = js::now();
    with(|a| {
        a.build.since = t0;
        a.report(Report::new("building", "full"));
        a.set_status(Status::Compiling);
    });
    let (mut code, mut failed) = (0i64, String::new());
    for (k, step) in steps.iter().enumerate() {
        if !current(gen) {
            break;
        }
        let program = pipeline::program(&step[0]);
        with(|a| {
            a.build.stage.start(&program, k + 1, steps.len());
            a.show();
        });
        let args: Array = step[1..].iter().map(|x| JsValue::from(x.as_str())).collect();
        let stdio: Array = ["ignore", "pipe", "pipe"].iter().map(|x| JsValue::from(*x)).collect();
        // detached outside Windows: its own process group, so that the whole tree can be stopped
        let child = call!(cp, spawn, step[0].as_str(), args, obj! { cwd: dir.as_str(), windowsHide: true, detached: !windows(), stdio: stdio, env: tex_env() });
        let exit = exit_of(&child);
        if s.below_normal {
            if let (Some(os), Some(pid)) = (node("os"), get(&child, "pid").as_f64()) {
                call!(os, setPriority, pid, get(&get(&get(&os, "constants"), "priority"), "PRIORITY_BELOW_NORMAL"));
            }
        }
        for pipe in ["stdout", "stderr"] {
            let p = get(&child, pipe);
            call!(p, setEncoding, "utf8");
            let st = stream.clone();
            call!(p, on, "data", f1(move |d| {
                call!(st, write, d.clone());
                with(|a| {
                    if a.build.generation == gen && a.build.stage.feed(&str(&d)) {
                        a.show();
                    }
                });
                JsValue::UNDEFINED
            }));
        }
        with(|a| a.build.child = Some((child.clone(), exit.clone())));
        let end = js::wait(exit).await.unwrap_or(JsValue::NULL);
        with(|a| {
            if a.build.generation == gen {
                a.build.child = None;
            }
        });
        if !current(gen) {
            call!(stream, end);
            return; // cancelled: the next build reports
        }
        match end.as_f64() {
            Some(c) => code = c as i64,
            None if end.is_object() => {
                call!(stream, end);
                return finish(gen, Err(format!("could not start {}: {}", step[0], files::reason(&end))));
            }
            None => code = -1,
        }
        if code != 0 {
            failed = program;
            break;
        }
    }
    // the build log is complete once the stream has flushed
    let flushed = Promise::new(&mut |resolve, _| _ = call!(stream, end, resolve));
    _ = js::wait(flushed.into()).await;
    if !current(gen) {
        return;
    }
    let seconds = (js::now() - t0) / 1000.0;
    let main_log = files::read(&out.join(&format!("{}.log", s.main()))).await.unwrap_or_default();
    let tools = files::read(&out.join(&s.build_log)).await.unwrap_or_default();
    let place = with(|a| out.shown(&a.vault_name())).unwrap_or_default();
    finish(gen, Ok(pipeline::summary(seconds, code, &failed, &main_log, &tools, &place, &s.build_log, &s.main())));
}

fn finish(gen: u32, result: Result<pipeline::Summary, String>) {
    let again = with(|a| {
        if a.build.generation != gen {
            return false;
        }
        a.build.running = false;
        let seconds = (js::now() - a.build.since) / 1000.0;
        let status = match result {
            Ok(sum) => {
                a.report(Report { errors: sum.errors, undefined: sum.undefined, seconds, summary: sum.text.clone(), ..Report::new(if sum.ok { "built" } else { "failed" }, "full") });
                a.log(&sum.text);
                a.last_build = format!("{} {}", crate::app::clock(), sum.text.lines().next().unwrap_or(""));
                if sum.ok {
                    // the SyncTeX file read again and the PDF shown (scroll.rs), outside this borrow of the app
                    js::timeout(0.0, || js::spawn(crate::scroll::built()));
                }
                if sum.ok {
                    Status::Compiled { at: crate::app::clock(), errors: sum.errors, undefined: sum.undefined }
                } else {
                    Status::Failed
                }
            }
            Err(e) => {
                a.report(Report { errors: 1, summary: format!("Not compiled: {e}"), ..Report::new("failed", "full") });
                a.log(&format!("Not compiled: {e}"));
                a.last_build = format!("{} not compiled: {e}", crate::app::clock());
                Status::Failed
            }
        };
        // a chapter changed during the build: the status keeps saying that the next build is waiting
        if matches!(a.status, Status::Waiting { .. }) {
            a.show();
        } else {
            a.set_status(status);
        }
        std::mem::take(&mut a.build.again)
    })
    .unwrap_or(false);
    if again {
        start();
    }
}

// ------------------------------------------------------------------ fast preview

/// The fast preview's slot, beside the build's: the waiting job, the countdown of saves, a request waiting while a
/// preview runs (only the newest: it previews whatever has changed when it runs).
#[derive(Default)]
pub struct Preview {
    /// The job's process, the promise of its exit, and its output so far.
    pub worker: Option<(JsValue, JsValue, std::rc::Rc<std::cell::RefCell<String>>)>,
    /// It printed FPREADY.
    pub ready: bool,
    pub running: bool,
    pub pending: bool,
    pub timer: Option<f64>,
    /// The precompiled preamble was good at the last full build.
    pub fmt: bool,
    /// The last preview finished with every page current, and no save has come since (the full build can wait).
    pub current: bool,
    /// The quiet period ended while a preview was waiting or running: decide once it has finished.
    pub decide: bool,
    /// The last preview, for the status bar ("preview 0.3 s · citation numbers updating").
    pub last: String,
    /// Previews run (counted for the tests).
    pub count: u32,
}

/// The waiting job stopped (a preamble or glossary change, unloading).
pub fn stop_worker() {
    let old = with(|a| {
        a.preview.ready = false;
        a.preview.worker.take()
    })
    .flatten();
    if let Some((child, exit, _)) = old {
        js::spawn(kill(&child, exit));
    }
}

/// A chapter was saved: the preview starts once no save has come for the preview delay.
pub fn preview_later() {
    with(|a| {
        a.preview.current = false;
        if let Some(t) = a.preview.timer.take() {
            js::clear_timeout(t);
        }
        a.preview.timer = Some(js::timeout(a.settings.preview_delay.max(0.0), preview_now));
    });
}

fn preview_now() {
    let go = with(|a| {
        a.preview.timer = None;
        a.preview.pending |= a.preview.running;
        !std::mem::replace(&mut a.preview.running, true)
    })
    .unwrap_or(false);
    if go {
        js::spawn(run_preview());
    }
}

async fn run_preview() {
    loop {
        let Some((host, o, fmt)) = with(|a| {
            a.report(Report::new("building", "preview"));
            (a.host(None), a.preview_options(), a.preview.fmt)
        }) else {
            return;
        };
        let r = crate::preview::preview(&host, &o, fmt).await;
        if matches!(&r, Ok(x) if x.shown) {
            crate::scroll::publish().await; // the spliced PDF into the pane, in one write
        }
        if with(|a| a.previewed(r)).unwrap_or(false) {
            start(); // the numbers shifted: the full build at once
        }
        if !with(|a| std::mem::take(&mut a.preview.pending)).unwrap_or(false) {
            break;
        }
    }
    with(|a| a.preview.running = false);
    if with(|a| std::mem::take(&mut a.preview.decide)).unwrap_or(false) {
        quiet_ended();
    }
    if let Some((host, fmt)) = with(|a| (a.host(None), a.preview.fmt)) {
        _ = crate::preview::prepare(&host, fmt).await;
    }
}

/// The full build with fast preview on (preview::full_build): on the precompiled preamble, only the steps the change
/// needs (Build only what changed), the output streamed into the build log.
async fn run_fast(gen: u32, out: crate::files::Place, s: crate::app::Settings, dir: String, fs: JsValue, base: Option<String>) {
    let build_log = out.join(&s.build_log).absolute(base.as_deref()).unwrap_or_default();
    let stream = call!(fs, createWriteStream, build_log.as_str());
    let t0 = js::now();
    let Some((o, whole)) = with(|a| {
        a.build.since = t0;
        a.report(Report::new("building", "full"));
        a.set_status(Status::Compiling);
        (a.preview_options(), std::mem::take(&mut a.build.whole) || !s.direct_builds)
    }) else {
        return;
    };
    let host = crate::host::Node { dir, build: Some((gen, stream.clone())) };
    let r = crate::preview::full_build(&host, &o, whole).await;
    let flushed = Promise::new(&mut |resolve, _| _ = call!(stream, end, resolve));
    _ = js::wait(flushed.into()).await;
    if !current(gen) {
        return;
    }
    match r {
        Err(e) if e == "cancelled" => {}
        Err(e) => finish(gen, Err(e)),
        Ok(b) => {
            with(|a| {
                a.preview.fmt = b.fmt;
                if b.code == 0 {
                    a.preview.last.clear();
                }
            });
            let seconds = (js::now() - t0) / 1000.0;
            let main_log = files::read(&out.join(&format!("{}.log", s.main()))).await.unwrap_or_default();
            let place = with(|a| out.shown(&a.vault_name())).unwrap_or_default();
            let mut sum = pipeline::summary(seconds, b.code, &b.failed, &main_log, &b.output, &place, &s.build_log, &s.main());
            sum.text += &format!("\n  steps: {}", b.ran.join(" → "));
            finish(gen, Ok(sum));
        }
    }
}

/// At start-up (fast preview on, this device compiles): the waiting job started on the last full build's files, so the
/// first save after Obsidian opens is previewed at once rather than after the job has loaded (user, 2026-10-27).
pub async fn warm_up() {
    let Some((true, host, o)) = with(|a| (a.settings.fast() && a.can_compile(), a.host(None), a.preview_options())) else { return };
    // the precompiled preamble checked first (made again if the preamble, a package or the engine changed)
    let fmt = crate::preview::ensure_format(&host, &o).await.unwrap_or(false);
    with(|a| a.preview.fmt = fmt);
    if let Err(e) = crate::preview::prepare(&host, fmt).await {
        with(|a| a.log(&format!("fast preview: the preview job did not start ({})", e.lines().rev().find(|l| l.starts_with('!')).unwrap_or("it stopped"))));
    }
}

/// The same, from a timer (inside the app's borrow).
pub fn stop_worker_later() {
    js::timeout(0.0, stop_worker);
}
