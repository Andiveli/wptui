use super::*;

#[test]
fn probe_audio_duration_reads_wav_seconds() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tone.wav");

    // Minimal RIFF/WAVE header: 16-bit PCM, mono, 8000 Hz, 2 seconds of
    // silence. Byte lengths must match the declared format for lofty.
    let sample_rate: u32 = 8000;
    let seconds: u32 = 2;
    let data_len: u32 = sample_rate * 2 * seconds;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.resize(44 + data_len as usize, 0);
    std::fs::write(&path, &wav).unwrap();

    assert_eq!(probe_audio_duration(&path), Some(2));
    assert_eq!(
        probe_audio_duration(&directory.path().join("missing.ogg")),
        None
    );
}

/// Hand-crafts a single Ogg page with a spec-compliant CRC.
fn ogg_page(
    header_type: u8,
    granule: u64,
    serial: u32,
    sequence: u32,
    segments: &[u8],
    payload: &[u8],
) -> Vec<u8> {
    let mut page = Vec::new();
    page.extend_from_slice(b"OggS");
    page.push(0);
    page.push(header_type);
    page.extend_from_slice(&granule.to_le_bytes());
    page.extend_from_slice(&serial.to_le_bytes());
    page.extend_from_slice(&sequence.to_le_bytes());
    page.extend_from_slice(&[0; 4]);
    page.push(segments.len() as u8);
    page.extend_from_slice(segments);
    page.extend_from_slice(payload);

    let mut crc: u32 = 0;
    for &byte in &page {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            if crc & 0x8000_0000 != 0 {
                crc = (crc << 1) ^ 0x04c1_1db7;
            } else {
                crc <<= 1;
            }
        }
    }
    page[22..26].copy_from_slice(&crc.to_le_bytes());
    page
}

/// Regression: WhatsApp stores voice notes as `.oga` (Ogg Opus), but lofty's
/// extension map only knows `opus`/`ogg` — so probing must sniff the content.
#[test]
fn probe_audio_duration_reads_ogg_opus_with_oga_extension() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("voice.oga");
    let serial: u32 = 0x1234_5678;

    let mut opus_head = Vec::new();
    opus_head.extend_from_slice(b"OpusHead");
    opus_head.push(1);
    opus_head.push(1);
    opus_head.extend_from_slice(&0u16.to_le_bytes());
    opus_head.extend_from_slice(&48000u32.to_le_bytes());
    opus_head.extend_from_slice(&0u16.to_le_bytes());
    opus_head.push(0);

    let vendor = b"wp-tui-test";
    let mut opus_tags = Vec::new();
    opus_tags.extend_from_slice(b"OpusTags");
    opus_tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    opus_tags.extend_from_slice(vendor);
    opus_tags.extend_from_slice(&0u32.to_le_bytes());

    let page1 = ogg_page(0x02, 0, serial, 0, &[opus_head.len() as u8], &opus_head);
    let page2 = ogg_page(
        0x04,
        48000 * 3,
        serial,
        1,
        &[opus_tags.len() as u8],
        &opus_tags,
    );

    let mut oga = Vec::new();
    oga.extend_from_slice(&page1);
    oga.extend_from_slice(&page2);
    std::fs::write(&path, &oga).unwrap();

    assert_eq!(probe_audio_duration(&path), Some(3));
}

#[test]
fn probe_real_whatsapp_audio_diagnostic() {
    let Some(path) = std::env::var("WPTUI_PROBE_PATH").ok() else {
        return;
    };
    let path = std::path::Path::new(&path);
    let result = probe_audio_duration(path);
    eprintln!("probe({}) = {result:?}", path.display());
    assert!(
        result.is_some(),
        "expected lofty to read {}",
        path.display()
    );
}
