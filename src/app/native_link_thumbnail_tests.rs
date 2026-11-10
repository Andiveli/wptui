//! T2a private contract: no viewport/UI activation, network, files or worker timing.
//! `reconcile` receives validated, actually visible keys from the post-draw adapter.
//! Cache entries and outstanding tickets have independent bounds/lifetimes.
use std::cell::Cell;
use std::io::Cursor;
use std::sync::{Arc, mpsc};

use image::DynamicImage;
use ratatui::layout::Size;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

use super::media_jobs::MediaJobOwner;
use super::native_link_thumbnails::{
    EntryState, NativeLinkThumbnails, ThumbnailKey, decode_thumbnail, prepare_thumbnail,
};

fn key(id: &str) -> ThumbnailKey {
    ThumbnailKey {
        chat: "alice@s.whatsapp.net".to_owned().into(),
        message_id: Arc::from(id),
        source: Arc::from("read https://example.test/article"),
        matched_url: Arc::from("https://example.test/article"),
        size: Size::new(16, 4),
    }
}

fn protocol() -> StatefulProtocol {
    Picker::halfblocks().new_resize_protocol(DynamicImage::new_rgb8(8, 8))
}

fn jpeg(width: u32, height: u32) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(width, height)
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .unwrap();
    output.into_inner()
}

#[test]
fn duplicate_loading_ready_and_failed_keys_copy_and_prepare_only_once() {
    for success in [true, false] {
        let mut cache = NativeLinkThumbnails::new(4, 2);
        let target = key("one");
        let copies = Cell::new(0);
        let decodes = Cell::new(0);
        let prepares = Cell::new(0);
        let bytes = jpeg(8, 8);
        let supply = || {
            copies.set(copies.get() + 1);
            Some(bytes.clone())
        };
        cache.reconcile(std::slice::from_ref(&target));
        let job = cache.admit(&target, supply).unwrap();
        assert_eq!(cache.state(&target), EntryState::Loading);
        assert_eq!(cache.outstanding(), 1);
        for _ in 0..3 {
            cache.reconcile(std::slice::from_ref(&target));
            assert!(cache.admit(&target, supply).is_none());
        }
        let result = job.run_with(
            |bytes| {
                decodes.set(decodes.get() + 1);
                decode_thumbnail(bytes)
            },
            |_, size| {
                prepares.set(prepares.get() + 1);
                assert_eq!(size, target.size);
                success.then(protocol)
            },
        );
        let ticket = job.ticket();
        assert!(cache.finish(ticket.clone(), result));
        assert_eq!(cache.outstanding(), 0);
        assert_eq!(
            cache.state(&target),
            if success {
                EntryState::Ready
            } else {
                EntryState::Failed
            }
        );
        assert_eq!(cache.protocol_mut(&target).is_some(), success);
        cache.reconcile(&[]);
        cache.reconcile(std::slice::from_ref(&target));
        assert!(cache.admit(&target, supply).is_none());
        assert!(!cache.finish(ticket, Some(protocol())));
        assert_eq!(cache.outstanding(), 0);
        assert_eq!(cache.protocol_mut(&target).is_some(), success);
        assert_eq!((copies.get(), decodes.get(), prepares.get()), (1, 1, 1));
        assert_eq!(cache.len(), 1);
    }
}

#[test]
fn lru_eviction_is_bounded_and_hits_refresh_recency() {
    let mut cache = NativeLinkThumbnails::new(2, 2);
    let [a, b, c] = [key("a"), key("b"), key("c")];
    cache.reconcile(&[a.clone(), b.clone(), c.clone()]);
    for target in [&a, &b] {
        let job = cache.admit(target, || Some(vec![1])).unwrap();
        assert!(cache.finish(job.ticket(), Some(protocol())));
    }
    assert!(
        cache
            .admit(&a, || panic!("ready hit copied bytes"))
            .is_none()
    );
    let job = cache.admit(&c, || Some(vec![1])).unwrap();
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.state(&a), EntryState::Ready);
    assert_eq!(cache.state(&b), EntryState::Absent);
    assert!(cache.finish(job.ticket(), None));
    assert_eq!(cache.state(&c), EntryState::Failed);
    assert!(
        cache
            .admit(&c, || panic!("negative hit copied bytes"))
            .is_none()
    );
    let job = cache.admit(&b, || Some(vec![1])).unwrap();
    assert_eq!(cache.state(&a), EntryState::Absent);
    assert_eq!(cache.state(&c), EntryState::Failed);
    assert!(cache.finish(job.ticket(), None));
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.outstanding(), 0);
}

