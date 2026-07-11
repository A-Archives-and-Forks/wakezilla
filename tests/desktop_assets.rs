#![cfg(feature = "desktop-tray")]

use std::collections::BTreeSet;
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

#[derive(Clone, Copy, Debug)]
struct AlphaCoverageBounds {
    visible_min: f64,
    visible_max: f64,
    opaque_min: f64,
    opaque_max: f64,
}

const MASTER_ALPHA_BOUNDS: AlphaCoverageBounds = AlphaCoverageBounds {
    visible_min: 0.10,
    visible_max: 0.70,
    opaque_min: 0.08,
    opaque_max: 0.70,
};
const DERIVED_ALPHA_BOUNDS: AlphaCoverageBounds = AlphaCoverageBounds {
    visible_min: 0.05,
    visible_max: 0.85,
    opaque_min: 0.03,
    opaque_max: 0.85,
};
const EXPECTED_ICNS_REPRESENTATIONS: [([u8; 4], u32); 10] = [
    (*b"ic04", 16),
    (*b"ic05", 32),
    (*b"ic07", 128),
    (*b"ic08", 256),
    (*b"ic09", 512),
    (*b"ic10", 1024),
    (*b"ic11", 32),
    (*b"ic12", 64),
    (*b"ic13", 256),
    (*b"ic14", 512),
];

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

