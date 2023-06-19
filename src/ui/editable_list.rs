use std::fmt::Debug;

use ratatui::{
    backend::Backend,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::Text,
    widgets::{Block, Borders, List, ListItem, ListState, Row, Table, TableState},
    Frame,
};
use tracing::info;

// Sure woulda been nice if tui had listed this API as a shared trait between
// ListState and TableState, huh? Since they didn't, we need to ourselves:
pub trait TuiState {
    fn selected(&self) -> Option<usize>;
    fn select(&mut self, index: Option<usize>);
}

impl TuiState for ListState {
    fn selected(&self) -> Option<usize> {
        self.selected()
    }

    fn select(&mut self, index: Option<usize>) {
        self.select(index)
    }
}

impl TuiState for TableState {
    fn selected(&self) -> Option<usize> {
        self.selected()
    }

    fn select(&mut self, index: Option<usize>) {
        self.select(index)
    }
}

#[derive(Debug, Default)]
pub struct EditableList<StateType: TuiState, T: Clone + Default + Debug = String> {
    pub options: Vec<T>,
    pub input: T,
    pub editing: bool,
    pub list_state: StateType,
    pub caps_lock: bool,
}

impl<StateType: TuiState + Default, T: Clone + Default + Debug> EditableList<StateType, T> {
    pub fn new(options: Vec<T>) -> Self {
        Self {
            options,
            caps_lock: Default::default(),
            editing: Default::default(),
            input: Default::default(),
            list_state: Default::default(),
        }
    }
}

impl<StateType: TuiState, T: Clone + Default + Debug> EditableList<StateType, T> {
    pub fn select_prev(&mut self) {
        let current = self.list_state.selected().unwrap_or(0);
        let prev = if current == 0 {
            self.options.len() - 1
        } else {
            current - 1
        };
        self.list_state.select(Some(prev));
    }

    pub fn select_next(&mut self) {
        let current = self.list_state.selected().unwrap_or(self.options.len() - 1);
        let next = if current == self.options.len() - 1 {
            0
        } else {
            current + 1
        };
        self.list_state.select(Some(next));
    }

    pub fn selected_is_last(&self) -> bool {
        let current = self.list_state.selected();
        current.map_or(true, |cur| cur == self.options.len() - 1)
    }

    /// default_item is the item to begin editing if none is currently selected in list_state
    pub fn start_editing(&mut self, default_item: Option<usize>) {
        // enter editing mode
        self.editing = true;

        // If no label is selected when Enter is pressed, select the open entry
        // number or 0.
        if self.list_state.selected().is_none() {
            self.list_state.select(default_item.or(Some(0)));
        }
        let selected = self.list_state.selected().unwrap();

        // Bonus thing RET does: preset the "input" state to the previous value
        // of the selected option, if any.
        self.input = self.options[selected].clone();

        info!("Editing item {:?} at index {}", self.input, selected);
    }

    /// Updates the state's selected item with the edits in `input`, then
    /// returns a tuple of the index of the edited field and its new value
    pub fn save_edit(&mut self) -> (usize, T) {
        self.editing = false;

        // mem::take will replace self.input with its default value
        let new_val = std::mem::take(&mut self.input);

        let edited_idx = self.list_state.selected().unwrap();

        // Update item in options list
        info!(
            "Saving over {:?} at index {} with new value {:?}",
            self.options[edited_idx], edited_idx, new_val
        );
        self.options[edited_idx] = new_val.clone();

        (edited_idx, new_val)
    }

    /// Deletes the state's selected item. If no item is selected, does nothing.
    /// Returns the index of the deleted item if there was a selection.
    pub fn delete_selected(&mut self) -> Option<usize> {
        if let Some(edited_idx) = self.list_state.selected() {
            self.input = Default::default();
            let old_val = self.options.remove(edited_idx);
            info!("Deleted value {:?} at index {}", old_val, edited_idx);

            self.list_state.select(if edited_idx > 0 {
                Some(edited_idx - 1)
            } else {
                None
            });

            return Some(edited_idx);
        }

        None
    }

    /// Inserts a new item after the selected one (or at the beginning if none
    /// is selected). Returns the index of the new item.
    pub fn insert_at_selection(&mut self, new_item: T) -> usize {
        let idx = self.list_state.selected().unwrap_or(0);
        info!("Inserting new value {:?} at index {}", new_item, idx);
        self.options.insert(idx, new_item);
        idx
    }

    /// Like insert_at_selection, but uses T's Default implementation to make
    /// the new item. Returns a tuple of the index of the new item and a clone
    /// of its value.
    pub fn insert_default_at_selection(&mut self) -> (usize, T) {
        let new_val: T = Default::default();
        let new_idx = self.insert_at_selection(new_val.clone());
        (new_idx, new_val)
    }

    /// Like insert_at_selection, but accepts a function to make the new item.
    /// The function will be called with the previously selected item as its
    /// only argument. Returns a tuple of the index of the new item and a clone
    /// of its value.
    pub fn insert_at_selection_with<F>(&mut self, f: F) -> (usize, T)
    where
        F: FnOnce(Option<T>) -> T,
    {
        let old_val = self
            .list_state
            .selected()
            .map(|idx| self.options[idx].clone());

        let new_val: T = f(old_val);
        let new_idx = self.insert_at_selection(new_val.clone());
        (new_idx, new_val)
    }
}

impl<T: Clone + Default + Debug> EditableList<ListState, T> {
    pub fn draw_list<B: Backend>(
        &mut self,
        f: &mut Frame<B>,
        rect: Rect,
        render_item: for<'a> fn(usize, &'a T, &'a T, bool) -> Text<'a>,
    ) {
        let widget = List::new(
            self.options
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let sel = self.list_state.selected().map_or(false, |s| s == i);
                    ListItem::new(render_item(i, item, &self.input, sel && self.editing))
                })
                .collect::<Vec<ListItem>>(),
        )
        .block(Block::default().borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");
        f.render_stateful_widget(widget, rect, &mut self.list_state);
    }
}

impl<T: Clone + Default + Debug> EditableList<TableState, T> {
    pub fn draw_table<'a, B: Backend, F: FnMut(usize, &'a T, &'a T, bool) -> Row<'a>>(
        &'a mut self,
        f: &mut Frame<B>,
        rect: Rect,
        widths: &'a [Constraint],
        mut render_item: F,
    ) {
        let widget = Table::new(
            self.options
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let sel = self.list_state.selected().map_or(false, |s| s == i);
                    render_item(i, item, &self.input, sel && self.editing)
                })
                .collect::<Vec<Row>>(),
        )
        .block(Block::default().borders(Borders::ALL))
        .widths(widths)
        .column_spacing(1)
        .style(Style::default().add_modifier(Modifier::DIM))
        .highlight_style(
            Style::default()
                .remove_modifier(Modifier::DIM)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

        f.render_stateful_widget(widget, rect, &mut self.list_state);
    }
}
