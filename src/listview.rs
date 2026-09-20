use cacao::{
    layout::Layout,
    listview::{ListView, ListViewDelegate},
    text::Label,
    view::{View, ViewDelegate},
};

const REACTIVE_ROW: &str = "ReactiveViewRowCell";

pub struct ReactiveListView {
    data: Vec<String>,
    view: Option<ListView>,
}

impl ReactiveListView {
    pub fn new(data: &Vec<String>) -> Self {
        Self {
            data: data.clone(),
            view: None,
        }
    }
}

impl ListViewDelegate for ReactiveListView {
    const NAME: &'static str = "ReactiveListView";

    fn did_load(&mut self, view: cacao::listview::ListView) {
        view.register(REACTIVE_ROW, ReactiveViewRow::default);
        self.view = Some(view);
    }

    fn number_of_items(&self) -> usize {
        self.data.len()
    }

    fn item_for(&self, row: usize) -> cacao::listview::ListViewRow {
        let mut view = self
            .view
            .as_ref()
            .unwrap()
            .dequeue::<ReactiveViewRow>(REACTIVE_ROW);

        if let Some(view) = &mut view.delegate {
            view.configure_with(&self.data[row]);
        }
        view.into_row()
    }
}

#[derive(Default)]
pub struct ReactiveViewRow {
    pub info: Label,
}

impl ReactiveViewRow {
    pub fn configure_with(&mut self, info: &str) {
        self.info.set_text(info);
    }
}

impl ViewDelegate for ReactiveViewRow {
    const NAME: &'static str = "ReactiveViewRow";

    fn did_load(&mut self, view: View) {
        view.add_subview(&self.info);
    }
}
