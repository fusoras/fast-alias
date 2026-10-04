use std::collections::BTreeMap;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table};

use crate::colors::*;
use crate::config::EXAMPLE_GLOBAL_CONFIG;

/// Native commands available for selection in the alias picker.
pub const NATIVE_COMMANDS: &[(&str, &str)] = &[
    ("--new", "Scaffold a new project from a recipe"),
    ("--list", "List available recipes, aliases, or packs"),
    ("--search", "Search recipes by query"),
    ("--show", "Show full details of a recipe or command"),
    ("--recipe new", "Create a new recipe file"),
    ("--recipe validate", "Validate recipe files"),
    ("--recipe edit", "Open recipe file in editor"),
    ("--recipe rm", "Remove a recipe file"),
    ("--template add", "Add files/folders into recipe templates"),
    ("--config", "Configure fast-alias settings interactively"),
    ("--self-update", "Check and update fa binary"),
    ("--self-uninstall", "Uninstall fa executable and data"),
];

/// Keys recognized by the interactive ANSI state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    Char(char),
    Other,
}

#[cfg(unix)]
#[repr(C)]
#[derive(Clone, Copy)]
struct Termios {
    c_iflag: u32,
    c_oflag: u32,
    c_cflag: u32,
    c_lflag: u32,
    c_line: u8,
    c_cc: [u8; 32],
    c_ispeed: u32,
    c_ospeed: u32,
}

#[cfg(unix)]
#[allow(unsafe_code)]
unsafe extern "C" {
    fn tcgetattr(fd: i32, termios_p: *mut Termios) -> i32;
    fn tcsetattr(fd: i32, optional_actions: i32, termios_p: *const Termios) -> i32;
    fn read(fd: i32, buf: *mut u8, count: usize) -> isize;
}

#[cfg(unix)]
#[repr(C)]
#[allow(dead_code)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

#[cfg(unix)]
#[allow(unsafe_code, dead_code)]
unsafe extern "C" {
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
}

#[cfg(unix)]
static ORIG_TERMIOS: std::sync::Mutex<Option<Termios>> = std::sync::Mutex::new(None);

/// Unbuffered reader directly accessing stdin fd 0 to prevent userspace buffering
/// from intercepting multi-byte escape sequences like arrow keys.
pub struct RawTerminalStdin;

impl Read for RawTerminalStdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        #[cfg(unix)]
        {
            if buf.is_empty() {
                return Ok(0);
            }
            #[allow(unsafe_code)]
            let res = unsafe { read(0, buf.as_mut_ptr(), buf.len()) };
            if res < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(res as usize)
        }
        #[cfg(not(unix))]
        {
            io::stdin().read(buf)
        }
    }
}

/// RAII Guard that manages raw terminal mode.
/// Restores the original terminal attributes and re-enables cursor on exit or panic.
pub struct RawModeGuard {
    active: bool,
}

impl RawModeGuard {
    #[allow(unsafe_code)]
    pub fn enter() -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let fd = io::stdin().as_raw_fd();
            let mut orig = std::mem::MaybeUninit::<Termios>::zeroed();
            if unsafe { tcgetattr(fd, orig.as_mut_ptr()) } == 0 {
                let orig = unsafe { orig.assume_init() };
                if let Ok(mut lock) = ORIG_TERMIOS.lock() {
                    *lock = Some(orig);
                }
                Self::enable_raw(&orig)?;
                return Ok(Self {
                    active: true,
                });
            }
        }
        Ok(Self {
            active: false,
        })
    }

    #[allow(unsafe_code)]
    fn enable_raw(orig: &Termios) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let fd = io::stdin().as_raw_fd();
            let mut raw = *orig;

            // ECHO | ICANON | IEXTEN | ISIG
            raw.c_lflag &= !(0x0008 | 0x0002 | 0x8000 | 0x0001);
            // BRKINT | ICRNL | INPCK | ISTRIP | IXON
            raw.c_iflag &= !(0x0002 | 0x0100 | 0x0010 | 0x0020 | 0x0400);
            // CS8
            raw.c_cflag |= 0x0030;
            // VMIN = 1, VTIME = 0
            raw.c_cc[6] = 1;
            raw.c_cc[5] = 0;

            let _ = unsafe { tcsetattr(fd, 0, &raw) };
            print!("\x1b[?25l");
            let _ = io::stdout().flush();
        }
        Ok(())
    }

    pub fn suspend() {
        #[cfg(unix)]
        {
            if let Ok(lock) = ORIG_TERMIOS.lock()
                && let Some(ref orig) = *lock
            {
                use std::os::fd::AsRawFd;
                let fd = io::stdin().as_raw_fd();
                #[allow(unsafe_code)]
                unsafe {
                    tcsetattr(fd, 0, orig);
                }
            }
            print!("\x1b[?25h\x1b[0m");
            let _ = io::stdout().flush();
        }
    }

    pub fn resume() {
        #[cfg(unix)]
        {
            if let Ok(lock) = ORIG_TERMIOS.lock()
                && let Some(ref orig) = *lock
            {
                let _ = Self::enable_raw(orig);
            }
        }
    }

    pub fn restore(&mut self) {
        if self.active {
            Self::suspend();
            self.active = false;
        }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

/// Polls stdin to see if a byte is available within `timeout_ms`.
#[cfg(all(unix, not(test)))]
#[allow(unsafe_code)]
fn poll_stdin(timeout_ms: i32) -> bool {
    use std::os::fd::AsRawFd;
    let fd = io::stdin().as_raw_fd();
    let mut pfd = PollFd {
        fd,
        events: 1, // POLLIN
        revents: 0,
    };
    let ret = unsafe { poll(&mut pfd, 1, timeout_ms) };
    ret > 0 && (pfd.revents & 1) != 0
}

#[cfg(any(not(unix), test))]
fn poll_stdin(_timeout_ms: i32) -> bool {
    true
}

/// Reads a single key event from a generic reader.
pub fn read_key_from<R: Read>(reader: &mut R) -> io::Result<Key> {
    let mut buf = [0u8; 1];
    if reader.read(&mut buf)? == 0 {
        return Ok(Key::Esc);
    }

    match buf[0] {
        b'\r' | b'\n' => Ok(Key::Enter),
        b'\t' => Ok(Key::Tab), // ASCII 9
        0x7f | 0x08 => Ok(Key::Backspace),
        0x03 => Ok(Key::Esc), // Ctrl+C maps to Esc/Cancel
        0x13 => Ok(Key::Char('s')), // Ctrl+S maps to 's' (Save)
        0x1b => {
            // Check if another byte follows immediately (e.g. arrow keys)
            if !poll_stdin(60) {
                return Ok(Key::Esc);
            }
            let mut next = [0u8; 1];
            if reader.read(&mut next)? == 0 {
                return Ok(Key::Esc);
            }
            if next[0] == b'[' || next[0] == b'O' {
                let mut code = [0u8; 1];
                if reader.read(&mut code)? == 0 {
                    return Ok(Key::Esc);
                }
                match code[0] {
                    b'A' => Ok(Key::Up),
                    b'B' => Ok(Key::Down),
                    b'C' => Ok(Key::Right),
                    b'D' => Ok(Key::Left),
                    b'Z' => Ok(Key::BackTab), // Shift-Tab escape sequence "\x1b[Z"
                    b'3' => {
                        // Delete key ~ sequence
                        let mut tilde = [0u8; 1];
                        let _ = reader.read(&mut tilde);
                        Ok(Key::Backspace)
                    }
                    _ => Ok(Key::Other),
                }
            } else {
                Ok(Key::Other)
            }
        }
        b if (32..=126).contains(&b) => Ok(Key::Char(b as char)),
        _ => Ok(Key::Other),
    }
}

/// Reads a line in raw mode with real-time backspace and cursor rendering.
pub fn prompt_line_raw<R: Read, W: Write>(
    prompt: &str,
    input: &mut R,
    output: &mut W,
) -> io::Result<Option<String>> {
    let mut buffer = String::new();

    write!(output, "\r\x1b[2K{prompt}")?;
    output.flush()?;

    loop {
        let key = read_key_from(input)?;
        match key {
            Key::Enter => {
                let trimmed = buffer.trim().to_string();
                return Ok(Some(trimmed));
            }
            Key::Esc => {
                return Ok(None);
            }
            Key::Backspace => {
                if !buffer.is_empty() {
                    buffer.pop();
                    write!(output, "\r\x1b[2K{prompt}{buffer}")?;
                    output.flush()?;
                }
            }
            Key::Char(c) if !c.is_control() => {
                buffer.push(c);
                write!(output, "{c}")?;
                output.flush()?;
            }
            _ => {}
        }
    }
}

/// Changes made during an interactive configuration session.
#[derive(Debug, Clone, Default)]
pub struct ConfigDelta {
    pub packs_behavior: Option<String>,
    pub aliases: BTreeMap<String, String>,
    pub reset_to_defaults: bool,
}

/// Applies a `ConfigDelta` to existing `config.toml` content using `toml_edit`
/// to preserve all user comments, layout, and whitespace formatting.
pub fn apply_config_delta(existing_toml: &str, delta: &ConfigDelta) -> anyhow::Result<String> {
    if delta.reset_to_defaults {
        return Ok(EXAMPLE_GLOBAL_CONFIG.to_string());
    }

    let mut doc: DocumentMut = if existing_toml.trim().is_empty() {
        EXAMPLE_GLOBAL_CONFIG.parse()?
    } else {
        existing_toml.parse()?
    };

    // 1. Packs default behavior:
    if let Some(ref behavior) = delta.packs_behavior {
        let is_default_behavior = behavior == "list";
        let has_packs = doc.contains_table("packs") || doc.contains_key("packs");

        // Sparse configuration: only persist if not default or if [packs] already exists
        if !is_default_behavior || has_packs {
            if !has_packs {
                doc["packs"] = Item::Table(Table::new());
            }
            doc["packs"]["default_behavior"] = toml_edit::value(behavior.as_str());
        }
    }

    // 2. Command aliases:
    // Ensure [alias] table exists if there are aliases
    if !delta.aliases.is_empty() && !doc.contains_table("alias") && !doc.contains_key("alias") {
        doc["alias"] = Item::Table(Table::new());
    }

    if let Some(table) = doc["alias"].as_table_like_mut() {
        // Collect existing keys in the toml table
        let existing_keys: Vec<String> = table.iter().map(|(k, _)| k.to_string()).collect();

        // Remove aliases that were deleted in delta
        for key in existing_keys {
            if !delta.aliases.contains_key(&key) {
                table.remove(&key);
            }
        }

        // Insert or update aliases
        for (k, v) in &delta.aliases {
            if let Some(item) = table.get_mut(k) {
                if let Some(val) = item.as_value_mut() {
                    let mut new_val = toml_edit::Value::from(v.as_str());
                    *new_val.decor_mut() = val.decor().clone();
                    *val = new_val;
                } else {
                    *item = Item::Value(toml_edit::Value::from(v.as_str()));
                }
            } else {
                table.insert(k, Item::Value(toml_edit::Value::from(v.as_str())));
            }
        }
    }

    Ok(doc.to_string())
}

/// Valid `packs.default_behavior` values in inline cycle order.
const PACKS_BEHAVIOR_VALUES: [&str; 3] = ["list", "default", "error"];

/// Cycles to the adjacent `packs.default_behavior` value.
/// Unknown values are treated as the first option.
fn cycle_packs_behavior(current: &str, forward: bool) -> String {
    let idx = PACKS_BEHAVIOR_VALUES
        .iter()
        .position(|v| *v == current)
        .unwrap_or(0);
    let len = PACKS_BEHAVIOR_VALUES.len();
    let next = if forward {
        (idx + 1) % len
    } else {
        (idx + len - 1) % len
    };
    PACKS_BEHAVIOR_VALUES[next].to_string()
}

/// Effect produced by a key press while the cursor is on the
/// "Packs default behavior" row of the main menu.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PacksRowEffect {
    /// Tab/BackTab: update the pending (unconfirmed) value.
    Cycle(String),
    /// Enter with a pending value: confirm it into the saved value.
    Confirm(String),
    /// Enter without a pending value: open the detailed sub-menu.
    OpenSubMenu,
    /// Up/Down: discard the pending value, then move the cursor.
    Move,
    /// Esc with a pending value: discard it and stay in the menu.
    DiscardStay,
    /// Esc without a pending value: leave the menu.
    Exit,
    /// Key not handled by the inline editor.
    Ignore,
}

