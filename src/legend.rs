use lazy_static::lazy_static;
use tui::{
    layout::Constraint,
    widgets::{Row, Table},
};

const LEGEND_WIDTHS: [Constraint; 24] = [Constraint::Ratio(1, 24); 24];
const TRUNC_LEGEND_WIDTHS: [Constraint; 24] = [
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(2, 24),
    Constraint::Ratio(0, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
    Constraint::Ratio(1, 24),
];

const LEGEND_LABELS: [&str; 24] = [
    "5am", "6", "7", "8", "9", "10", "11", "12pm", "1", "2", "3", "4", "5", "6", "7", "8pm", "9",
    "10", "11", "12am", "1", "2", "3", "4am",
];
const TRUNC_LEGEND_LABELS: [&str; 24] = [
    "5am", "", "7", "8", "9", "10", "", "12p", "", "2", "3", "4", "5", "6", "7", "8pm", "", "10",
    "", "12am", "", "2", "3", "4am",
];

lazy_static! {
    pub static ref LEGEND_TABLE: Table<'static> = Table::new([Row::new(LEGEND_LABELS)])
        .column_spacing(0)
        .widths(&LEGEND_WIDTHS);
    pub static ref TRUNC_LEGEND_TABLE: Table<'static> = Table::new([Row::new(TRUNC_LEGEND_LABELS)])
        .column_spacing(0)
        .widths(&TRUNC_LEGEND_WIDTHS);
}