fn validate_decoded_rgba_icon(
    decoded: &DecodedPng,
    label: &str,
    expected_size: u32,
    bounds: AlphaCoverageBounds,
) -> Result<(), String> {
    if (decoded.width, decoded.height) != (expected_size, expected_size) {
        return Err(format!(
            "{label} must be {expected_size}x{expected_size}, got {}x{}",
            decoded.width, decoded.height
        ));
    }
    if decoded.source_color_type != png::ColorType::Rgba {
        return Err(format!(
            "{label} must be stored as RGBA, got {:?}",
            decoded.source_color_type
        ));
    }
    if decoded.source_bit_depth != png::BitDepth::Eight {
        return Err(format!(
            "{label} must use eight-bit channels, got {:?}",
            decoded.source_bit_depth
        ));
    }

    let expected_bytes = expected_size as usize * expected_size as usize * 4;
    if decoded.pixels.len() != expected_bytes {
        return Err(format!(
            "{label} decoded RGBA byte count must be {expected_bytes}, got {}",
            decoded.pixels.len()
        ));
    }

    let width = decoded.width as usize;
    let height = decoded.height as usize;
    let alpha_at = |x: usize, y: usize| decoded.pixels[(y * width + x) * 4 + 3];
    for (x, y) in [
        (0, 0),
        (width - 1, 0),
        (0, height - 1),
        (width - 1, height - 1),
    ] {
        let alpha = alpha_at(x, y);
        if alpha != 0 {
            return Err(format!(
                "{label} corner ({x}, {y}) must be transparent, got alpha {alpha}"
            ));
        }
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

    if !(bounds.visible_min..=bounds.visible_max).contains(&visible_coverage) {
        return Err(format!(
            "{label} visible alpha coverage {visible_coverage:.3} must be within {:.3}..={:.3}",
            bounds.visible_min, bounds.visible_max
        ));
    }
    if !(bounds.opaque_min..=bounds.opaque_max).contains(&opaque_coverage) {
        return Err(format!(
            "{label} opaque alpha coverage {opaque_coverage:.3} must be within {:.3}..={:.3}",
            bounds.opaque_min, bounds.opaque_max
        ));
    }

    Ok(())
}

fn validate_rgba_icon(
    bytes: &[u8],
    label: &str,
    expected_size: u32,
    bounds: AlphaCoverageBounds,
) -> Result<DecodedPng, String> {
    let decoded = decode_png(bytes, label);
    validate_decoded_rgba_icon(&decoded, label, expected_size, bounds)?;
    Ok(decoded)
}

fn assert_rgba_icon(
    bytes: &[u8],
    label: &str,
    expected_size: u32,
    bounds: AlphaCoverageBounds,
) -> DecodedPng {
    validate_rgba_icon(bytes, label, expected_size, bounds)
        .unwrap_or_else(|error| panic!("{error}"))
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
    assert_rgba_icon(MASTER_PNG, "master icon", 1024, MASTER_ALPHA_BOUNDS);
}

#[test]
fn linux_hicolor_icons_are_square_rgba_pngs_at_required_sizes() {
    for (bytes, size) in [(HICOLOR_48, 48), (HICOLOR_128, 128), (HICOLOR_256, 256)] {
        let label = format!("Linux hicolor {size}x{size} icon");
        assert_rgba_icon(bytes, &label, size, DERIVED_ALPHA_BOUNDS);
    }
}

fn validate_windows_icon(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 6 {
        return Err("ICO header must be present".to_string());
    }
    if &bytes[..4] != b"\0\0\x01\0" {
        return Err("ICO magic must be 00 00 01 00".to_string());
    }

    let entry_count = little_endian_u16(bytes, 4) as usize;
    if entry_count != 4 {
        return Err(format!(
            "ICO must contain exactly four images, got {entry_count}"
        ));
    }
    let table_end = entry_count
        .checked_mul(16)
        .and_then(|size| size.checked_add(6))
        .ok_or_else(|| "ICO directory size must not overflow".to_string())?;
    if table_end > bytes.len() {
        return Err("ICO directory must fit inside the file".to_string());
    }

    let mut sizes = Vec::with_capacity(entry_count);
    let mut expected_payload_offset = table_end;
    for index in 0..entry_count {
        let entry = 6 + index * 16;
        let width = match bytes[entry] {
            0 => 256,
            value => u32::from(value),
        };
        let height = match bytes[entry + 1] {
            0 => 256,
            value => u32::from(value),
        };
        if width != height {
            return Err(format!("ICO entry {index} must be square"));
        }
        if bytes[entry + 2] != 0 {
            return Err(format!("ICO entry {index} color count must be zero"));
        }
        if bytes[entry + 3] != 0 {
            return Err(format!("ICO entry {index} reserved byte must be zero"));
        }
        if little_endian_u16(bytes, entry + 4) != 1 {
            return Err(format!("ICO entry {index} color planes must be one"));
        }
        if little_endian_u16(bytes, entry + 6) != 32 {
            return Err(format!("ICO entry {index} bits per pixel must be 32"));
        }

        let payload_size = little_endian_u32(bytes, entry + 8) as usize;
        let payload_offset = little_endian_u32(bytes, entry + 12) as usize;
        if payload_size <= 8 {
            return Err(format!("ICO entry {index} payload is trivial"));
        }
        if payload_offset != expected_payload_offset {
            return Err(format!(
                "ICO entry {index} payload ranges must be exactly contiguous: expected offset {expected_payload_offset}, got {payload_offset}"
            ));
        }
        let payload_end = payload_offset
            .checked_add(payload_size)
            .ok_or_else(|| format!("ICO entry {index} payload end must not overflow"))?;
        if payload_end > bytes.len() {
            return Err(format!(
                "ICO entry {index} payload must fit inside the file"
            ));
        }

        let label = format!("ICO {width}x{height} payload");
        validate_rgba_icon(
            &bytes[payload_offset..payload_end],
            &label,
            width,
            DERIVED_ALPHA_BOUNDS,
        )?;
        sizes.push(width);
        expected_payload_offset = payload_end;
    }

    sizes.sort_unstable();
    if sizes != [16, 32, 48, 256] {
        return Err(format!(
            "ICO image sizes must be 16/32/48/256, got {sizes:?}"
        ));
    }
    if expected_payload_offset != bytes.len() {
        return Err(format!(
            "ICO final payload must end at EOF {}, got {expected_payload_offset}",
            bytes.len()
        ));
    }

    Ok(())
}

#[test]
fn windows_icon_contains_valid_png_entries_at_required_sizes() {
    validate_windows_icon(WINDOWS_ICO).unwrap_or_else(|error| panic!("{error}"));
}

fn validate_macos_icon(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 8 {
        return Err("ICNS header must be present".to_string());
    }
    if &bytes[..4] != b"icns" {
        return Err("ICNS magic must be icns".to_string());
    }
    let declared_length = big_endian_u32(bytes, 4) as usize;
    if declared_length != bytes.len() {
        return Err(format!(
            "ICNS declared length {declared_length} must equal file length {}",
            bytes.len()
        ));
    }

    let mut offset = 8;
    let mut seen_representations = BTreeSet::new();
    while offset < bytes.len() {
        if offset + 8 > bytes.len() {
            return Err("ICNS chunk header must fit inside the file".to_string());
        }
        let kind: [u8; 4] = bytes[offset..offset + 4]
            .try_into()
            .expect("four-byte ICNS chunk type");
        let kind_label = String::from_utf8_lossy(&kind);
        let chunk_size = big_endian_u32(bytes, offset + 4) as usize;
        if chunk_size <= 8 {
            return Err(format!("ICNS {kind_label} chunk is trivial"));
        }
        let chunk_end = offset
            .checked_add(chunk_size)
            .ok_or_else(|| format!("ICNS {kind_label} chunk end must not overflow"))?;
        if chunk_end > bytes.len() {
            return Err(format!("ICNS {kind_label} chunk must fit inside the file"));
        }

        if let Some((_, expected_size)) = EXPECTED_ICNS_REPRESENTATIONS
            .iter()
            .find(|(expected_kind, _)| expected_kind == &kind)
        {
            if !seen_representations.insert(kind) {
                return Err(format!(
                    "ICNS {kind_label} representation must not be duplicated"
                ));
            }
            let payload = &bytes[offset + 8..chunk_end];
            if matches!(&kind, b"ic04" | b"ic05") {
                if !payload.starts_with(b"ARGB") {
                    return Err(format!(
                        "ICNS {kind_label} legacy representation must start with ARGB"
                    ));
                }
            } else {
                let label = format!("ICNS {kind_label} {expected_size}x{expected_size} payload");
                validate_rgba_icon(payload, &label, *expected_size, DERIVED_ALPHA_BOUNDS)?;
            }
        } else if kind == *b"info" {
            if !bytes[offset + 8..chunk_end].starts_with(b"bplist00") {
                return Err("ICNS info metadata must be a binary plist".to_string());
            }
        } else {
            return Err(format!(
                "ICNS chunk type {kind_label} is not emitted by the canonical generator"
            ));
        }

        offset = chunk_end;
    }

    for (kind, _) in EXPECTED_ICNS_REPRESENTATIONS {
        if !seen_representations.contains(&kind) {
            return Err(format!(
                "ICNS is missing required {} representation",
                String::from_utf8_lossy(&kind)
            ));
        }
    }

    Ok(())
}

#[test]
fn macos_icon_contains_all_canonical_png_representations() {
    validate_macos_icon(MACOS_ICNS).unwrap_or_else(|error| panic!("{error}"));
}

#[test]
fn macos_tray_icon_uses_three_tone_rendering() {
    const TRAY_SOURCE: &str = include_str!("../src/tray/desktop.rs");

    assert!(
        TRAY_SOURCE.contains(".with_icon_as_template(false)"),
        "the macOS tray icon must preserve white, gray, and black tones"
    );
    assert!(
        TRAY_SOURCE.contains("macos_monochrome_rgba(&mut rgba);"),
        "the macOS tray icon must convert the full mascot to three tones"
    );
}

fn icns_without_chunk(bytes: &[u8], removed_kind: &[u8; 4]) -> Vec<u8> {
    let mut rebuilt = bytes[..8].to_vec();
    let mut offset = 8;
    while offset < bytes.len() {
        let chunk_size = big_endian_u32(bytes, offset + 4) as usize;
        let chunk_end = offset + chunk_size;
        if &bytes[offset..offset + 4] != removed_kind {
            rebuilt.extend_from_slice(&bytes[offset..chunk_end]);
        }
        offset = chunk_end;
    }
    let rebuilt_size = u32::try_from(rebuilt.len()).expect("ICNS fixture must fit in u32");
    rebuilt[4..8].copy_from_slice(&rebuilt_size.to_be_bytes());
    rebuilt
}

#[test]
fn semantic_icon_validator_rejects_a_fully_transparent_fixture() {
    let mut decoded = decode_png(MASTER_PNG, "transparent fixture");
    for pixel in decoded.pixels.chunks_exact_mut(4) {
        pixel[3] = 0;
    }

    let error =
        validate_decoded_rgba_icon(&decoded, "transparent fixture", 1024, MASTER_ALPHA_BOUNDS)
            .expect_err("a fully transparent icon must be rejected");
    assert!(error.contains("visible alpha coverage"), "{error}");
}

#[test]
fn semantic_icon_validator_rejects_an_opaque_rgb_fixture() {
    let mut decoded = decode_png(MASTER_PNG, "opaque RGB fixture");
    decoded.source_color_type = png::ColorType::Rgb;
    for pixel in decoded.pixels.chunks_exact_mut(4) {
        pixel[3] = 255;
    }

    let error =
        validate_decoded_rgba_icon(&decoded, "opaque RGB fixture", 1024, MASTER_ALPHA_BOUNDS)
            .expect_err("an opaque RGB icon must be rejected");
    assert!(error.contains("RGBA"), "{error}");
}

#[test]
fn windows_icon_validator_rejects_a_payload_gap() {
    let mut corrupted = WINDOWS_ICO.to_vec();
    let table_end = 6 + little_endian_u16(&corrupted, 4) as usize * 16;
    corrupted[18..22].copy_from_slice(&(table_end as u32 + 1).to_le_bytes());

    let error = validate_windows_icon(&corrupted).expect_err("ICO payload gaps must be rejected");
    assert!(error.contains("contiguous"), "{error}");
}

#[test]
fn macos_icon_validator_rejects_a_missing_representation() {
    let incomplete = icns_without_chunk(MACOS_ICNS, b"ic10");

    let error = validate_macos_icon(&incomplete)
        .expect_err("an ICNS missing its 1024 representation must be rejected");
    assert!(error.contains("ic10"), "{error}");
}
