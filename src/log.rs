use anstyle::{AnsiColor, Style};
pub use log::*;
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
pub struct Logger {
    // `AutoStream` turns ANSI codes into Console API calls where native VT support is missing.
    console: anstream::AutoStream<std::io::Stdout>,
    // `StripStream` keeps the console's ANSI codes out of the log file.
    file: anstream::StripStream<File>,
}

impl Logger {
    pub fn new() -> Self {
        Self {
            console: anstream::AutoStream::new(std::io::stdout(), color_choice()),
            file: anstream::StripStream::new(File::create("arrabbiata.log").unwrap()),
        }
    }

    pub fn init(self) {
        env_logger::builder()
            .filter_level(LevelFilter::Error)
            .filter_module(
                "arrabbiata",
                if cfg!(debug_assertions) {
                    LevelFilter::Trace
                } else {
                    LevelFilter::Info
                },
            )
            .parse_default_env()
            .target(env_logger::Target::Pipe(Box::new(self)))
            .format(|f, record| {
                let target = record.target();
                let max_width = max_target_width(target);

                let level = colored_level(record.level());

                let target_style = if style_enabled() {
                    Style::new().bold()
                } else {
                    Style::new()
                };
                let target = Padded {
                    value: target,
                    width: max_width,
                }
                .styled(target_style);

                let time = chrono::Local::now().format("%d/%m/%Y %H:%M:%S");

                writeln!(f, "[{time}] {level} {target} -> {}", record.args())
            })
            .init();
    }
}

impl Write for Logger {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // The shared console runs in VT mode, where a bare line feed does not return to column 0.
        let _ = self.console.write_all(&to_crlf(buf));
        self.file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = self.console.flush();
        self.file.flush()
    }
}

/// Gives every line feed a carriage return, leaving ones that already have a pair alone.
fn to_crlf(buf: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(buf.len() + 8);
    for &byte in buf {
        if byte == b'\n' && out.last() != Some(&b'\r') {
            out.push(b'\r');
        }
        out.push(byte);
    }

    out
}

struct Padded<T> {
    value: T,
    width: usize,
}

impl<T: fmt::Display> fmt::Display for Padded<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{: <width$}", self.value, width = self.width)
    }
}

static MAX_MODULE_WIDTH: AtomicUsize = AtomicUsize::new(0);

fn max_target_width(target: &str) -> usize {
    let max_width = MAX_MODULE_WIDTH.load(Ordering::Relaxed);
    if max_width < target.len() {
        MAX_MODULE_WIDTH.store(target.len(), Ordering::Relaxed);
        target.len()
    } else {
        max_width
    }
}

struct Styled<T> {
    style: Style,
    item: T,
}

impl<T: fmt::Display> fmt::Display for Styled<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}{:#}", self.style, self.item, self.style)
    }
}

trait ToStyled<T> {
    fn styled(self, style: Style) -> Styled<T>;
}

impl<T> ToStyled<T> for T {
    fn styled(self, style: Style) -> Styled<T> {
        Styled { style, item: self }
    }
}

// env_logger disables styling for `Target::Pipe`, so colour support is resolved here instead.
fn color_choice() -> anstream::ColorChoice {
    static CHOICE: OnceLock<anstream::ColorChoice> = OnceLock::new();
    *CHOICE.get_or_init(|| match std::env::var("RUST_LOG_STYLE").as_deref() {
        Ok("always") => anstream::ColorChoice::Always,
        Ok("never") => anstream::ColorChoice::Never,
        _ => anstream::AutoStream::choice(&std::io::stdout()),
    })
}

fn style_enabled() -> bool {
    color_choice() != anstream::ColorChoice::Never
}

fn colored_level(level: Level) -> Styled<&'static str> {
    let (text, color) = match level {
        Level::Trace => ("TRACE", AnsiColor::Magenta),
        Level::Debug => ("DEBUG", AnsiColor::Blue),
        Level::Info => (" INFO", AnsiColor::Green),
        Level::Warn => (" WARN", AnsiColor::Yellow),
        Level::Error => ("ERROR", AnsiColor::Red),
    };

    let style = if style_enabled() {
        Style::new().fg_color(Some(color.into()))
    } else {
        Style::new()
    };

    text.styled(style)
}

#[cfg(test)]
mod tests {
    use super::to_crlf;

    #[test]
    fn bare_line_feeds_get_a_carriage_return() {
        assert_eq!(to_crlf(b"one\ntwo\n"), b"one\r\ntwo\r\n");
    }

    #[test]
    fn pairs_that_already_exist_are_left_alone() {
        assert_eq!(to_crlf(b"one\r\ntwo\r\n"), b"one\r\ntwo\r\n");
    }

    #[test]
    fn multi_line_payloads_are_handled_throughout() {
        // A multi-line message arrives as one write.
        assert_eq!(to_crlf(b"{\n  \"a\": 1\n}\n"), b"{\r\n  \"a\": 1\r\n}\r\n");
    }

    #[test]
    fn text_without_line_feeds_is_unchanged() {
        assert_eq!(to_crlf(b"no newline here"), b"no newline here");
        assert_eq!(to_crlf(b""), b"");
    }
}