#[test]
fn evicted_or_cancelled_jobs_keep_slots_until_completion_is_handled() {
    let mut cache = NativeLinkThumbnails::new(1, 2);
    let [a, b, c] = [key("a"), key("b"), key("c")];
    cache.reconcile(&[a.clone(), b.clone()]);
    let first = cache.admit(&a, || Some(vec![1])).unwrap();
    let second = cache.admit(&b, || Some(vec![2])).unwrap();
    assert_eq!(cache.state(&a), EntryState::Absent);
    assert!(first.guard().is_cancelled());
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.outstanding(), 2);
    cache.reconcile(std::slice::from_ref(&c));
    assert!(second.guard().is_cancelled());
    for _ in 0..8 {
        assert!(cache.admit(&c, || panic!("no free job slot")).is_none());
        assert_eq!(cache.outstanding(), 2);
    }
    assert!(!cache.finish(first.ticket(), Some(protocol())));
    assert_eq!(cache.outstanding(), 1);
    assert!(!cache.finish(first.ticket(), None));
    assert_eq!(
        cache.outstanding(),
        1,
        "duplicate completion cannot free a slot"
    );
    let third = cache.admit(&c, || Some(vec![3])).unwrap();
    assert_eq!(cache.outstanding(), 2);
    assert!(!cache.finish(second.ticket(), Some(protocol())));
    assert_eq!(cache.outstanding(), 1);
    assert_eq!(cache.state(&c), EntryState::Loading);
    assert!(cache.finish(third.ticket(), Some(protocol())));
    assert_eq!(cache.outstanding(), 0);
    assert_eq!(cache.state(&c), EntryState::Ready);
    assert_eq!(cache.len(), 1);
}

#[test]
fn changed_identity_or_visibility_rejects_stale_tickets_including_away_and_back() {
    for change in [
        "visibility",
        "source",
        "chat",
        "message",
        "url",
        "geometry",
        "generation",
    ] {
        let mut cache = NativeLinkThumbnails::new(4, 2);
        let original = key("one");
        cache.reconcile(std::slice::from_ref(&original));
        let old = cache.admit(&original, || Some(vec![1])).unwrap();
        let mut current = original.clone();
        match change {
            "source" => current.source = Arc::from("edited https://example.test/article"),
            "chat" => current.chat = "bob@s.whatsapp.net".to_owned().into(),
            "message" => current.message_id = Arc::from("replacement"),
            "url" => current.matched_url = Arc::from("https://example.test/other"),
            "geometry" => current.size = Size::new(12, 3),
            _ => {}
        }
        if matches!(change, "visibility" | "generation") {
            cache.reconcile(&[]);
        } else {
            cache.reconcile(std::slice::from_ref(&current));
        }
        assert!(old.guard().is_cancelled(), "{change}");
        if change == "generation" {
            cache.reconcile(std::slice::from_ref(&current));
        }
        let new =
            (change != "visibility").then(|| cache.admit(&current, || Some(vec![2])).unwrap());
        if let Some(new) = &new {
            assert_ne!(old.ticket(), new.ticket(), "{change}");
        }
        assert!(!cache.finish(old.ticket(), Some(protocol())), "{change}");
        assert!(cache.protocol_mut(&original).is_none(), "{change}");
        if let Some(new) = new {
            assert_eq!(cache.outstanding(), 1, "{change}");
            assert_eq!(cache.state(&current), EntryState::Loading, "{change}");
            assert!(cache.finish(new.ticket(), Some(protocol())), "{change}");
            assert!(cache.protocol_mut(&current).is_some(), "{change}");
        }
        assert_eq!(cache.outstanding(), 0, "{change}");
    }
}

#[test]
fn cancellation_is_checked_before_decode_and_again_before_protocol_preparation() {
    for stage in ["before_decode", "after_decode"] {
        let mut cache = NativeLinkThumbnails::new(2, 2);
        let target = key("one");
        cache.reconcile(std::slice::from_ref(&target));
        let job = cache.admit(&target, || Some(vec![1])).unwrap();
        let guard = job.guard();
        let decodes = Cell::new(0);
        let prepares = Cell::new(0);
        if stage == "before_decode" {
            cache.reconcile(&[]);
        }
        let result = job.run_with(
            |_| {
                decodes.set(decodes.get() + 1);
                guard.cancel();
                Some(DynamicImage::new_rgb8(8, 8))
            },
            |_, _| {
                prepares.set(prepares.get() + 1);
                Some(protocol())
            },
        );
        assert!(result.is_none(), "{stage}");
        assert_eq!(
            decodes.get(),
            usize::from(stage == "after_decode"),
            "{stage}"
        );
        assert_eq!(prepares.get(), 0, "{stage}");
        assert!(!cache.finish(job.ticket(), result), "{stage}");
        assert_eq!(cache.outstanding(), 0, "{stage}");
        assert!(cache.protocol_mut(&target).is_none(), "{stage}");
        assert_eq!(cache.state(&target), EntryState::Absent, "{stage}");
        assert_eq!(cache.len(), 0, "{stage}");
    }
}

