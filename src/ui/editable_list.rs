use tui::{
    backend::Backend,
    layout::Rect,
    text::Text,
    widgets::{Block, Borders, List, ListItem, ListState, Row, Table, TableState},
    Frame,
};

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
pub struct EditableList<StateType: TuiState, T: Clone = String> {
    pub options: Vec<T>,
    pub input: T,
    pub editing: bool,
    pub list_state: StateType,
    pub caps_lock: bool,
}

impl<StateType: TuiState + Default, T: Clone + Default> EditableList<StateType, T> {
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

impl<StateType: TuiState, T: Clone + Default> EditableList<StateType, T> {
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
    }

    /// Updates the state's selected item with the edits in `input`, then
    /// returns a tuple of the index of the edited field and its new value
    pub fn save_edit(&mut self) -> (usize, T) {
        self.editing = false;

        // mem::take will replace state.input with its default value (empty
        // string)
        let new_val = std::mem::take(&mut self.input);

        let edited_idx = self.list_state.selected().unwrap();

        // Update settings page option
        self.options[edited_idx] = new_val.clone();

        (edited_idx, new_val)
    }
}

impl<T: Clone> EditableList<ListState, T> {
    pub fn draw_list<B: Backend>(
        &mut self,
        f: &mut Frame<B>,
        rect: Rect,
        render_item: for<'a> fn(usize, &'a T, &'a T, bool, bool) -> Text<'a>,
    ) {
        let settings_list = List::new(
            self.options
                .iter()
                .enumerate()
                .map(|(i, label)| {
                    let sel = self.list_state.selected().map_or(false, |s| s == i);
                    ListItem::new(render_item(i, label, &self.input, sel, self.editing))
                })
                .collect::<Vec<ListItem>>(),
        )
        .block(Block::default().borders(Borders::ALL));
        f.render_stateful_widget(settings_list, rect, &mut self.list_state);
    }
}

impl<T: Clone> EditableList<TableState, T> {
    pub fn draw_table<B: Backend>(
        &mut self,
        f: &mut Frame<B>,
        rect: Rect,
        render_item: for<'a> fn(usize, &'a T, &'a T, bool, bool) -> Row<'a>,
    ) {
        let settings_list = Table::new(
            self.options
                .iter()
                .enumerate()
                .map(|(i, label)| {
                    let sel = self.list_state.selected().map_or(false, |s| s == i);
                    render_item(i, label, &self.input, sel, self.editing)
                })
                .collect::<Vec<Row>>(),
        )
        .block(Block::default().borders(Borders::ALL));
        f.render_stateful_widget(settings_list, rect, &mut self.list_state);
    }
}