/// Pure state transition for the inline edit of "Packs default behavior".
/// `confirmed` is the stored value, `pending` the unconfirmed cycled value.
fn packs_row_transition(
    confirmed: &str,
    pending: Option<&str>,
    key: &Key,
) -> PacksRowEffect {
    match key {
        Key::Tab => PacksRowEffect::Cycle(cycle_packs_behavior(
            pending.unwrap_or(confirmed),
            true,
        )),
        Key::BackTab => PacksRowEffect::Cycle(cycle_packs_behavior(
            pending.unwrap_or(confirmed),
            false,
        )),
        Key::Enter => match pending {
            Some(value) => PacksRowEffect::Confirm(value.to_string()),
            None => PacksRowEffect::OpenSubMenu,
        },
        Key::Up | Key::Down => PacksRowEffect::Move,
        Key::Esc => {
            if pending.is_some() {
                PacksRowEffect::DiscardStay
            } else {
                PacksRowEffect::Exit
            }
        }
        _ => PacksRowEffect::Ignore,
    }
}

/// Interactive TUI manager for fast-alias configuration.
pub struct ConfigEditor {
    config_path: PathBuf,
    initial_content: String,
    packs_behavior: String,
    /// Tab-cycled value not yet confirmed with Enter on the main menu.
    pending_behavior: Option<String>,
    aliases: BTreeMap<String, String>,
    status_message: Option<String>,
}

impl ConfigEditor {
    pub fn new(user_dir: &Path) -> anyhow::Result<Self> {
        let config_path = user_dir.join("config.toml");
        let initial_content = if config_path.is_file() {
            fs::read_to_string(&config_path)?
        } else {
            String::new()
        };

        let parsed = crate::config::parse_global_config(&initial_content).unwrap_or_default();
        let packs_behavior = parsed.packs.default_behavior;
        let aliases = parsed.alias;

        Ok(Self {
            config_path,
            initial_content,
            packs_behavior,
            pending_behavior: None,
            aliases,
            status_message: None,
        })
    }

