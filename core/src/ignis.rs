//! Building through Ignis (user, 2026-10-28): an optional extra, off by default (Settings, Compiling, "Build through
//! Ignis"). When the vault is opened in a browser through Ignis (a self-hosted Obsidian server) and Obsidian is not
//! open on the computer that runs it, nothing on that computer would build the document after a save. With this on,
//! the plugin in the browser converts the notes as the desktop app would and asks Ignis to build: a POST to
//! `<page origin>/relay/latex/compile`. Ignis's launcher builds with one fixed command in a sandbox of its own and
//! only for a signed-in Cloudflare Access user (ignis/SETUP.md, "The LaTeX build"); the plugin then follows
//! `/relay/latex/status` until the build ends, logs its result and shows the PDF. Nothing here runs a program.

use serde_json::Value;

use crate::app::{with, Status};
use crate::js::{self, call, get, obj, str};

/// One request to the relay on the page's own origin: its HTTP status and JSON body.
async fn request(method: &str, path: &str) -> Result<(u16, Value), String> {
    let init = obj! { method: method, credentials: "same-origin" };
    let resp = js::wait(call!(js::global(), fetch, format!("/relay/latex/{path}"), init)).await.map_err(|e| str(&get(&e, "message")))?;
    let code = get(&resp, "status").as_f64().unwrap_or(0.0) as u16;
    let body = js::wait(call!(resp, json)).await.ok().and_then(|v| serde_wasm_bindgen::from_value::<Value>(v).ok()).unwrap_or(Value::Null);
    Ok((code, body))
}

/// Whether builds go to Ignis now: this page in Ignis (a browser, which cannot compile; detected, no setting), and Ignis
/// ready to build, with Obsidian closed on the computer (it builds the document itself when open). Kept in the app.
pub async fn check() -> bool {
    let wanted = js::in_ignis() && !with(|a| a.can_compile()).unwrap_or(false);
    let ok = wanted
        && match request("GET", "status").await {
            Ok((200, v)) => v["available"] == true && v["desktop_open"] != true,
            Ok((code, v)) => {
                with(|a| a.log(&format!("Ignis build: not available (HTTP {code}: {})", v["error"].as_str().unwrap_or("no reason given"))));
                false
            }
            Err(e) => {
                with(|a| a.log(&format!("Ignis build: the relay did not answer ({e})")));
                false
            }
        };
    with(|a| a.ignis_ok = ok);
    ok
}

/// A build asked of Ignis, followed to its end.
pub async fn build() {
    with(|a| {
        a.build.running = true;
        a.set_status(Status::Compiling);
        a.log("Ignis build: asked for (the computer builds the document in its sandbox)");
    });
    let started = match request("POST", "compile").await {
        Ok((202, _)) => true,
        Ok((code, v)) => {
            let why = v["error"].as_str().unwrap_or("refused").to_string();
            with(|a| {
                a.log(&format!("Ignis build: not started (HTTP {code}: {why})"));
                a.notice(&format!("LaTeX: Ignis did not build: {why}"));
            });
            false
        }
        Err(e) => {
            with(|a| a.log(&format!("Ignis build: the relay did not answer ({e})")));
            false
        }
    };
    if !started {
        with(|a| {
            a.build.running = false;
            a.set_status(Status::Failed);
        });
        return;
    }
    // followed every three seconds until it ends (a build takes about half a minute)
    let last = loop {
        js::sleep(3000.0).await;
        match request("GET", "status").await {
            Ok((200, v)) if v["running"] != true => break v["last"].clone(),
            Ok((200, _)) => continue,
            Ok((code, v)) => break serde_json::json!({ "ok": false, "errors": [format!("HTTP {code}: {}", v["error"].as_str().unwrap_or(""))] }),
            Err(e) => break serde_json::json!({ "ok": false, "errors": [e] }),
        }
    };
    let errors: Vec<String> = last["errors"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
    let undefined = last["undefined"].as_u64().unwrap_or(0) as usize;
    let ok = last["ok"] == true;
    with(|a| {
        a.build.running = false;
        let seconds = last["seconds"].as_f64().unwrap_or(0.0);
        a.log(&format!("Ignis build: {} in {seconds} s{}{}", if ok { "built" } else { "FAILED" },
            if undefined > 0 { format!("; {undefined} undefined references or citations") } else { String::new() },
            errors.iter().map(|e| format!("\n  {e}")).collect::<String>()));
        a.set_status(if ok { Status::Compiled { at: crate::app::clock(), errors: errors.len(), undefined } } else { Status::Failed });
    });
    if ok {
        crate::scroll::built().await;
    }
}
