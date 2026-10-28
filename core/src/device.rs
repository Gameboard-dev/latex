//! This device's commands (user, 2026-10-27). The compile and highlight commands are programs this computer runs, so
//! they are kept in this device's storage (Obsidian's local storage), never in data.json: the vault shares that with
//! every device and with Ignis in a browser, which could otherwise choose what desktop Obsidian runs. A command typed
//! in an older version's settings moves here once, on desktop. Each step's program must be a TeX tool (or Python
//! running Pygments) unless this device allows other programs.
//!
//! What stays outside this check: latexmk reads a latexmkrc (Perl) from the folder it runs in, and the default command
//! passes it Perl with -e, as LaTeX Workshop's recipe does; a TeX run with --shell-escape runs commands. Those are
//! TeX's own, and the plugin never adds --shell-escape.

use serde::{Deserialize, Serialize};

/// The key in Obsidian's local storage (per vault and device).
pub const KEY: &str = "latex-device";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Device {
    /// Empty: the default for the document's engine (pipeline::default_command).
    pub compile_command: String,
    /// Empty: the default (DEFAULT_HIGHLIGHT).
    pub highlight_command: String,
    /// Not used since 2026-10-28 (user): a command may run only TeX tools and Python running Pygments, always; kept
    /// so that a device's saved settings still read.
    pub allow_other_programs: bool,
    /// A folder of TeX programs, searched after PATH (user, 2026-10-27): for a TeX installed after Obsidian started,
    /// which Obsidian's PATH does not have yet. Empty: the usual places (`usual_folders`) that exist.
    pub tex_folder: String,
}

/// Where TeX distributions put their programs, by platform (`home`, `appdata`, `localappdata`: the user's folders):
/// TinyTeX, TeX Live, MiKTeX, MacTeX.
pub fn usual_folders(windows: bool, home: &str, appdata: &str, localappdata: &str) -> Vec<String> {
    if windows {
        let mut v = vec![format!("{appdata}\\TinyTeX\\bin\\windows"), format!("{localappdata}\\Programs\\MiKTeX\\miktex\\bin\\x64"), "C:\\Program Files\\MiKTeX\\miktex\\bin\\x64".into()];
        v.extend((2022..=2030).rev().map(|y| format!("C:\\texlive\\{y}\\bin\\windows")));
        v
    } else {
        let mut v = vec![format!("{home}/.TinyTeX/bin/x86_64-linux"), format!("{home}/Library/TinyTeX/bin/universal-darwin"), "/Library/TeX/texbin".into()];
        v.extend((2022..=2030).rev().map(|y| format!("/usr/local/texlive/{y}/bin/x86_64-linux")));
        v
    }
}

/// The PATH a TeX program runs with: this one, then the TeX folders (`extra`: the setting's folder, or the usual ones
/// that exist), each once.
pub fn with_tex_path(path: &str, extra: &[String], sep: char) -> String {
    let mut parts: Vec<String> = path.split(sep).filter(|p| !p.is_empty()).map(str::to_string).collect();
    for e in extra {
        let e = e.trim();
        if !e.is_empty() && !parts.iter().any(|p| p.eq_ignore_ascii_case(e)) {
            parts.push(e.to_string());
        }
    }
    parts.join(&sep.to_string())
}

pub const DEFAULT_HIGHLIGHT: &str = "python -m pygments -l %LANG% -f latex -o %OUT% %SRC%";

/// The programs a command may run (the only ones, since 2026-10-28): TeX engines and drivers, and the bibliography,
/// glossary and index tools a TeX build calls (MiKTeX's and TeX Live's names).
pub const TOOLS: &[&str] = &[
    "latexmk", "lualatex", "luatex", "luahbtex", "pdflatex", "pdftex", "xelatex", "xetex", "latex", "tex", "biber", "bibtex",
    "bibtex8", "bibtexu", "makeglossaries", "makeglossaries-lite", "makeindex", "xindy", "texindy", "dvipdfmx", "dvipdfm",
    "dvips", "ps2pdf", "pygmentize",
];

impl Device {
    /// This device's compile command, or the default for `engine` (the Engine setting).
    pub fn compile_command(&self, engine: &str) -> String {
        if self.compile_command.trim().is_empty() { crate::pipeline::default_command(engine) } else { self.compile_command.clone() }
    }

    pub fn highlight_command(&self) -> &str {
        if self.highlight_command.trim().is_empty() { DEFAULT_HIGHLIGHT } else { &self.highlight_command }
    }

    /// Whether a step (program and arguments) may run on this device, and why not.
    pub fn allows(&self, step: &[String]) -> Result<(), String> {
        let Some(first) = step.first() else { return Err("an empty command".into()) };
        let name = crate::pipeline::program(first).to_lowercase();
        let python = ["python", "python3", "py"].contains(&name.as_str());
        if TOOLS.contains(&name.as_str()) || (python && step.get(1).map(String::as_str) == Some("-m") && step.get(2).map(String::as_str) == Some("pygments")) {
            return Ok(());
        }
        Err(format!("{first} is not a TeX tool (latexmk, lualatex, biber, makeglossaries …) or Python running Pygments, the only programs the plugin runs"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_tex_tools_unless_allowed() {
        let d = Device::default();
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(d.allows(&s(&["C:\\texlive\\bin\\windows\\latexmk.exe", "-lualatex", "main"])).is_ok());
        assert!(d.allows(&s(&["python", "-m", "pygments", "-l", "rust"])).is_ok());
        assert!(d.allows(&s(&["python", "-c", "import os"])).is_err());
        assert!(d.allows(&s(&["cmd.exe", "/c", "calc"])).is_err());
        assert!(Device { allow_other_programs: true, ..d.clone() }.allows(&s(&["make"])).is_err());
        // the TeX folders come after PATH, each once
        assert_eq!(with_tex_path("C:\\a;C:\\b", &["C:\\tex".into(), "c:\\A".into(), "".into()], ';'), "C:\\a;C:\\b;C:\\tex");
        assert!(usual_folders(true, "", "C:\\Users\\u\\AppData\\Roaming", "")[0].ends_with("TinyTeX\\bin\\windows"));
        assert_eq!(d.compile_command("lualatex"), crate::pipeline::DEFAULT_COMMAND);
        for (engine, _, flag) in crate::pipeline::TEX_ENGINES {
            let steps = crate::pipeline::steps(&d.compile_command(engine), "C:\\out", "main", "\\").unwrap();
            assert!(steps.iter().all(|st| d.allows(st).is_ok()), "the default command passes");
            assert_eq!(steps[0][1], flag, "{engine}");
            assert!(crate::pipeline::is_default_command(&d.compile_command(engine)));
        }
        assert!(!crate::pipeline::is_default_command("latexmk -pdf main"));
    }
}
