//! Based on the `dialoguer` crate, this module provides a simple way to open an editor for the user to input text.
//!
//! The MIT License (MIT)
//!
//! Copyright (c) 2017 Armin Ronacher <armin.ronacher@active-4.com>
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

use std::{
    env,
    ffi::OsString,
    fs,
    io::{Read, Write},
    process,
};
use which::which;

/// Launches the default editor to edit a string.
///
/// ## Example
///
/// ```rust,no_run
/// use dialoguer::Editor;
///
/// if let Some(rv) = Editor::new().edit("Enter a commit message").unwrap() {
///     println!("Your message:");
///     println!("{}", rv);
/// } else {
///     println!("Abort!");
/// }
/// ```
pub struct Editor {
    editor: OsString,
    extension: String,
    require_save: bool,
    trim_newlines: bool,
}

/// Launches the default editor to edit a string.
fn get_default_editor() -> OsString {
    if let Some(prog) = env::var_os("VISUAL") {
        return prog;
    }
    if let Some(prog) = env::var_os("EDITOR") {
        return prog;
    }
    if let Ok(editor_path) = which("editor") {
        return editor_path.into_os_string();
    }
    if let Ok(editor_path) = which("sensible-editor") {
        return editor_path.into_os_string();
    }
    if cfg!(windows) {
        "notepad.exe".into()
    } else {
        "vi".into()
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    /// Creates a new editor.
    pub fn new() -> Self {
        Self {
            editor: get_default_editor(),
            extension: ".txt".into(),
            require_save: true,
            trim_newlines: true,
        }
    }

    /// Launches the editor to edit a string.
    ///
    /// Returns `None` if the file was not saved or otherwise the
    /// entered text.
    pub fn edit(&self, s: &str) -> anyhow::Result<Option<String>> {
        let mut f = tempfile::Builder::new()
            .prefix("edit-")
            .suffix(&self.extension)
            .rand_bytes(12)
            .tempfile()?;
        f.write_all(s.as_bytes())?;
        f.flush()?;
        let ts = fs::metadata(f.path())?.modified()?;

        let s: String = self.editor.clone().into_string().unwrap();
        // Use shlex instead of shell_words.
        let (cmd, args) = match shlex::split(&s) {
            Some(mut parts) if !parts.is_empty() => {
                let cmd = parts.remove(0);
                (cmd, parts)
            }
            _ => (s, vec![]),
        };

        let rv = process::Command::new(cmd)
            .args(args)
            .arg(f.path())
            .spawn()?
            .wait()?;

        if rv.success() && self.require_save && ts >= fs::metadata(f.path())?.modified()? {
            return Ok(None);
        }

        let mut new_f = fs::File::open(f.path())?;
        let mut rv = String::new();
        new_f.read_to_string(&mut rv)?;

        if self.trim_newlines {
            let len = rv.trim_end_matches(&['\n', '\r'][..]).len();
            rv.truncate(len);
        }

        Ok(Some(rv))
    }
}
