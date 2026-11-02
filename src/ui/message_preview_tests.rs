use std::sync::Arc;

use image::{DynamicImage, ImageBuffer, Rgba};
use ratatui::{
    Terminal,
    backend::TestBackend,
    crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers},
    layout::{Rect, Size},
    style::Color,
};
use ratatui_image::{
    FontSize,
    picker::{Picker, ProtocolType},
};
use whatsrust as wr;

use super::message_preview_cache::{StackPreviewCache, bounded_source};
use crate::app::{
    FileMeta, Metadata,
    actions::{ConversationMode, FocusPane},
    events::MediaRenderPlan,
    read_receipts::VisibilityPlan,
    test_support::TestApp,
};

fn pixels(rgb: [u8; 3]) -> DynamicImage {
    ImageBuffer::from_pixel(240, 240, Rgba([rgb[0], rgb[1], rgb[2], 255])).into()
}

fn fixture(sent: bool, count: usize) -> TestApp {
    let mut app = TestApp::new();
    let chat: wr::JID = "stack@example.test".to_owned().into();
    let colors = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]];
    for (index, color) in colors.into_iter().take(count).enumerate() {
        let id: wr::MessageId = format!("item-{index}").into();
        let path: Arc<str> = format!("{index}.png").into();
        app.add_message(wr::Message {
            info: wr::MessageInfo {
                id: id.clone(),
                chat: chat.clone(),
                sender: "alice@example.test".to_owned().into(),
                timestamp: 1_700_000_000 + index as i64,
                is_from_me: sent,
                mentions_self: false,
                quote_id: None,
                read_by: 0,
                forwarding: Default::default(),
            },
            message: wr::MessageContent::File(wr::FileContent {
                path: path.clone(),
                kind: if index % 2 == 0 {
                    wr::FileKind::Image
                } else {
                    wr::FileKind::Video
                },
                caption: Some(format!("caption-{index}").into()),
                ..Default::default()
            }),
        });
        let source = bounded_source(&pixels(color));
        app.cache_file_preview(
            id,
            path.clone(),
            Picker::halfblocks().new_resize_protocol((*source).clone()),
        );
        app.stack_previews.install(path, source);
    }
    app.open_chat = Some(chat);
    app.focus_pane = FocusPane::Conversation;
    app.conversation_mode = ConversationMode::MessageNavigation;
    app.message_list_state.select(Some(0));
    app
}

fn draw(app: &mut TestApp, width: u16, height: u16) -> (ratatui::buffer::Buffer, VisibilityPlan) {
    draw_with_picker(app, width, height, &mut Picker::halfblocks())
}

fn draw_with_picker(
    app: &mut TestApp,
    width: u16,
    height: u16,
    picker: &mut Picker,
) -> (ratatui::buffer::Buffer, VisibilityPlan) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut visibility = VisibilityPlan::default();
    terminal
        .draw(|frame| {
            super::render_messages_with_plan(
                frame,
                app,
                &mut MediaRenderPlan::default(),
                &mut visibility,
                Rect::new(0, 0, width, height),
            );
        })
        .unwrap();
    // Deterministically complete only worker preparation, never bypass rendering.
    while let Some(work) = app.stack_previews.take_work() {
        let results = work.prepare(picker);
        app.stack_previews.finish(results);
    }
    terminal
        .draw(|frame| {
            super::render_messages_with_plan(
                frame,
                app,
                &mut MediaRenderPlan::default(),
                &mut visibility,
                Rect::new(0, 0, width, height),
            );
        })
        .unwrap();
    (terminal.backend().buffer().clone(), visibility)
}

fn colored_run(buffer: &ratatui::buffer::Buffer, color: Color) -> Vec<u16> {
    (0..buffer.area.width)
        .filter(|x| {
            (0..buffer.area.height)
                .any(|y| buffer[(*x, y)].fg == color || buffer[(*x, y)].bg == color)
        })
        .collect()
}

#[test]
fn real_right_hand_strips_shift_with_original_selection_and_reverse() {
    for sent in [false, true] {
        let mut app = fixture(sent, 4);
        let (first, _) = draw(&mut app, 80, 40);
        let yellow = colored_run(&first, Color::Rgb(255, 255, 0));
        let blue = colored_run(&first, Color::Rgb(0, 0, 255));
        let green = colored_run(&first, Color::Rgb(0, 255, 0));
        assert!(yellow.len() > blue.len() && !blue.is_empty());
        assert!(blue.len() <= 4 && green.len() <= 4);
        assert!(yellow.last().unwrap() < blue.first().unwrap());
        assert!(blue.last().unwrap() < green.first().unwrap());
        let text = first
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("4 photos/videos"));
        assert!(text.contains("caption-3") && !text.contains("caption-2"));
        assert!(!text.contains("┌─"));
        app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)));
        let (moved, visibility) = draw(&mut app, 80, 40);
        assert_eq!(
            colored_run(&moved, Color::Rgb(0, 0, 255)).first(),
            yellow.first()
        );
        assert!(colored_run(&moved, Color::Rgb(0, 0, 255)).len() > 4);
        assert!(colored_run(&moved, Color::Rgb(255, 255, 0)).len() <= 4);
        assert_eq!(
            app.message_list_state.get_selected_message().as_deref(),
            Some("item-2")
        );
        app.apply_visibility_plan(visibility);
        let receipts = std::iter::from_fn(|| app.read_receipts.take_staged())
            .map(|item| item.message_id)
            .collect::<Vec<_>>();
        if !sent {
            assert!(!receipts.is_empty());
        }
        assert!(receipts.iter().all(|id| id == "item-2"));
        app.on_terminal_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
        let (reversed, _) = draw(&mut app, 80, 40);
        assert_eq!(colored_run(&reversed, Color::Rgb(255, 255, 0)), yellow);
        assert_eq!(colored_run(&reversed, Color::Rgb(0, 0, 255)), blue);
    }
}

