use tui::{backend::Backend, style::Color, Frame};

use crate::App;

mod home;
mod settings;

#[derive(Default, Debug)]
pub enum Page {
    #[default]
    Home,
    Settings,
    Edit,
}

fn char_to_color(c: char) -> Color {
    match c {
        '1' => Color::Blue,
        '2' => Color::Cyan,
        '3' => Color::Green,
        '4' => Color::Magenta,
        '5' => Color::Red,
        '6' => Color::Yellow,
        '7' => Color::LightBlue,
        '8' => Color::LightCyan,
        _ => Color::Reset,
    }
}

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &App) {
    match app.selected_page {
        Page::Home => home::draw(f, app),
        Page::Settings => settings::draw(f, app),
        Page::Edit => todo!(),
    }
}
