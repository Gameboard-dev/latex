//! The output folder's files: the chapters, the log, the backups. A folder in the vault goes through Obsidian's adapter,
//! so it works on every device; an absolute path outside the vault through Node's fs, which only the desktop app has.
//! Every call fails soft: an Err with the reason, for the log.
//!
//! References: <https://docs.obsidian.md/Reference/TypeScript+API/DataAdapter> (exists, read, write, append, remove,
//! mkdir, copy), FileSystemAdapter.getBasePath (desktop), <https://nodejs.org/api/fs.html#promises-api>,
//! <https://nodejs.org/api/path.html>.

use js_sys::Function;
use wasm_bindgen::prelude::*;

use crate::app::with;
use crate::js::{self, call, get, obj};

/// A file or folder: in the vault (a vault path) or on the computer (an absolute path).
#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    Vault(String),
    Disk(String),
}

/// `C:\x`, `\\server\x`, `/x`: a path outside the vault.
pub fn is_absolute(p: &str) -> bool {
    let b = p.as_bytes();
    p.starts_with('/') || p.starts_with("\\\\") || (b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
}

/// A vault path as Obsidian writes them: forward slashes, none at either end.
pub fn vault_path(p: &str) -> String {
    p.replace('\\', "/").split('/').filter(|s| !s.is_empty() && *s != ".").collect::<Vec<_>>().join("/")
}

impl Place {
    pub fn folder(setting: &str) -> Place {
        let s = setting.trim();
        if is_absolute(s) {
            Place::Disk(s.trim_end_matches(['/', '\\']).to_string())
        } else {
            Place::Vault(vault_path(s))
        }
    }

    pub fn join(&self, name: &str) -> Place {
        match self {
            Place::Vault(d) if d.is_empty() => Place::Vault(vault_path(name)),
            Place::Vault(d) => Place::Vault(vault_path(&format!("{d}/{name}"))),
            Place::Disk(d) => Place::Disk(on_disk(d, name)),
        }
    }

    /// Where it is on this computer: `base` (the vault's folder, desktop only) joined to a vault path.
    pub fn absolute(&self, base: Option<&str>) -> Option<String> {
        match self {
            Place::Disk(d) => Some(d.clone()),
            Place::Vault(p) => base.map(|b| on_disk(b, p)),
        }
    }

    /// As the log names it: the vault's name and the vault path (Vault/LaTeX/compiled), or the absolute path.
    pub fn shown(&self, vault: &str) -> String {
        match self {
            Place::Vault(p) if p.is_empty() => vault.to_string(),
            Place::Vault(p) => format!("{vault}/{p}"),
            Place::Disk(d) => d.clone(),
        }
    }
}

/// A path on disk joined to a relative one, in the separator the first already uses (`\` on Windows).
fn on_disk(dir: &str, rel: &str) -> String {
    let sep = if dir.contains('\\') && !dir.contains('/') { '\\' } else { '/' };
    let rel = rel.replace(['/', '\\'], &sep.to_string());
    match rel.trim_start_matches(sep) {
        "" => dir.to_string(),
        r => format!("{}{sep}{r}", dir.trim_end_matches(['/', '\\'])),
    }
}

/// A Node module through `require`; `None` where there is no Node (Obsidian on a phone).
pub fn node_in(require: &Option<Function>, name: &str) -> Option<JsValue> {
    require.as_ref().and_then(|r| r.call1(&JsValue::NULL, &name.into()).ok()).filter(JsValue::is_object)
}

/// The same, outside the app's `with` (inside it, use `App::node`).
pub fn node(name: &str) -> Option<JsValue> {
    with(|a| node_in(&a.require, name)).flatten()
}

fn fsp() -> Result<JsValue, String> {
    node("fs").map(|fs| get(&fs, "promises")).ok_or_else(|| "a folder outside the vault needs the desktop app".to_string())
}

fn adapter() -> Result<JsValue, String> {
    with(|a| a.adapter.clone()).ok_or_else(|| "the plugin is unloaded".to_string())
}

/// What a rejected promise said.
pub fn reason(e: &JsValue) -> String {
    get(e, "message").as_string().or_else(|| e.as_string()).unwrap_or_else(|| "unknown error".into())
}

async fn done(p: JsValue) -> Result<JsValue, String> {
    js::wait(p).await.map_err(|e| reason(&e))
}

pub async fn exists(p: &Place) -> bool {
    match p {
        Place::Vault(v) => match adapter() {
            Ok(ad) => done(call!(ad, exists, v.as_str())).await.is_ok_and(|x| x.is_truthy()),
            Err(_) => false,
        },
        Place::Disk(d) => node("fs").is_some_and(|fs| call!(fs, existsSync, d.as_str()).is_truthy()),
    }
}

/// The text of a file, or None when there is none (or it cannot be read).
pub async fn read(p: &Place) -> Option<String> {
    if !exists(p).await {
        return None;
    }
    let r = match p {
        Place::Vault(v) => done(call!(adapter().ok()?, read, v.as_str())).await,
        Place::Disk(d) => done(call!(fsp().ok()?, readFile, d.as_str(), "utf8")).await,
    };
    r.ok().and_then(|t| t.as_string())
}

pub async fn mkdir(dir: &Place) -> Result<(), String> {
    if exists(dir).await {
        return Ok(());
    }
    match dir {
        Place::Vault(v) => done(call!(adapter()?, mkdir, v.as_str())).await.map(drop),
        Place::Disk(d) => done(call!(fsp()?, mkdir, d.as_str(), obj! { recursive: true })).await.map(drop),
    }
}

fn parent(p: &Place) -> Option<Place> {
    match p {
        Place::Vault(v) => v.rsplit_once('/').map(|(d, _)| Place::Vault(d.to_string())),
        Place::Disk(d) => d.rsplit_once(['/', '\\']).filter(|(d, _)| !d.is_empty() && !d.ends_with(':')).map(|(d, _)| Place::Disk(d.to_string())),
    }
}

pub async fn write(p: &Place, text: &str) -> Result<(), String> {
    if let Some(dir) = parent(p) {
        mkdir(&dir).await?;
    }
    match p {
        Place::Vault(v) => done(call!(adapter()?, write, v.as_str(), text)).await.map(drop),
        Place::Disk(d) => done(call!(fsp()?, writeFile, d.as_str(), text, "utf8")).await.map(drop),
    }
}

pub async fn append(p: &Place, text: &str) -> Result<(), String> {
    if !exists(p).await {
        return write(p, text).await;
    }
    match p {
        Place::Vault(v) => done(call!(adapter()?, append, v.as_str(), text)).await.map(drop),
        Place::Disk(d) => done(call!(fsp()?, appendFile, d.as_str(), text, "utf8")).await.map(drop),
    }
}

pub async fn copy(from: &Place, to: &Place) -> Result<(), String> {
    if let Some(dir) = parent(to) {
        mkdir(&dir).await?;
    }
    match (from, to) {
        (Place::Vault(a), Place::Vault(b)) => done(call!(adapter()?, copy, a.as_str(), b.as_str())).await.map(drop),
        (Place::Disk(a), Place::Disk(b)) => done(call!(fsp()?, copyFile, a.as_str(), b.as_str())).await.map(drop),
        _ => Err("cannot copy between the vault and a folder outside it".into()),
    }
}

/// Deleted if it is there; a file that is not there is no error.
pub async fn remove(p: &Place) {
    if exists(p).await {
        let _ = match p {
            Place::Vault(v) => match adapter() {
                Ok(ad) => done(call!(ad, remove, v.as_str())).await,
                Err(e) => Err(e),
            },
            Place::Disk(d) => match fsp() {
                Ok(f) => done(call!(f, unlink, d.as_str())).await,
                Err(e) => Err(e),
            },
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places() {
        assert!(is_absolute("C:\\x") && is_absolute("d:/x") && is_absolute("/home/x") && is_absolute("\\\\srv\\x"));
        assert!(!is_absolute("LaTeX/compiled") && !is_absolute("C:x"));
        assert_eq!(Place::folder(" /LaTeX//compiled/ "), Place::Disk("/LaTeX//compiled".into()));
        assert_eq!(Place::folder("LaTeX\\compiled/"), Place::Vault("LaTeX/compiled".into()));
        assert_eq!(Place::folder("LaTeX/compiled").join("main.log"), Place::Vault("LaTeX/compiled/main.log".into()));
        assert_eq!(Place::folder("").join("x.tex"), Place::Vault("x.tex".into()));
        assert_eq!(Place::folder("LaTeX/compiled").shown("Vault"), "Vault/LaTeX/compiled");
        assert_eq!(Place::folder("C:\\Thesis\\out").join("code/x.tex"), Place::Disk("C:\\Thesis\\out\\code\\x.tex".into()));
        assert_eq!(Place::folder("/home/me/out/").join("main.log"), Place::Disk("/home/me/out/main.log".into()));
        assert_eq!(Place::folder("LaTeX/compiled").absolute(Some("C:\\Vault")).unwrap(), "C:\\Vault\\LaTeX\\compiled");
        assert_eq!(Place::folder("LaTeX/compiled").absolute(None), None);
        assert_eq!(parent(&Place::Disk("C:\\a\\b.tex".into())), Some(Place::Disk("C:\\a".into())));
    }
}
