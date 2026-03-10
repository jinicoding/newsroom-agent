//! Formatting helpers: ANSI colors, cost, duration, tokens, context bar, truncation.

use std::sync::OnceLock;

// --- Color support with NO_COLOR and --no-color ---

/// Whether color output has been disabled (via NO_COLOR env or --no-color flag).
static COLOR_DISABLED: OnceLock<bool> = OnceLock::new();

/// Disable color output. Call before any formatting happens (e.g., from CLI arg parsing).
pub fn disable_color() {
    let _ = COLOR_DISABLED.set(true);
}

/// Check if color output is enabled. Cached after first call.
/// Respects the NO_COLOR environment variable (https://no-color.org/).
fn color_enabled() -> bool {
    !*COLOR_DISABLED.get_or_init(|| std::env::var("NO_COLOR").is_ok())
}

/// A color code that respects the NO_COLOR convention.
/// When color is disabled, formats as an empty string.
pub struct Color(pub &'static str);

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if color_enabled() {
            f.write_str(self.0)
        } else {
            Ok(())
        }
    }
}

// ANSI color helpers — respect NO_COLOR env var and --no-color flag
pub static RESET: Color = Color("\x1b[0m");
pub static BOLD: Color = Color("\x1b[1m");
pub static DIM: Color = Color("\x1b[2m");
pub static GREEN: Color = Color("\x1b[32m");
pub static YELLOW: Color = Color("\x1b[33m");
pub static CYAN: Color = Color("\x1b[36m");
pub static RED: Color = Color("\x1b[31m");
pub static BOLD_CYAN: Color = Color("\x1b[1;36m");

// --- Syntax highlighting for code blocks ---

/// Languages recognized for syntax highlighting.
fn normalize_lang(lang: &str) -> Option<&'static str> {
    match lang.to_lowercase().as_str() {
        "rust" | "rs" => Some("rust"),
        "python" | "py" => Some("python"),
        "javascript" | "js" | "typescript" | "ts" | "jsx" | "tsx" => Some("js"),
        "go" | "golang" => Some("go"),
        "sh" | "bash" | "shell" | "zsh" => Some("shell"),
        _ => None,
    }
}

/// Get the keyword list for a normalized language.
fn lang_keywords(lang: &str) -> &'static [&'static str] {
    match lang {
        "rust" => &[
            "fn", "let", "mut", "if", "else", "for", "while", "loop", "match", "return", "use",
            "mod", "pub", "struct", "enum", "impl", "trait", "where", "async", "await", "move",
            "self", "super", "crate", "const", "static", "type", "as", "in", "ref", "true",
            "false", "Some", "None", "Ok", "Err",
        ],
        "python" => &[
            "def", "class", "if", "elif", "else", "for", "while", "return", "import", "from", "as",
            "with", "try", "except", "finally", "raise", "yield", "lambda", "pass", "break",
            "continue", "and", "or", "not", "in", "is", "None", "True", "False", "self", "async",
            "await",
        ],
        "js" => &[
            "function",
            "const",
            "let",
            "var",
            "if",
            "else",
            "for",
            "while",
            "return",
            "import",
            "export",
            "from",
            "class",
            "new",
            "this",
            "async",
            "await",
            "try",
            "catch",
            "finally",
            "throw",
            "typeof",
            "instanceof",
            "true",
            "false",
            "null",
            "undefined",
            "switch",
            "case",
            "default",
            "break",
            "continue",
            "interface",
            "type",
            "enum",
        ],
        "go" => &[
            "func",
            "var",
            "const",
            "if",
            "else",
            "for",
            "range",
            "return",
            "import",
            "package",
            "type",
            "struct",
            "interface",
            "map",
            "chan",
            "go",
            "defer",
            "select",
            "case",
            "switch",
            "default",
            "break",
            "continue",
            "nil",
            "true",
            "false",
        ],
        "shell" => &[
            "if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac",
            "function", "return", "exit", "echo", "export", "local", "readonly", "set", "unset",
            "in", "true", "false",
        ],
        _ => &[],
    }
}

/// Get the line-comment prefix for a normalized language.
fn comment_prefix(lang: &str) -> &'static str {
    match lang {
        "python" | "shell" => "#",
        _ => "//",
    }
}

