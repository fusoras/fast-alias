//! Centralized ANSI color constants.
//!
//! All modules should import colors from here to avoid duplication
//! and inconsistent inline escape codes.

#![allow(dead_code)]

pub const BOLD_GREEN: &str = "\x1b[1;32m";
pub const BOLD_BLUE: &str = "\x1b[1;34m";
pub const BOLD_CYAN: &str = "\x1b[1;36m";
pub const BOLD_YELLOW: &str = "\x1b[1;33m";
pub const BOLD_RED: &str = "\x1b[1;31m";
pub const WHITE: &str = "\x1b[37m";
pub const DIM: &str = "\x1b[2m";
pub const DIM_GRAY: &str = "\x1b[90m";
pub const RESET: &str = "\x1b[0m";
