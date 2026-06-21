use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::input::Backend;
use crate::output::paste;

#[derive(Debug, Clone)]
pub enum Command {
    Text(String),
    Enter,
    Key(String),
    Wait(Duration),
    Paste(String, String),
    Set(String, String),
    Repeat(u64, Vec<Command>),
    Include(String),
    In(Duration, Vec<Command>),
    At(String, Vec<Command>),
    KeepAwake(Duration),
    PasteFile(String),
    PasteDir(String),
    Exec(String, String),
    MouseMove(String, String, String),
    MouseClick(String, String, String, String),
    MouseDrag(String, String, String, String, String),
    MouseDown(String),
    MouseUp(String),
    MouseScroll(String, String),
    Screenshot(String),
}

pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    if s.is_empty() {
        bail!("Empty duration string");
    }

    let mut total_duration = Duration::ZERO;
    let mut chars = s.chars().peekable();
    let mut parsed_any = false;

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }

        let mut num_str = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() {
                num_str.push(c);
                chars.next();
            } else {
                break;
            }
        }

        if num_str.is_empty() {
            bail!("Duration must start with a number or have a valid number in segment: {}", s);
        }

        let num: u64 = num_str.parse()
            .with_context(|| format!("Invalid number in duration: {}", num_str))?;

        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }

        let mut unit_str = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_alphabetic() {
                unit_str.push(c);
                chars.next();
            } else {
                break;
            }
        }

        if unit_str.is_empty() {
            bail!("Missing unit for number '{}' in duration '{}'", num, s);
        }

        let segment_duration = match unit_str.as_str() {
            "ms" => Duration::from_millis(num),
            "s" => Duration::from_secs(num),
            "m" => Duration::from_secs(num * 60),
            "h" => Duration::from_secs(num * 3600),
            _ => bail!("Unknown duration unit: {unit_str} (use ms, s, m, or h) in duration '{s}'"),
        };

        total_duration += segment_duration;
        parsed_any = true;
    }

    if !parsed_any {
        bail!("Could not parse any duration segments in: {}", s);
    }

    Ok(total_duration)
}

#[allow(dead_code)]
pub fn parse_script(content: &str) -> Result<Vec<Command>> {
    let vars = HashMap::new();
    parse_script_with_vars(content, &vars, None)
}

pub fn parse_script_with_vars(
    content: &str,
    vars: &HashMap<String, String>,
    base_dir: Option<&Path>,
) -> Result<Vec<Command>> {
    let lines: Vec<&str> = content.lines().collect();
    let (commands, _) = parse_script_inner(&lines, 0, vars, base_dir)?;
    Ok(commands)
}