    /// Renders the main menu and handles user selection.
    pub fn run<R: Read, W: Write>(&mut self, input: &mut R, output: &mut W) -> anyhow::Result<bool> {
        let menu_items = [
            "Packs default behavior",
            "Command aliases",
            "Open in editor",
            "Reset to defaults",
            "Save and exit",
            "Cancel",
        ];

        let mut selected = 0;
        let mut last_lines_drawn = 0;

        loop {
            if last_lines_drawn > 0 {
                write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
            }
            let mut lines = 0;
            let path_str = self.config_path.display().to_string();
            let prefix = "  Fast-Alias Configuration  •  ";
            let min_inner_w = 61;
            let content_w = prefix.chars().count() + path_str.chars().count() + 2;
            let inner_w = content_w.max(min_inner_w);
            let right_pad = " ".repeat(inner_w.saturating_sub(prefix.chars().count() + path_str.chars().count()));

            writeln!(
                output,
                "{BOLD_CYAN}╭{}╮{RESET}",
                "─".repeat(inner_w)
            )?;
            lines += 1;
            writeln!(
                output,
                "{BOLD_CYAN}│{RESET}  {BOLD_WHITE}Fast-Alias Configuration{RESET}  {DIM}•{RESET}  {DIM}{path_str}{RESET}{right_pad}{BOLD_CYAN}│{RESET}"
            )?;
            lines += 1;
            writeln!(
                output,
                "{BOLD_CYAN}╰{}╯{RESET}",
                "─".repeat(inner_w)
            )?;
            lines += 1;
            writeln!(
                output,
                " {BOLD_CYAN}fa config{RESET} {DIM}›{RESET} {BOLD_WHITE}Main Menu{RESET}"
            )?;
            lines += 1;

            let categories: [(&str, &[(usize, &str)]); 3] = [
                ("SETTINGS", &[
                    (0, "Packs default behavior"),
                    (1, "Command aliases"),
                ]),
                ("ADVANCED", &[
                    (2, "Open in editor"),
                    (3, "Reset to defaults"),
                ]),
                ("SESSION", &[
                    (4, "Save and exit"),
                    (5, "Cancel"),
                ]),
            ];

            for (cat_name, items) in categories.iter() {
                writeln!(output)?;
                lines += 1;
                writeln!(output, " {BOLD_WHITE}{cat_name}{RESET}")?;
                lines += 1;

                for &(idx, item) in items.iter() {
                    let is_sel = idx == selected;
                    let marker = if is_sel {
                        format!("{BOLD_GREEN}▸{RESET}")
                    } else {
                        " ".to_string()
                    };

                    let detail = match idx {
                        0 => match &self.pending_behavior {
                            Some(pending) => {
                                format!(" {BOLD_YELLOW}[ {pending} ]{RESET} {DIM}(pending){RESET}")
                            }
                            None => format!(" {BOLD_CYAN}[ {} ]{RESET}", self.packs_behavior),
                        },
                        1 => format!(" {DIM}({} defined){RESET}", self.aliases.len()),
                        _ => String::new(),
                    };

                    if is_sel {
                        writeln!(output, "  {marker} {BOLD_WHITE}{item}{RESET}{detail}")?;
                    } else {
                        writeln!(output, "  {marker} {item}{detail}")?;
                    }
                    lines += 1;
                }
            }

            // Reserved 2-line slot for status feedback right above the footer shortcuts
            writeln!(output)?;
            lines += 1;
            if let Some(msg) = &self.status_message {
                writeln!(output, "  {msg}")?;
            } else {
                writeln!(output)?;
            }
            lines += 1;

            if self.pending_behavior.is_some() {
                writeln!(
                    output,
                    " {DIM}Cycle with {RESET}{BOLD_CYAN}Tab{RESET}{DIM}/{RESET}{BOLD_CYAN}Shift-Tab{RESET}{DIM}, {RESET}{BOLD_CYAN}Enter{RESET}{DIM} to confirm, {RESET}{BOLD_CYAN}s{RESET}{DIM} to save, {RESET}{BOLD_CYAN}↑/↓{RESET}{DIM} to discard + move, {RESET}{BOLD_CYAN}Esc{RESET}{DIM} to discard{RESET}"
                )?;
            } else if selected == 0 {
                writeln!(
                    output,
                    " {DIM}Navigate with {RESET}{BOLD_CYAN}↑/↓{RESET}{DIM}, {RESET}{BOLD_CYAN}Enter{RESET}{DIM} to select, {RESET}{BOLD_CYAN}Tab{RESET}{DIM} to cycle value, {RESET}{BOLD_CYAN}s{RESET}{DIM} to save, {RESET}{BOLD_CYAN}Esc{RESET}{DIM} to cancel{RESET}"
                )?;
            } else {
                writeln!(
                    output,
                    " {DIM}Navigate with {RESET}{BOLD_CYAN}↑/↓{RESET}{DIM}, {RESET}{BOLD_CYAN}Enter{RESET}{DIM} to select, {RESET}{BOLD_CYAN}s{RESET}{DIM} to save, {RESET}{BOLD_CYAN}Esc{RESET}{DIM} to cancel{RESET}"
                )?;
            }
            lines += 1;

            last_lines_drawn = lines;
            output.flush()?;

            let key = read_key_from(input)?;

            // Inline edit of "Packs default behavior": Tab/BackTab cycle a
            // pending value, Enter confirms it, ↑/↓ or Esc discard it.
            if selected == 0 {
                let effect = packs_row_transition(
                    &self.packs_behavior,
                    self.pending_behavior.as_deref(),
                    &key,
                );
                match effect {
                    PacksRowEffect::Cycle(value) => {
                        self.pending_behavior = Some(value);
                        continue;
                    }
                    PacksRowEffect::Confirm(value) => {
                        self.packs_behavior = value.clone();
                        self.pending_behavior = None;
                        self.status_message = Some(format!(
                            "{BOLD_GREEN}✔ '{value}' applied successfully{RESET}"
                        ));
                        continue;
                    }
                    PacksRowEffect::DiscardStay => {
                        self.pending_behavior = None;
                        continue;
                    }
                    PacksRowEffect::Move => {
                        // Discard the pending value, then fall through so the
                        // regular ↑/↓ branches move the cursor.
                        self.pending_behavior = None;
                    }
                    // OpenSubMenu / Exit / Ignore: handled by the match below.
                    PacksRowEffect::OpenSubMenu
                    | PacksRowEffect::Exit
                    | PacksRowEffect::Ignore => {}
                }
            }

            match key {
                Key::Char('s') | Key::Char('S') => {
                    if let Some(pending) = self.pending_behavior.take() {
                        self.packs_behavior = pending;
                    }
                    self.save()?;
                    write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                    writeln!(
                        output,
                        "{BOLD_GREEN}✔{RESET} Configuration saved to {BOLD_WHITE}{}{RESET}",
                        self.config_path.display()
                    )?;
                    output.flush()?;
                    return Ok(true);
                }
                Key::Up => {
                    if selected == 0 {
                        selected = menu_items.len() - 1;
                    } else {
                        selected -= 1;
                    }
                }
                Key::Down => {
                    if selected + 1 >= menu_items.len() {
                        selected = 0;
                    } else {
                        selected += 1;
                    }
                }
                Key::Enter => match selected {
                    0 => {
                        write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                        output.flush()?;
                        last_lines_drawn = 0;
                        self.menu_packs_behavior(input, output)?;
                    }
                    1 => {
                        write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                        output.flush()?;
                        last_lines_drawn = 0;
                        self.menu_aliases(input, output)?;
                    }
                    2 => {
                        write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                        output.flush()?;
                        last_lines_drawn = 0;
                        self.action_open_editor(output)?;
                    }
                    3 => {
                        write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                        output.flush()?;
                        last_lines_drawn = 0;
                        self.action_reset_defaults(input, output)?;
                    }
                    4 => {
                        self.save()?;
                        write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                        writeln!(
                            output,
                            "{BOLD_GREEN}✔{RESET} Configuration saved to {BOLD_WHITE}{}{RESET}",
                            self.config_path.display()
                        )?;
                        output.flush()?;
                        return Ok(true);
                    }
                    5 => {
                        write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                        writeln!(output, "{DIM}Configuration changes discarded.{RESET}")?;
                        output.flush()?;
                        return Ok(false);
                    }
                    _ => {}
                },
                Key::Esc => {
                    write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                    writeln!(output, "{DIM}Configuration cancelled.{RESET}")?;
                    output.flush()?;
                    return Ok(false);
                }
                _ => {}
            }
        }
    }

