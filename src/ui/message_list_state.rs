use whatsrust as wr;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct ViewportAnchor {
    pub(crate) index: usize,
    pub(crate) y: isize,
    pub(crate) width: usize,
    pub(crate) offset: usize,
    pub(crate) generation: u64,
    pub(crate) message_id: wr::MessageId,
    pub(crate) bottom: u16,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Hash)]
pub struct MessageListState {
    pub selected: Option<usize>,
    pub offset: usize,
    pub(crate) selected_message: Option<wr::MessageId>,
    pub update_selected: bool,
    pub(crate) viewport_anchor: Option<ViewportAnchor>,
}

impl MessageListState {
    pub fn get_selected_message(&self) -> Option<wr::MessageId> {
        self.selected_message.clone()
    }

    pub fn set_selected_message(&mut self, msg_id: wr::MessageId) {
        self.selected_message = Some(msg_id);
        self.selected = None;
        self.update_selected = false;
        self.viewport_anchor = None;
    }

    pub fn follow_latest(&mut self, message_id: wr::MessageId) {
        self.selected_message = Some(message_id);
        self.selected = Some(0);
        self.offset = 0;
        self.update_selected = false;
        self.viewport_anchor = None;
    }

    pub fn reset(&mut self) {
        self.selected = None;
        self.offset = 0;
        self.selected_message = None;
        self.update_selected = false;
        self.viewport_anchor = None;
    }

    pub fn select(&mut self, index: Option<usize>) {
        self.selected = index;
        if index.is_none() {
            self.offset = 0;
        } else {
            self.update_selected = true;
        }
    }

    pub fn select_next(&mut self) {
        let next = self.selected.map_or(0, |i| i.saturating_add(1));
        self.select(Some(next));
    }

    pub fn select_previous(&mut self) {
        let previous = self.selected.map_or(usize::MAX, |i| i.saturating_sub(1));
        self.select(Some(previous));
    }

    pub fn select_first(&mut self) {
        self.select(Some(0));
    }

    pub fn select_last(&mut self) {
        self.select(Some(usize::MAX));
    }

    pub fn scroll_down_by(&mut self, amount: u16) {
        let selected = self.selected.unwrap_or_default();
        self.select(Some(selected.saturating_add(amount as usize)));
    }

    pub fn scroll_up_by(&mut self, amount: u16) {
        let selected = self.selected.unwrap_or_default();
        self.select(Some(selected.saturating_sub(amount as usize)));
    }

    pub fn select_next_bounded(&mut self, item_count: usize) {
        if self.selected.is_none() {
            self.select_bounded(item_count, 0);
        } else {
            self.move_by(item_count, 1);
        }
    }

    pub fn select_previous_bounded(&mut self, item_count: usize) {
        self.move_by(item_count, -1);
    }

    pub fn jump_top_bounded(&mut self, item_count: usize) {
        self.select_bounded(item_count, 0);
    }

    pub fn jump_bottom_bounded(&mut self, item_count: usize) {
        self.select_bounded(item_count, item_count.saturating_sub(1));
    }

    pub fn half_page_down_bounded(&mut self, item_count: usize, page_size: usize) {
        self.move_by(item_count, page_size as isize);
    }

    pub fn half_page_up_bounded(&mut self, item_count: usize, page_size: usize) {
        self.move_by(item_count, -(page_size as isize));
    }

    fn move_by(&mut self, item_count: usize, delta: isize) {
        let current = self.selected.unwrap_or_default();
        self.select_bounded(item_count, current.saturating_add_signed(delta));
    }

    fn select_bounded(&mut self, item_count: usize, index: usize) {
        if item_count == 0 {
            self.selected = None;
            self.offset = 0;
            self.selected_message = None;
            self.update_selected = false;
        } else {
            self.select(Some(index.min(item_count - 1)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MessageListState;

    #[test]
    fn bounded_navigation_clamps_selection_and_preserves_update_signal() {
        let mut state = MessageListState::default();

        state.select_next_bounded(3);
        state.select_next_bounded(3);
        state.select_next_bounded(3);
        state.select_next_bounded(3);

        assert_eq!(state.selected, Some(2));
        assert!(state.update_selected);
    }

    #[test]
    fn empty_bounded_navigation_clears_selection_and_offset() {
        let mut state = MessageListState {
            selected: Some(2),
            offset: 4,
            ..Default::default()
        };

        state.jump_top_bounded(0);

        assert_eq!(state.selected, None);
        assert_eq!(state.offset, 0);
        assert!(!state.update_selected);
    }

    #[test]
    fn setting_a_message_selection_clears_index_and_viewport_anchor() {
        let mut state = MessageListState::default();
        state.select(Some(2));
        state.set_selected_message("message-1".into());

        assert_eq!(state.get_selected_message().as_deref(), Some("message-1"));
        assert_eq!(state.selected, None);
        assert!(!state.update_selected);
        assert!(state.viewport_anchor.is_none());
    }
}
