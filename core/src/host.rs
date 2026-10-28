//! preview::Host on Obsidian's desktop app: the output folder's files through Node's fs, programs through
//! child_process at below-normal priority, and the waiting preview job (compile.rs's preview slot holds it). A full
//! build's programs stream their output into the build log and the status bar, and stop when the build is cancelled.
//!
//! References: <https://nodejs.org/api/fs.html#promises-api>, <https://nodejs.org/api/child_process.html#child_processspawncommand-args-options>.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Array, Promise, Uint8Array};
use wasm_bindgen::prelude::*;

use crate::app::with;
use crate::compile;
use crate::files::{self, node};
use crate::js::{self, call, f1, get, obj, str};
use crate::preview::Host;

/// The host of a preview (`build`: None) or of the full build of generation `build` (its output goes to `stream`).
pub struct Node {
    pub dir: String,
    pub build: Option<(u32, JsValue)>,
}

impl Node {
    fn path(&self, name: &str) -> String {
        if files::is_absolute(name) {
            name.to_string()
        } else {
            let sep = if self.dir.contains('\\') { "\\" } else { "/" };
            format!("{}{sep}{}", self.dir.trim_end_matches(['/', '\\']), name.replace(['/', '\\'], sep))
        }
    }
}

fn fsp() -> Option<JsValue> {
    node("fs").map(|fs| get(&fs, "promises"))
}

/// A child process started in `dir`, below normal priority, its output piped.
fn spawn(cmd: &[String], dir: &str, stdin: &str) -> Result<JsValue, String> {
    let cp = node("child_process").ok_or("compiling needs the desktop app")?;
    let args: Array = cmd[1..].iter().map(|x| JsValue::from(x.as_str())).collect();
    let stdio: Array = [stdin, "pipe", "pipe"].iter().map(|x| JsValue::from(*x)).collect();
    let child = call!(cp, spawn, cmd[0].as_str(), args, obj! { cwd: dir, windowsHide: true, detached: !compile::windows(), stdio: stdio, env: compile::tex_env() });
    if let (Some(os), Some(pid)) = (node("os"), get(&child, "pid").as_f64()) {
        call!(os, setPriority, pid, get(&get(&get(&os, "constants"), "priority"), "PRIORITY_BELOW_NORMAL"));
    }
    Ok(child)
}

/// The child's output collected (and passed to `each` as it streams).
fn collect(child: &JsValue, out: Rc<RefCell<String>>, each: impl Fn(&str) + Clone + 'static) {
    for pipe in ["stdout", "stderr"] {
        let p = get(child, pipe);
        if p.is_null() || p.is_undefined() {
            continue;
        }
        call!(p, setEncoding, "utf8");
        let (out, each) = (out.clone(), each.clone());
        call!(p, on, "data", f1(move |d| {
            let s = str(&d);
            out.borrow_mut().push_str(&s);
            each(&s);
            JsValue::UNDEFINED
        }));
    }
}

impl Host for Node {
    async fn read(&self, name: &str) -> Option<Vec<u8>> {
        let b = js::wait(call!(fsp()?, readFile, self.path(name))).await.ok()?;
        Some(Uint8Array::new(&b).to_vec())
    }

    async fn write(&self, name: &str, data: &[u8]) -> Result<(), String> {
        let fs = fsp().ok_or("no file system")?;
        let p = self.path(name);
        if let Some((dir, _)) = p.rsplit_once(['/', '\\']) {
            _ = js::wait(call!(fs, mkdir, dir, obj! { recursive: true })).await;
        }
        // a PDF is written beside and renamed, so that the viewer never reads half of one
        let tmp = if name.ends_with(".pdf") { format!("{p}.part") } else { p.clone() };
        js::wait(call!(fs, writeFile, tmp.as_str(), Uint8Array::from(data))).await.map_err(|e| files::reason(&e))?;
        if tmp != p {
            js::wait(call!(fs, rename, tmp.as_str(), p.as_str())).await.map_err(|e| files::reason(&e))?;
        }
        Ok(())
    }

