use tui::{
    backend::Backend,
    style::Color,
    widgets::{Paragraph, Wrap},
    Frame,
};

use crate::App;

mod home;
mod settings;

#[derive(Default, Debug)]
pub enum Page {
    #[default]
    Home,
    Settings(settings::State),
}

fn number_to_color(i: u8) -> Color {
    match i {
        1 => Color::Blue,
        2 => Color::Cyan,
        3 => Color::Green,
        4 => Color::Magenta,
        5 => Color::Red,
        6 => Color::Yellow,
        7 => Color::LightBlue,
        8 => Color::LightCyan,
        _ => Color::Reset,
    }
}

fn message_widget(app: &App) -> Paragraph {
    let message = app.message.as_ref().map_or("", |m| m.0.as_str());
    Paragraph::new(message).wrap(Wrap { trim: false })
}

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    match app.selected_page {
        Page::Home => home::draw(f, app),
        Page::Settings(_) => settings::draw(f, app),
    }
}
