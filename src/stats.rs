use crate::TimeLog;

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
            mean: self.total / (self.count as i32),
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
