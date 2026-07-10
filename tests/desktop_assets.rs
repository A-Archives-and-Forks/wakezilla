#![cfg(feature = "desktop-tray")]

use std::io::Cursor;

const MASTER_PNG: &[u8] = include_bytes!("../assets/desktop/wakezilla-1024.png");
const MACOS_ICNS: &[u8] = include_bytes!("../assets/desktop/Wakezilla.icns");
const WINDOWS_ICO: &[u8] = include_bytes!("../assets/desktop/wakezilla.ico");
const HICOLOR_48: &[u8] =
    include_bytes!("../assets/desktop/hicolor/48x48/apps/dev.wakezilla.Wakezilla.png");
const HICOLOR_128: &[u8] =
    include_bytes!("../assets/desktop/hicolor/128x128/apps/dev.wakezilla.Wakezilla.png");
const HICOLOR_256: &[u8] =
    include_bytes!("../assets/desktop/hicolor/256x256/apps/dev.wakezilla.Wakezilla.png");

#[derive(Debug)]
struct DecodedPng {
    width: u32,
    height: u32,
    source_color_type: png::ColorType,
    source_bit_depth: png::BitDepth,
    pixels: Vec<u8>,
}

fn decode_png(bytes: &[u8], label: &str) -> DecodedPng {
    assert_eq!(
        bytes.get(..8),
        Some(&b"\x89PNG\r\n\x1a\n"[..]),
        "{label} must start with the PNG signature"
    );

    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|error| panic!("{label} must have a valid PNG header: {error}"));
    let source_color_type = reader.info().color_type;
    let source_bit_depth = reader.info().bit_depth;
    let output_size = reader
        .output_buffer_size()
        .unwrap_or_else(|| panic!("{label} decoded buffer must fit in memory"));
    let mut pixels = vec![0; output_size];
    let frame = reader
        .next_frame(&mut pixels)
        .unwrap_or_else(|error| panic!("{label} must contain valid PNG pixels: {error}"));
    pixels.truncate(frame.buffer_size());

    DecodedPng {
        width: frame.width,
        height: frame.height,
        source_color_type,
        source_bit_depth,
        pixels,
    }
}

fn assert_rgba_png(bytes: &[u8], label: &str, expected_size: u32) -> DecodedPng {
    let decoded = decode_png(bytes, label);
    assert_eq!(decoded.width, expected_size, "{label} width");
    assert_eq!(decoded.height, expected_size, "{label} height");
    assert_eq!(
        decoded.source_color_type,
        png::ColorType::Rgba,
        "{label} must be stored as RGBA"
    );
    assert_eq!(
        decoded.source_bit_depth,
        png::BitDepth::Eight,
        "{label} must use eight-bit channels"
    );
    assert_eq!(
        decoded.pixels.len(),
        expected_size as usize * expected_size as usize * 4,
        "{label} decoded RGBA byte count"
    );
    decoded
}

fn little_endian_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("two-byte little-endian value"),
    )
}

fn little_endian_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("four-byte little-endian value"),
    )
}

fn big_endian_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("four-byte big-endian value"),
    )
}

#[test]
fn master_icon_is_a_transparent_1024_pixel_rgba_png() {
    let decoded = assert_rgba_png(MASTER_PNG, "master icon", 1024);
    let width = decoded.width as usize;
    let height = decoded.height as usize;
    let alpha_at = |x: usize, y: usize| decoded.pixels[(y * width + x) * 4 + 3];

    for (x, y) in [
        (0, 0),
        (width - 1, 0),
        (0, height - 1),
        (width - 1, height - 1),
    ] {
        assert_eq!(alpha_at(x, y), 0, "master corner ({x}, {y})");
    }

    let pixel_count = width * height;
    let visible_count = decoded
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] != 0)
        .count();
    let opaque_count = decoded
        .pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] >= 250)
        .count();
    let visible_coverage = visible_count as f64 / pixel_count as f64;
    let opaque_coverage = opaque_count as f64 / pixel_count as f64;

    assert!(
        (0.10..=0.70).contains(&visible_coverage),
        "master visible alpha coverage {visible_coverage:.3} must leave generous padding"
    );
    assert!(
        (0.08..=0.70).contains(&opaque_coverage),
        "master opaque alpha coverage {opaque_coverage:.3} must contain a substantial subject"
    );
}

#[test]
fn linux_hicolor_icons_are_square_rgba_pngs_at_required_sizes() {
    for (bytes, size) in [(HICOLOR_48, 48), (HICOLOR_128, 128), (HICOLOR_256, 256)] {
        let label = format!("Linux hicolor {size}x{size} icon");
        assert_rgba_png(bytes, &label, size);
    }
}