/// Apply syntax-aware ANSI highlighting to a single code line.
///
/// Colorizes keywords (bold cyan), strings (green), comments (dim), and numbers (yellow).
/// Falls back to DIM when language is unrecognized.
pub fn highlight_code_line(lang: &str, line: &str) -> String {
    let norm = match normalize_lang(lang) {
        Some(n) => n,
        None => return format!("{DIM}{line}{RESET}"),
    };

    let cp = comment_prefix(norm);
    let trimmed = line.trim_start();

    // Full-line comment detection
    if trimmed.starts_with(cp) {
        return format!("{DIM}{line}{RESET}");
    }
    // Shell also supports # comments even when norm isn't "shell"
    // but we only check the language-appropriate prefix above

    let keywords = lang_keywords(norm);
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut result = String::with_capacity(line.len() + 64);
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        // Check for inline comment: // or # (at current position)
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '/' && cp == "//" {
            // Rest of line is a comment
            let rest: String = chars[i..].iter().collect();
            result.push_str(&format!("{DIM}{rest}{RESET}"));
            break;
        }
        if ch == '#' && cp == "#" {
            let rest: String = chars[i..].iter().collect();
            result.push_str(&format!("{DIM}{rest}{RESET}"));
            break;
        }

        // String literals: "..." or '...'
        if ch == '"' || ch == '\'' {
            let quote = ch;
            let mut s = String::new();
            s.push(ch);
            i += 1;
            while i < len {
                let c = chars[i];
                s.push(c);
                i += 1;
                if c == '\\' && i < len {
                    s.push(chars[i]);
                    i += 1;
                } else if c == quote {
                    break;
                }
            }
            result.push_str(&format!("{GREEN}{s}{RESET}"));
            continue;
        }

        // Numbers: digit sequences (possibly with . for floats)
        if ch.is_ascii_digit()
            && (i == 0 || !chars[i - 1].is_ascii_alphanumeric() && chars[i - 1] != '_')
        {
            let mut num = String::new();
            while i < len && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == '_') {
                num.push(chars[i]);
                i += 1;
            }
            // Don't color if followed by an alpha char (it's part of an identifier)
            if i < len && (chars[i].is_ascii_alphabetic() || chars[i] == '_') {
                result.push_str(&num);
            } else {
                result.push_str(&format!("{YELLOW}{num}{RESET}"));
            }
            continue;
        }

        // Word: check for keyword
        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut word = String::new();
            let start = i;
            while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                word.push(chars[i]);
                i += 1;
            }
            // Only highlight if it's a standalone keyword (not part of a larger identifier)
            let before_ok = start == 0
                || (!chars[start - 1].is_ascii_alphanumeric() && chars[start - 1] != '_');
            let after_ok = i >= len || (!chars[i].is_ascii_alphanumeric() && chars[i] != '_');
            if before_ok && after_ok && keywords.contains(&word.as_str()) {
                result.push_str(&format!("{BOLD_CYAN}{word}{RESET}"));
            } else {
                result.push_str(&word);
            }
            continue;
        }

        result.push(ch);
        i += 1;
    }

    result
}

/// Get pricing rates (per MTok) for a model.
/// Returns (input, cache_write, cache_read, output) or None if model is unknown.
fn model_pricing(model: &str) -> Option<(f64, f64, f64, f64)> {
    // Pricing from https://docs.anthropic.com/en/about-claude/pricing
    if model.contains("opus") {
        if model.contains("4-6")
            || model.contains("4-5")
            || model.contains("4.6")
            || model.contains("4.5")
        {
            Some((5.0, 6.25, 0.50, 25.0))
        } else {
            // Opus 4, 4.1 etc.
            Some((15.0, 18.75, 1.50, 75.0))
        }
    } else if model.contains("sonnet") {
        Some((3.0, 3.75, 0.30, 15.0))
    } else if model.contains("haiku") {
        if model.contains("4-5") || model.contains("4.5") {
            Some((1.0, 1.25, 0.10, 5.0))
        } else {
            Some((0.80, 1.0, 0.08, 4.0))
        }
    } else {
        None
    }
}

/// Estimate cost in USD for a given usage and model.
/// Returns None if the model pricing is unknown.
pub fn estimate_cost(usage: &yoagent::Usage, model: &str) -> Option<f64> {
    let (input_cost, cw_cost, cr_cost, output_cost) = cost_breakdown(usage, model)?;
    Some(input_cost + cw_cost + cr_cost + output_cost)
}

/// Get individual cost components for a usage and model.
/// Returns (input_cost, cache_write_cost, cache_read_cost, output_cost) or None if model unknown.
pub fn cost_breakdown(usage: &yoagent::Usage, model: &str) -> Option<(f64, f64, f64, f64)> {
    let (input_per_m, cache_write_per_m, cache_read_per_m, output_per_m) = model_pricing(model)?;

    let input_cost = usage.input as f64 * input_per_m / 1_000_000.0;
    let cache_write_cost = usage.cache_write as f64 * cache_write_per_m / 1_000_000.0;
    let cache_read_cost = usage.cache_read as f64 * cache_read_per_m / 1_000_000.0;
    let output_cost = usage.output as f64 * output_per_m / 1_000_000.0;

    Some((input_cost, cache_write_cost, cache_read_cost, output_cost))
}