#[test]
#[allow(deprecated)] // Deterministic font size without querying the test terminal.
fn native_crop_payloads_match_the_final_rectangle() {
    for protocol in [ProtocolType::Kitty, ProtocolType::Sixel] {
        let mut cache = StackPreviewCache::default();
        let path: Arc<str> = "native.png".into();
        cache.install(path.clone(), bounded_source(&pixels([255, 0, 0])));
        let key = cache
            .key(&path, Size::new(24, 12), Rect::new(20, 5, 4, 3))
            .unwrap();
        assert!(cache.request(key.clone()));
        let mut picker = Picker::from_fontsize(FontSize::new(8, 16));
        picker.set_protocol_type(protocol);
        let work = cache.take_work().unwrap();
        cache.finish(work.prepare(&mut picker));
        let image = cache.get(&key).unwrap();
        assert_eq!(image.size(), Size::new(4, 3));
        let mut buffer = ratatui::buffer::Buffer::empty(Rect::new(0, 0, 12, 8));
        ratatui::widgets::Widget::render(
            ratatui_image::Image::new(image),
            Rect::new(2, 2, 4, 3),
            &mut buffer,
        );
        let payload = buffer[(2, 2)].symbol();
        assert!(payload.contains('\x1b'));
        match image {
            ratatui_image::protocol::Protocol::Kitty(_) => {
                assert!(payload.contains("s=32,v=48,"));
            }
            ratatui_image::protocol::Protocol::Sixel(_) => {
                assert!(payload.contains("\"1;1;32;48"));
            }
            _ => panic!("expected native image protocol"),
        }
        assert_eq!(buffer[(6, 2)].symbol(), " ");
        assert_eq!(buffer[(2, 5)].symbol(), " ");
        assert!(
            !cache.request(key.clone()),
            "ready payloads are not re-encoded each frame"
        );
        assert!(cache.take_work().is_none());
    }
}

#[test]
fn native_stacks_clip_payload_pixels_to_the_actual_visible_rows_and_columns() {
    for protocol in [ProtocolType::Kitty, ProtocolType::Sixel] {
        let mut app = fixture(false, 4);
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(protocol);
        let font = picker.font_size();
        let (buffer, _) = draw_with_picker(&mut app, 80, 8, &mut picker);
        let mut payloads = 0;
        for y in 0..8 {
            for x in 0..80 {
                let symbol = buffer[(x, y)].symbol();
                let dimensions = if let Some(header) = symbol.split("s=").nth(1) {
                    let mut fields = header.split(',');
                    let width = fields.next().unwrap().parse::<u32>().unwrap();
                    let height = fields
                        .next()
                        .unwrap()
                        .strip_prefix("v=")
                        .unwrap()
                        .parse::<u32>()
                        .unwrap();
                    Some((width, height))
                } else if let Some(header) = symbol.split("\"1;1;").nth(1) {
                    let (width, rest) = header.split_once(';').unwrap();
                    let height = rest
                        .chars()
                        .take_while(char::is_ascii_digit)
                        .collect::<String>();
                    Some((
                        width.parse::<u32>().unwrap(),
                        height.parse::<u32>().unwrap(),
                    ))
                } else {
                    None
                };
                if let Some((width, height)) = dimensions {
                    payloads += 1;
                    assert!(width <= u32::from(80 - x) * u32::from(font.width));
                    assert!(height <= u32::from(8 - y) * u32::from(font.height));
                }
            }
        }
        assert!(payloads >= 2, "main and neighboring real native payloads");
    }
}