#[test]
fn windows_icon_contains_valid_png_entries_at_required_sizes() {
    assert!(WINDOWS_ICO.len() >= 6, "ICO header must be present");
    assert_eq!(&WINDOWS_ICO[..4], b"\0\0\x01\0", "ICO magic");

    let entry_count = little_endian_u16(WINDOWS_ICO, 4) as usize;
    assert_eq!(entry_count, 4, "ICO must contain exactly four images");
    let table_end = 6 + entry_count * 16;
    assert!(table_end <= WINDOWS_ICO.len(), "ICO directory must fit");

    let mut sizes = Vec::with_capacity(entry_count);
    let mut ranges = Vec::with_capacity(entry_count);
    for index in 0..entry_count {
        let entry = 6 + index * 16;
        let width = match WINDOWS_ICO[entry] {
            0 => 256,
            value => u32::from(value),
        };
        let height = match WINDOWS_ICO[entry + 1] {
            0 => 256,
            value => u32::from(value),
        };
        assert_eq!(width, height, "ICO entry {index} must be square");
        assert_eq!(
            little_endian_u16(WINDOWS_ICO, entry + 4),
            1,
            "ICO entry {index} color planes"
        );
        assert_eq!(
            little_endian_u16(WINDOWS_ICO, entry + 6),
            32,
            "ICO entry {index} bits per pixel"
        );

        let payload_size = little_endian_u32(WINDOWS_ICO, entry + 8) as usize;
        let payload_offset = little_endian_u32(WINDOWS_ICO, entry + 12) as usize;
        let payload_end = payload_offset
            .checked_add(payload_size)
            .expect("ICO payload end must not overflow");
        assert!(payload_size > 8, "ICO entry {index} payload is trivial");
        assert!(
            payload_offset >= table_end && payload_end <= WINDOWS_ICO.len(),
            "ICO entry {index} payload must be inside the file"
        );

        let label = format!("ICO {width}x{height} payload");
        let decoded = decode_png(&WINDOWS_ICO[payload_offset..payload_end], &label);
        assert_eq!((decoded.width, decoded.height), (width, height), "{label}");
        sizes.push(width);
        ranges.push(payload_offset..payload_end);
    }

    sizes.sort_unstable();
    assert_eq!(sizes, [16, 32, 48, 256], "ICO image sizes");
    ranges.sort_unstable_by_key(|range| range.start);
    for pair in ranges.windows(2) {
        assert!(
            pair[0].end <= pair[1].start,
            "ICO payload ranges must not overlap"
        );
    }
}

#[test]
fn macos_icon_has_a_well_formed_1024_pixel_representation() {
    assert!(MACOS_ICNS.len() >= 8, "ICNS header must be present");
    assert_eq!(&MACOS_ICNS[..4], b"icns", "ICNS magic");
    assert_eq!(
        big_endian_u32(MACOS_ICNS, 4) as usize,
        MACOS_ICNS.len(),
        "ICNS declared length"
    );

    let mut offset = 8;
    let mut chunk_count = 0;
    let mut has_1024_representation = false;
    while offset < MACOS_ICNS.len() {
        assert!(
            offset + 8 <= MACOS_ICNS.len(),
            "ICNS chunk {chunk_count} header must fit"
        );
        let kind = &MACOS_ICNS[offset..offset + 4];
        let chunk_size = big_endian_u32(MACOS_ICNS, offset + 4) as usize;
        assert!(chunk_size > 8, "ICNS chunk {chunk_count} is trivial");
        let chunk_end = offset
            .checked_add(chunk_size)
            .expect("ICNS chunk end must not overflow");
        assert!(
            chunk_end <= MACOS_ICNS.len(),
            "ICNS chunk {chunk_count} must fit inside the file"
        );

        if kind == b"ic10" {
            let decoded = decode_png(&MACOS_ICNS[offset + 8..chunk_end], "ICNS ic10 payload");
            assert_eq!(
                (decoded.width, decoded.height),
                (1024, 1024),
                "ICNS ic10 representation"
            );
            has_1024_representation = true;
        }

        chunk_count += 1;
        offset = chunk_end;
    }

    assert_eq!(
        offset,
        MACOS_ICNS.len(),
        "ICNS chunks must consume the file"
    );
    assert!(chunk_count >= 4, "ICNS must contain several icon chunks");
    assert!(
        has_1024_representation,
        "ICNS must contain an ic10 1024x1024 representation"
    );
}