    /// Sub-menu for choosing packs default behavior.
    fn menu_packs_behavior<R: Read, W: Write>(
        &mut self,
        input: &mut R,
        output: &mut W,
    ) -> anyhow::Result<()> {
        let options = [
            ("[Back]", "Return to main menu"),
            ("list", "Displays all available packs and components (default)"),
            ("default", "Automatically installs the pack specified in recipe default_pack"),
            ("error", "Raises an error requiring an explicit pack or component"),
        ];

        let mut selected = match self.packs_behavior.as_str() {
            "default" => 2,
            "error" => 3,
            _ => 1,
        };

        let mut last_lines_drawn = 0;

        loop {
            if last_lines_drawn > 0 {
                write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
            }
            let mut lines = 0;
            writeln!(
                output,
                " {BOLD_CYAN}fa config{RESET} {DIM}›{RESET} {BOLD_WHITE}Packs Default Behavior{RESET}"
            )?;
            lines += 1;
            writeln!(
                output,
                " {DIM}Select behavior when 'fa --new <recipe>' is run without a pack argument:{RESET}\n"
            )?;
            lines += 2;

            for (i, (val, desc)) in options.iter().enumerate() {
                let is_sel = i == selected;
                let marker = if is_sel {
                    format!("{BOLD_GREEN}▸{RESET}")
                } else {
                    " ".to_string()
                };

                let current_marker = if *val == self.packs_behavior.as_str() {
                    format!(" {BOLD_GREEN}✓{RESET}")
                } else {
                    String::new()
                };

                if is_sel {
                    writeln!(
                        output,
                        "  {marker} {BOLD_WHITE}{:<8}{RESET} {DIM}-{RESET} {desc}{current_marker}",
                        val
                    )?;
                } else {
                    writeln!(
                        output,
                        "  {marker} {:<8} {DIM}-{RESET} {DIM}{desc}{RESET}{current_marker}",
                        val
                    )?;
                }
                lines += 1;
            }
            writeln!(
                output,
                "\n {DIM}Navigate with {RESET}{BOLD_CYAN}↑/↓{RESET}{DIM}, {RESET}{BOLD_CYAN}Enter{RESET}{DIM} to select, {RESET}{BOLD_CYAN}Esc{RESET}{DIM} to return{RESET}"
            )?;
            lines += 2;

            last_lines_drawn = lines;
            output.flush()?;

            let key = read_key_from(input)?;
            match key {
                Key::Up => {
                    if selected == 0 {
                        selected = options.len() - 1;
                    } else {
                        selected -= 1;
                    }
                }
                Key::Down => {
                    if selected + 1 >= options.len() {
                        selected = 0;
                    } else {
                        selected += 1;
                    }
                }
                Key::Enter => {
                    if selected > 0 && selected < options.len() {
                        let chosen = options[selected].0;
                        self.packs_behavior = chosen.to_string();
                        self.status_message = Some(format!(
                            "{BOLD_GREEN}✔ '{chosen}' applied successfully{RESET}"
                        ));
                    }
                    if last_lines_drawn > 0 {
                        write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                        output.flush()?;
                    }
                    return Ok(());
                }
                Key::Esc => {
                    if last_lines_drawn > 0 {
                        write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                        output.flush()?;
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    /// Sub-menu for managing aliases: table view, adding new aliases, deleting existing aliases.
    fn menu_aliases<R: Read, W: Write>(
        &mut self,
        input: &mut R,
        output: &mut W,
    ) -> anyhow::Result<()> {
        let mut selected = 0;
        let mut last_lines_drawn = 0;

        loop {
            let alias_list: Vec<(String, String)> = self
                .aliases
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            // Total selectable rows:
            // 0: [Back to main menu]
            // 1: [+ Add new alias]
            // 2..=len+1: existing aliases
            let total_rows = alias_list.len() + 2;
            if selected >= total_rows {
                selected = total_rows.saturating_sub(1);
            }

            if last_lines_drawn > 0 {
                write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
            }
            let mut lines = 0;
            writeln!(
                output,
                " {BOLD_CYAN}fa config{RESET} {DIM}›{RESET} {BOLD_WHITE}Command Aliases{RESET}"
            )?;
            lines += 1;
            writeln!(output)?;
            lines += 1;

            // Render [Back to main menu]
            let is_back = selected == 0;
            let back_marker = if is_back {
                format!("{BOLD_GREEN}▸{RESET}")
            } else {
                " ".to_string()
            };
            if is_back {
                writeln!(
                    output,
                    "  {back_marker} {BOLD_WHITE}[Back to main menu]{RESET}"
                )?;
            } else {
                writeln!(output, "  {back_marker} [Back to main menu]")?;
            }
            lines += 1;

            // Render [+ Add new alias]
            let is_add = selected == 1;
            let add_marker = if is_add {
                format!("{BOLD_GREEN}▸{RESET}")
            } else {
                " ".to_string()
            };
            if is_add {
                writeln!(
                    output,
                    "  {add_marker} {BOLD_GREEN}[+ Add new alias]{RESET}\n"
                )?;
            } else {
                writeln!(output, "  {add_marker} [+ Add new alias]\n")?;
            }
            lines += 2;

            // Calculate max width for alias column
            let max_k_len = alias_list
                .iter()
                .map(|(k, _)| k.len())
                .max()
                .unwrap_or(5)
                .max(5);

            writeln!(
                output,
                "    {BOLD_WHITE}{:<width$}{RESET}     {BOLD_WHITE}TARGET COMMAND{RESET}",
                "ALIAS",
                width = max_k_len
            )?;
            lines += 1;
            writeln!(
                output,
                "    {DIM}{}{RESET}",
                "-".repeat(max_k_len.max(5) + 30)
            )?;
            lines += 1;

            if alias_list.is_empty() {
                writeln!(output, "    {DIM}(No aliases configured){RESET}")?;
                lines += 1;
            } else {
                for (idx, (k, v)) in alias_list.iter().enumerate() {
                    let row_idx = idx + 2;
                    let is_sel = selected == row_idx;
                    let marker = if is_sel {
                        format!("{BOLD_GREEN}▸{RESET}")
                    } else {
                        " ".to_string()
                    };

                    let badge = if v.starts_with('!') {
                        format!("{BOLD_MAGENTA}[shell]{RESET}")
                    } else {
                        format!("{BOLD_CYAN}[native]{RESET}")
                    };

                    if is_sel {
                        writeln!(
                            output,
                            "  {marker} {BOLD_YELLOW}{:<width$}{RESET}  ->  {BOLD_WHITE}{}{RESET}  {badge}",
                            k,
                            v,
                            width = max_k_len
                        )?;
                    } else {
                        writeln!(
                            output,
                            "  {marker} {:<width$}  {DIM}->{RESET}  {}  {badge}",
                            k,
                            v,
                            width = max_k_len
                        )?;
                    }
                    lines += 1;
                }
            }

            writeln!(
                output,
                "    {DIM}{}{RESET}",
                "-".repeat(max_k_len.max(5) + 30)
            )?;
            lines += 1;

            writeln!(
                output,
                "\n {DIM}Navigate with {RESET}{BOLD_CYAN}↑/↓{RESET}{DIM}, {RESET}{BOLD_CYAN}Enter{RESET}{DIM} to select/delete, {RESET}{BOLD_CYAN}Esc{RESET}{DIM} to return{RESET}"
            )?;
            lines += 2;

            last_lines_drawn = lines;
            output.flush()?;

            let key = read_key_from(input)?;
            match key {
                Key::Up => {
                    if selected == 0 {
                        selected = total_rows - 1;
                    } else {
                        selected -= 1;
                    }
                }
                Key::Down => {
                    if selected + 1 >= total_rows {
                        selected = 0;
                    } else {
                        selected += 1;
                    }
                }
                Key::Enter => {
                    if selected == 0 {
                        // [Back to main menu]
                        if last_lines_drawn > 0 {
                            write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                            output.flush()?;
                        }
                        return Ok(());
                    } else if selected == 1 {
                        // [+ Add new alias]
                        if last_lines_drawn > 0 {
                            write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                            output.flush()?;
                            last_lines_drawn = 0;
                        }
                        self.action_add_alias(input, output)?;
                    } else {
                        // Existing alias selected: offer deletion
                        let alias_to_delete = alias_list[selected - 2].0.clone();
                        if last_lines_drawn > 0 {
                            write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                            output.flush()?;
                            last_lines_drawn = 0;
                        }
                        self.prompt_delete_alias(&alias_to_delete, input, output)?;
                    }
                }
                Key::Esc => {
                    if last_lines_drawn > 0 {
                        write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                        output.flush()?;
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    /// Prompts confirmation to delete an alias.
    fn prompt_delete_alias<R: Read, W: Write>(
        &mut self,
        alias_name: &str,
        input: &mut R,
        output: &mut W,
    ) -> anyhow::Result<()> {
        let mut last_lines_drawn = 0;
        let mut delete_sel = 0;
        loop {
            if last_lines_drawn > 0 {
                write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
            }
            let mut lines = 0;
            writeln!(
                output,
                " {BOLD_YELLOW}Delete alias '{alias_name}'?{RESET}\n"
            )?;
            lines += 2;
            let opts = ["Yes, delete this alias", "No, keep it"];
            for (i, opt) in opts.iter().enumerate() {
                let marker = if i == delete_sel {
                    format!("{BOLD_GREEN}▸{RESET}")
                } else {
                    " ".to_string()
                };
                if i == delete_sel {
                    writeln!(output, "  {marker} {BOLD_WHITE}{opt}{RESET}")?;
                } else {
                    writeln!(output, "  {marker} {opt}")?;
                }
                lines += 1;
            }

            writeln!(
                output,
                "\n {DIM}Navigate with {RESET}{BOLD_CYAN}↑/↓{RESET}{DIM}, {RESET}{BOLD_CYAN}Enter{RESET}{DIM} to confirm, {RESET}{BOLD_CYAN}Esc{RESET}{DIM} to cancel{RESET}"
            )?;
            lines += 2;

            last_lines_drawn = lines;
            output.flush()?;

            let key = read_key_from(input)?;
            match key {
                Key::Up | Key::Down => delete_sel = 1 - delete_sel,
                Key::Enter => {
                    if delete_sel == 0 {
                        self.aliases.remove(alias_name);
                        self.status_message = Some(format!(
                            "{BOLD_YELLOW}✔ Deleted alias '{alias_name}'{RESET}"
                        ));
                    }
                    if last_lines_drawn > 0 {
                        write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                        output.flush()?;
                    }
                    return Ok(());
                }
                Key::Esc => {
                    if last_lines_drawn > 0 {
                        write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                        output.flush()?;
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    /// Step-by-step wizard to add a new alias:
    /// 1. Line editor for alias name
    /// 2. Selectable native fa command picker
    fn action_add_alias<R: Read, W: Write>(
        &mut self,
        input: &mut R,
        output: &mut W,
    ) -> anyhow::Result<()> {
        let mut lines = 0;
        writeln!(
            output,
            " {BOLD_CYAN}fa config{RESET} {DIM}›{RESET} {BOLD_WHITE}Add New Command Alias{RESET}"
        )?;
        lines += 1;
        writeln!(
            output,
            " {DIM}Type the alias shortcut (e.g. 'c', 'st', 'b') and press Enter (Esc to cancel):{RESET}\n"
        )?;
        lines += 2;
        let mut last_lines_drawn = lines;
        output.flush()?;

        // 1. Line editor for alias name:
        let name_opt = prompt_line_raw(" Alias name: ", input, output)?;
        last_lines_drawn += 1;
        let Some(name) = name_opt else {
            if last_lines_drawn > 0 {
                write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                output.flush()?;
            }
            return Ok(());
        };
        if name.trim().is_empty() {
            if last_lines_drawn > 0 {
                write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                output.flush()?;
            }
            return Ok(());
        }
        let name = name.trim().to_string();

        // 2. Select command from native command list:
        let mut choices: Vec<(String, String)> = Vec::new();
        choices.push(("[Cancel]".to_string(), "Cancel alias creation".to_string()));
        for (cmd, desc) in NATIVE_COMMANDS {
            choices.push((cmd.to_string(), desc.to_string()));
        }
        choices.push(("[Custom command...]".to_string(), "Enter a custom shell command".to_string()));

        let mut selected = 0;
        let mut picker_lines_drawn = last_lines_drawn;
        let final_cmd = loop {
            if picker_lines_drawn > 0 {
                write!(output, "\r\x1b[{}A\x1b[J", picker_lines_drawn)?;
            }
            let mut plines = 0;
            writeln!(
                output,
                " {BOLD_CYAN}Select Target Command for '{name}':{RESET}"
            )?;
            plines += 1;
            writeln!(output)?;
            plines += 1;

            for (i, (cmd, desc)) in choices.iter().enumerate() {
                let is_sel = i == selected;
                let marker = if is_sel {
                    format!("{BOLD_GREEN}▸{RESET}")
                } else {
                    " ".to_string()
                };

                if is_sel {
                    writeln!(
                        output,
                        "  {marker} {BOLD_WHITE}{:<22}{RESET} {DIM}-{RESET} {desc}",
                        cmd
                    )?;
                } else {
                    writeln!(
                        output,
                        "  {marker} {:<22} {DIM}-{RESET} {DIM}{desc}{RESET}",
                        cmd
                    )?;
                }
                plines += 1;
            }

            writeln!(
                output,
                "\n {DIM}Navigate with {RESET}{BOLD_CYAN}↑/↓{RESET}{DIM}, {RESET}{BOLD_CYAN}Enter{RESET}{DIM} to select, {RESET}{BOLD_CYAN}Esc{RESET}{DIM} to cancel{RESET}"
            )?;
            plines += 2;

            picker_lines_drawn = plines;
            output.flush()?;

            let key = read_key_from(input)?;
            match key {
                Key::Up => {
                    if selected == 0 {
                        selected = choices.len() - 1;
                    } else {
                        selected -= 1;
                    }
                }
                Key::Down => {
                    if selected + 1 >= choices.len() {
                        selected = 0;
                    } else {
                        selected += 1;
                    }
                }
                Key::Enter => {
                    if selected == 0 {
                        // Cancel
                        if picker_lines_drawn > 0 {
                            write!(output, "\r\x1b[{}A\x1b[J", picker_lines_drawn)?;
                            output.flush()?;
                        }
                        return Ok(());
                    } else if selected == choices.len() - 1 {
                        // Custom command
                        if picker_lines_drawn > 0 {
                            write!(output, "\r\x1b[{}A\x1b[J", picker_lines_drawn)?;
                            output.flush()?;
                        }
                        writeln!(
                            output,
                            " {BOLD_CYAN}Custom Command for '{name}'{RESET}"
                        )?;
                        writeln!(
                            output,
                            " {DIM}Prefix external shell commands with '!' (e.g. '!git status'):{RESET}\n"
                        )?;
                        let custom = prompt_line_raw(" Command: ", input, output)?;
                        write!(output, "\r\x1b[3A\x1b[J")?;
                        output.flush()?;
                        if let Some(cmd_val) = custom
                            && !cmd_val.trim().is_empty()
                        {
                            let trimmed = cmd_val.trim();
                            if trimmed.starts_with('!') {
                                break trimmed.to_string();
                            } else {
                                break format!("!{trimmed}");
                            }
                        } else {
                            return Ok(());
                        }
                    } else {
                        let chosen = choices[selected].0.clone();
                        if picker_lines_drawn > 0 {
                            write!(output, "\r\x1b[{}A\x1b[J", picker_lines_drawn)?;
                            output.flush()?;
                        }
                        break chosen;
                    }
                }
                Key::Esc => {
                    if picker_lines_drawn > 0 {
                        write!(output, "\r\x1b[{}A\x1b[J", picker_lines_drawn)?;
                        output.flush()?;
                    }
                    return Ok(());
                }
                _ => {}
            }
        };

        self.status_message = Some(format!(
            "{BOLD_GREEN}✔ Added alias '{name}' -> '{final_cmd}'{RESET}"
        ));
        self.aliases.insert(name, final_cmd);
        Ok(())
    }

    /// Launches $EDITOR on ~/.config/fa/config.toml, reloading any changes afterwards.
    fn action_open_editor<W: Write>(&mut self, output: &mut W) -> anyhow::Result<()> {
        // Ensure file exists with current in-memory edits before opening
        self.save()?;

        // Suspend raw mode so editor has normal terminal access
        RawModeGuard::suspend();
        let edit_res = crate::recipe::open_editor(&self.config_path);
        RawModeGuard::resume();
        edit_res?;

        // Reload content from disk in case user edited it in the editor
        if self.config_path.is_file() {
            let reloaded = fs::read_to_string(&self.config_path)?;
            self.initial_content = reloaded.clone();
            if let Ok(cfg) = crate::config::parse_global_config(&reloaded) {
                self.packs_behavior = cfg.packs.default_behavior;
                self.aliases = cfg.alias;
            }
        }

        // Re-hide cursor for menu
        print!("\x1b[?25l");
        let _ = output.flush();
        Ok(())
    }

    /// Resets configuration to default template.
    fn action_reset_defaults<R: Read, W: Write>(
        &mut self,
        input: &mut R,
        output: &mut W,
    ) -> anyhow::Result<()> {
        let mut last_lines_drawn = 0;
        let mut sel = 1; // default to No
        loop {
            if last_lines_drawn > 0 {
                write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
            }
            let mut lines = 0;
            writeln!(
                output,
                " {BOLD_YELLOW}Reset all configuration to initial defaults?{RESET}\n"
            )?;
            lines += 2;
            let opts = ["Yes, reset to defaults", "No, keep current settings"];
            for (i, opt) in opts.iter().enumerate() {
                let marker = if i == sel {
                    format!("{BOLD_GREEN}▸{RESET}")
                } else {
                    " ".to_string()
                };
                if i == sel {
                    writeln!(output, "  {marker} {BOLD_WHITE}{opt}{RESET}")?;
                } else {
                    writeln!(output, "  {marker} {opt}")?;
                }
                lines += 1;
            }

            writeln!(
                output,
                "\n {DIM}Navigate with {RESET}{BOLD_CYAN}↑/↓{RESET}{DIM}, {RESET}{BOLD_CYAN}Enter{RESET}{DIM} to select, {RESET}{BOLD_CYAN}Esc{RESET}{DIM} to cancel{RESET}"
            )?;
            lines += 2;

            last_lines_drawn = lines;
            output.flush()?;

            let key = read_key_from(input)?;
            match key {
                Key::Up | Key::Down => sel = 1 - sel,
                Key::Enter => {
                    if sel == 0 {
                        let parsed = crate::config::parse_global_config(EXAMPLE_GLOBAL_CONFIG)
                            .unwrap_or_default();
                        self.packs_behavior = parsed.packs.default_behavior;
                        self.aliases = parsed.alias;
                        self.status_message = Some(format!(
                            "{BOLD_YELLOW}✔ Reset configuration to defaults (unsaved){RESET}"
                        ));
                    }
                    if last_lines_drawn > 0 {
                        write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                        output.flush()?;
                    }
                    return Ok(());
                }
                Key::Esc => {
                    if last_lines_drawn > 0 {
                        write!(output, "\r\x1b[{}A\x1b[J", last_lines_drawn)?;
                        output.flush()?;
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    /// Persists configuration to disk using `apply_config_delta`.
    pub fn save(&self) -> anyhow::Result<()> {
        let delta = ConfigDelta {
            packs_behavior: Some(self.packs_behavior.clone()),
            aliases: self.aliases.clone(),
            reset_to_defaults: false,
        };

        let updated_toml = apply_config_delta(&self.initial_content, &delta)?;
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.config_path, updated_toml)?;
        Ok(())
    }
}

/// Internal runner allowing dependency injection of terminal state, input, and output.
pub fn run_interactive_config_inner<R: Read, W: Write>(
    user_dir: &Path,
    is_terminal: bool,
    input: &mut R,
    output: &mut W,
) -> anyhow::Result<()> {
    if !is_terminal {
        writeln!(
            output,
            "info: no interactive terminal (TTY) detected; skipping interactive visual config."
        )?;
        writeln!(
            output,
            "To configure fast-alias, run 'fa --config' in an interactive terminal or edit {} directly.",
            user_dir.join("config.toml").display()
        )?;
        return Ok(());
    }

    let _guard = RawModeGuard::enter()?;
    let mut editor = ConfigEditor::new(user_dir)?;
    editor.run(input, output)?;
    Ok(())
}

/// Entrypoint for interactive configuration from CLI.
pub fn run_interactive_config(user_dir: &Path) -> anyhow::Result<()> {
    let is_terminal = io::stdin().is_terminal();
    let mut raw_stdin = RawTerminalStdin;
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

    run_interactive_config_inner(user_dir, is_terminal, &mut raw_stdin, &mut stdout_lock)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sparse_config_preserves_comments_and_spacing() {
        let original_toml = r#"# User header comment
# Spacing preserved below

[packs]
# Important comment about packs behavior
default_behavior = "list"

[alias]
# Custom user comment on aliases
n = "--new"
l = "--list"
"#;

        let mut aliases = BTreeMap::new();
        aliases.insert("n".to_string(), "--new".to_string());
        aliases.insert("l".to_string(), "--list".to_string());
        aliases.insert("c".to_string(), "--config".to_string());

        let delta = ConfigDelta {
            packs_behavior: Some("default".to_string()),
            aliases,
            reset_to_defaults: false,
        };

        let result = apply_config_delta(original_toml, &delta).unwrap();

        assert!(
            result.contains("# User header comment"),
            "Must preserve header comments"
        );
        assert!(
            result.contains("# Important comment about packs behavior"),
            "Must preserve section comments"
        );
        assert!(
            result.contains("# Custom user comment on aliases"),
            "Must preserve alias comments"
        );
        assert!(
            result.contains("default_behavior = \"default\""),
            "Must update default_behavior"
        );
        assert!(
            result.contains("c = \"--config\""),
            "Must add newly introduced alias"
        );
    }

    #[test]
    fn test_sparse_config_does_not_add_default_packs_behavior() {
        let minimal_toml = r#"[alias]
n = "--new"
"#;

        let mut aliases = BTreeMap::new();
        aliases.insert("n".to_string(), "--new".to_string());

        let delta = ConfigDelta {
            packs_behavior: Some("list".to_string()),
            aliases,
            reset_to_defaults: false,
        };

        let result = apply_config_delta(minimal_toml, &delta).unwrap();
        assert!(
            !result.contains("[packs]"),
            "Sparse configuration must not insert [packs] table when left at default 'list'"
        );
        assert!(
            !result.contains("default_behavior"),
            "Sparse configuration must not insert default_behavior when left at default 'list'"
        );
    }

    #[test]
    fn test_alias_deletion_in_delta() {
        let toml_with_aliases = r#"[alias]
n = "--new"
rm_me = "--recipe rm"
"#;

        let mut aliases = BTreeMap::new();
        aliases.insert("n".to_string(), "--new".to_string());

        let delta = ConfigDelta {
            packs_behavior: None,
            aliases,
            reset_to_defaults: false,
        };

        let result = apply_config_delta(toml_with_aliases, &delta).unwrap();
        assert!(result.contains("n = \"--new\""));
        assert!(
            !result.contains("rm_me"),
            "Deleted alias must be removed from config"
        );
    }

    #[test]
    fn test_reset_to_defaults() {
        let custom_toml = r#"[packs]
default_behavior = "error"
[alias]
custom = "!echo hello"
"#;

        let delta = ConfigDelta {
            packs_behavior: None,
            aliases: BTreeMap::new(),
            reset_to_defaults: true,
        };

        let result = apply_config_delta(custom_toml, &delta).unwrap();
        assert_eq!(result, EXAMPLE_GLOBAL_CONFIG);
    }

    #[test]
    fn test_line_editor_characters_and_backspace() {
        let input_bytes = b"myalias\x7f\x7fs\r";
        let mut reader = &input_bytes[..];
        let mut output = Vec::new();

        let line = prompt_line_raw("Prompt: ", &mut reader, &mut output)
            .unwrap()
            .expect("Line must return Some");
        assert_eq!(line, "myalis");
    }

    #[test]
    fn test_line_editor_escape_cancels() {
        let input_bytes = b"abc\x1b";
        let mut reader = &input_bytes[..];
        let mut output = Vec::new();

        let line = prompt_line_raw("Prompt: ", &mut reader, &mut output).unwrap();
        assert_eq!(line, None);
    }

    #[test]
    fn test_menu_state_esc_cancels() {
        let temp_dir = std::env::temp_dir().join(format!("fa-test-cfg-esc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut editor = ConfigEditor::new(&temp_dir).unwrap();
        let input_bytes = b"\x1b";
        let mut reader = &input_bytes[..];
        let mut output = Vec::new();

        let res = editor.run(&mut reader, &mut output).unwrap();
        assert!(!res, "Esc in main menu must cancel and return false");
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_menu_state_save_and_exit() {
        let temp_dir = std::env::temp_dir().join(format!("fa-test-cfg-save-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut editor = ConfigEditor::new(&temp_dir).unwrap();
        editor.aliases.insert("c".to_string(), "--config".to_string());

        // 4 Down keys, then Enter:
        // Key::Down is not sent directly via single bytes without poll,
        // but we can test save() directly and cancel via Esc:
        editor.save().unwrap();

        let saved = fs::read_to_string(temp_dir.join("config.toml")).unwrap();
        assert!(saved.contains("c = \"--config\""));
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_read_key_mapping() {
        let mut enter_bytes = &b"\r"[..];
        assert_eq!(read_key_from(&mut enter_bytes).unwrap(), Key::Enter);

        let mut newline_bytes = &b"\n"[..];
        assert_eq!(read_key_from(&mut newline_bytes).unwrap(), Key::Enter);

        let mut backspace_bytes = &b"\x7f"[..];
        assert_eq!(read_key_from(&mut backspace_bytes).unwrap(), Key::Backspace);

        let mut ctrl_c_bytes = &b"\x03"[..];
        assert_eq!(read_key_from(&mut ctrl_c_bytes).unwrap(), Key::Esc);

        let mut char_bytes = &b"x"[..];
        assert_eq!(read_key_from(&mut char_bytes).unwrap(), Key::Char('x'));

        let mut up_bytes = &b"\x1b[A"[..];
        assert_eq!(read_key_from(&mut up_bytes).unwrap(), Key::Up);

        let mut down_bytes = &b"\x1b[B"[..];
        assert_eq!(read_key_from(&mut down_bytes).unwrap(), Key::Down);
    }

    #[test]
    fn test_tab_cycles_packs_behavior_inline() {
        let temp_dir =
            std::env::temp_dir().join(format!("fa-test-cfg-tabcycle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut editor = ConfigEditor::new(&temp_dir).unwrap();
        // Tab, Tab, Shift-Tab (cycles backwards), then Esc discards the pending
        // value; the trailing EOF read is reported as Esc and leaves the menu.
        let input_bytes = b"\t\t\x1b[Z\x1b";
        let mut reader = &input_bytes[..];
        let mut output = Vec::new();

        let res = editor.run(&mut reader, &mut output).unwrap();
        assert!(!res, "Main menu must cancel (no save) after cycling");

        let out = String::from_utf8_lossy(&output);
        assert!(
            out.contains("[ default ]"),
            "Tab must cycle 'list' -> 'default' inline, rendered output:\n{out}"
        );
        assert!(
            out.contains("[ error ]"),
            "Tab must cycle 'default' -> 'error' inline, rendered output:\n{out}"
        );
        assert!(
            !out.contains("{DIM}") && !out.contains("{RESET}"),
            "Footer placeholders must be color-expanded, not emitted literally, output:\n{out}"
        );
        assert_eq!(
            editor.packs_behavior, "list",
            "Cycling alone must never modify the confirmed value"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_enter_confirms_tab_cycled_packs_behavior() {
        let temp_dir =
            std::env::temp_dir().join(format!("fa-test-cfg-tabconfirm-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut editor = ConfigEditor::new(&temp_dir).unwrap();
        // Tab cicles to 'default' (pending), Enter confirms it.
        let input_bytes = b"\t\r";
        let mut reader = &input_bytes[..];
        let mut output = Vec::new();

        let res = editor.run(&mut reader, &mut output).unwrap();
        assert!(!res, "Main menu must cancel (no save) after confirming");

        assert_eq!(
            editor.packs_behavior, "default",
            "Enter with a pending Tab-cycled value must commit it"
        );
        let out = String::from_utf8_lossy(&output);
        assert!(
            out.contains("✔ 'default' applied successfully"),
            "Minimal confirmation status message must be rendered, output:\n{out}"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_main_menu_rendered_categories_and_indentation() {
        let temp_dir =
            std::env::temp_dir().join(format!("fa-test-cfg-cat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut editor = ConfigEditor::new(&temp_dir).unwrap();
        let input_bytes = b"\x1b";
        let mut reader = &input_bytes[..];
        let mut output = Vec::new();

        let res = editor.run(&mut reader, &mut output).unwrap();
        assert!(!res);

        let out = String::from_utf8_lossy(&output);
        assert!(
            out.contains(&format!(" {BOLD_WHITE}SETTINGS{RESET}")),
            "Main menu category title must be rendered in BOLD_WHITE"
        );
        assert!(
            out.contains(&format!("  {BOLD_GREEN}▸{RESET} ")),
            "Selected item under category must be indented with 2 spaces"
        );
        assert!(
            !out.contains("{DIM}"),
            "Literal {{DIM}} placeholder must not leak into rendered output"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_reserved_status_slot_preserves_exact_height() {
        let temp_dir =
            std::env::temp_dir().join(format!("fa-test-cfg-height-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut editor_no_msg = ConfigEditor::new(&temp_dir).unwrap();
        let mut out_no_msg = Vec::new();
        editor_no_msg.run(&mut &b"\x1b"[..], &mut out_no_msg).unwrap();
        let lines_no_msg = String::from_utf8_lossy(&out_no_msg).lines().count();

        let mut editor_with_msg = ConfigEditor::new(&temp_dir).unwrap();
        editor_with_msg.status_message = Some("✔ 'default' applied successfully".to_string());
        let mut out_with_msg = Vec::new();
        editor_with_msg.run(&mut &b"\x1b"[..], &mut out_with_msg).unwrap();
        let lines_with_msg = String::from_utf8_lossy(&out_with_msg).lines().count();

        assert_eq!(
            lines_no_msg, lines_with_msg,
            "Layout height must remain constant: status slot must be reserved"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_menu_aliases_renders_shortcut_hint_at_bottom() {
        let temp_dir =
            std::env::temp_dir().join(format!("fa-test-cfg-aliasbottom-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut editor = ConfigEditor::new(&temp_dir).unwrap();
        let mut out = Vec::new();
        editor.menu_aliases(&mut &b"\x1b"[..], &mut out).unwrap();

        let rendered = String::from_utf8_lossy(&out);
        let back_pos = rendered.find("[Back to main menu]").expect("Must render [Back to main menu]");
        let hint_pos = rendered.find("Navigate with").expect("Must render navigation hint");

        assert!(
            hint_pos > back_pos,
            "Navigation shortcut hint must appear at the BOTTOM, after options: back_pos={back_pos}, hint_pos={hint_pos}"
        );
        assert!(
            rendered.contains(&format!("{BOLD_CYAN}↑/↓{RESET}")),
            "Menu aliases footer must highlight ↑/↓ in BOLD_CYAN"
        );
        assert!(
            rendered.contains(&format!("{BOLD_CYAN}Enter{RESET}")),
            "Menu aliases footer must highlight Enter in BOLD_CYAN"
        );
        assert!(
            rendered.contains(&format!("{BOLD_CYAN}Esc{RESET}")),
            "Menu aliases footer must highlight Esc in BOLD_CYAN"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_footer_shortcuts_are_highlighted_with_color() {
        let temp_dir =
            std::env::temp_dir().join(format!("fa-test-cfg-shortcol-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut editor = ConfigEditor::new(&temp_dir).unwrap();
        let mut out = Vec::new();
        editor.run(&mut &b"\x1b"[..], &mut out).unwrap();

        let rendered = String::from_utf8_lossy(&out);
        assert!(
            rendered.contains(&format!("{BOLD_CYAN}↑/↓{RESET}")),
            "Shortcut ↑/↓ must be highlighted in BOLD_CYAN, rendered:\n{rendered}"
        );
        assert!(
            rendered.contains(&format!("{BOLD_CYAN}Enter{RESET}")),
            "Shortcut Enter must be highlighted in BOLD_CYAN, rendered:\n{rendered}"
        );
        assert!(
            rendered.contains(&format!("{BOLD_CYAN}Tab{RESET}")),
            "Shortcut Tab must be highlighted in BOLD_CYAN, rendered:\n{rendered}"
        );
        assert!(
            rendered.contains(&format!("{BOLD_CYAN}Esc{RESET}")),
            "Shortcut Esc must be highlighted in BOLD_CYAN, rendered:\n{rendered}"
        );

        // Also test the pending Tab cycling state footer
        let mut out_tab = Vec::new();
        editor.run(&mut &b"\t\x1b"[..], &mut out_tab).unwrap();
        let rendered_tab = String::from_utf8_lossy(&out_tab);
        assert!(
            rendered_tab.contains(&format!("{BOLD_CYAN}Tab{RESET}{DIM}/{RESET}{BOLD_CYAN}Shift-Tab{RESET}")),
            "Pending cycling footer must highlight Tab/Shift-Tab in BOLD_CYAN, rendered:\n{rendered_tab}"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_arrow_discards_tab_cycled_packs_behavior() {
        // Down arrow: discards the pending value, then moves the cursor.
        let temp_dir =
            std::env::temp_dir().join(format!("fa-test-cfg-tabdown-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut editor = ConfigEditor::new(&temp_dir).unwrap();
        let input_bytes = b"\t\x1b[B";
        let mut reader = &input_bytes[..];
        let mut output = Vec::new();
        let res = editor.run(&mut reader, &mut output).unwrap();
        assert!(!res, "Main menu must cancel (no save) after navigating");

        assert_eq!(
            editor.packs_behavior, "list",
            "Down arrow must restore the confirmed value"
        );
        let out = String::from_utf8_lossy(&output);
        assert_eq!(
            out.matches("[ default ]").count(),
            1,
            "Pending value must render exactly once and be discarded, output:\n{out}"
        );
        assert!(
            out.contains(&format!(
                "  {BOLD_GREEN}▸{RESET} {BOLD_WHITE}Command aliases{RESET}"
            )),
            "Cursor must move down after discarding, output:\n{out}"
        );
        let _ = fs::remove_dir_all(&temp_dir);

        // Up arrow: same discard behavior, cursor wraps to the last row.
        let temp_dir_up =
            std::env::temp_dir().join(format!("fa-test-cfg-tabup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir_up);
        fs::create_dir_all(&temp_dir_up).unwrap();

        let mut editor_up = ConfigEditor::new(&temp_dir_up).unwrap();
        let input_up = b"\t\x1b[A";
        let mut reader_up = &input_up[..];
        let mut output_up = Vec::new();
        let res_up = editor_up.run(&mut reader_up, &mut output_up).unwrap();
        assert!(!res_up, "Main menu must cancel (no save) after navigating");

        assert_eq!(
            editor_up.packs_behavior, "list",
            "Up arrow must restore the confirmed value"
        );
        let out_up = String::from_utf8_lossy(&output_up);
        assert_eq!(
            out_up.matches("[ default ]").count(),
            1,
            "Pending value must render exactly once and be discarded, output:\n{out_up}"
        );
        assert!(
            out_up.contains(&format!("  {BOLD_GREEN}▸{RESET} {BOLD_WHITE}Cancel{RESET}")),
            "Cursor must wrap up to the last row after discarding, output:\n{out_up}"
        );
        let _ = fs::remove_dir_all(&temp_dir_up);
    }

    #[test]
    fn test_non_interactive_run_interactive_config_exits_cleanly() {
        let temp_dir = std::env::temp_dir().join(format!("fa-test-cfg-nonint-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut input = std::io::Cursor::new(b"");
        let mut output = Vec::new();
        let result = run_interactive_config_inner(&temp_dir, false, &mut input, &mut output);
        assert!(result.is_ok(), "Must exit cleanly when not a TTY");

        let out_str = String::from_utf8_lossy(&output);
        assert!(out_str.contains("no interactive terminal (TTY) detected"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    fn strip_ansi_codes(s: &str) -> String {
        let mut result = String::new();
        let mut in_escape = false;
        for c in s.chars() {
            if c == '\x1b' {
                in_escape = true;
            } else if in_escape {
                if c.is_ascii_alphabetic() {
                    in_escape = false;
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    #[test]
    fn test_main_menu_save_shortcut_key_s() {
        let temp_dir = std::env::temp_dir().join(format!("fa-test-cfg-saveshort-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut editor = ConfigEditor::new(&temp_dir).unwrap();
        editor.packs_behavior = "error".to_string();
        let mut out = Vec::new();
        // Send 's' key to trigger save immediately
        let res = editor.run(&mut &b"s"[..], &mut out).unwrap();
        assert!(res, "Pressing 's' must save and return Ok(true)");

        // Verify the saved file on disk
        let saved_content = fs::read_to_string(temp_dir.join("config.toml")).unwrap();
        assert!(saved_content.contains("default_behavior = \"error\""));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_banner_box_borders_match_length_and_close() {
        let temp_dir = std::env::temp_dir().join(format!("fa-test-cfg-bannerbox-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut editor = ConfigEditor::new(&temp_dir).unwrap();
        let mut out = Vec::new();
        editor.run(&mut &b"\x1b"[..], &mut out).unwrap();

        let rendered = String::from_utf8_lossy(&out);
        let lines: Vec<&str> = rendered.lines().collect();
        let top_idx = lines.iter().position(|l| l.contains("╭")).expect("Must have ╭");
        let mid_idx = lines.iter().position(|l| l.contains("Fast-Alias Configuration")).expect("Must have title line");
        let bot_idx = lines.iter().position(|l| l.contains("╰")).expect("Must have ╰");

        assert_eq!(mid_idx, top_idx + 1);
        assert_eq!(bot_idx, top_idx + 2);

        let clean_top = strip_ansi_codes(lines[top_idx]);
        let clean_mid = strip_ansi_codes(lines[mid_idx]);
        let clean_bot = strip_ansi_codes(lines[bot_idx]);

        assert!(clean_mid.ends_with('│'), "Middle line must be closed with right border '│', got: {clean_mid}");
        assert_eq!(clean_top.chars().count(), clean_mid.chars().count(), "Top and middle must have same width");
        assert_eq!(clean_top.chars().count(), clean_bot.chars().count(), "Top and bottom must have same width");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_status_feedback_is_rendered_below_menu_above_shortcuts() {
        let temp_dir = std::env::temp_dir().join(format!("fa-test-cfg-statpos-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut editor = ConfigEditor::new(&temp_dir).unwrap();
        editor.status_message = Some("MY_TEST_STATUS_FEEDBACK".to_string());
        let mut out = Vec::new();
        editor.run(&mut &b"\x1b"[..], &mut out).unwrap();

        let rendered = String::from_utf8_lossy(&out);
        let cancel_pos = rendered.find("Cancel").expect("Must contain Cancel option");
        let status_pos = rendered.find("MY_TEST_STATUS_FEEDBACK").expect("Must contain status message");
        let footer_pos = rendered.find("Navigate with").expect("Must contain navigation hint");

        assert!(
            status_pos > cancel_pos,
            "Status message must be positioned BELOW the menu items (after Cancel): status_pos={status_pos}, cancel_pos={cancel_pos}"
        );
        assert!(
            status_pos < footer_pos,
            "Status message must be positioned ABOVE the shortcuts footer: status_pos={status_pos}, footer_pos={footer_pos}"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