fn parse_script_inner(
    lines: &[&str],
    start: usize,
    vars: &HashMap<String, String>,
    base_dir: Option<&Path>,
) -> Result<(Vec<Command>, usize)> {
    let mut commands = Vec::new();
    let mut i = start;

    while i < lines.len() {
        let raw_line = lines[i].trim();
        if raw_line.is_empty() || raw_line.starts_with('#') {
            i += 1;
            continue;
        }

        let line_string = strip_comments(raw_line);
        let line = line_string.trim();
        if line.is_empty() {
            i += 1;
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        let keyword = parts[0];

        match keyword {
            "set" => {
                let arg = parts
                    .get(1)
                    .context("set command requires: set name \"value\"")?;
                let arg = expand_vars(arg, vars);
                let mut tokens = arg.splitn(2, ' ');
                let name = tokens
                    .next()
                    .context("set command requires a variable name")?;
                let value = tokens
                    .next()
                    .context("set command requires a value")?;
                let value = unquote(&value);
                commands.push(Command::Set(name.to_string(), value.to_string()));
                i += 1;
            }
            "repeat" => {
                let arg = parts
                    .get(1)
                    .context("repeat command requires: repeat N { ... }")?;
                let arg = expand_vars(arg, vars);
                let arg = arg.trim();
                let (count_str, brace_on_same_line) = if let Some(pos) = arg.find('{') {
                    (arg[..pos].trim(), true)
                } else {
                    (arg, false)
                };
                let count: u64 = count_str
                    .parse()
                    .context("repeat count must be a number")?;

                if brace_on_same_line {
                    i += 1;
                } else {
                    i += 1;
                    while i < lines.len() && lines[i].trim().is_empty() {
                        i += 1;
                    }
                    if i >= lines.len() || !lines[i].trim().starts_with('{') {
                        bail!("repeat block requires opening '{{' on line {}", i + 1);
                    }
                    i += 1;
                }

                let (inner, new_i) = parse_script_inner(lines, i, vars, base_dir)?;
                i = new_i;

                commands.push(Command::Repeat(count, inner));
            }
            "}" => {
                return Ok((commands, i + 1));
            }
            "include" => {
                let arg = parts
                    .get(1)
                    .context("include command requires a file path")?;
                let arg = expand_vars(arg, vars);
                let path = unquote(&arg);
                commands.push(Command::Include(path.to_string()));
                i += 1;
            }
            "in" => {
                let arg = parts
                    .get(1)
                    .context("in command requires: in \"duration\" { ... }")?;
                let arg = expand_vars(arg, vars);
                let arg = arg.trim();

                // Extract duration and check for opening brace
                let (dur_str, brace_on_same_line) = if let Some(pos) = arg.find('{') {
                    (arg[..pos].trim(), true)
                } else {
                    (arg, false)
                };
                let dur = parse_duration(unquote(dur_str))?;

                if brace_on_same_line {
                    i += 1;
                } else {
                    i += 1;
                    while i < lines.len() && lines[i].trim().is_empty() {
                        i += 1;
                    }
                    if i >= lines.len() || !lines[i].trim().starts_with('{') {
                        bail!("in block requires opening '{{' on line {}", i + 1);
                    }
                    i += 1;
                }

                let (inner, new_i) = parse_script_inner(lines, i, vars, base_dir)?;
                i = new_i;

                commands.push(Command::In(dur, inner));
            }
            "at" => {
                let arg = parts
                    .get(1)
                    .context("at command requires: at \"HH:MM\" { ... }")?;
                let arg = expand_vars(arg, vars);
                let arg = arg.trim();

                // Extract time and check for opening brace
                let (time_str, brace_on_same_line) = if let Some(pos) = arg.find('{') {
                    (arg[..pos].trim(), true)
                } else {
                    (arg, false)
                };
                let time_str = unquote(time_str).to_string();

                if brace_on_same_line {
                    i += 1;
                } else {
                    i += 1;
                    while i < lines.len() && lines[i].trim().is_empty() {
                        i += 1;
                    }
                    if i >= lines.len() || !lines[i].trim().starts_with('{') {
                        bail!("at block requires opening '{{' on line {}", i + 1);
                    }
                    i += 1;
                }

                let (inner, new_i) = parse_script_inner(lines, i, vars, base_dir)?;
                i = new_i;

                commands.push(Command::At(time_str, inner));
            }
            "keep-awake" => {
                let dur_str = parts
                    .get(1)
                    .map(|s| unquote(expand_vars(s, vars).trim()).to_string())
                    .unwrap_or_else(|| "4m".to_string());
                let dur = parse_duration(&dur_str)?;
                commands.push(Command::KeepAwake(dur));
                i += 1;
            }
            "text" => {
                let arg = parts
                    .get(1)
                    .context("text command requires an argument")?;
                let arg = expand_vars(arg, vars);
                let text = unquote(&arg);
                commands.push(Command::Text(text.to_string()));
                i += 1;
            }
            "enter" => {
                commands.push(Command::Enter);
                i += 1;
            }
            "key" => {
                let arg = parts
                    .get(1)
                    .context("key command requires an argument")?;
                let arg = expand_vars(arg, vars);
                let key = unquote(&arg);
                commands.push(Command::Key(key.to_string()));
                i += 1;
            }
            "wait" => {
                let arg = parts
                    .get(1)
                    .context("wait command requires an argument")?;
                let arg = expand_vars(arg, vars);
                let dur = parse_duration(unquote(&arg))?;
                commands.push(Command::Wait(dur));
                i += 1;
            }
            "paste" => {
                let arg1 = parts
                    .get(1)
                    .context("paste command requires at least one argument")?;
                let arg1 = expand_vars(arg1, vars);
                let text = unquote(&arg1).to_string();

                let shortcut = if let Some(arg2) = parts.get(2) {
                    let arg2 = expand_vars(arg2, vars);
                    unquote(&arg2).to_string()
                } else {
                    "ctrl+v".to_string()
                };

                commands.push(Command::Paste(text, shortcut));
                i += 1;
            }
            "paste-file" => {
                let arg = parts
                    .get(1)
                    .context("paste-file command requires a file path")?;
                let arg = expand_vars(arg, vars);
                let path = unquote(&arg);
                commands.push(Command::PasteFile(path.to_string()));
                i += 1;
            }
            "paste-dir" => {
                let arg = parts
                    .get(1)
                    .context("paste-dir command requires a directory path")?;
                let arg = expand_vars(arg, vars);
                let path = unquote(&arg);
                commands.push(Command::PasteDir(path.to_string()));
                i += 1;
            }
            "exec" => {
                let arg = parts
                    .get(1)
                    .context("exec command requires arguments")?;
                let arg = expand_vars(arg, vars);
                let mut tokens = arg.splitn(2, ' ');
                let var_name = tokens
                    .next()
                    .context("exec command requires a variable name")?;
                let command_str = tokens
                    .next()
                    .context("exec command requires a command string")?;
                let command_str = unquote(&command_str);
                commands.push(Command::Exec(var_name.to_string(), command_str.to_string()));
                i += 1;
            }
            "move" => {
                let arg = parts
                    .get(1)
                    .context("move command requires: move x y [duration]")?;
                let arg = expand_vars(arg, vars);
                let args: Vec<&str> = arg.split_whitespace().collect();
                if args.len() < 2 || args.len() > 3 {
                    bail!("move command requires 2 or 3 arguments: move x y [duration]");
                }
                let x = args[0].to_string();
                let y = args[1].to_string();
                let dur = if args.len() == 3 {
                    args[2].to_string()
                } else {
                    "0s".to_string()
                };
                commands.push(Command::MouseMove(x, y, dur));
                i += 1;
            }
            "click" => {
                let arg = parts
                    .get(1)
                    .context("click command requires: click x y [button] [duration]")?;
                let arg = expand_vars(arg, vars);
                let args: Vec<&str> = arg.split_whitespace().collect();
                if args.len() < 2 || args.len() > 4 {
                    bail!("click command requires 2 to 4 arguments: click x y [button] [duration]");
                }
                let x = args[0].to_string();
                let y = args[1].to_string();
                let button = if args.len() >= 3 {
                    args[2].to_string()
                } else {
                    "left".to_string()
                };
                let dur = if args.len() == 4 {
                    args[3].to_string()
                } else {
                    "0s".to_string()
                };
                commands.push(Command::MouseClick(x, y, button, dur));
                i += 1;
            }
            "drag" => {
                let arg = parts
                    .get(1)
                    .context("drag command requires: drag x1 y1 x2 y2 [duration]")?;
                let arg = expand_vars(arg, vars);
                let args: Vec<&str> = arg.split_whitespace().collect();
                if args.len() < 4 || args.len() > 5 {
                    bail!("drag command requires 4 or 5 arguments: drag x1 y1 x2 y2 [duration]");
                }
                let x1 = args[0].to_string();
                let y1 = args[1].to_string();
                let x2 = args[2].to_string();
                let y2 = args[3].to_string();
                let dur = if args.len() == 5 {
                    args[4].to_string()
                } else {
                    "500ms".to_string()
                };
                commands.push(Command::MouseDrag(x1, y1, x2, y2, dur));
                i += 1;
            }
            "mouse-down" => {
                let button = parts
                    .get(1)
                    .map(|s| expand_vars(s, vars))
                    .unwrap_or_else(|| "left".to_string());
                commands.push(Command::MouseDown(button));
                i += 1;
            }
            "mouse-up" => {
                let button = parts
                    .get(1)
                    .map(|s| expand_vars(s, vars))
                    .unwrap_or_else(|| "left".to_string());
                commands.push(Command::MouseUp(button));
                i += 1;
            }
            "scroll" => {
                let arg = parts
                    .get(1)
                    .context("scroll command requires: scroll clicks [horizontal]")?;
                let arg = expand_vars(arg, vars);
                let args: Vec<&str> = arg.split_whitespace().collect();
                if args.is_empty() || args.len() > 2 {
                    bail!("scroll command requires 1 or 2 arguments: scroll clicks [horizontal]");
                }
                let clicks = args[0].to_string();
                let horizontal = if args.len() == 2 {
                    args[1].to_string()
                } else {
                    "false".to_string()
                };
                commands.push(Command::MouseScroll(clicks, horizontal));
                i += 1;
            }
            "screenshot" => {
                let arg = parts
                    .get(1)
                    .context("screenshot command requires a file path")?;
                let arg = expand_vars(arg, vars);
                commands.push(Command::Screenshot(arg));
                i += 1;
            }
            _ => bail!("Unknown command: {keyword} on line {}", i + 1),
        }
    }

    Ok((commands, i))
}

pub fn expand_vars(s: &str, vars: &HashMap<String, String>) -> String {
    let mut result = s.to_string();
    for (name, value) in vars {
        let pattern = format!("${{{name}}}");
        result = result.replace(&pattern, value);
    }
    result
}

pub fn unquote(s: &str) -> &str {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

pub fn resolve_includes(
    commands: &[Command],
    base_dir: &Path,
    vars: &HashMap<String, String>,
) -> Result<Vec<Command>> {
    let mut result = Vec::new();

    for cmd in commands {
        match cmd {
            Command::Include(path) => {
                let file_path = if Path::new(path).is_absolute() {
                    PathBuf::from(path)
                } else {
                    base_dir.join(path)
                };
                let content = fs::read_to_string(&file_path)
                    .with_context(|| format!("Failed to read include file: {}", file_path.display()))?;
                let include_base = file_path
                    .parent()
                    .unwrap_or(base_dir)
                    .to_path_buf();
                let inner = parse_script_with_vars(&content, vars, Some(&include_base))?;
                let resolved = resolve_includes(&inner, &include_base, vars)?;
                result.extend(resolved);
            }
            Command::Repeat(count, inner) => {
                let resolved_inner = resolve_includes(inner, base_dir, vars)?;
                result.push(Command::Repeat(*count, resolved_inner));
            }
            Command::In(dur, inner) => {
                let resolved_inner = resolve_includes(inner, base_dir, vars)?;
                result.push(Command::In(*dur, resolved_inner));
            }
            Command::At(time, inner) => {
                let resolved_inner = resolve_includes(inner, base_dir, vars)?;
                result.push(Command::At(time.clone(), resolved_inner));
            }
            other => result.push(other.clone()),
        }
    }

    Ok(result)
}



pub struct Interpreter<'a> {
    backend: &'a dyn Backend,
    dry_run: bool,
    pub vars: HashMap<String, String>,
    logger: Option<&'a mut Logger>,
}

impl<'a> Interpreter<'a> {
    pub fn new(
        backend: &'a dyn Backend,
        dry_run: bool,
        logger: Option<&'a mut Logger>,
    ) -> Self {
        Self {
            backend,
            dry_run,
            vars: HashMap::new(),
            logger,
        }
    }

    pub fn run(&mut self, commands: &[Command]) -> Result<()> {
        for cmd in commands {
            self.run_one(cmd)?;
        }
        Ok(())
    }

    fn run_one(&mut self, cmd: &Command) -> Result<()> {
        match cmd {
            Command::Set(name, value) => {
                let expanded = expand_vars(value, &self.vars);
                self.vars.insert(name.clone(), expanded);
                self.log_action("set", &format!("{name} = ..."), "ok")?;
            }
            Command::Repeat(count, inner) => {
                for _ in 0..*count {
                    self.run(inner)?;
                }
            }
            Command::In(dur, inner) => {
                if self.dry_run {
                    println!("[dry-run] in {dur:?} {{ ... }} ({} action(s))", inner.len());
                    let mut interp = Interpreter::new(self.backend, true, None);
                    interp.vars = self.vars.clone();
                    interp.run(inner)?;
                } else {
                    println!("Waiting {dur:?}...");
                    std::thread::sleep(*dur);
                    self.run(inner)?;
                }
                self.log_action("in", &format!("{dur:?}"), "ok")?;
            }
            Command::At(time, inner) => {
                let delay = crate::scheduler::delay_until_time(time)?;
                if self.dry_run {
                    println!("[dry-run] at {time} (wait {delay:?}, {} action(s))", inner.len());
                    let mut interp = Interpreter::new(self.backend, true, None);
                    interp.vars = self.vars.clone();
                    interp.run(inner)?;
                } else {
                    println!("Waiting until {time} ({delay:?})...");
                    std::thread::sleep(delay);
                    self.run(inner)?;
                }
                self.log_action("at", time, "ok")?;
            }
            Command::KeepAwake(dur) => {
                if self.dry_run {
                    println!("[dry-run] keep-awake every {dur:?}");
                } else {
                    crate::scheduler::keep_awake_background(*dur, "F15".to_string());
                }
                self.log_action("keep-awake", &format!("{dur:?}"), "ok")?;
            }
            Command::Include(_) => {
                // Already resolved before execution
            }
            Command::Text(text) => {
                let text = expand_vars(text, &self.vars);
                if self.dry_run {
                    println!("[dry-run] type_text: \"{text}\"");
                } else {
                    self.backend.type_text(&text)?;
                }
                self.log_action("type_text", &text, "ok")?;
            }
            Command::Enter => {
                if self.dry_run {
                    println!("[dry-run] press_key: \"enter\"");
                } else {
                    self.backend.press_key("enter")?;
                }
                self.log_action("press_key", "enter", "ok")?;
            }
            Command::Key(key) => {
                let key = expand_vars(key, &self.vars);
                if self.dry_run {
                    println!("[dry-run] press_key: \"{key}\"");
                } else {
                    self.backend.press_key(&key)?;
                }
                self.log_action("press_key", &key, "ok")?;
            }
            Command::Wait(dur) => {
                if self.dry_run {
                    println!("[dry-run] wait: {dur:?}");
                } else {
                    thread::sleep(*dur);
                }
                self.log_action("wait", &format!("{dur:?}"), "ok")?;
            }
            Command::Paste(text, shortcut) => {
                let text = expand_vars(text, &self.vars);
                let shortcut = expand_vars(shortcut, &self.vars);
                if self.dry_run {
                    println!("[dry-run] paste: \"{text}\" (shortcut: {shortcut})");
                } else {
                    paste::paste(&text, &shortcut, self.backend)?;
                }
                self.log_action("paste", &text, "ok")?;
            }
            Command::PasteFile(path) => {
                let path = expand_vars(path, &self.vars);
                let file_path = Path::new(&path);
                let content = std::fs::read_to_string(file_path)
                    .with_context(|| format!("Failed to read paste-file: {path}"))?;
                let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let formatted = format!(
                    "Archivo: {}\n```{}\n{}\n```\n",
                    path, extension, content
                );
                if self.dry_run {
                    println!("[dry-run] paste-file: \"{path}\"");
                } else {
                    paste::paste(&formatted, "ctrl+v", self.backend)?;
                }
                self.log_action("paste-file", &path, "ok")?;
            }
            Command::PasteDir(path) => {
                let path = expand_vars(path, &self.vars);
                let dir_path = Path::new(&path);
                if self.dry_run {
                    println!("[dry-run] paste-dir: \"{path}\"");
                } else {
                    let formatted = collect_dir_contents(dir_path)?;
                    paste::paste(&formatted, "ctrl+v", self.backend)?;
                }
                self.log_action("paste-dir", &path, "ok")?;
            }
            Command::Exec(var_name, command_str) => {
                let command_str = expand_vars(command_str, &self.vars);
                if self.dry_run {
                    println!("[dry-run] exec: {var_name} = \"{command_str}\"");
                    self.vars.insert(var_name.clone(), format!("[dry-run output of {command_str}]"));
                } else {
                    let output = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(&command_str)
                        .output();
                    let output_str = match output {
                        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
                        Err(e) => format!("Error executing command: {e}"),
                    };
                    self.vars.insert(var_name.clone(), output_str);
                }
                self.log_action("exec", &format!("{var_name} = ..."), "ok")?;
            }
            Command::MouseMove(x, y, dur) => {
                let x_val = expand_vars(x, &self.vars);
                let y_val = expand_vars(y, &self.vars);
                let dur_val = expand_vars(dur, &self.vars);
                let x_parsed: i32 = unquote(&x_val).parse().context("x coordinate must be a number")?;
                let y_parsed: i32 = unquote(&y_val).parse().context("y coordinate must be a number")?;
                let dur_parsed = parse_duration(unquote(&dur_val))?;

                if self.dry_run {
                    println!("[dry-run] mouse_move: to ({x_parsed}, {y_parsed}) over {dur_parsed:?}");
                } else {
                    self.backend.mouse_move(x_parsed, y_parsed, dur_parsed.as_millis() as u64)?;
                }
                self.log_action("mouse_move", &format!("({x_parsed}, {y_parsed})"), "ok")?;
            }
            Command::MouseClick(x, y, button, dur) => {
                let x_val = expand_vars(x, &self.vars);
                let y_val = expand_vars(y, &self.vars);
                let button_val = expand_vars(button, &self.vars);
                let dur_val = expand_vars(dur, &self.vars);
                let x_parsed: i32 = unquote(&x_val).parse().context("x coordinate must be a number")?;
                let y_parsed: i32 = unquote(&y_val).parse().context("y coordinate must be a number")?;
                let button_clean = unquote(&button_val);
                let dur_parsed = parse_duration(unquote(&dur_val))?;

                if self.dry_run {
                    println!("[dry-run] mouse_click: button {button_clean} at ({x_parsed}, {y_parsed}) over {dur_parsed:?}");
                } else {
                    self.backend.mouse_click(x_parsed, y_parsed, button_clean, dur_parsed.as_millis() as u64)?;
                }
                self.log_action("mouse_click", &format!("button {button_clean} at ({x_parsed}, {y_parsed})"), "ok")?;
            }
            Command::MouseDrag(x1, y1, x2, y2, dur) => {
                let x1_val = expand_vars(x1, &self.vars);
                let y1_val = expand_vars(y1, &self.vars);
                let x2_val = expand_vars(x2, &self.vars);
                let y2_val = expand_vars(y2, &self.vars);
                let dur_val = expand_vars(dur, &self.vars);
                let x1_parsed: i32 = unquote(&x1_val).parse().context("x1 coordinate must be a number")?;
                let y1_parsed: i32 = unquote(&y1_val).parse().context("y1 coordinate must be a number")?;
                let x2_parsed: i32 = unquote(&x2_val).parse().context("x2 coordinate must be a number")?;
                let y2_parsed: i32 = unquote(&y2_val).parse().context("y2 coordinate must be a number")?;
                let dur_parsed = parse_duration(unquote(&dur_val))?;

                if self.dry_run {
                    println!("[dry-run] mouse_drag: from ({x1_parsed}, {y1_parsed}) to ({x2_parsed}, {y2_parsed}) over {dur_parsed:?}");
                } else {
                    self.backend.mouse_drag(x1_parsed, y1_parsed, x2_parsed, y2_parsed, dur_parsed.as_millis() as u64)?;
                }
                self.log_action("mouse_drag", &format!("({x1_parsed}, {y1_parsed}) -> ({x2_parsed}, {y2_parsed})"), "ok")?;
            }
            Command::MouseDown(button) => {
                let button_val = expand_vars(button, &self.vars);
                let button_clean = unquote(&button_val);
                if self.dry_run {
                    println!("[dry-run] mouse_down: button {button_clean}");
                } else {
                    self.backend.mouse_down(button_clean)?;
                }
                self.log_action("mouse_down", button_clean, "ok")?;
            }
            Command::MouseUp(button) => {
                let button_val = expand_vars(button, &self.vars);
                let button_clean = unquote(&button_val);
                if self.dry_run {
                    println!("[dry-run] mouse_up: button {button_clean}");
                } else {
                    self.backend.mouse_up(button_clean)?;
                }
                self.log_action("mouse_up", button_clean, "ok")?;
            }
            Command::MouseScroll(clicks, horizontal) => {
                let clicks_val = expand_vars(clicks, &self.vars);
                let horiz_val = expand_vars(horizontal, &self.vars);
                let clicks_parsed: i32 = unquote(&clicks_val).parse().context("scroll clicks must be a number")?;
                let h_str = unquote(&horiz_val).to_lowercase();
                let horiz_parsed = h_str == "horizontal" || h_str == "true" || h_str == "h";

                if self.dry_run {
                    println!("[dry-run] mouse_scroll: clicks {clicks_parsed}, horizontal {horiz_parsed}");
                } else {
                    self.backend.mouse_scroll(clicks_parsed, horiz_parsed)?;
                }
                self.log_action("mouse_scroll", &format!("clicks {clicks_parsed}, horizontal {horiz_parsed}"), "ok")?;
            }
            Command::Screenshot(path) => {
                let path_val = expand_vars(path, &self.vars);
                let path_clean = unquote(&path_val);
                if self.dry_run {
                    println!("[dry-run] take_screenshot: save to {path_clean}");
                } else {
                    crate::vision::capture_screen(path_clean)?;
                }
                self.log_action("screenshot", path_clean, "ok")?;
            }
        }
        Ok(())
    }

    fn log_action(&mut self, action: &str, detail: &str, result: &str) -> Result<()> {
        if let Some(ref mut logger) = self.logger {
            logger.log(action, detail, result)?;
        }
        Ok(())
    }
}

fn collect_dir_contents(dir: &Path) -> Result<String> {
    let mut combined = String::new();
    collect_dir_recursive(dir, dir, &mut combined)?;
    if combined.is_empty() {
        bail!("No text files found in directory: {}", dir.display());
    }
    Ok(combined)
}

fn collect_dir_recursive(root: &Path, current: &Path, combined: &mut String) -> Result<()> {
    if current.is_file() {
        if let Ok(content) = std::fs::read_to_string(current) {
            let rel_path = current.strip_prefix(root).unwrap_or(current);
            let extension = current.extension().and_then(|e| e.to_str()).unwrap_or("");
            combined.push_str(&format!(
                "Archivo: {}\n```{}\n{}\n```\n\n",
                rel_path.display(), extension, content
            ));
        }
        return Ok(());
    }

    if current.is_dir() {
        let dir_name = current.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if dir_name.starts_with('.') 
            || dir_name == "node_modules" 
            || dir_name == "target" 
            || dir_name == "build"
            || dir_name == "dist"
            || dir_name == "venv"
        {
            return Ok(());
        }

        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            collect_dir_recursive(root, &entry.path(), combined)?;
        }
    }
    Ok(())
}

