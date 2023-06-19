use std::borrow::Cow;

use ratatui::{
    style::{Modifier, Style},
    text::Span,
};

pub fn bold<'a, T>(text: T) -> Span<'a>
where
    T: Into<Cow<'a, str>>,
{
    Span::styled(text, Style::default().add_modifier(Modifier::BOLD))
}

pub fn dim<'a, T>(text: T) -> Span<'a>
where
    T: Into<Cow<'a, str>>,
{
    Span::styled(text, Style::default().add_modifier(Modifier::DIM))
}

pub fn blinky_underline<'a, T>(text: T) -> Span<'a>
where
    T: Into<Cow<'a, str>>,
{
    Span::styled(
        text,
        Style::default().add_modifier(Modifier::UNDERLINED | Modifier::SLOW_BLINK),
    )
}

pub fn blinky_if_index_matches<'a, T>(cursor_pos: usize, pos: usize, text: T) -> Span<'a>
where
    T: Into<Cow<'a, str>>,
{
    if cursor_pos == pos {
        blinky_underline(text)
    } else {
        Span::raw(text)
    }
}