/// Format a cost in USD for display (e.g., "$0.0042", "$1.23").
pub fn format_cost(cost: f64) -> String {
    if cost < 0.01 {
        format!("${:.4}", cost)
    } else if cost < 1.0 {
        format!("${:.3}", cost)
    } else {
        format!("${:.2}", cost)
    }
}

/// Format a duration for display (e.g., "1.2s", "350ms", "2m 15s").
pub fn format_duration(d: std::time::Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) / 1000;
        format!("{mins}m {secs}s")
    }
}

/// Format a token count for display (e.g., 1500 -> "1.5k", 1000000 -> "1.0M").
pub fn format_token_count(count: u64) -> String {
    if count < 1000 {
        format!("{count}")
    } else if count < 1_000_000 {
        format!("{:.1}k", count as f64 / 1000.0)
    } else {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    }
}

/// Build a context usage bar (e.g., "████████░░░░░░░░░░░░ 40%").
pub fn context_bar(used: u64, max: u64) -> String {
    let pct = if max == 0 {
        0.0
    } else {
        (used as f64 / max as f64).min(1.0)
    };
    let width = 20;
    let filled = (pct * width as f64).round() as usize;
    let empty = width - filled;
    let bar: String = "█".repeat(filled) + &"░".repeat(empty);
    format!("{bar} {:.0}%", pct * 100.0)
}

/// Truncate a string with an ellipsis if it exceeds `max` characters.
pub fn truncate_with_ellipsis(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((idx, _)) => format!("{}…", &s[..idx]),
        None => s.to_string(),
    }
}

/// Truncate a string to `max` characters (no ellipsis).
#[cfg(test)]
pub fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Get the current git branch name, if we're in a git repo.
pub fn git_branch() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Format a human-readable summary for a tool execution.
pub fn format_tool_summary(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "bash" => {
            let cmd = args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("...");
            format!("$ {}", truncate_with_ellipsis(cmd, 80))
        }
        "read_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("read {}", path)
        }
        "write_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("write {}", path)
        }
        "edit_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("edit {}", path)
        }
        "list_files" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            format!("ls {}", path)
        }
        "search" => {
            let pat = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
            format!("search '{}'", truncate_with_ellipsis(pat, 60))
        }
        _ => tool_name.to_string(),
    }
}

/// Print usage stats after a prompt response.
pub fn print_usage(
    usage: &yoagent::Usage,
    total: &yoagent::Usage,
    model: &str,
    elapsed: std::time::Duration,
) {
    if usage.input > 0 || usage.output > 0 {
        let cache_info = if usage.cache_read > 0 || usage.cache_write > 0 {
            format!(
                "  [cache: {} read, {} write]",
                usage.cache_read, usage.cache_write
            )
        } else {
            String::new()
        };
        let cost_info = estimate_cost(usage, model)
            .map(|c| format!("  cost: {}", format_cost(c)))
            .unwrap_or_default();
        let total_cost_info = estimate_cost(total, model)
            .map(|c| format!("  total: {}", format_cost(c)))
            .unwrap_or_default();
        let elapsed_str = format_duration(elapsed);
        println!(
            "\n{DIM}  tokens: {} in / {} out{cache_info}  (session: {} in / {} out){cost_info}{total_cost_info}  ⏱ {elapsed_str}{RESET}",
            usage.input, usage.output, total.input, total.output
        );
    }
}

/// Incremental markdown renderer for streamed text output.
/// Tracks state across partial deltas to apply ANSI formatting for
/// code blocks, inline code, bold text, and headers.
pub struct MarkdownRenderer {
    in_code_block: bool,
    code_lang: Option<String>,
    line_buffer: String,
}

impl MarkdownRenderer {
    /// Create a new renderer with empty state.
    pub fn new() -> Self {
        Self {
            in_code_block: false,
            code_lang: None,
            line_buffer: String::new(),
        }
    }

    /// Process a delta chunk and return ANSI-formatted output.
    /// Buffers partial lines to detect fences and line-level formatting.
    pub fn render_delta(&mut self, delta: &str) -> String {
        let mut output = String::new();
        self.line_buffer.push_str(delta);

        // Process all complete lines (those ending with \n)
        while let Some(newline_pos) = self.line_buffer.find('\n') {
            let line = self.line_buffer[..newline_pos].to_string();
            self.line_buffer = self.line_buffer[newline_pos + 1..].to_string();
            output.push_str(&self.render_line(&line));
            output.push('\n');
        }

        output
    }

    /// Flush any remaining buffered content (call after stream ends).
    pub fn flush(&mut self) -> String {
        if self.line_buffer.is_empty() {
            return String::new();
        }
        let line = std::mem::take(&mut self.line_buffer);
        self.render_line(&line)
    }

