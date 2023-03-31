use std::{ffi::OsStr, fs, io};

use chrono::NaiveDate;
use tracing::warn;

use crate::{get_save_file_dir, load_log_file, TimeLog};

#[derive(Debug, Clone, Copy)]
pub struct TimeStats {
    pub task_number: u8,
    pub count: u16,
    pub total: chrono::Duration,
    pub mean: chrono::Duration,
}

impl Default for TimeStats {
    fn default() -> Self {
        Self {
            task_number: Default::default(),
            count: Default::default(),
            total: chrono::Duration::zero(),
            mean: chrono::Duration::zero(),
        }
    }
}

#[derive(Debug)]
struct TimeStatsBuilder {
    number: u8,
    count: u16,
    total: chrono::Duration,
}

impl TimeStatsBuilder {
    fn new(number: u8) -> Self {
        Self {
            number,
            count: 0,
            total: chrono::Duration::zero(),
        }
    }

    fn add(&mut self, entry: TimeLog) -> &mut Self {
        self.count += 1;
        // For some reason, chrono::Duration implements Add for itself, but not
        // AddAssign? Weird.
        self.total = self.total + (entry.end.unwrap_or(entry.start) - entry.start);
        self
    }

    // Since all of TimeStatsBuilder's fields are Copy, it's easy to have
    // build() only take `&self` instead of `self`
    fn build(&self) -> TimeStats {
        TimeStats {
            task_number: self.number,
            count: self.count,
            total: self.total,
            mean: if self.count == 0 {
                chrono::Duration::zero()
            } else {
                self.total / (self.count as i32)
            },
        }
    }
}

// Normally I'd choose &Item over Item, but TimeLog is Copy woot
pub fn compute_stats(logs: impl IntoIterator<Item = TimeLog>) -> [TimeStats; 8] {
    // There's gotta be a more elegant way to do this but meh this is fine. At
    // least this is probably performant 🤷
    let mut result = [
        TimeStatsBuilder::new(1),
        TimeStatsBuilder::new(2),
        TimeStatsBuilder::new(3),
        TimeStatsBuilder::new(4),
        TimeStatsBuilder::new(5),
        TimeStatsBuilder::new(6),
        TimeStatsBuilder::new(7),
        TimeStatsBuilder::new(8),
    ];

    for log in logs {
        result[(log.number - 1) as usize].add(log);
    }

    result.map(|tsb| tsb.build())
}

/// Returns the stats from all historical files available in the save directory.
/// Includes TimeStats for each task as well as the minimum dated file located
/// if available.
pub fn load_history(
    min_date: Option<NaiveDate>,
    max_date: Option<NaiveDate>,
) -> io::Result<([TimeStats; 8], Option<NaiveDate>)> {
    if let Some(dir) = get_save_file_dir() {
        let (dates, logs): (Vec<_>, Vec<_>) = fs::read_dir(dir)?
            .filter_map(|res| {
                let path = res.map(|e| e.path());

                // If no path, no extension, or extension != .ron, return None
                // to skip this file. Else unwrap the successfully read path.
                if path.as_ref().map_or(true, |p| {
                    p.extension().map_or(true, |ext| ext != OsStr::new("ron"))
                }) {
                    return None;
                }
                let path = path.unwrap();

                let file_date = path
                    .file_name()
                    .expect("loadable files have names")
                    .to_string_lossy()
                    .trim_end_matches(".ron")
                    .parse::<NaiveDate>();

                if let Err(e) = file_date {
                    warn!("Undated file found in save directory, skipping: {}", e);
                    return None;
                }
                let file_date = file_date.unwrap();

                // Skip files outside our date range
                if min_date.map_or(false, |min| file_date < min)
                    || max_date.map_or(false, |max| file_date > max)
                {
                    return None;
                }

                let r = load_log_file(&path).map(|loaded_log| (file_date, loaded_log));
                if let Err(e) = r.as_ref() {
                    warn!("Unable to load history from a file in the save dir: {}", e);
                }
                r.ok()
            })
            .unzip();

        Ok((
            compute_stats(logs.into_iter().flatten()),
            dates.into_iter().min(),
        ))
    } else {
        warn!("Unable to load history: cannot locate and/or open save file directory");
        Ok(([TimeStats::default(); 8], None))
    }
}
