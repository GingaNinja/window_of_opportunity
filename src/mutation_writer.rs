use dioxus_core::WriteMutations;

pub struct MutationWriter {}

impl WriteMutations for MutationWriter {
    fn append_children(&mut self, id: dioxus_core::ElementId, m: usize) {
        println!("appendChildren... elementId: {id:?}, m: {m}");
    }

    fn assign_node_id(&mut self, path: &'static [u8], id: dioxus_core::ElementId) {
        println!("assigne_node_id");
    }

    fn create_placeholder(&mut self, id: dioxus_core::ElementId) {
        println!("create_placeholder");
    }

    fn create_text_node(&mut self, value: &str, id: dioxus_core::ElementId) {
        println!("create_text_node");
    }

    fn load_template(
        &mut self,
        template: dioxus_core::Template,
        index: usize,
        id: dioxus_core::ElementId,
    ) {
        println!("loadTemplate... {:?}", template);
    }

    fn replace_node_with(&mut self, id: dioxus_core::ElementId, m: usize) {
        println!("replace_node_with");
    }

    fn replace_placeholder_with_nodes(&mut self, path: &'static [u8], m: usize) {
        println!("replace_placeholder_with_nodes");
    }

    fn insert_nodes_after(&mut self, id: dioxus_core::ElementId, m: usize) {
        println!("insert_nodes_after");
    }

    fn insert_nodes_before(&mut self, id: dioxus_core::ElementId, m: usize) {
        println!("insert_nodes_before");
    }

    fn set_attribute(
        &mut self,
        name: &'static str,
        ns: Option<&'static str>,
        value: &dioxus_core::AttributeValue,
        id: dioxus_core::ElementId,
    ) {
        todo!()
    }

    fn set_node_text(&mut self, value: &str, id: dioxus_core::ElementId) {
        println!("set_node_text");
    }

    fn create_event_listener(&mut self, name: &'static str, id: dioxus_core::ElementId) {
        todo!()
    }

    fn remove_event_listener(&mut self, name: &'static str, id: dioxus_core::ElementId) {
        todo!()
    }

    fn remove_node(&mut self, id: dioxus_core::ElementId) {
        todo!()
    }

    fn push_root(&mut self, id: dioxus_core::ElementId) {
        println!("push_root");
    }
}
