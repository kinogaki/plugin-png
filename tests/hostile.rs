//! Hostile PNGs, each made here and read as File ▸ Import reads one (on its
//! 2 MiB stack, within a small budget): a clean error or warning, quickly.
use kinogaki_png_format::Png;
use kinogaki_studio_api::format::{ReadBudget, ReadLimits};
use kinogaki_studio_api::geometry::image::ColorTag;
use kinogaki_studio_api::{
    CancelToken, Error, FileFormat, Imported, Input, Purpose, Reading, Units, standard,
};
use kinogaki_studio_api_testing::{Unconverted, read_hostile};
use std::time::Duration;

/// What a read of a hostile file may take, in a debug build.
const QUICK: Duration = Duration::from_secs(5);

/// A budget of 64 MiB in all.
fn small() -> ReadLimits {
    ReadLimits {
        allocation: 64 << 20,
        total: 64 << 20,
        ..ReadLimits::default()
    }
}

fn read(bytes: &[u8], limits: ReadLimits) -> Result<Imported, Error> {
    let input = Input::memory("hostile.png", bytes);
    let (read, took) = read_hostile(&Png, &input, &Png.options(), limits);
    assert!(took < QUICK, "{took:?}");
    read
}

/// CRC-32 (ISO-HDLC), a chunk's check.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