pub struct Logger {
    file: fs::File,
}

impl Logger {
    pub fn new(path: &str) -> Result<Self> {
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        writeln!(file, "--- mk log session started ---")?;

        Ok(Self { file })
    }

    pub fn log(&mut self, action: &str, detail: &str, result: &str) -> Result<()> {
        use std::io::Write;

        let now = chrono_timestamp();
        writeln!(self.file, "[{now}] {action}: {detail} → {result}")?;
        Ok(())
    }
}

fn chrono_timestamp() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn strip_comments(line: &str) -> String {
    let mut result = String::new();
    let mut in_quotes = false;
    let mut quote_char = ' ';

    let chars: Vec<char> = line.chars().collect();
    let mut idx = 0;
    while idx < chars.len() {
        let c = chars[idx];
        if (c == '"' || c == '\'') && (idx == 0 || chars[idx - 1] != '\\') {
            if in_quotes {
                if c == quote_char {
                    in_quotes = false;
                }
            } else {
                in_quotes = true;
                quote_char = c;
            }
        } else if c == '#' && !in_quotes {
            break;
        }
        result.push(c);
        idx += 1;
    }
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Duration ---

    #[test]
    fn test_parse_duration_ms() {
        assert_eq!(parse_duration("250ms").unwrap(), Duration::from_millis(250));
    }

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(parse_duration("10s").unwrap(), Duration::from_secs(10));
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(parse_duration("2h").unwrap(), Duration::from_secs(7200));
    }

    #[test]
    fn test_parse_duration_compound() {
        assert_eq!(parse_duration("1h 53m").unwrap(), Duration::from_secs(3600 + 53 * 60));
        assert_eq!(parse_duration("1h53m").unwrap(), Duration::from_secs(3600 + 53 * 60));
        assert_eq!(parse_duration("2h 30m 10s 500ms").unwrap(), Duration::from_millis(7200 * 1000 + 1800 * 1000 + 10 * 1000 + 500));
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(parse_duration("10x").is_err());
    }

    #[test]
    fn test_parse_duration_empty() {
        assert!(parse_duration("").is_err());
    }

    #[test]
    fn test_parse_duration_no_number() {
        assert!(parse_duration("ms").is_err());
    }

    // --- unquote ---

    #[test]
    fn test_unquote_double() {
        assert_eq!(unquote("\"hello\""), "hello");
    }

    #[test]
    fn test_unquote_single() {
        assert_eq!(unquote("'hello'"), "hello");
    }

    #[test]
    fn test_unquote_none() {
        assert_eq!(unquote("hello"), "hello");
    }

    // --- Variables ---

    #[test]
    fn test_expand_vars() {
        let mut vars = HashMap::new();
        vars.insert("name".into(), "Claude".into());
        assert_eq!(expand_vars("Hello ${name}", &vars), "Hello Claude");
    }

    #[test]
    fn test_expand_vars_missing() {
        let vars = HashMap::new();
        assert_eq!(expand_vars("Hello ${name}", &vars), "Hello ${name}");
    }

    #[test]
    fn test_expand_vars_multiple() {
        let mut vars = HashMap::new();
        vars.insert("a".into(), "1".into());
        vars.insert("b".into(), "2".into());
        assert_eq!(expand_vars("${a}+${b}", &vars), "1+2");
    }

    #[test]
    fn test_set_command() {
        let script = "set name \"Claude\"\ntext \"Hello ${name}\"";
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 2);
        match &cmds[0] {
            Command::Set(n, v) => {
                assert_eq!(n, "name");
                assert_eq!(v, "Claude");
            }
            _ => panic!("expected Set command"),
        }
        match &cmds[1] {
            Command::Text(t) => assert_eq!(t, "Hello ${name}"),
            _ => panic!("expected Text command"),
        }
    }

    // --- Repeat ---

    #[test]
    fn test_repeat_block() {
        let script = "repeat 3 {\n  text \"hello\"\n  enter\n}";
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Command::Repeat(count, inner) => {
                assert_eq!(*count, 3);
                assert_eq!(inner.len(), 2);
            }
            _ => panic!("expected Repeat command"),
        }
    }

    #[test]
    fn test_repeat_with_variables() {
        let script = "set msg \"hi\"\nrepeat 2 {\n  text \"${msg}\"\n}";
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 2);
        match &cmds[1] {
            Command::Repeat(count, inner) => {
                assert_eq!(*count, 2);
                match &inner[0] {
                    Command::Text(t) => assert_eq!(t, "${msg}"),
                    _ => panic!("expected Text"),
                }
            }
            _ => panic!("expected Repeat"),
        }
    }

    #[test]
    fn test_repeat_nested() {
        let script = "repeat 2 {\n  repeat 3 {\n    text \"x\"\n  }\n}";
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Command::Repeat(outer, inner) => {
                assert_eq!(*outer, 2);
                assert_eq!(inner.len(), 1);
                match &inner[0] {
                    Command::Repeat(inner_count, inner_inner) => {
                        assert_eq!(*inner_count, 3);
                        assert_eq!(inner_inner.len(), 1);
                    }
                    _ => panic!("expected inner Repeat"),
                }
            }
            _ => panic!("expected outer Repeat"),
        }
    }

    #[test]
    fn test_repeat_no_brace() {
        assert!(parse_script("repeat 3").is_err());
    }

    // --- Include ---

    #[test]
    fn test_include_command() {
        let script = "include \"common.mk\"";
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Command::Include(p) => assert_eq!(p, "common.mk"),
            _ => panic!("expected Include command"),
        }
    }

    // --- Comments and empty lines in blocks ---

    #[test]
    fn test_comments_in_blocks() {
        let script = "repeat 2 {\n  # comment\n  text \"hi\"\n  \n  enter\n}";
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Command::Repeat(_, inner) => {
                assert_eq!(inner.len(), 2);
            }
            _ => panic!("expected Repeat"),
        }
    }

    // --- In block ---

    #[test]
    fn test_in_block() {
        let script = "in \"10s\" {\n  paste \"sigue\"\n  enter\n}";
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Command::In(dur, inner) => {
                assert_eq!(*dur, Duration::from_secs(10));
                assert_eq!(inner.len(), 2);
            }
            _ => panic!("expected In command"),
        }
    }

    #[test]
    fn test_in_block_ms() {
        let script = "in \"500ms\" {\n  text \"fast\"\n}";
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Command::In(dur, _) => {
                assert_eq!(*dur, Duration::from_millis(500));
            }
            _ => panic!("expected In command"),
        }
    }

    #[test]
    fn test_in_block_no_brace() {
        assert!(parse_script("in \"5s\"").is_err());
    }

    // --- At block ---

    #[test]
    fn test_at_block() {
        let script = "at \"03:30\" {\n  paste \"revisa errores\"\n  enter\n}";
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Command::At(time, inner) => {
                assert_eq!(time, "03:30");
                assert_eq!(inner.len(), 2);
            }
            _ => panic!("expected At command"),
        }
    }

    #[test]
    fn test_at_block_no_brace() {
        assert!(parse_script("at \"12:00\"").is_err());
    }

    // --- Keep-awake ---

    #[test]
    fn test_keep_awake_default() {
        let cmds = parse_script("keep-awake").unwrap();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Command::KeepAwake(dur) => {
                assert_eq!(*dur, Duration::from_secs(240)); // 4m
            }
            _ => panic!("expected KeepAwake command"),
        }
    }

    #[test]
    fn test_keep_awake_custom() {
        let cmds = parse_script("keep-awake \"2m\"").unwrap();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Command::KeepAwake(dur) => {
                assert_eq!(*dur, Duration::from_secs(120));
            }
            _ => panic!("expected KeepAwake command"),
        }
    }

    // --- Existing tests ---

    #[test]
    fn test_parse_script_comments_and_empty() {
        let script = "# comment\n\n  \n# another";
        let cmds = parse_script(script).unwrap();
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_parse_script_text() {
        let script = "text \"hello world\"";
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Command::Text(t) => assert_eq!(t, "hello world"),
            _ => panic!("expected Text command"),
        }
    }

    #[test]
    fn test_parse_script_enter() {
        let cmds = parse_script("enter").unwrap();
        assert_eq!(cmds.len(), 1);
        assert!(matches!(cmds[0], Command::Enter));
    }

    #[test]
    fn test_parse_script_key() {
        let cmds = parse_script("key \"ctrl+s\"").unwrap();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Command::Key(k) => assert_eq!(k, "ctrl+s"),
            _ => panic!("expected Key command"),
        }
    }

    #[test]
    fn test_parse_script_wait() {
        let cmds = parse_script("wait \"5s\"").unwrap();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Command::Wait(d) => assert_eq!(*d, Duration::from_secs(5)),
            _ => panic!("expected Wait command"),
        }
    }

    #[test]
    fn test_parse_script_paste() {
        let script = "paste \"some text\"";
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Command::Paste(t, s) => {
                assert_eq!(t, "some text");
                assert_eq!(s, "ctrl+v");
            }
            _ => panic!("expected Paste command"),
        }
    }

    #[test]
    fn test_parse_script_full() {
        let script = r#"# my script
text "hello"
enter
wait "1s"
key "ctrl+s"
paste "clipboard text"
"#;
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 5);
    }

    #[test]
    fn test_parse_script_unknown_command() {
        assert!(parse_script("foobar").is_err());
    }

    #[test]
    fn test_parse_script_text_no_arg() {
        assert!(parse_script("text").is_err());
    }

    #[test]
    fn test_parse_script_paste_no_arg() {
        assert!(parse_script("paste").is_err());
    }

    // --- Timestamp ---

    #[test]
    fn test_chrono_timestamp_format() {
        let ts = chrono_timestamp();
        assert!(ts.ends_with('Z'));
        assert!(ts.contains('T'));
    }

    #[test]
    fn test_strip_comments() {
        assert_eq!(strip_comments("text \"hello\" # comment"), "text \"hello\"");
        assert_eq!(strip_comments("text \"hello #1\""), "text \"hello #1\"");
        assert_eq!(strip_comments("# full comment"), "");
    }

    // --- Expand vars in repeat ---

    #[test]
    fn test_expand_in_text_after_set() {
        let script = "set x \"42\"\ntext \"val=${x}\"";
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 2);
        match &cmds[0] {
            Command::Set(name, val) => {
                assert_eq!(name, "x");
                assert_eq!(val, "42");
            }
            _ => panic!("expected Set"),
        }
        match &cmds[1] {
            Command::Text(t) => assert_eq!(t, "val=${x}"),
            _ => panic!("expected Text"),
        }
    }

    // --- Mixed schedule blocks ---

    #[test]
    fn test_in_and_at_mixed() {
        let script = r#"in "5s" {
  text "after 5s"
}
at "12:00" {
  text "at noon"
}
keep-awake "3m"
"#;
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 3);
        assert!(matches!(cmds[0], Command::In(_, _)));
        assert!(matches!(cmds[1], Command::At(_, _)));
        assert!(matches!(cmds[2], Command::KeepAwake(_)));
    }

    #[test]
    fn test_in_with_comments_inside() {
        let script = "in \"2s\" {\n  # delay comment\n  text \"ok\"\n}";
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            Command::In(_, inner) => assert_eq!(inner.len(), 1),
            _ => panic!("expected In"),
        }
    }

    #[test]
    fn test_parse_paste_file_dir_exec() {
        let script = r#"paste-file "src/main.rs"
paste-dir "src"
exec output "echo 'hello'"
"#;
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 3);
        match &cmds[0] {
            Command::PasteFile(p) => assert_eq!(p, "src/main.rs"),
            _ => panic!("expected PasteFile"),
        }
        match &cmds[1] {
            Command::PasteDir(d) => assert_eq!(d, "src"),
            _ => panic!("expected PasteDir"),
        }
        match &cmds[2] {
            Command::Exec(v, c) => {
                assert_eq!(v, "output");
                assert_eq!(c, "echo 'hello'");
            }
            _ => panic!("expected Exec"),
        }
    }

    #[test]
    fn test_parse_mouse_and_screenshot() {
        let script = r#"move 100 200 "500ms"
click 300 400 "right" "1s"
drag 10 20 30 40 "2s"
mouse-down "left"
mouse-up "left"
scroll 3 "true"
screenshot "shot.png"
"#;
        let cmds = parse_script(script).unwrap();
        assert_eq!(cmds.len(), 7);
        match &cmds[0] {
            Command::MouseMove(x, y, d) => {
                assert_eq!(x, "100");
                assert_eq!(y, "200");
                assert_eq!(d, "\"500ms\"");
            }
            _ => panic!("expected MouseMove"),
        }
        match &cmds[1] {
            Command::MouseClick(x, y, b, d) => {
                assert_eq!(x, "300");
                assert_eq!(y, "400");
                assert_eq!(b, "\"right\"");
                assert_eq!(d, "\"1s\"");
            }
            _ => panic!("expected MouseClick"),
        }
        match &cmds[2] {
            Command::MouseDrag(x1, y1, x2, y2, d) => {
                assert_eq!(x1, "10");
                assert_eq!(y1, "20");
                assert_eq!(x2, "30");
                assert_eq!(y2, "40");
                assert_eq!(d, "\"2s\"");
            }
            _ => panic!("expected MouseDrag"),
        }
        match &cmds[3] {
            Command::MouseDown(b) => assert_eq!(b, "\"left\""),
            _ => panic!("expected MouseDown"),
        }
        match &cmds[4] {
            Command::MouseUp(b) => assert_eq!(b, "\"left\""),
            _ => panic!("expected MouseUp"),
        }
        match &cmds[5] {
            Command::MouseScroll(clicks, horizontal) => {
                assert_eq!(clicks, "3");
                assert_eq!(horizontal, "\"true\"");
            }
            _ => panic!("expected MouseScroll"),
        }
        match &cmds[6] {
            Command::Screenshot(p) => assert_eq!(p, "\"shot.png\""),
            _ => panic!("expected Screenshot"),
        }
    }
}
