//! LaTeX: an Obsidian plugin in Rust. main.js (the only JavaScript, a few lines) loads this module and calls `start`
//! (app). Each note in the notes folder becomes its LaTeX chapter by fixed rules (convert) whenever it is saved, and the
//! document is compiled once the notes have been quiet for a while (compile, pipeline), with every step in a log and the
//! status bar; the chapters and logs may be in the vault or anywhere on the computer (files). The compiled PDF is shown
//! beside the chapter notes and scrolled with them (scroll), through the build's SyncTeX file (synctex).

pub use obsidian_kit::js; // shared with the other plugins (../../shared)

mod app;
mod compile;
mod device;
pub mod convert;
mod files;
mod ignis;
mod host;
pub mod order;
pub mod pdf;
pub mod pipeline;
pub mod preview;
mod remote;
mod scroll;
pub mod splice;
pub mod starter;
pub mod synctex;
pub mod wordcount;
