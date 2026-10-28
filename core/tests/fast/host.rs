//! The fast preview's steps (latex_core::preview) on this computer: std's files and processes for preview::Host, the
//! plugin's conversion of a folder of notes, and the full build, as the plugin runs them. For the correctness suite
//! (tests/fast_preview.rs).

#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::process::{Child, Command, Stdio};
use std::task::{Context, Poll, Waker};
use std::time::Instant;

use latex_core::convert::{md_to_tex, Index, Options as ConvertOptions};
use latex_core::preview::{self, Host, Options};

/// Runs a future whose steps never wait (the host's calls are blocking).
pub fn block_on<F: Future>(f: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());
    let mut f = pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

pub struct Native {
    pub dir: PathBuf,
    worker: RefCell<Option<Child>>,
    pub log: RefCell<Vec<String>>,
    t0: Instant,
}

fn below_normal(c: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0000_4000 | 0x0800_0000); // BELOW_NORMAL_PRIORITY_CLASS, CREATE_NO_WINDOW
    }
    c
}

impl Native {
    pub fn new(dir: &Path) -> Native {
        Native { dir: dir.to_path_buf(), worker: RefCell::new(None), log: RefCell::new(Vec::new()), t0: Instant::now() }
    }

    fn path(&self, name: &str) -> PathBuf {
        let p = Path::new(name);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.dir.join(name)
        }
    }

    pub fn text(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).unwrap_or_default()
    }
}

impl Host for Native {
    async fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(name)).ok()
    }

    async fn write(&self, name: &str, data: &[u8]) -> Result<(), String> {
        let p = self.path(name);
        if let Some(d) = p.parent() {
            _ = std::fs::create_dir_all(d);
        }
        std::fs::write(p, data).map_err(|e| e.to_string())
    }

    async fn stat(&self, path: &str) -> Option<(f64, f64)> {
        let m = std::fs::metadata(self.path(path)).ok()?;
        let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as f64;
        Some((t, m.len() as f64))
    }

    async fn run(&self, cmd: &[String]) -> Result<(i64, String), String> {
        let out = below_normal(Command::new(&cmd[0]).args(&cmd[1..]).current_dir(&self.dir).stdin(Stdio::null())).output().map_err(|e| format!("{}: {e}", cmd[0]))?;
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text += &String::from_utf8_lossy(&out.stderr);
        Ok((out.status.code().unwrap_or(-1) as i64, text))
    }

    async fn start_worker(&self, cmd: &[String]) -> Result<(), String> {
        self.stop_worker();
        let mut child = below_normal(Command::new(&cmd[0]).args(&cmd[1..]).current_dir(&self.dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())).spawn().map_err(|e| e.to_string())?;
        let mut so = child.stdout.take().unwrap();
        let mut seen = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = so.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                _ = child.wait();
                return Err(String::from_utf8_lossy(&seen).into_owned());
            }
            seen.extend_from_slice(&buf[..n]);
            if seen.windows(7).any(|w| w == b"FPREADY") {
                break;
            }
        }
        child.stdout = Some(so);
        *self.worker.borrow_mut() = Some(child);
        Ok(())
    }

    fn worker_waiting(&self) -> bool {
        self.worker.borrow().is_some()
    }

    async fn request(&self, line: &str) -> Result<(i64, String), String> {
        let mut child = self.worker.borrow_mut().take().ok_or("no worker")?;
        let mut si = child.stdin.take().unwrap();
        si.write_all(format!("{line}\n").as_bytes()).map_err(|e| e.to_string())?;
        drop(si);
        let mut out = String::new();
        _ = child.stdout.take().unwrap().read_to_string(&mut out);
        let code = child.wait().map_err(|e| e.to_string())?.code().unwrap_or(-1);
        Ok((code as i64, out))
    }

    fn stop_worker(&self) {
        if let Some(mut c) = self.worker.borrow_mut().take() {
            _ = c.kill();
            _ = c.wait();
        }
    }

    fn log(&self, line: &str) {
        self.log.borrow_mut().push(line.to_string());
    }

    fn now(&self) -> f64 {
        self.t0.elapsed().as_secs_f64() * 1000.0
    }
}

/// The notes of a folder converted into its compiled/ folder, as the plugin converts them (Build all chapters).
/// Returns each note's .tex name and text.
pub fn convert_all(notes: &Path) -> BTreeMap<String, String> {
    let o = ConvertOptions::default();
    let mut texts: Vec<(String, String)> = std::fs::read_dir(notes).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".md")).map(|n| (n.clone(), std::fs::read_to_string(notes.join(&n)).unwrap())).collect();
    texts.sort();
    let idx = Index::new(texts.iter().map(|(n, t)| (n.as_str(), t.as_str())), &o);
    let mut out = BTreeMap::new();
    for (n, md) in &texts {
        let (tex, _) = md_to_tex(n, md, &idx, &o).unwrap();
        let name = latex_core::pipeline::tex_name(n);
        std::fs::write(notes.join("compiled").join(&name), &tex).unwrap();
        out.insert(name, tex);
    }
    out
}

//// The full build as the plugin runs it with fast preview on (preview::full_build): seconds, or Err with its first
/// errors.
pub fn full_build(h: &Native, o: &Options, whole: bool) -> Result<f64, String> {
    let t = Instant::now();
    let b = block_on(preview::full_build(h, o, whole))?;
    if b.code != 0 {
        return Err(format!("exit {}: {}", b.code, b.output.lines().filter(|l| l.starts_with('!') || l.contains("rror")).take(5).collect::<Vec<_>>().join(" / ")));
    }
    Ok(t.elapsed().as_secs_f64())
}

// A copy of a folder (the fixture into a scratch folder).
pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().filter_map(|e| e.ok()) {
        let p = e.path();
        let q = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &q);
        } else {
            std::fs::copy(&p, &q).unwrap();
        }
    }
}
