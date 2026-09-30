//! A crate's hooks on the JS rust-js writes (ADR 0117): transforms, which
//! change each module's text, the map moved to it, and checks, which read
//! what a build wrote, what they say of the JS said of the Rust.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::format::Map;
use crate::settings::{Fail, Settings, When};

/// `code` and its `map`, a module's, which will be `js_path`, through each
/// of the crate's transforms, in order. What one gives is kept only if it's
/// the same program: otherwise, or if it fails, a warning says why, and the
/// next is given what it was given.
pub fn transform(
    settings: &Settings,
    js_path: &Path,
    jsx: bool,
    js_file_name: &str,
    code: String,
    map: String,
) -> (String, String) {
    let (mut code, mut map) = (code, map);
    for command in &settings.hooks.transform {
        let given = run_transform(settings, command, js_path, &code)
            .and_then(|text| crate::format::transformed(&code, text, &map, jsx, js_file_name));
        match given {
            Ok((text, moved)) => (code, map) = (text, moved),
            Err(why) => eprintln!(
                "warning: rust-js: the transform `{}` {why}: `{}` is written as it was before it",
                name(command),
                js_path.display()
            ),
        }
    }
    (code, map)
}

/// What `command` writes given `code`, `{file}` in it `js_path`.
fn run_transform(settings: &Settings, command: &[String], js_path: &Path, code: &str) -> Result<String, String> {
    let file = js_path.display().to_string();
    let args: Vec<String> = command.iter().map(|arg| arg.replace("{file}", &file)).collect();
    let (program, args) = args.split_first().ok_or("is no command")?;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(&settings.dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("can't be run: {e}"))?;
    let mut stdin = child.stdin.take().expect("a piped stdin");
    // Written as it's read, so neither side waits on a full pipe.
    let output = std::thread::scope(|scope| {
        scope.spawn(move || stdin.write_all(code.as_bytes()));
        child.wait_with_output()
    })
    .map_err(|e| format!("can't be run: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "exited {}",
            output
                .status
                .code()
                .map_or("by a signal".to_string(), |code| code.to_string())
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| "gave what isn't UTF-8".to_string())
}

/// Run the crate's checks of `written`, the JS files a build wrote, or,
/// for a `save`, those not only for a build. What each says is printed, of
/// the Rust where it names the JS; an error if one that fails the build did.
pub fn check(settings: &Settings, written: &[PathBuf], save: bool) -> Result<(), String> {
    let mut maps = Maps::default();
    let mut failed = Vec::new();
    for check in &settings.hooks.check {
        if save && check.when == When::Build {
            continue;
        }
        let tool = name(&check.run);
        let args: Vec<String> = check
            .run
            .iter()
            .flat_map(|arg| match arg.as_str() {
                "{files}" => written.iter().map(|file| file.display().to_string()).collect(),
                _ => vec![arg.clone()],
            })
            .collect();
        let Some((program, args)) = args.split_first() else {
            failed.push(format!("the check `{tool}` is no command"));
            continue;
        };
        let output = Command::new(program)
            .args(args)
            .current_dir(&settings.dir)
            .stdin(Stdio::null())
            .output();
        let output = match output {
            Ok(output) => output,
            Err(e) => {
                failed.push(format!("the check `{tool}` can't be run: {e}"));
                continue;
            }
        };
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let said = said_of_rust(settings, written, &mut maps, &said, &tool);
        if !output.status.success() && check.fail == Fail::Build {
            failed.push(format!("the check `{tool}` failed:\n{}", said.trim_end()));
        } else {
            eprint!("{said}");
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("\n"))
    }
}

/// A command's name, as what's said of it says: its program's file name.
fn name(command: &[String]) -> String {
    command
        .first()
        .map(|program| {
            Path::new(program)
                .file_name()
                .map_or(program.clone(), |name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
}

/// Each file's source map, read once.
#[derive(Default)]
struct Maps(HashMap<PathBuf, Option<Map>>);

/// `text`, a check's output: each line that names a place in a file the
/// build wrote, at JS some Rust wrote, said of the Rust, and named for
/// `tool`. Any other line is as it is.
fn said_of_rust(settings: &Settings, written: &[PathBuf], maps: &mut Maps, text: &str, tool: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let rust = place(line).and_then(|(file, line, col, rest)| {
            let file = crate::output::absolute(&settings.dir.join(file)).ok()?;
            written.contains(&file).then_some(())?;
            let (path, line, col) = rust_of(settings, maps, &file, line, col)?;
            Some(format!("{path}:{line}:{col}: [{tool}] {rest}"))
        });
        out.push_str(rust.as_deref().unwrap_or(line));
        out.push('\n');
    }
    out
}

/// The file, line and column a line of a tool's output names, 1-based, and
/// what it says there: `file:line:col: ..`, as ESLint's and oxlint's
/// `unix` format has it, or `file(line,col): ..`, as `tsc` does.
fn place(line: &str) -> Option<(&str, u32, u32, &str)> {
    if let Some(open) = line.find('(')
        && let Some(close) = line[open..].find("):").map(|i| open + i)
        && let Some((l, c)) = line[open + 1..close].split_once(',')
        && let (Ok(l), Ok(c)) = (l.trim().parse(), c.trim().parse())
    {
        return Some((&line[..open], l, c, line[close + 2..].trim_start()));
    }
    for (i, _) in line.match_indices(':') {
        let mut parts = line[i + 1..].splitn(3, ':');
        if let (Some(l), Some(c), Some(rest)) = (parts.next(), parts.next(), parts.next())
            && let (Ok(l), Ok(c)) = (l.parse(), c.parse())
        {
            return Some((&line[..i], l, c, rest.trim_start()));
        }
    }
    None
}

/// Where JS `file`'s `line` and `col`, 1-based, came from in the Rust, by
/// its map: the Rust's path, from the crate's directory if it's in it, and
/// its line and column, 1-based.
fn rust_of(settings: &Settings, maps: &mut Maps, file: &Path, line: u32, col: u32) -> Option<(String, u32, u32)> {
    let map = maps
        .0
        .entry(file.to_path_buf())
        .or_insert_with(|| {
            let text = std::fs::read_to_string(format!("{}.map", file.display())).ok()?;
            Map::read(&text)
        })
        .as_ref()?;
    let (source, line, col) = map.original(line.checked_sub(1)?, col.checked_sub(1)?)?;
    let rust = crate::output::absolute(&file.parent()?.join(source)).ok()?;
    let shown = rust
        .strip_prefix(crate::output::absolute(&settings.dir).ok()?)
        .unwrap_or(&rust);
    Some((shown.display().to_string(), line + 1, col + 1))
}
