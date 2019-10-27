use std::borrow::Cow;
use std::env;
use std::io::Write as _;
use std::sync::RwLock;

lazy_static::lazy_static! {
    static ref SPAN: RwLock<Cow<'static, str>> = RwLock::new(Cow::Borrowed("core"));
}

pub fn span(new: impl Into<String>) -> SpanGuard {
    *SPAN.write().unwrap() = Cow::Owned(new.into());
    SpanGuard
}

pub fn unset_span() {
    *SPAN.write().unwrap() = Cow::Borrowed("core");
}

pub struct SpanGuard;

impl Drop for SpanGuard {
    fn drop(&mut self) {
        unset_span();
    }
}

pub fn init_logger(v_count: u64, is_silent: bool) -> Result<(), failure::Error> {
    use env_logger::fmt::Color;

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
    if env::var("RUST_LOG").is_err() || v_count != 0 {
        logger.filter_level(level);
    }

    // Set formatter
    logger.format(|fmt, record| {
        let mut with_prefix =
            |record: &log::Record, prefix: &'static str, color: Color, color_whole_line: bool, verbose: bool| {
                let mut clean_style = fmt.style();
                clean_style.set_color(Color::White).set_intense(true);

                let mut accent_style = fmt.style();
                accent_style.set_color(color);

                // Write span and prefix
                let span = SPAN.read().unwrap();
                accent_style.set_bold(true);
                write!(fmt, "[{}] ", accent_style.value(span))?;
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
            log::Level::Info => with_prefix(record, "", Color::White, false, false),
            log::Level::Warn => with_prefix(record, "WARN: ", Color::Yellow, true, false),
            log::Level::Error => with_prefix(record, "ERROR: ", Color::Red, true, false),
            log::Level::Debug => with_prefix(record, "DEBUG: ", Color::Cyan, false, true),
            log::Level::Trace => with_prefix(record, "TRACE: ", Color::White, false, true),
        }
    });

    logger.try_init()?;

    Ok(())
}