#[test]
fn nonvisible_keys_never_copy_bytes_or_start_jobs() {
    let mut cache = NativeLinkThumbnails::new(4, 2);
    let target = key("hidden");
    for visible in [vec![], vec![key("other")]] {
        cache.reconcile(&visible);
        assert!(cache.admit(&target, || panic!("offscreen copy")).is_none());
        assert_eq!(cache.state(&target), EntryState::Absent);
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.outstanding(), 0);
    }
}

#[test]
fn absent_bytes_start_no_job_and_bad_bytes_are_negative_cached() {
    let mut cache = NativeLinkThumbnails::new(4, 2);
    let target = key("one");
    cache.reconcile(std::slice::from_ref(&target));
    assert!(cache.admit(&target, || None).is_none());
    assert_eq!(cache.state(&target), EntryState::Absent);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.outstanding(), 0);
    for bytes in [Vec::new(), b"not a JPEG".to_vec(), vec![0; 256 * 1024 + 1]] {
        let mut cache = NativeLinkThumbnails::new(4, 2);
        cache.reconcile(std::slice::from_ref(&target));
        let prepares = Cell::new(0);
        // Invalid supplied bytes may fail at admission or in bounded decode.
        if let Some(job) = cache.admit(&target, || Some(bytes.clone())) {
            let result = job.run_with(decode_thumbnail, |_, _| {
                prepares.set(prepares.get() + 1);
                Some(protocol())
            });
            assert!(result.is_none());
            assert!(cache.finish(job.ticket(), result));
        }
        assert_eq!(cache.state(&target), EntryState::Failed);
        assert_eq!(cache.outstanding(), 0);
        assert_eq!(cache.len(), 1);
        assert_eq!(prepares.get(), 0);
        assert!(cache.protocol_mut(&target).is_none());
        assert!(
            cache
                .admit(&target, || panic!("bad bytes copied twice"))
                .is_none()
        );
    }
}

#[test]
fn memory_jpeg_preparation_is_bounded_before_decode_and_uses_no_terminal_query() {
    let bytes = jpeg(8, 8);
    let image = decode_thumbnail(&bytes).unwrap();
    assert_eq!((image.width(), image.height()), (8, 8));
    let picker = Arc::new(std::sync::Mutex::new(Picker::halfblocks()));
    assert!(prepare_thumbnail(&bytes, Size::new(16, 4), &picker).is_some());
    assert!(decode_thumbnail(b"bad").is_none());
    assert!(decode_thumbnail(&vec![0; 256 * 1024 + 1]).is_none());
    // Patch SOF dimensions rather than allocate a giant fixture. These remain
    // parseable JPEG headers: inspect dimensions before allocating pixel storage.
    for (width, height) in [(u16::MAX, 8), (8, u16::MAX), (u16::MAX, u16::MAX)] {
        let mut bomb = bytes.clone();
        let sof = bomb.windows(2).position(|w| w == [0xff, 0xc0]).unwrap();
        bomb[sof + 5..sof + 7].copy_from_slice(&height.to_be_bytes());
        bomb[sof + 7..sof + 9].copy_from_slice(&width.to_be_bytes());
        let dimensions = image::ImageReader::new(Cursor::new(&bomb))
            .with_guessed_format()
            .unwrap()
            .into_dimensions()
            .unwrap();
        assert_eq!(dimensions, (u32::from(width), u32::from(height)));
        assert!(decode_thumbnail(&bomb).is_none());
        assert!(prepare_thumbnail(&bomb, Size::new(16, 4), &picker).is_none());
    }
}

#[test]
fn managed_publication_still_delivers_active_results_and_blocks_after_shutdown() {
    let mut owner = MediaJobOwner::new();
    let permit = owner.permit();
    let (tx, rx) = mpsc::channel();
    permit.send(&tx, "active");
    assert_eq!(rx.try_recv().unwrap(), "active");
    owner.shutdown();
    owner.shutdown();
    permit.send(&tx, "late thumbnail");
    assert!(matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
}
