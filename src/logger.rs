use env_logger::fmt::Color;
use std::io::Write as _;
use std::sync::RwLock;

lazy_static::lazy_static! {
    static ref SPANS: RwLock<Vec<String>> = RwLock::new(Vec::new());
}

pub fn span(new: impl Into<String>) -> SpanGuard {
    SPANS.write().unwrap().push(new.into());
    SpanGuard
}

// This method is private, 'cause if the user was allowed to
// remove spans in any way other than dropping the SpanGuard,
// that would break the order of dropping
fn pop_span() {
    SPANS.write().unwrap().pop();
}

pub fn empty_line() {
    println!();
}

pub struct SpanGuard;

impl Drop for SpanGuard {
    fn drop(&mut self) {
        pop_span();
    }
}

pub fn init_logger(v_count: u8, is_silent: bool) -> Result<(), failure::Error> {
    // Derive LevelFilter from command line args
    let level = if is_silent {
        log::LevelFilter::Off
    } else {
        match v_count {
            0 => log::LevelFilter::Info,
            1 => log::LevelFilter::Debug,
            _ => log::LevelFilter::Trace,
        }
    };

    let mut logger = env_logger::Builder::from_default_env();

    // Set log level from "-v" if RUST_LOG is set or -v flags are present
    if std::env::var("RUST_LOG").is_err() || v_count != 0 {
        logger.filter_level(level);
    }

    // Set formatter
    logger.format(|fmt, record| {
        let mut with_prefix =
            |record: &log::Record, prefix: &'static str, color: Color, color_whole_line: bool, verbose: bool| {
                let mut clean_style = fmt.style();
                clean_style.set_color(Color::White).set_intense(true);

                let mut accent_style = fmt.style();
                accent_style.set_color(color.clone());

                // Write spans and prefix
                accent_style.set_bold(true);
                let spans = SPANS.read().unwrap();
                if let Some((first_span, spans)) = spans.split_first() {
                    let mut span_colors = Colors(color.next());
                    let mut span_accent = accent_style.clone();
                    span_accent.set_color(span_colors.next().unwrap());

                    write!(fmt, "[")?;
                    write!(fmt, "{}", span_accent.value(first_span))?;
                    for (span, color) in spans.iter().zip(span_colors) {
                        span_accent.set_color(color);
                        write!(fmt, "|{}", span_accent.value(span))?;
                    }

                    write!(fmt, "] ")?;
                }
                write!(fmt, "{}", accent_style.value(prefix))?;
                accent_style.set_bold(false);

                // Extended verbosity for TRACE and DEBUG
                if verbose && record.module_path().is_some() {
                    // Print module path
                    let path = record.module_path().unwrap();
                    write!(fmt, "{}", accent_style.value(path))?;
                    // Print line in the file
                    if let Some(line) = record.line() {
                        write!(fmt, ":{}", accent_style.value(line))?;
                    }
                    // Add some padding to mitigate the formatting issue a bit
                    write!(fmt, "\t")?;
                }

                if color_whole_line {
                    writeln!(fmt, "{}", accent_style.value(record.args()))
                } else {
                    writeln!(fmt, "{}", clean_style.value(record.args()))
                }
            };

        match record.level() {
            log::Level::Info => with_prefix(record, "", seed_color::White, false, false),
            log::Level::Warn => with_prefix(record, "WARN: ", seed_color::Yellow, true, false),
            log::Level::Error => with_prefix(record, "ERROR: ", seed_color::Red, true, false),
            log::Level::Debug => with_prefix(record, "DEBUG: ", seed_color::Grey, false, true),
            log::Level::Trace => with_prefix(record, "TRACE: ", seed_color::DarkGrey, false, true),
        }
    });

    logger.try_init()?;

    Ok(())
}

// A set of colors suitable for main accent color, and a seed for span accents
#[allow(non_upper_case_globals)]
mod seed_color {
    use env_logger::fmt::Color;

    pub const White: Color = Color::White;
    pub const Yellow: Color = Color::Yellow;
    pub const Red: Color = Color::Red;
    pub const Grey: Color = Color::Ansi256(250);
    pub const DarkGrey: Color = Color::Ansi256(240);
}

trait ColorExt {
    fn next(&self) -> Self;
}

impl ColorExt for Color {
    fn next(&self) -> Self {
        match self {
            Color::Green => Color::Cyan,
            Color::Cyan => Color::Magenta,
            Color::Magenta => Color::Blue,
            Color::Blue => Color::Green,
            _ => Color::Green,
        }
    }
}

struct Colors(Color);

impl Iterator for Colors {
    type Item = Color;

    fn next(&mut self) -> Option<Self::Item> {
        let color = self.0.clone();
        self.0 = color.next();
        Some(color)
    }
}
