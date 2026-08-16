use super::App;
use super::actions::{FocusPane, Section};

impl App<'_> {
    pub(crate) fn select_next(&mut self) {
        match self.focus_pane {
            FocusPane::SectionRail => {
                if !self.move_logout_selection_next() {
                    self.selected_section = self.selected_section.next();
                }
            }
            FocusPane::ChatList => self.move_selection_next(),
            FocusPane::Conversation => self.move_message_selection(-1),
        }
    }

    pub(crate) fn select_previous(&mut self) {
        match self.focus_pane {
            FocusPane::SectionRail => {
                if !self.move_logout_selection_previous() {
                    self.selected_section = self.selected_section.previous();
                }
            }
            FocusPane::ChatList => self.move_selection_previous(),
            FocusPane::Conversation => self.move_message_selection(1),
        }
    }

    pub(crate) fn jump_top(&mut self) {
        match self.focus_pane {
            FocusPane::SectionRail => self.jump_logout_selection_top(),
            FocusPane::ChatList => self.jump_selection_top(),
            FocusPane::Conversation => self.jump_message_selection(false),
        }
    }

    pub(crate) fn jump_bottom(&mut self) {
        match self.focus_pane {
            FocusPane::SectionRail => self.jump_logout_selection_bottom(),
            FocusPane::ChatList => self.jump_selection_bottom(),
            FocusPane::Conversation => self.jump_message_selection(true),
        }
    }

    pub(crate) fn half_page_down(&mut self) {
        if self.focus_pane == FocusPane::Conversation {
            self.message_list_state
                .half_page_down_bounded(self.message_count_for_navigation(), 10);
        }
    }

    pub(crate) fn half_page_up(&mut self) {
        if self.focus_pane == FocusPane::Conversation {
            self.message_list_state
                .half_page_up_bounded(self.message_count_for_navigation(), 10);
        }
    }

    fn move_message_selection(&mut self, delta: isize) {
        let count = self.message_count_for_navigation();
        if delta < 0 {
            self.message_list_state.select_previous_bounded(count);
        } else {
            self.message_list_state.select_next_bounded(count);
        }
    }

    fn jump_message_selection(&mut self, bottom: bool) {
        let count = self.message_count_for_navigation();
        if bottom {
            self.message_list_state.jump_top_bounded(count);
        } else {
            self.message_list_state.jump_bottom_bounded(count);
        }
    }

    fn message_count_for_navigation(&self) -> usize {
        if self.selected_section == Section::Status {
            self.status_message_count()
        } else {
            self.message_count()
        }
    }

    pub(crate) fn message_count(&self) -> usize {
        self.open_chat()
            .and_then(|chat| self.chat_messages.get(&chat))
            .map_or(0, Vec::len)
    }

    pub(crate) fn status_message_count(&self) -> usize {
        self.open_status_contact()
            .map(|contact| self.status_messages(&contact).len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests;
