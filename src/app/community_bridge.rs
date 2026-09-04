use super::{App, Section};
use log::warn;
use whatsrust as wr;

impl App<'_> {
    pub(crate) fn load_communities(&mut self) {
        let result = self.community_query.get_communities();
        self.apply_community_result(result);
    }

    pub fn refresh_communities<F>(&mut self, fetch: F)
    where
        F: FnOnce() -> Result<Vec<wr::CommunityInfo>, wr::CommunitiesError>,
    {
        self.apply_community_result(fetch());
    }

    fn apply_community_result(
        &mut self,
        result: Result<Vec<wr::CommunityInfo>, wr::CommunitiesError>,
    ) {
        match result {
            Ok(records) => {
                let selected = (self.selected_section == Section::Communities)
                    .then(|| self.selected_community_node_jid())
                    .flatten();
                let next = Self::build_community_nodes(&records);
                let changed = self.communities != next;
                self.communities = next;
                self.communities_unavailable = false;
                self.communities_loaded = true;
                if self.selected_section == Section::Communities {
                    self.select_community_node(selected);
                }
                if changed {
                    self.invalidate_chat_list();
                }
            }
            Err(error) => {
                warn!("Could not load communities: {error:?}");
                self.communities_unavailable = !self.communities_loaded;
            }
        }
    }
}

#[cfg(test)]
mod tests;
