//! An ICC profile's description, what an `iCCP` chunk is named by
//! ([`name`]).

/// What an `iCCP` chunk holding `profile` is named: the profile's own
/// description (its `desc` tag, version 2's ASCII or version 4's first
/// localized text) in the Latin-1 a PNG allows, at most 79 characters;
/// "ICC profile" when it has none.
pub(crate) fn name(profile: &[u8]) -> String {
    let named: String = description(profile)
        .unwrap_or_default()
        .chars()
        .filter(|&c| matches!(c, ' '..='~' | '\u{a1}'..='\u{ff}'))
        .collect();
    let words: Vec<&str> = named.split(' ').filter(|w| !w.is_empty()).collect();
    let mut name = words.join(" ");
    if let Some((at, _)) = name.char_indices().nth(79) {
        name.truncate(at);
        name.truncate(name.trim_end().len());
    }
    match name.is_empty() {
        true => "ICC profile".into(),
        false => name,
    }
}

/// The `desc` tag's text.
fn description(profile: &[u8]) -> Option<String> {
    let u32_at = |at: usize| {
        Some(u32::from_be_bytes(
            profile.get(at..at + 4)?.try_into().ok()?,
        ))
    };
    let count = u32_at(128)? as usize;
    let entry = (0..count.min(1024))
        .map(|k| 132 + k * 12)
        .find(|&at| profile.get(at..at + 4) == Some(b"desc"))?;
    let (start, size) = (u32_at(entry + 4)? as usize, u32_at(entry + 8)? as usize);
    let tag = profile.get(start..start.checked_add(size)?)?;
    let tag_u32 = |at: usize| Some(u32::from_be_bytes(tag.get(at..at + 4)?.try_into().ok()?));
    match tag.get(..4)? {
        b"desc" => {
            let length = tag_u32(8)? as usize;
            let text = tag.get(12..12 + length)?;
            let text = text.split(|&b| b == 0).next()?;
            Some(text.iter().map(|&b| char::from(b)).collect())
        }
        b"mluc" => {
            let (records, length, offset) =
                (tag_u32(8)?, tag_u32(20)? as usize, tag_u32(24)? as usize);
            (records > 0).then_some(())?;
            let units = tag.get(offset..offset + length)?;
            let units = units
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_be_bytes(*b));
            Some(
                char::decode_utf16(units)
                    .map(|c| c.unwrap_or('?'))
                    .collect(),
            )
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A profile of no tags but `desc`, holding `tag`.
    fn profile(tag: &[u8]) -> Vec<u8> {
        let mut profile = vec![0u8; 128];
        profile.extend(1u32.to_be_bytes());
        profile.extend(b"desc");
        profile.extend(144u32.to_be_bytes());
        profile.extend((tag.len() as u32).to_be_bytes());
        profile.extend(tag);
        profile
    }

    #[test]
    fn a_profile_is_named_by_its_description_of_either_version() {
        let units: Vec<u8> = "Display P3"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect();
        let mut mluc = b"mluc\0\0\0\0".to_vec();
        mluc.extend(1u32.to_be_bytes());
        mluc.extend(12u32.to_be_bytes());
        mluc.extend(b"enUS");
        mluc.extend((units.len() as u32).to_be_bytes());
        mluc.extend(28u32.to_be_bytes());
        mluc.extend(units);
        assert_eq!(name(&profile(&mluc)), "Display P3");
        let mut desc = b"desc\0\0\0\0".to_vec();
        desc.extend(12u32.to_be_bytes());
        desc.extend(b"sRGB  v2 \0\0\0");
        assert_eq!(name(&profile(&desc)), "sRGB v2");
    }

    #[test]
    fn a_profile_without_a_readable_description_is_an_icc_profile() {
        assert_eq!(name(&[0; 10]), "ICC profile");
        assert_eq!(name(&profile(b"mluc\0\0\0\0\0\0\0\0")), "ICC profile");
        let long = "x".repeat(100);
        let mut desc = b"desc\0\0\0\0".to_vec();
        desc.extend(100u32.to_be_bytes());
        desc.extend(long.as_bytes());
        assert_eq!(name(&profile(&desc)).len(), 79);
    }
}