    /// Render a single complete line, updating state for code fences.
    fn render_line(&mut self, line: &str) -> String {
        let trimmed = line.trim();

        // Check for code fence (``` with optional language)
        if let Some(after_fence) = trimmed.strip_prefix("```") {
            if self.in_code_block {
                // Closing fence
                self.in_code_block = false;
                self.code_lang = None;
                return format!("{DIM}{line}{RESET}");
            } else {
                // Opening fence — capture language if present
                self.in_code_block = true;
                let lang = after_fence.trim();
                self.code_lang = if lang.is_empty() {
                    None
                } else {
                    Some(lang.to_string())
                };
                return format!("{DIM}{line}{RESET}");
            }
        }

        if self.in_code_block {
            // Code block content: syntax highlight if language is known, else dim
            return if let Some(ref lang) = self.code_lang {
                highlight_code_line(lang, line)
            } else {
                format!("{DIM}{line}{RESET}")
            };
        }

        // Header: # at line start → BOLD+CYAN
        if trimmed.starts_with('#') {
            return format!("{BOLD}{CYAN}{line}{RESET}");
        }

        // Apply inline formatting for normal text
        self.render_inline(line)
    }

    /// Apply inline formatting (bold, inline code) to a line of normal text.
    fn render_inline(&self, line: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            // Check for bold: **text**
            if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
                // Find closing **
                if let Some(close) = self.find_double_star(&chars, i + 2) {
                    let inner: String = chars[i + 2..close].iter().collect();
                    result.push_str(&format!("{BOLD}{inner}{RESET}"));
                    i = close + 2;
                    continue;
                }
            }

            // Check for inline code: `text`
            if chars[i] == '`' {
                // Find closing backtick (not another opening fence)
                if let Some(close) = self.find_backtick(&chars, i + 1) {
                    let inner: String = chars[i + 1..close].iter().collect();
                    result.push_str(&format!("{CYAN}{inner}{RESET}"));
                    i = close + 1;
                    continue;
                }
            }

