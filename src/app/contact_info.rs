use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use whatsrust as wr;

use super::App;
use super::actions::{FocusPane, Section};
use super::events::{AppEvent, AppInput};
use super::media_jobs::MediaJobOwner;
use crate::input_key::{Key, KeyCode};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedGroup {
    pub jid: wr::JID,
    pub name: Arc<str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SharedGroups {
    Loading,
    Ready(Vec<SharedGroup>),
    Unavailable,
}

pub struct ContactInfo {
    pub contact: wr::JID,
    pub generation: u64,
    pub groups: SharedGroups,
    pub scroll: usize,
    pub focused: bool,
    pending: bool,
    cancelled: Arc<AtomicBool>,
}

impl Drop for ContactInfo {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// Existing bridge participant queries cannot distinguish failure from an empty
/// roster. Keep that ambiguity explicit rather than claiming no shared groups.
pub trait ContactInfoQuery: Send + Sync {
    fn participants(&self, group: &wr::JID) -> Result<Vec<wr::GroupParticipant>, String>;
}

pub struct WhatsRustContactInfoQuery;

impl ContactInfoQuery for WhatsRustContactInfoQuery {
    fn participants(&self, group: &wr::JID) -> Result<Vec<wr::GroupParticipant>, String> {
        wr::get_group_info(group).map_err(|error| format!("{error:?}"))?;
        let participants = wr::get_group_participants(group);
        if participants.is_empty() {
            Err("Participant lookup unavailable".into())
        } else {
            Ok(participants)
        }
    }
}

#[derive(Debug)]
pub struct ContactInfoResult {
    pub generation: u64,
    pub contact: wr::JID,
    pub groups: Result<Vec<SharedGroup>, String>,
}

fn phone_digits(jid: &wr::JID) -> Option<&str> {
    let user = jid.0.strip_suffix("@s.whatsapp.net")?;
    (!user.is_empty() && user.bytes().all(|byte| byte.is_ascii_digit())).then_some(user)
}

impl App<'_> {
    pub(crate) fn contact_info_target(&mut self) -> Option<wr::JID> {
        if self.rail_on_logout
            || !matches!(self.selected_section, Section::Chats | Section::Communities)
        {
            return None;
        }
        let target = match self.focus_pane {
            FocusPane::Conversation => self.open_chat(),
            FocusPane::ChatList => self.get_selected_chat(),
            FocusPane::SectionRail => None,
        }?;
        (target.0.ends_with("@lid") || target.0.ends_with("@s.whatsapp.net")).then_some(target)
    }

    pub(crate) fn contact_info_phone(&self, contact: &wr::JID) -> Option<String> {
        phone_digits(contact)
            .or_else(|| {
                self.verified_phones
                    .get(contact)
                    .map(|phone| phone.as_ref())
            })
            .filter(|phone| !phone.is_empty() && phone.bytes().all(|byte| byte.is_ascii_digit()))
            .map(|phone| format!("+{phone}"))
    }

    pub(crate) fn open_contact_info(&mut self) {
        let Some(contact) = self.contact_info_target() else {
            self.unavailable("Select a contact conversation first");
            return;
        };
        self.remember_verified_phone(&contact);
        self.contact_info_generation = self.contact_info_generation.wrapping_add(1);
        self.contact_info = Some(ContactInfo {
            contact,
            generation: self.contact_info_generation,
            groups: SharedGroups::Loading,
            scroll: 0,
            focused: true,
            pending: true,
            cancelled: Arc::new(AtomicBool::new(false)),
        });
    }

    pub(crate) fn close_contact_info(&mut self) {
        if self.contact_info.take().is_some() {
            // Reuse the global prefix cancellation boundary; an interrupted g
            // sequence must not swallow the first key after closing details.
            let _ = self.kh.resolve(Key::k(KeyCode::Esc));
        }
    }

    /// Called only after composer, menus and pickers decline ownership.
    pub(crate) fn handle_contact_info_input(&mut self, key: &Key) -> bool {
        let Some(info) = &mut self.contact_info else {
            return false;
        };
        if *key == Key::k(KeyCode::Esc) {
            self.close_contact_info();
            return true;
        }
        if !info.focused {
            return false;
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down if *key == Key::k(key.code) => {
                let count = match &info.groups {
                    SharedGroups::Ready(groups) => groups.len(),
                    _ => 0,
                };
                info.scroll = info.scroll.saturating_add(1).min(count.saturating_sub(1));
                true
            }
            KeyCode::Char('k') | KeyCode::Up if *key == Key::k(key.code) => {
                info.scroll = info.scroll.saturating_sub(1);
                true
            }
            _ => {
                // Ordinary focus/composer shortcuts retain their existing meaning.
                info.focused = false;
                false
            }
        }
    }

    pub(crate) fn schedule_contact_info(&mut self, jobs: &mut MediaJobOwner) {
        let Some(info) = self.contact_info.as_mut().filter(|info| info.pending) else {
            return;
        };
        info.pending = false;
        let contact = info.contact.clone();
        let generation = info.generation;
        let cancelled = Arc::clone(&info.cancelled);
        let phone = self
            .verified_phones
            .get(&contact)
            .map(|phone| wr::JID::from(format!("{phone}@s.whatsapp.net")));
        // The local contact/chat catalog already contains joined groups. Do not
        // call get_contacts again: that bridge path can panic on network failure.
        let mut candidates = self
            .contacts
            .keys()
            .chain(self.chats.keys())
            .filter(|jid| Self::is_group_chat(jid))
            .cloned()
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.0.cmp(&right.0));
        candidates.dedup();
        let candidates = candidates
            .into_iter()
            .map(|jid| SharedGroup {
                name: self.contact_name(&jid),
                jid,
            })
            .collect::<Vec<_>>();
        let query = Arc::clone(&self.contact_info_query);
        let tx = self.tx.clone();
        jobs.spawn(move |permit| {
            let groups = query_shared_groups(
                query.as_ref(),
                &contact,
                phone.as_ref(),
                candidates,
                &cancelled,
            );
            if cancelled.load(Ordering::Acquire) {
                return;
            }
            permit.send(
                &tx,
                AppInput::App(AppEvent::ContactInfoLoaded(ContactInfoResult {
                    generation,
                    contact,
                    groups,
                })),
            );
        });
    }

    pub(crate) fn apply_contact_info_result(&mut self, result: ContactInfoResult) -> bool {
        let Some(info) = self
            .contact_info
            .as_mut()
            .filter(|info| info.generation == result.generation && info.contact == result.contact)
        else {
            return false;
        };
        info.groups = match result.groups {
            Ok(groups) => SharedGroups::Ready(groups),
            Err(_) => SharedGroups::Unavailable,
        };
        true
    }
}

fn query_shared_groups(
    query: &dyn ContactInfoQuery,
    contact: &wr::JID,
    phone: Option<&wr::JID>,
    candidates: Vec<SharedGroup>,
    cancelled: &AtomicBool,
) -> Result<Vec<SharedGroup>, String> {
    let mut groups = Vec::new();
    for group in candidates {
        if cancelled.load(Ordering::Acquire) {
            return Err("Contact query cancelled".into());
        }
        let participants = query.participants(&group.jid)?;
        if participants.iter().any(|participant| {
            [&participant.jid, &participant.phone_number]
                .into_iter()
                .any(|jid| jid == contact || phone.is_some_and(|phone| jid == phone))
        }) {
            groups.push(group);
        }
    }
    groups.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.jid.0.cmp(&right.jid.0))
    });
    Ok(groups)
}