#[test]
fn strips_are_real_fitted_pixel_crops_not_squeezed_whole_images() {
    let source: DynamicImage = ImageBuffer::from_fn(240, 240, |x, _| {
        if x < 120 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 255, 0, 255])
        }
    })
    .into();
    let mut cache = StackPreviewCache::default();
    let path: Arc<str> = "split.png".into();
    cache.install(path.clone(), bounded_source(&source));
    let key = cache
        .key(&path, Size::new(24, 12), Rect::new(20, 0, 4, 12))
        .unwrap();
    cache.request(key.clone());
    let work = cache.take_work().unwrap();
    cache.finish(work.prepare(&mut Picker::halfblocks()));
    let mut buffer = ratatui::buffer::Buffer::empty(Rect::new(0, 0, 4, 12));
    ratatui::widgets::Widget::render(
        ratatui_image::Image::new(cache.get(&key).unwrap()),
        Rect::new(0, 0, 4, 12),
        &mut buffer,
    );
    assert_eq!(colored_run(&buffer, Color::Rgb(0, 255, 0)).len(), 4);
    assert!(colored_run(&buffer, Color::Rgb(255, 0, 0)).is_empty());
}

#[test]
fn crop_cache_is_bounded_and_stale_sources_cannot_repopulate_it() {
    let mut cache = StackPreviewCache::default();
    let path: Arc<str> = "source.png".into();
    let source = bounded_source(&DynamicImage::new_rgba8(2048, 2048));
    assert!(source.width() <= 512 && source.height() <= 256);
    assert!(source.as_bytes().len() <= 512 * 256 * 4);
    cache.install(path.clone(), source);
    for x in 0..100 {
        let key = cache
            .key(&path, Size::new(120, 12), Rect::new(x, 0, 1, 12))
            .unwrap();
        cache.request(key);
    }
    assert_eq!(cache.len(), 64);
    let work = cache.take_work().unwrap();
    cache.remove(&path);
    cache.install(path, bounded_source(&pixels([0, 255, 0])));
    cache.finish(work.prepare(&mut Picker::halfblocks()));
    assert_eq!(cache.len(), 0);
}

#[test]
#[allow(deprecated)] // Exercise extreme font bounds without querying a terminal.
fn crop_jobs_and_pixel_scratch_are_bounded_and_failures_are_stable() {
    let mut cache = StackPreviewCache::default();
    let path: Arc<str> = "source.png".into();
    cache.install(path.clone(), bounded_source(&pixels([255, 0, 0])));
    let mut keys = Vec::new();
    for x in 0..20 {
        let key = cache
            .key(&path, Size::new(120, 12), Rect::new(x, 0, 1, 12))
            .unwrap();
        cache.request(key.clone());
        keys.push(key);
    }
    let work = (0..4)
        .map(|_| cache.take_work().unwrap())
        .collect::<Vec<_>>();
    assert!(cache.take_work().is_none(), "at most four jobs at once");
    let mut picker = Picker::from_fontsize(FontSize::new(1000, 1000));
    for batch in work {
        cache.finish(batch.prepare(&mut picker));
    }
    assert!(cache.get(&keys[0]).is_none());
    assert!(
        !cache.request(keys[0].clone()),
        "oversized geometry is not retried per frame"
    );
    let remaining = cache
        .take_work()
        .expect("four batches prepare only sixteen of twenty keys");
    cache.finish(remaining.prepare(&mut picker));
    assert!(cache.take_work().is_none());
}

#[test]
fn narrow_and_clipped_stacks_keep_pixels_inside_the_viewport() {
    for width in [1, 4, 12, 30, 80] {
        let mut app = fixture(false, 4);
        let (buffer, _) = draw(&mut app, width, 8);
        assert_eq!(buffer.area, Rect::new(0, 0, width, 8));
        assert_eq!(
            app.message_list_state.get_selected_message().as_deref(),
            Some("item-3")
        );
        assert!(colored_run(&buffer, Color::Rgb(0, 0, 255)).len() <= 4);
    }
}

#[test]
fn bounded_sources_preserve_original_fit_geometry_without_upscaling_tiny_images() {
    let mut cache = StackPreviewCache::default();
    let path: Arc<str> = "wide.png".into();
    let original = DynamicImage::new_rgba8(2048, 512);
    let font = FontSize::new(10, 20);
    let available = Size::new(80, 12);
    let expected = ratatui_image::Resize::Fit(None).size_for(&original, font, available);
    cache.install_original(path.clone(), bounded_source(&original), (2048, 512));
    assert_eq!(cache.fit_for(&path, font, available), Some(expected));
    assert!(cache.was_downsampled(&path));
    let tiny = bounded_source(&DynamicImage::new_rgba8(2, 2));
    assert_eq!((tiny.width(), tiny.height()), (2, 2));
}

#[test]
fn singleton_keeps_its_original_colored_preview_and_caption_without_a_strip() {
    let mut app = fixture(false, 1);
    let (buffer, _) = draw(&mut app, 80, 40);
    assert!(colored_run(&buffer, Color::Rgb(255, 0, 0)).len() > 4);
    assert!(colored_run(&buffer, Color::Rgb(0, 255, 0)).is_empty());
    let text = buffer
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("caption-0"));
    assert!(!text.contains("photos/videos"));
    assert_eq!(app.stack_previews.len(), 0);
}