/// `data` as a zlib stream of stored blocks.
fn stored(data: &[u8]) -> Vec<u8> {
    let mut stream = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = data.chunks(65_535).collect();
    for (k, block) in blocks.iter().enumerate() {
        let len = block.len() as u16;
        stream.push(u8::from(k + 1 == blocks.len()));
        stream.extend(len.to_le_bytes());
        stream.extend((!len).to_le_bytes());
        stream.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    stream.extend(((b << 16) | a).to_be_bytes());
    stream
}

/// A PNG of `width` × `height` pixels of color type `color` (8 bits), its
/// chunks `before` the image data, then `idat`.
fn png(width: u32, height: u32, color: u8, before: &[(&[u8; 4], Vec<u8>)], idat: &[u8]) -> Vec<u8> {
    let mut file = kinogaki_png::SIGNATURE.to_vec();
    let mut chunk = |kind: &[u8; 4], body: &[u8]| {
        file.extend_from_slice(&(body.len() as u32).to_be_bytes());
        file.extend_from_slice(kind);
        file.extend_from_slice(body);
        file.extend_from_slice(&crc32(&[&kind[..], body].concat()).to_be_bytes());
    };
    let mut header = [width.to_be_bytes(), height.to_be_bytes()].concat();
    header.extend_from_slice(&[8, color, 0, 0, 0]);
    chunk(b"IHDR", &header);
    for (kind, body) in before {
        chunk(kind, body);
    }
    chunk(b"IDAT", idat);
    chunk(b"IEND", &[]);
    file
}

/// An RGBA picture of zeros, `side` pixels square, encoded.
fn zeros(side: u32) -> Vec<u8> {
    let image = kinogaki_png::Image::rgba8(side, side, vec![0; (side * side * 4) as usize]);
    kinogaki_png::encode(&image, kinogaki_png::Level::Fast).unwrap()
}

/// A zlib stream cut after its one dynamic block's header, whose codes
/// are literal 0 (`0`) and the end (`1`): past its end a decoder's zero bits
/// read as literals, which once decoded on to whatever size was claimed.
fn cut_stream() -> Vec<u8> {
    let mut bits: Vec<bool> = Vec::new();
    let mut put = |value: u32, count: u32| bits.extend((0..count).map(|k| value >> k & 1 == 1));
    put(1, 1); // the last block
    put(2, 2); // dynamic
    put(0, 5); // 257 literal/length codes
    put(0, 5); // one distance code
    put(14, 4); // 18 code-length codes, in RFC 1951's order
    for symbol in [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1] {
        put(u32::from(matches!(symbol, 1 | 18)), 3);
    }
    // Code-length codes: 1 is `0`, 18 (a run of zeros) is `1`.
    put(0, 1); // literal 0: one bit
    put(1, 1);
    put(127, 7); // 138 zeros
    put(1, 1);
    put(106, 7); // 117 zeros
    put(0, 1); // the end: one bit
    put(0, 1); // the distance code: one bit
    let mut stream = vec![0x78, 0x01];
    stream.extend(
        bits.chunks(8).map(|byte| {
            (byte.iter().enumerate()).fold(0u8, |b, (k, &bit)| b | (u8::from(bit) << k))
        }),
    );
    stream
}

#[test]
fn a_cut_stream_claiming_a_gigabyte_is_refused_before_inflating() {
    // 16384² RGBA from 30 bytes: once 4.3 GB of zeros.
    let file = png(16384, 16384, 6, &[], &cut_stream());
    let why = read(&file, ReadLimits::default()).unwrap_err();
    assert!(why.message().contains("over 1032 times its size"), "{why}");
}

#[test]
fn a_cut_stream_within_its_ratio_fails_at_its_end() {
    let file = png(32, 32, 6, &[], &cut_stream());
    let why = read(&file, small()).unwrap_err();
    assert!(why.message().contains("ends early"), "{why}");
}

#[test]
fn a_picture_past_the_pixel_limit_is_refused_before_decoding() {
    let rows = stored(&[0; 2]);
    for (width, height) in [(1 << 28, 1), (1 << 15, 1 << 15), (0x7FFF_FFFF, 0x7FFF_FFFF)] {
        let why = read(&png(width, height, 0, &[], &rows), ReadLimits::default()).unwrap_err();
        assert!(why.message().contains("hostile.png"), "{why}");
    }
    // The limit is the read's budget's.
    let fewer = ReadLimits {
        pixels: 64 * 64 - 1,
        ..ReadLimits::default()
    };
    let why = read(&png(64, 64, 0, &[], &rows), fewer).unwrap_err();
    assert!(
        why.message()
            .contains("64 × 64 pixels, over the read's limit"),
        "{why}"
    );
}

#[test]
fn a_picture_past_the_budget_is_refused_before_decoding() {
    // 4096² RGBA of zeros: 64 MiB inflated from 65 KB.
    let file = zeros(4096);
    let why = read(&file, small()).unwrap_err();
    assert!(why.message().contains("over the read's budget"), "{why}");
    assert!(read(&file, ReadLimits::default()).is_ok());
}

#[test]
fn a_profile_that_doesnt_inflate_leaves_the_colors_unknown() {
    // A 64 MiB profile claimed by a cut stream: once kept as 64 MiB of zeros.
    let mut iccp = b"hostile\0\0".to_vec();
    iccp.extend(cut_stream());
    let file = png(1, 1, 0, &[(b"iCCP", iccp)], &stored(&[0; 2]));
    let read = read(&file, small()).unwrap();
    assert!(
        read.warnings[0]
            .message
            .contains("color profile can't be read")
    );
    let image = standard::image()
        .get(read.items[0].data.as_ref().unwrap())
        .unwrap();
    let tag = image.layers()[0].format().encoding.clone();
    assert!(
        matches!(
            tag,
            kinogaki_studio_api::geometry::image::Encoding::Color {
                tag: ColorTag::Unknown { .. },
                ..
            }
        ),
        "{tag:?}"
    );
}

#[test]
fn a_cancelled_read_stops() {
    let file = zeros(1024);
    let token = CancelToken::default();
    token.cancel();
    let reading = Reading {
        budget: ReadBudget::default().cancelled_by(token),
        ..Reading::new(Purpose::Value, Units::default(), &Unconverted)
    };
    let read = Png.read(&Input::memory("a.png", &file), &Png.options(), &reading);
    assert_eq!(read.unwrap_err(), Error::cancelled());
}
