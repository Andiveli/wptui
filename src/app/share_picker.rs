use std::collections::{HashMap, HashSet};

use whatsrust as wr;

#[cfg(test)]
mod tests;

pub struct SharePicker {
    contacts: Vec<wr::JID>,
    labels: HashMap<wr::JID, String>,
    pub query: String,
    pub selected: usize,
    pub offset: usize,
    viewport_height: usize,
    selected_contacts: HashSet<wr::JID>,
}

impl SharePicker {
    pub fn new(
        mut contacts: Vec<wr::JID>,
        labels: HashMap<wr::JID, String>,
        recency: HashMap<wr::JID, i64>,
    ) -> Self {
        contacts.sort_by(|left, right| {
            recency
                .get(right)
                .unwrap_or(&i64::MIN)
                .cmp(recency.get(left).unwrap_or(&i64::MIN))
                .then_with(|| left.0.cmp(&right.0))
        });
        Self {
            contacts,
            labels,
            query: String::new(),
            selected: 0,
            offset: 0,
            viewport_height: 1,
            selected_contacts: HashSet::new(),
        }
    }

    pub fn visible_contacts(&self) -> Vec<&wr::JID> {
        let query = self.query.to_lowercase();
        self.contacts
            .iter()
            .filter(|jid| {
                query.is_empty()
                    || jid.0.to_lowercase().contains(&query)
                    || self
                        .labels
                        .get(*jid)
                        .is_some_and(|name| name.to_lowercase().contains(&query))
            })
            .collect()
    }

    pub fn selected_count(&self) -> usize {
        self.selected_contacts.len()
    }

    pub fn is_selected(&self, jid: &wr::JID) -> bool {
        self.selected_contacts.contains(jid)
    }

    pub fn destinations(&self) -> Vec<wr::JID> {
        self.contacts
            .iter()
            .filter(|jid| self.is_selected(jid))
            .cloned()
            .collect()
    }

    pub fn viewport(&self) -> std::ops::Range<usize> {
        let end = self.visible_contacts().len();
        let height = self.viewport_height.max(1).min(end);
        let start = self.offset.min(end.saturating_sub(height));
        start..start.saturating_add(height)
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = self.selected.saturating_add_signed(delta);
        self.clamp_selection();
        self.keep_selected_visible();
    }

    pub fn toggle_selected(&mut self) {
        let Some(jid) = self.visible_contacts().get(self.selected).cloned().cloned() else {
            return;
        };
        if !self.selected_contacts.insert(jid.clone()) {
            self.selected_contacts.remove(&jid);
        }
    }

    pub fn search_backspace(&mut self) {
        self.query.pop();
        self.reset_search_position();
        self.clamp_selection();
    }

    pub fn search_character(&mut self, character: char) {
        self.query.push(character);
        self.reset_search_position();
        self.clamp_selection();
    }

    fn clamp_selection(&mut self) {
        self.selected = self
            .selected
            .min(self.visible_contacts().len().saturating_sub(1));
        self.offset = self.offset.min(self.selected);
    }

    pub fn set_viewport_height(&mut self, height: usize) {
        self.viewport_height = height.max(1);
        self.keep_selected_visible();
    }

    pub fn keep_selected_visible(&mut self) {
        let height = self.viewport_height.max(1);
        if self.selected < self.offset {
            self.offset = self.selected;
        }
        if self.selected >= self.offset.saturating_add(height) {
            self.offset = self.selected + 1 - height;
        }
    }

    pub fn reset_search_position(&mut self) {
        self.selected = 0;
        self.offset = 0;
    }
}
