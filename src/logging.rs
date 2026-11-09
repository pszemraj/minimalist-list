//! Opt-in diagnostic logging in the platform's per-user state directory.

use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Records app lifecycle events or detailed diagnostics for this process.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Off,
    Log,
    Debug,
}

struct DailyLog {
    directory: PathBuf,
    mode: Mode,
    file: Mutex<(NaiveDate, File)>,
}

impl DailyLog {
    fn append(&self, now: DateTime<Utc>, record: &Record<'_>) -> io::Result<()> {
        let line = format!(
            "{} {:<5} [{} {}] {}\n",
            now.to_rfc3339_opts(SecondsFormat::Millis, true),
            record.level(),
            std::process::id(),
            record.target(),
            record.args(),
        );
        let mut current = self.file.lock().unwrap_or_else(|error| error.into_inner());
        let date = now.date_naive();
        if current.0 != date {
            *current = (date, open_day(&self.directory, date)?);
        }
        current.1.write_all(line.as_bytes())
    }
}

impl Log for DailyLog {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        let app = metadata.target().starts_with("minimalist_list");
        match self.mode {
            Mode::Off => false,
            Mode::Log => app && metadata.level() <= Level::Info,
            Mode::Debug => app || metadata.level() <= Level::Warn,
        }
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata())
            && let Err(error) = self.append(Utc::now(), record)
        {
            let _ = writeln!(io::stderr(), "Could not write the diagnostic log: {error}");
        }
    }

    fn flush(&self) {}
}

fn open_day(directory: &Path, date: NaiveDate) -> io::Result<File> {
    File::options()
        .create(true)
        .append(true)
        .open(directory.join(format!("{date}.log")))
}

#[cfg(not(target_os = "macos"))]
fn absolute_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn logs_dir() -> io::Result<PathBuf> {
    let home = crate::storage::home_dir();
    let home_root = |relative| home.is_absolute().then(|| home.join(relative));
    #[cfg(target_os = "windows")]
    let directory = absolute_env("LOCALAPPDATA")
        .or_else(|| home_root("AppData/Local"))
        .map(|root| root.join("minimalist-list/logs"));
    #[cfg(target_os = "macos")]
    let directory = home_root("Library/Logs/minimalist-list");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let directory = absolute_env("XDG_STATE_HOME")
        .or_else(|| home_root(".local/state"))
        .map(|root| root.join("minimalist-list/logs"));
    directory.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no per-user log directory"))
}

/// Opens the daily log and installs the process logger and panic diagnostics.
///
/// # Returns
/// Success when logging is off or the requested logger is active.
///
/// # Errors
/// Returns an error if the log directory or initial log file cannot be opened.
pub(crate) fn init(mode: Mode) -> Result<(), String> {
    if mode == Mode::Off {
        return Ok(());
    }
    let directory = logs_dir().map_err(|error| error.to_string())?;
    let date = Utc::now().date_naive();
    let file = fs::create_dir_all(&directory)
        .and_then(|()| open_day(&directory, date))
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    log::set_boxed_logger(Box::new(DailyLog {
        directory,
        mode,
        file: Mutex::new((date, file)),
    }))
    .map_err(|error| error.to_string())?;
    log::set_max_level(if mode == Mode::Debug {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    });
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(location) = info.location() {
            log::error!("Panic at {location}");
        } else {
            log::error!("Panic at unknown location");
        }
        if mode == Mode::Debug {
            log::debug!("{}", std::backtrace::Backtrace::force_capture());
        }
        previous(info);
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_levels_and_appends_to_the_file_for_each_utc_day() {
        let directory =
            std::env::temp_dir().join(format!("minimalist-logs-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let first: DateTime<Utc> = "2026-11-05T23:59:59Z".parse().unwrap();
        let next: DateTime<Utc> = "2026-11-06T00:00:00Z".parse().unwrap();
        let first_path = directory.join("2026-11-05.log");
        fs::write(&first_path, "earlier launch\n").unwrap();
        let mut logger = DailyLog {
            mode: Mode::Log,
            file: Mutex::new((
                first.date_naive(),
                open_day(&directory, first.date_naive()).unwrap(),
            )),
            directory: directory.clone(),
        };
        let enabled = |logger: &DailyLog, level, target| {
            logger.enabled(&Metadata::builder().level(level).target(target).build())
        };
        assert!(enabled(&logger, Level::Info, "minimalist_list"));
        assert!(!enabled(&logger, Level::Debug, "minimalist_list"));
        assert!(!enabled(&logger, Level::Warn, "eframe"));
        logger.mode = Mode::Debug;
        assert!(enabled(&logger, Level::Debug, "minimalist_list"));
        assert!(enabled(&logger, Level::Warn, "eframe"));
        assert!(!enabled(&logger, Level::Debug, "eframe"));
        for (time, message) in [(first, "before midnight"), (next, "after midnight")] {
            let args = format_args!("{message}");
            let record = Record::builder()
                .level(Level::Info)
                .target("minimalist_list")
                .args(args)
                .build();
            logger.append(time, &record).unwrap();
        }
        let first_text = fs::read_to_string(first_path).unwrap();
        assert!(first_text.starts_with("earlier launch\n"));
        assert!(first_text.contains("before midnight"));
        assert!(!first_text.contains("after midnight"));
        let second_text = fs::read_to_string(directory.join("2026-11-06.log")).unwrap();
        assert!(second_text.starts_with("2026-11-06T00:00:00.000Z INFO"));
        assert!(second_text.ends_with("after midnight\n"));
        drop(logger);
        fs::remove_dir_all(directory).unwrap();
    }
}
