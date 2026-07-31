use std::borrow::Cow;

pub(crate) fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

pub(crate) fn decode_hex(value: &str) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return None;
    }

    bytes
        .chunks_exact(2)
        .map(|pair| Some((hex_value(pair[0])? << 4) | hex_value(pair[1])?))
        .collect()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn strip_windows_extended_path(path: &str) -> Cow<'_, str> {
    const UNC_PREFIX: &str = "\\\\?\\UNC\\";
    const PATH_PREFIX: &str = "\\\\?\\";

    if let Some(stripped) = path.strip_prefix(UNC_PREFIX) {
        Cow::Owned(format!("\\\\{stripped}"))
    } else {
        path.strip_prefix(PATH_PREFIX)
            .map_or_else(|| Cow::Borrowed(path), Cow::Borrowed)
    }
}

pub(crate) fn is_image_media_type(media_type: &str) -> bool {
    media_type.starts_with("image/")
}

pub(crate) fn is_video_media_type(media_type: &str) -> bool {
    media_type.starts_with("video/")
}
