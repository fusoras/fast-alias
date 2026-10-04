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
    Enter,
    Esc,
    Backspace,
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
}

#[cfg(unix)]
#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

#[cfg(unix)]
#[allow(unsafe_code)]
unsafe extern "C" {
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
}

/// RAII Guard that manages raw terminal mode.
/// Restores the original terminal attributes and re-enables cursor on exit or panic.
pub struct RawModeGuard {
    #[cfg(unix)]
    orig_termios: Option<Termios>,
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
                let mut raw = orig;

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
                // Hide cursor during interactive menu navigation
                print!("\x1b[?25l");
                let _ = io::stdout().flush();
                return Ok(Self {
                    orig_termios: Some(orig),
                    active: true,
                });
            }
        }
        Ok(Self {
            #[cfg(unix)]
            orig_termios: None,
            active: false,
        })
    }

    pub fn restore(&mut self) {
        if self.active {
            #[cfg(unix)]
            {
                if let Some(ref orig) = self.orig_termios {
                    use std::os::fd::AsRawFd;
                    let fd = io::stdin().as_raw_fd();
                    #[allow(unsafe_code)]
                    unsafe {
                        tcsetattr(fd, 0, orig);
                    }
                }
            }
            // Show cursor and reset styles
            print!("\x1b[?25h\x1b[0m");
            let _ = io::stdout().flush();
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
#[cfg(unix)]
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

#[cfg(not(unix))]
fn poll_stdin(_timeout_ms: i32) -> bool {
    false
}

/// Reads a single key event from a generic reader.
pub fn read_key_from<R: Read>(reader: &mut R) -> io::Result<Key> {
    let mut buf = [0u8; 1];
    if reader.read(&mut buf)? == 0 {
        return Ok(Key::Esc);
    }

    match buf[0] {
        b'\r' | b'\n' => Ok(Key::Enter),
        0x7f | 0x08 => Ok(Key::Backspace),
        0x03 => Ok(Key::Esc), // Ctrl+C maps to Esc/Cancel
        0x1b => {
            // Check if another byte follows immediately (e.g. arrow keys)
            if !poll_stdin(25) {
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

/// Interactive TUI manager for fast-alias configuration.
pub struct ConfigEditor {
    config_path: PathBuf,
    initial_content: String,
    packs_behavior: String,
    aliases: BTreeMap<String, String>,
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
            aliases,
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

        loop {
            write!(output, "\x1b[H\x1b[2J")?; // Clear screen and home cursor
            writeln!(
                output,
                "{BOLD_CYAN}Fast-Alias Configuration{RESET} {DIM}({}){RESET}",
                self.config_path.display()
            )?;
            writeln!(
                output,
                "{DIM}Use ↑/↓ to navigate, Enter to select, Esc to cancel{RESET}\n"
            )?;

            for (i, item) in menu_items.iter().enumerate() {
                let is_sel = i == selected;
                let marker = if is_sel {
                    format!("{BOLD_CYAN}>{RESET}")
                } else {
                    " ".to_string()
                };

                let detail = match i {
                    0 => format!(" {DIM}[ {} ]{RESET}", self.packs_behavior),
                    1 => format!(" {DIM}({} defined){RESET}", self.aliases.len()),
                    _ => String::new(),
                };

                if is_sel {
                    writeln!(output, "  {marker} {BOLD_WHITE}{item}{RESET}{detail}")?;
                } else {
                    writeln!(output, "  {marker} {item}{detail}")?;
                }
            }
            output.flush()?;

            let key = read_key_from(input)?;
            match key {
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
                        self.menu_packs_behavior(input, output)?;
                    }
                    1 => {
                        self.menu_aliases(input, output)?;
                    }
                    2 => {
                        self.action_open_editor(output)?;
                    }
                    3 => {
                        self.action_reset_defaults(input, output)?;
                    }
                    4 => {
                        self.save()?;
                        write!(output, "\x1b[H\x1b[2J")?;
                        writeln!(
                            output,
                            "{BOLD_GREEN}✓{RESET} Configuration saved to {BOLD_WHITE}{}{RESET}",
                            self.config_path.display()
                        )?;
                        output.flush()?;
                        return Ok(true);
                    }
                    5 => {
                        write!(output, "\x1b[H\x1b[2J")?;
                        writeln!(output, "Configuration changes discarded.")?;
                        output.flush()?;
                        return Ok(false);
                    }
                    _ => {}
                },
                Key::Esc => {
                    write!(output, "\x1b[H\x1b[2J")?;
                    writeln!(output, "Configuration cancelled.")?;
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
            ("list", "Displays all available packs and components (default)"),
            ("default", "Automatically installs the pack specified in recipe default_pack"),
            ("error", "Raises an error requiring an explicit pack or component"),
            ("[Back]", "Return to main menu"),
        ];

        let mut selected = match self.packs_behavior.as_str() {
            "default" => 1,
            "error" => 2,
            _ => 0,
        };

        loop {
            write!(output, "\x1b[H\x1b[2J")?;
            writeln!(output, "{BOLD_CYAN}Packs Default Behavior{RESET}")?;
            writeln!(
                output,
                "{DIM}Select behavior when 'fa --new <recipe>' is run without a pack argument:{RESET}\n"
            )?;

            for (i, (val, desc)) in options.iter().enumerate() {
                let is_sel = i == selected;
                let marker = if is_sel {
                    format!("{BOLD_CYAN}>{RESET}")
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
            }
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
                    if selected < 3 {
                        self.packs_behavior = options[selected].0.to_string();
                    }
                    return Ok(());
                }
                Key::Esc => return Ok(()),
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

        loop {
            let alias_list: Vec<(String, String)> = self
                .aliases
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            // Total selectable rows:
            // 0: [+ Add new alias]
            // 1..=len: existing aliases
            // len + 1: [Back to main menu]
            let total_rows = alias_list.len() + 2;
            if selected >= total_rows {
                selected = total_rows.saturating_sub(1);
            }

            write!(output, "\x1b[H\x1b[2J")?;
            writeln!(output, "{BOLD_CYAN}Command Aliases Manager{RESET}")?;
            writeln!(
                output,
                "{DIM}↑/↓: navigate, Enter on alias to delete, Esc to return{RESET}\n"
            )?;

            // Render [+ Add new alias]
            let is_add = selected == 0;
            let add_marker = if is_add {
                format!("{BOLD_CYAN}>{RESET}")
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
            writeln!(
                output,
                "    {DIM}{}{RESET}",
                "-".repeat(max_k_len.max(5) + 30)
            )?;

            if alias_list.is_empty() {
                writeln!(output, "    {DIM}(No aliases configured){RESET}")?;
            } else {
                for (idx, (k, v)) in alias_list.iter().enumerate() {
                    let row_idx = idx + 1;
                    let is_sel = selected == row_idx;
                    let marker = if is_sel {
                        format!("{BOLD_CYAN}>{RESET}")
                    } else {
                        " ".to_string()
                    };

                    if is_sel {
                        writeln!(
                            output,
                            "  {marker} {BOLD_YELLOW}{:<width$}{RESET}  ->  {BOLD_WHITE}{}{RESET}",
                            k,
                            v,
                            width = max_k_len
                        )?;
                    } else {
                        writeln!(
                            output,
                            "  {marker} {:<width$}  {DIM}->{RESET}  {}",
                            k,
                            v,
                            width = max_k_len
                        )?;
                    }
                }
            }

            writeln!(
                output,
                "    {DIM}{}{RESET}",
                "-".repeat(max_k_len.max(5) + 30)
            )?;

            let back_idx = alias_list.len() + 1;
            let is_back = selected == back_idx;
            let back_marker = if is_back {
                format!("{BOLD_CYAN}>{RESET}")
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
                        self.action_add_alias(input, output)?;
                    } else if selected == back_idx {
                        return Ok(());
                    } else {
                        // Existing alias selected: offer deletion
                        let alias_to_delete = alias_list[selected - 1].0.clone();
                        self.prompt_delete_alias(&alias_to_delete, input, output)?;
                    }
                }
                Key::Esc => return Ok(()),
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
        write!(output, "\x1b[H\x1b[2J")?;
        writeln!(
            output,
            "{BOLD_YELLOW}Delete alias '{alias_name}'?{RESET}\n"
        )?;
        writeln!(output, "  > Yes, delete this alias")?;
        writeln!(output, "    No, keep it")?;
        output.flush()?;

        let mut delete_sel = 0;
        loop {
            write!(output, "\x1b[H\x1b[2J")?;
            writeln!(
                output,
                "{BOLD_YELLOW}Delete alias '{alias_name}'?{RESET}\n"
            )?;
            let opts = ["Yes, delete this alias", "No, keep it"];
            for (i, opt) in opts.iter().enumerate() {
                let marker = if i == delete_sel {
                    format!("{BOLD_CYAN}>{RESET}")
                } else {
                    " ".to_string()
                };
                if i == delete_sel {
                    writeln!(output, "  {marker} {BOLD_WHITE}{opt}{RESET}")?;
                } else {
                    writeln!(output, "  {marker} {opt}")?;
                }
            }
            output.flush()?;

            let key = read_key_from(input)?;
            match key {
                Key::Up | Key::Down => delete_sel = 1 - delete_sel,
                Key::Enter => {
                    if delete_sel == 0 {
                        self.aliases.remove(alias_name);
                    }
                    return Ok(());
                }
                Key::Esc => return Ok(()),
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
        write!(output, "\x1b[H\x1b[2J")?;
        writeln!(output, "{BOLD_CYAN}Add New Command Alias{RESET}")?;
        writeln!(
            output,
            "{DIM}Type the alias shortcut (e.g. 'c', 'st', 'b') and press Enter (Esc to cancel):{RESET}\n"
        )?;

        // 1. Line editor for alias name:
        let name_opt = prompt_line_raw("Alias name: ", input, output)?;
        let Some(name) = name_opt else {
            return Ok(());
        };
        if name.trim().is_empty() {
            return Ok(());
        }
        let name = name.trim().to_string();

        // 2. Select command from native command list:
        let mut choices: Vec<(String, String)> = NATIVE_COMMANDS
            .iter()
            .map(|(cmd, desc)| (cmd.to_string(), desc.to_string()))
            .collect();
        choices.push(("[Custom command...]".to_string(), "Enter a custom shell command".to_string()));
        choices.push(("[Cancel]".to_string(), "Cancel alias creation".to_string()));

        let mut selected = 0;
        let final_cmd = loop {
            write!(output, "\x1b[H\x1b[2J")?;
            writeln!(
                output,
                "{BOLD_CYAN}Select Target Command for '{name}':{RESET}"
            )?;
            writeln!(
                output,
                "{DIM}↑/↓: navigate, Enter: select, Esc: cancel{RESET}\n"
            )?;

            for (i, (cmd, desc)) in choices.iter().enumerate() {
                let is_sel = i == selected;
                let marker = if is_sel {
                    format!("{BOLD_CYAN}>{RESET}")
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
            }
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
                    if selected == choices.len() - 1 {
                        // Cancel
                        return Ok(());
                    } else if selected == choices.len() - 2 {
                        // Custom command
                        write!(output, "\x1b[H\x1b[2J")?;
                        writeln!(
                            output,
                            "{BOLD_CYAN}Custom Command for '{name}'{RESET}"
                        )?;
                        writeln!(
                            output,
                            "{DIM}Prefix external shell commands with '!' (e.g. '!git status'):{RESET}\n"
                        )?;
                        let custom = prompt_line_raw("Command: ", input, output)?;
                        if let Some(cmd_val) = custom
                            && !cmd_val.trim().is_empty()
                        {
                            break cmd_val.trim().to_string();
                        } else {
                            return Ok(());
                        }
                    } else {
                        break choices[selected].0.clone();
                    }
                }
                Key::Esc => return Ok(()),
                _ => {}
            }
        };

        self.aliases.insert(name, final_cmd);
        Ok(())
    }

    /// Launches $EDITOR on ~/.config/fa/config.toml, reloading any changes afterwards.
    fn action_open_editor<W: Write>(&mut self, output: &mut W) -> anyhow::Result<()> {
        // Ensure file exists with current in-memory edits before opening
        self.save()?;

        // Suspend raw mode so editor has normal terminal access
        print!("\x1b[?25h\x1b[0m");
        let _ = io::stdout().flush();

        crate::recipe::open_editor(&self.config_path)?;

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
        write!(output, "\x1b[H\x1b[2J")?;
        writeln!(
            output,
            "{BOLD_YELLOW}Reset all configuration to initial defaults?{RESET}\n"
        )?;

        let mut sel = 1; // default to No
        loop {
            write!(output, "\x1b[H\x1b[2J")?;
            writeln!(
                output,
                "{BOLD_YELLOW}Reset all configuration to initial defaults?{RESET}\n"
            )?;
            let opts = ["Yes, reset to defaults", "No, keep current settings"];
            for (i, opt) in opts.iter().enumerate() {
                let marker = if i == sel {
                    format!("{BOLD_CYAN}>{RESET}")
                } else {
                    " ".to_string()
                };
                if i == sel {
                    writeln!(output, "  {marker} {BOLD_WHITE}{opt}{RESET}")?;
                } else {
                    writeln!(output, "  {marker} {opt}")?;
                }
            }
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
                    }
                    return Ok(());
                }
                Key::Esc => return Ok(()),
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
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdin_lock = stdin.lock();
    let mut stdout_lock = stdout.lock();

    run_interactive_config_inner(user_dir, is_terminal, &mut stdin_lock, &mut stdout_lock)
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
}