    async fn stat(&self, path: &str) -> Option<(f64, f64)> {
        let s = js::wait(call!(fsp()?, stat, self.path(path))).await.ok()?;
        Some((get(&s, "mtimeMs").as_f64()?, get(&s, "size").as_f64()?))
    }

    async fn run(&self, cmd: &[String]) -> Result<(i64, String), String> {
        let child = spawn(cmd, &self.dir, "ignore")?;
        let exit = compile::exit_of(&child);
        let out = Rc::new(RefCell::new(String::new()));
        match &self.build {
            Some((gen, stream)) => {
                let (gen, stream) = (*gen, stream.clone());
                let program = crate::pipeline::program(&cmd[0]);
                with(|a| {
                    a.build.stage.start(&program, 1, 1);
                    a.build.child = Some((child.clone(), exit.clone()));
                    a.show();
                });
                collect(&child, out.clone(), move |s| {
                    call!(stream, write, s);
                    with(|a| {
                        if a.build.generation == gen && a.build.stage.feed(s) {
                            a.show();
                        }
                    });
                });
            }
            None => collect(&child, out.clone(), |_| {}),
        }
        let end = js::wait(exit).await.unwrap_or(JsValue::NULL);
        if let Some((gen, _)) = &self.build {
            if with(|a| a.build.generation != *gen).unwrap_or(true) {
                return Err("cancelled".into());
            }
        }
        let text = out.borrow().clone();
        match end.as_f64() {
            Some(c) => Ok((c as i64, text)),
            None if end.is_object() => Err(format!("could not start {}: {}", cmd[0], files::reason(&end))),
            None => Ok((-1, text)),
        }
    }

    async fn start_worker(&self, cmd: &[String]) -> Result<(), String> {
        self.stop_worker();
        let child = spawn(cmd, &self.dir, "pipe")?;
        let exit = compile::exit_of(&child);
        let out = Rc::new(RefCell::new(String::new()));
        // ready when it prints FPREADY, or ended
        let ready = Promise::new(&mut |resolve, _| {
            let r = resolve.clone();
            let seen = out.clone();
            collect(&child, out.clone(), move |_| {
                if seen.borrow().contains("FPREADY") {
                    _ = r.call1(&JsValue::NULL, &JsValue::TRUE);
                }
            });
            let e = exit.clone();
            js::spawn(async move {
                _ = js::wait(e).await;
                _ = resolve.call1(&JsValue::NULL, &JsValue::FALSE);
            });
        });
        with(|a| a.preview.worker = Some((child.clone(), exit.clone(), out.clone())));
        let ok = js::wait(ready.into()).await.map(|v| v.is_truthy()).unwrap_or(false);
        if !ok {
            // only if it is still this job's slot: a job stopped because a newer one replaced it must not clear the newer
            with(|a| {
                if a.preview.worker.as_ref().is_some_and(|(c, _, _)| *c == child) {
                    a.preview.worker = None;
                }
            });
            return Err(out.borrow().clone());
        }
        with(|a| a.preview.ready = true);
        Ok(())
    }

    fn worker_waiting(&self) -> bool {
        with(|a| a.preview.ready && a.preview.worker.is_some()).unwrap_or(false)
    }

    async fn request(&self, line: &str) -> Result<(i64, String), String> {
        let (child, exit, out) = with(|a| {
            a.preview.ready = false;
            a.preview.worker.take()
        })
        .flatten()
        .ok_or("no preview job is waiting")?;
        let stdin = get(&child, "stdin");
        call!(stdin, write, format!("{line}\n"));
        call!(stdin, end);
        let end = js::wait(exit).await.unwrap_or(JsValue::NULL);
        let text = out.borrow().clone();
        Ok((end.as_f64().map_or(-1, |c| c as i64), text))
    }

    fn stop_worker(&self) {
        compile::stop_worker();
    }

    fn log(&self, line: &str) {
        let line = line.to_string();
        js::timeout(0.0, move || _ = with(|a| a.log(&line)));
    }

    fn now(&self) -> f64 {
        js::now()
    }
}