            result.push(chars[i]);
            i += 1;
        }

        result
    }

    /// Find closing ** starting from position `from` in char slice.
    fn find_double_star(&self, chars: &[char], from: usize) -> Option<usize> {
        let len = chars.len();
        let mut j = from;
        while j + 1 < len {
            if chars[j] == '*' && chars[j + 1] == '*' {
                return Some(j);
            }
            j += 1;
        }
        None
    }

    /// Find closing backtick starting from position `from` in char slice.
    fn find_backtick(&self, chars: &[char], from: usize) -> Option<usize> {
        (from..chars.len()).find(|&j| chars[j] == '`')
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

// --- Waiting spinner for AI responses ---

/// Braille spinner frames used for the "thinking" animation.
pub const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Get the spinner frame for a given tick index (wraps around).
pub fn spinner_frame(tick: usize) -> char {
    SPINNER_FRAMES[tick % SPINNER_FRAMES.len()]
}

/// A handle to a running spinner task. Dropping or calling `stop()` cancels it.
pub struct Spinner {
    cancel: tokio::sync::watch::Sender<bool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Spinner {
    /// Start a spinner that prints frames to stderr every 100ms.
    /// The spinner shows `⠋ thinking...` cycling through braille characters.
    pub fn start() -> Self {
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn(async move {
            let mut tick: usize = 0;
            loop {
                // Check cancellation before printing
                if *cancel_rx.borrow() {
                    // Clear the spinner line
                    eprint!("\r\x1b[K");
                    break;
                }
                let frame = spinner_frame(tick);
                eprint!("\r{DIM}  {frame} thinking...{RESET}");
                tick = tick.wrapping_add(1);

                // Wait 100ms or until cancelled
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
                    _ = cancel_rx.changed() => {
                        // Clear the spinner line
                        eprint!("\r\x1b[K");
                        break;
                    }
                }
            }
        });
        Self {
            cancel: cancel_tx,
            handle: Some(handle),
        }
    }

    /// Stop the spinner and clear its output.
    pub fn stop(mut self) {
        let _ = self.cancel.send(true);
        // Take the handle so Drop doesn't try to stop again
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_unicode() {
        assert_eq!(truncate("héllo wörld", 5), "héllo");
    }

    #[test]
    fn test_truncate_empty() {
        assert_eq!(truncate("", 5), "");
    }

    #[test]
    fn test_truncate_adds_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello world", 5), "hello…");
        assert_eq!(truncate_with_ellipsis("hi", 5), "hi");
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn test_format_token_count() {
        assert_eq!(format_token_count(0), "0");
        assert_eq!(format_token_count(999), "999");
        assert_eq!(format_token_count(1000), "1.0k");
        assert_eq!(format_token_count(1500), "1.5k");
        assert_eq!(format_token_count(10000), "10.0k");
        assert_eq!(format_token_count(150000), "150.0k");
        assert_eq!(format_token_count(1000000), "1.0M");
        assert_eq!(format_token_count(2500000), "2.5M");
    }

    #[test]
    fn test_context_bar() {
        let bar = context_bar(50000, 200000);
        assert!(bar.contains('█'));
        assert!(bar.contains("25%"));

        let bar_empty = context_bar(0, 200000);
        assert!(bar_empty.contains("0%"));

        let bar_full = context_bar(200000, 200000);
        assert!(bar_full.contains("100%"));
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(format_cost(0.0001), "$0.0001");
        assert_eq!(format_cost(0.0042), "$0.0042");
        assert_eq!(format_cost(0.05), "$0.050");
        assert_eq!(format_cost(0.123), "$0.123");
        assert_eq!(format_cost(1.5), "$1.50");
        assert_eq!(format_cost(12.345), "$12.35");
    }

    #[test]
    fn test_format_duration_ms() {
        assert_eq!(
            format_duration(std::time::Duration::from_millis(50)),
            "50ms"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(999)),
            "999ms"
        );
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(
            format_duration(std::time::Duration::from_millis(1000)),
            "1.0s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(1500)),
            "1.5s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(30000)),
            "30.0s"
        );
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(
            format_duration(std::time::Duration::from_millis(60000)),
            "1m 0s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(90000)),
            "1m 30s"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_millis(125000)),
            "2m 5s"
        );
    }

    #[test]
    fn test_estimate_cost_opus() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "claude-opus-4-6").unwrap();
        assert!((cost - 7.5).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_sonnet() {
        let usage = yoagent::Usage {
            input: 500_000,
            output: 50_000,
            cache_read: 200_000,
            cache_write: 100_000,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "claude-sonnet-4-6").unwrap();
        assert!((cost - 2.685).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_haiku() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 500_000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        let cost = estimate_cost(&usage, "claude-haiku-4-5").unwrap();
        assert!((cost - 3.5).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_unknown_model() {
        let usage = yoagent::Usage {
            input: 1000,
            output: 1000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        assert!(estimate_cost(&usage, "gpt-4o").is_none());
    }

    #[test]
    fn test_cost_breakdown_opus() {
        let usage = yoagent::Usage {
            input: 1_000_000,
            output: 100_000,
            cache_read: 500_000,
            cache_write: 200_000,
            total_tokens: 0,
        };
        let (input, cw, cr, output) = cost_breakdown(&usage, "claude-opus-4-6").unwrap();
        // input: 1M * 5/M = 5.0
        assert!((input - 5.0).abs() < 0.001);
        // output: 100k * 25/M = 2.5
        assert!((output - 2.5).abs() < 0.001);
        // cache_read: 500k * 0.50/M = 0.25
        assert!((cr - 0.25).abs() < 0.001);
        // cache_write: 200k * 6.25/M = 1.25
        assert!((cw - 1.25).abs() < 0.001);
        // Total should match estimate_cost
        let total = input + cw + cr + output;
        let expected = estimate_cost(&usage, "claude-opus-4-6").unwrap();
        assert!((total - expected).abs() < 0.001);
    }

    #[test]
    fn test_cost_breakdown_unknown_model() {
        let usage = yoagent::Usage {
            input: 1000,
            output: 1000,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 0,
        };
        assert!(cost_breakdown(&usage, "gpt-4o").is_none());
    }

    #[test]
    fn test_format_tool_summary_bash() {
        let args = serde_json::json!({"command": "echo hello"});
        assert_eq!(format_tool_summary("bash", &args), "$ echo hello");
    }

    #[test]
    fn test_format_tool_summary_bash_long_command() {
        let long_cmd = "a".repeat(100);
        let args = serde_json::json!({"command": long_cmd});
        let result = format_tool_summary("bash", &args);
        assert!(result.starts_with("$ "));
        assert!(result.ends_with('…'));
        assert!(result.len() < 100);
    }

    #[test]
    fn test_format_tool_summary_read_file() {
        let args = serde_json::json!({"path": "src/main.rs"});
        assert_eq!(format_tool_summary("read_file", &args), "read src/main.rs");
    }

    #[test]
    fn test_format_tool_summary_write_file() {
        let args = serde_json::json!({"path": "out.txt"});
        assert_eq!(format_tool_summary("write_file", &args), "write out.txt");
    }

    #[test]
    fn test_format_tool_summary_edit_file() {
        let args = serde_json::json!({"path": "foo.rs"});
        assert_eq!(format_tool_summary("edit_file", &args), "edit foo.rs");
    }

    #[test]
    fn test_format_tool_summary_list_files() {
        let args = serde_json::json!({"path": "src/"});
        assert_eq!(format_tool_summary("list_files", &args), "ls src/");
    }

    #[test]
    fn test_format_tool_summary_list_files_no_path() {
        let args = serde_json::json!({});
        assert_eq!(format_tool_summary("list_files", &args), "ls .");
    }

    #[test]
    fn test_format_tool_summary_search() {
        let args = serde_json::json!({"pattern": "TODO"});
        assert_eq!(format_tool_summary("search", &args), "search 'TODO'");
    }

    #[test]
    fn test_format_tool_summary_unknown_tool() {
        let args = serde_json::json!({});
        assert_eq!(format_tool_summary("custom_tool", &args), "custom_tool");
    }

    #[test]
    fn test_git_branch_returns_something_in_repo() {
        let branch = git_branch();
        // Outside a git repo (e.g. cargo-mutants temp dir), branch is None — that's fine
        if let Some(name) = branch {
            assert!(!name.is_empty(), "Branch name should not be empty");
            assert!(
                !name.contains('\n'),
                "Branch name should not contain newlines"
            );
        }
    }

    #[test]
    fn test_color_struct_display_outputs_ansi() {
        // Color struct should produce the ANSI code when color is enabled
        let c = Color("\x1b[1m");
        let formatted = format!("{c}");
        // We can't guarantee NO_COLOR isn't set in the test environment,
        // but the type itself should compile and format correctly.
        assert!(formatted == "\x1b[1m" || formatted.is_empty());
    }

    #[test]
    fn test_color_struct_display_consistency() {
        // All color constants should be the same type and format without panic
        let result = format!("{BOLD}{DIM}{GREEN}{YELLOW}{CYAN}{RED}{RESET}");
        // Should either have all codes or be empty (if NO_COLOR is set)
        assert!(result.contains('\x1b') || result.is_empty());
    }

    // --- MarkdownRenderer tests ---

    /// Helper: render a full string through the renderer (not streamed).
    fn render_full(input: &str) -> String {
        let mut r = MarkdownRenderer::new();
        let mut out = r.render_delta(input);
        out.push_str(&r.flush());
        out
    }

    #[test]
    fn test_md_code_block_detection() {
        let input = "before\n```\ncode line\n```\nafter\n";
        let out = render_full(input);
        // "code line" should be wrapped in DIM
        assert!(out.contains(&format!("{DIM}code line{RESET}")));
        // "before" and "after" should NOT be dim
        assert!(out.contains("before"));
        assert!(out.contains("after"));
    }

    #[test]
    fn test_md_code_block_with_language() {
        let input = "```rust\nlet x = 1;\n```\n";
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta(input);
        let flushed = r.flush();
        let full = format!("{out}{flushed}");
        // Language should be captured and fence dimmed
        assert!(full.contains(&format!("{DIM}```rust{RESET}")));
        // "let" should be keyword-highlighted, not just DIM
        assert!(full.contains(&format!("{BOLD_CYAN}let{RESET}")));
        // Number should be yellow
        assert!(full.contains(&format!("{YELLOW}1{RESET}")));
    }

    #[test]
    fn test_md_inline_code() {
        let out = render_full("use `Option<T>` here\n");
        assert!(out.contains(&format!("{CYAN}Option<T>{RESET}")));
    }

    #[test]
    fn test_md_bold_text() {
        let out = render_full("this is **important** stuff\n");
        assert!(out.contains(&format!("{BOLD}important{RESET}")));
    }

    #[test]
    fn test_md_header_rendering() {
        let out = render_full("# Hello World\n");
        assert!(out.contains(&format!("{BOLD}{CYAN}# Hello World{RESET}")));
    }

    #[test]
    fn test_md_header_h2() {
        let out = render_full("## Section Two\n");
        assert!(out.contains(&format!("{BOLD}{CYAN}## Section Two{RESET}")));
    }

    #[test]
    fn test_md_partial_delta_fence() {
        // Fence marker split across multiple deltas
        let mut r = MarkdownRenderer::new();
        let out1 = r.render_delta("``");
        // Nothing emitted yet — still buffered (no newline)
        assert_eq!(out1, "");
        let out2 = r.render_delta("`\n");
        // Now the fence line is complete
        assert!(out2.contains(&format!("{DIM}```{RESET}")));
        let out3 = r.render_delta("code here\n");
        assert!(out3.contains(&format!("{DIM}code here{RESET}")));
        let out4 = r.render_delta("```\n");
        assert!(out4.contains(&format!("{DIM}```{RESET}")));
        // After closing, normal text again
        let out5 = r.render_delta("normal\n");
        assert!(out5.contains("normal"));
        assert!(!out5.contains(&format!("{DIM}")));
    }

    #[test]
    fn test_md_empty_delta() {
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("");
        assert_eq!(out, "");
        let flushed = r.flush();
        assert_eq!(flushed, "");
    }

    #[test]
    fn test_md_multiple_code_blocks() {
        let input = "text\n```\nblock1\n```\nmiddle\n```python\nblock2\n```\nend\n";
        let out = render_full(input);
        // Untagged code block → DIM fallback
        assert!(out.contains(&format!("{DIM}block1{RESET}")));
        assert!(out.contains("middle"));
        // Python-tagged code block → syntax highlighted (no keyword match, plain output)
        assert!(out.contains("block2"));
        assert!(out.contains("end"));
    }

    #[test]
    fn test_md_inline_code_inside_bold() {
        // Inline code backticks inside bold — bold wraps, code is separate
        let out = render_full("**bold** and `code`\n");
        assert!(out.contains(&format!("{BOLD}bold{RESET}")));
        assert!(out.contains(&format!("{CYAN}code{RESET}")));
    }

    #[test]
    fn test_md_unmatched_backtick() {
        // Single backtick without closing — should pass through literally
        let out = render_full("it's a `partial\n");
        assert!(out.contains('`'));
        assert!(out.contains("partial"));
    }

    #[test]
    fn test_md_unmatched_bold() {
        // Unmatched ** should pass through literally
        let out = render_full("star **power\n");
        assert!(out.contains("**"));
        assert!(out.contains("power"));
    }

    #[test]
    fn test_md_flush_partial_line() {
        let mut r = MarkdownRenderer::new();
        let out = r.render_delta("no newline here");
        assert_eq!(out, ""); // buffered
        let flushed = r.flush();
        assert!(flushed.contains("no newline here"));
    }

    #[test]
    fn test_md_flush_with_inline_formatting() {
        let mut r = MarkdownRenderer::new();
        let _ = r.render_delta("hello **world**");
        let flushed = r.flush();
        assert!(flushed.contains(&format!("{BOLD}world{RESET}")));
    }

    #[test]
    fn test_md_default_trait() {
        let r = MarkdownRenderer::default();
        assert!(!r.in_code_block);
        assert!(r.code_lang.is_none());
        assert!(r.line_buffer.is_empty());
    }

    #[test]
    fn test_md_plain_text_unchanged() {
        let out = render_full("just plain text\n");
        assert!(out.contains("just plain text"));
    }

    #[test]
    fn test_md_multiple_inline_codes_one_line() {
        let out = render_full("use `foo` and `bar` here\n");
        assert!(out.contains(&format!("{CYAN}foo{RESET}")));
        assert!(out.contains(&format!("{CYAN}bar{RESET}")));
    }

    #[test]
    fn test_md_code_block_preserves_content() {
        let input = "```\nfn main() {\n    println!(\"hello\");\n}\n```\n";
        let out = render_full(input);
        assert!(out.contains("fn main()"));
        assert!(out.contains("println!"));
    }

    // --- Syntax highlighting tests ---

    #[test]
    fn test_highlight_rust_keywords() {
        let out = highlight_code_line("rust", "    let mut x = 42;");
        assert!(out.contains(&format!("{BOLD_CYAN}let{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}mut{RESET}")));
        assert!(out.contains(&format!("{YELLOW}42{RESET}")));
    }

    #[test]
    fn test_highlight_rust_fn() {
        let out = highlight_code_line("rust", "fn main() {");
        assert!(out.contains(&format!("{BOLD_CYAN}fn{RESET}")));
        assert!(out.contains("main"));
    }

    #[test]
    fn test_highlight_rust_string() {
        let out = highlight_code_line("rs", r#"let s = "hello world";"#);
        assert!(out.contains(&format!("{GREEN}\"hello world\"{RESET}")));
    }

    #[test]
    fn test_highlight_rust_comment() {
        let out = highlight_code_line("rust", "    // this is a comment");
        assert!(out.contains(&format!("{DIM}")));
        assert!(out.contains("this is a comment"));
    }

    #[test]
    fn test_highlight_rust_full_line_comment() {
        let out = highlight_code_line("rust", "// full line comment");
        assert_eq!(out, format!("{DIM}// full line comment{RESET}"));
    }

    #[test]
    fn test_highlight_python_keywords() {
        let out = highlight_code_line("python", "def hello(self):");
        assert!(out.contains(&format!("{BOLD_CYAN}def{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}self{RESET}")));
    }

    #[test]
    fn test_highlight_python_comment() {
        let out = highlight_code_line("py", "# a comment");
        assert_eq!(out, format!("{DIM}# a comment{RESET}"));
    }

    #[test]
    fn test_highlight_js_keywords() {
        let out = highlight_code_line("javascript", "const x = async () => {");
        assert!(out.contains(&format!("{BOLD_CYAN}const{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}async{RESET}")));
    }

    #[test]
    fn test_highlight_ts_alias() {
        let out = highlight_code_line("ts", "let y = 10;");
        assert!(out.contains(&format!("{BOLD_CYAN}let{RESET}")));
        assert!(out.contains(&format!("{YELLOW}10{RESET}")));
    }

    #[test]
    fn test_highlight_go_keywords() {
        let out = highlight_code_line("go", "func main() {");
        assert!(out.contains(&format!("{BOLD_CYAN}func{RESET}")));
    }

    #[test]
    fn test_highlight_shell_keywords() {
        let out = highlight_code_line("bash", "if [ -f file ]; then");
        assert!(out.contains(&format!("{BOLD_CYAN}if{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}then{RESET}")));
    }

    #[test]
    fn test_highlight_shell_comment() {
        let out = highlight_code_line("sh", "# shell comment");
        assert_eq!(out, format!("{DIM}# shell comment{RESET}"));
    }

    #[test]
    fn test_highlight_unknown_lang_falls_back_to_dim() {
        let out = highlight_code_line("haskell", "main = putStrLn");
        assert_eq!(out, format!("{DIM}main = putStrLn{RESET}"));
    }

    #[test]
    fn test_highlight_empty_line() {
        let out = highlight_code_line("rust", "");
        assert_eq!(out, "");
    }

    #[test]
    fn test_highlight_no_false_keyword_in_identifier() {
        // "letter" contains "let" but should NOT be highlighted
        let out = highlight_code_line("rust", "let letter = 1;");
        assert!(out.contains(&format!("{BOLD_CYAN}let{RESET}")));
        // "letter" should appear plain
        assert!(out.contains("letter"));
        // Make sure "letter" isn't colored as keyword
        let letter_highlighted = format!("{BOLD_CYAN}letter{RESET}");
        assert!(!out.contains(&letter_highlighted));
    }

    #[test]
    fn test_highlight_string_with_escape() {
        let out = highlight_code_line("rust", r#"let s = "he\"llo";"#);
        assert!(out.contains(&format!("{GREEN}")));
        assert!(out.contains(&format!("{BOLD_CYAN}let{RESET}")));
    }

    #[test]
    fn test_highlight_inline_comment_after_code() {
        let out = highlight_code_line("rust", "let x = 1; // comment");
        assert!(out.contains(&format!("{BOLD_CYAN}let{RESET}")));
        assert!(out.contains(&format!("{DIM}// comment{RESET}")));
    }

    #[test]
    fn test_highlight_number_float() {
        let out = highlight_code_line("rust", "let pi = 3.14;");
        assert!(out.contains(&format!("{YELLOW}3.14{RESET}")));
    }

    #[test]
    fn test_normalize_lang_aliases() {
        assert_eq!(normalize_lang("rust"), Some("rust"));
        assert_eq!(normalize_lang("rs"), Some("rust"));
        assert_eq!(normalize_lang("Python"), Some("python"));
        assert_eq!(normalize_lang("JS"), Some("js"));
        assert_eq!(normalize_lang("typescript"), Some("js"));
        assert_eq!(normalize_lang("tsx"), Some("js"));
        assert_eq!(normalize_lang("golang"), Some("go"));
        assert_eq!(normalize_lang("zsh"), Some("shell"));
        assert_eq!(normalize_lang("haskell"), None);
    }

    #[test]
    fn test_highlight_renders_through_markdown() {
        // End-to-end: markdown renderer should use highlighting for tagged blocks
        let input = "```rust\nfn main() {\n    return 42;\n}\n```\n";
        let out = render_full(input);
        assert!(out.contains(&format!("{BOLD_CYAN}fn{RESET}")));
        assert!(out.contains(&format!("{BOLD_CYAN}return{RESET}")));
        assert!(out.contains(&format!("{YELLOW}42{RESET}")));
    }

    // --- Spinner tests ---

    #[test]
    fn test_spinner_frames_not_empty() {
        assert!(!SPINNER_FRAMES.is_empty());
    }

    #[test]
    fn test_spinner_frames_are_braille() {
        // All braille characters are in the Unicode range U+2800..U+28FF
        for &frame in SPINNER_FRAMES {
            assert!(
                ('\u{2800}'..='\u{28FF}').contains(&frame),
                "Expected braille character, got {:?}",
                frame
            );
        }
    }

    #[test]
    fn test_spinner_frame_cycling() {
        // First 10 frames should match SPINNER_FRAMES exactly
        for (i, &expected) in SPINNER_FRAMES.iter().enumerate() {
            assert_eq!(spinner_frame(i), expected);
        }
    }

    #[test]
    fn test_spinner_frame_wraps_around() {
        let len = SPINNER_FRAMES.len();
        // After one full cycle, it should repeat
        assert_eq!(spinner_frame(0), spinner_frame(len));
        assert_eq!(spinner_frame(1), spinner_frame(len + 1));
        assert_eq!(spinner_frame(2), spinner_frame(len + 2));
    }

    #[test]
    fn test_spinner_frame_large_index() {
        // Should not panic even with very large indices
        let frame = spinner_frame(999_999);
        assert!(SPINNER_FRAMES.contains(&frame));
    }

    #[test]
    fn test_spinner_frames_all_unique() {
        // Each frame in the animation should be distinct
        let mut seen = std::collections::HashSet::new();
        for &frame in SPINNER_FRAMES {
            assert!(seen.insert(frame), "Duplicate spinner frame: {:?}", frame);
        }
    }
}
