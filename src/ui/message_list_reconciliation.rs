use ratatui::layout::Rect;
use whatsrust as wr;

use crate::app::App;

use super::{AuthorGroupContext, message_layout_integration, spacing_after_message};

const SELECTION_PADDING: usize = 4;

pub(super) fn reconcile(
    app: &mut App,
    list_area: Rect,
    items: &[wr::MessageId],
    author_groups: &[AuthorGroupContext],
    unread_count: usize,
) -> (usize, isize) {
    let selected = reconciled_index(
        app.message_list_state.selected,
        app.message_list_state
            .selected_message
            .as_ref()
            .and_then(|message_id| items.iter().position(|item| item == message_id)),
        items.len(),
    );
    if app.message_list_state.selected.is_none()
        && app.message_list_state.selected_message.is_some()
    {
        app.message_list_state.select(selected);
    } else if app.message_list_state.selected != selected {
        app.message_list_state.selected = selected;
    }

    let width = list_area.width as isize;
    let divider_after = unread_count.checked_sub(1);
    let previous_offset = app.message_list_state.offset;
    let mut previous_anchor = app
        .message_list_state
        .viewport_anchor
        .clone()
        .filter(|anchor| {
            anchor.width == width as usize
                && anchor.offset == app.message_list_state.offset
                && anchor.generation == message_layout_integration::height_generation(app)
                && app
                    .message_list_state
                    .selected
                    .is_none_or(|selected| selected >= anchor.index)
                && items
                    .get(anchor.index)
                    .is_some_and(|item| item == &anchor.message_id)
        });
    if let Some(anchor) = previous_anchor.as_mut() {
        let bottom_delta = list_area.bottom() as isize - anchor.bottom as isize;
        anchor.y = anchor.y.saturating_add(bottom_delta);
        anchor.bottom = list_area.bottom();
    }

    app.message_list_state.selected_message = app
        .message_list_state
        .selected
        .map(|selected| items[selected].clone());

    if let Some(selected) = app
        .message_list_state
        .selected
        .filter(|_| app.message_list_state.update_selected)
    {
        app.message_list_state.update_selected = false;

        let acc_height = previous_anchor
            .as_ref()
            .filter(|anchor| anchor.index <= selected)
            .map(|anchor| {
                let mut cursor = anchor.y;
                for index in anchor.index..selected {
                    cursor -= message_layout_integration::message_height_for_id(
                        &items[index],
                        width as usize,
                        app.message_list_state.selected == Some(index),
                        author_groups[index],
                        app,
                    ) as isize;
                    cursor -= spacing_after_message(
                        index,
                        author_groups,
                        app.message_list_state.selected,
                    ) as isize;
                    if divider_after == Some(index) {
                        cursor -= 1;
                    }
                }
                (list_area.bottom() as isize + app.message_list_state.offset as isize - cursor)
                    as usize
            })
            .unwrap_or_else(|| {
                items
                    .iter()
                    .take(selected)
                    .enumerate()
                    .map(|(index, item)| {
                        usize::from(divider_after == Some(index))
                            + message_layout_integration::message_height_for_id(
                                item,
                                width as usize,
                                app.message_list_state.selected == Some(index),
                                author_groups[index],
                                app,
                            )
                            + spacing_after_message(
                                index,
                                author_groups,
                                app.message_list_state.selected,
                            )
                    })
                    .sum::<usize>()
            });

        let selected_height = message_layout_integration::message_height_for_id(
            &items[selected],
            width as usize,
            true,
            author_groups[selected],
            app,
        );
        let low = acc_height < app.message_list_state.offset + SELECTION_PADDING;
        let high = acc_height + selected_height
            > app
                .message_list_state
                .offset
                .saturating_add((list_area.height as usize).saturating_sub(SELECTION_PADDING));

        if low {
            app.message_list_state.offset = acc_height.saturating_sub(SELECTION_PADDING);
        } else if high {
            app.message_list_state.offset = (acc_height + selected_height + SELECTION_PADDING)
                .saturating_sub(list_area.height as usize);
        }
        if app.message_list_state.offset != previous_offset {
            if let Some(anchor) = previous_anchor.as_mut() {
                let delta = app
                    .message_list_state
                    .offset
                    .abs_diff(previous_offset)
                    .min(isize::MAX as usize) as isize;
                anchor.y = if app.message_list_state.offset >= previous_offset {
                    anchor.y.saturating_add(delta)
                } else {
                    anchor.y.saturating_sub(delta)
                };
                anchor.offset = app.message_list_state.offset;
            }
        }
    }

    previous_anchor
        .map(|anchor| (anchor.index, anchor.y))
        .unwrap_or((
            0,
            list_area.bottom() as isize + app.message_list_state.offset as isize,
        ))
}

fn reconciled_index(
    selected: Option<usize>,
    identity_index: Option<usize>,
    len: usize,
) -> Option<usize> {
    selected
        .or(identity_index)
        .map(|index| index.min(len.saturating_sub(1)))
}

#[cfg(test)]
mod tests {
    use super::reconciled_index;

    #[test]
    fn resolves_identity_before_clamping_position() {
        assert_eq!(reconciled_index(None, Some(1), 2), Some(1));
        assert_eq!(reconciled_index(Some(9), None, 2), Some(1));
    }

    #[test]
    fn missing_identity_does_not_select_an_item() {
        assert_eq!(reconciled_index(None, None, 2), None);
        assert_eq!(reconciled_index(None, None, 0), None);
    }
}
