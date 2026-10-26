//! Original implementation of the standard MD5 digest used only for the OEM
//! export checksum. This checksum is neither a signature nor device identity.
use serde_json::Value;

pub(super) fn profile_hash(value: &Value) -> Result<String, String> {
    fn stable(value: &Value, out: &mut String) {
        match value {
            Value::Object(map) => {
                out.push('{');
                let mut keys = map.keys().collect::<Vec<_>>();
                keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
                for (i, key) in keys.into_iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::to_string(key).unwrap());
                    out.push(':');
                    stable(&map[key], out);
                }
                out.push('}');
            }
            Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    stable(item, out);
                }
                out.push(']');
            }
            _ => out.push_str(&serde_json::to_string(value).unwrap()),
        }
    }
    let mut value = value.clone();
    let object = value
        .as_object_mut()
        .ok_or("profile payload must be an object")?;
    object.remove("hash");
    object.remove("gamemode");
    let mut canonical = String::new();
    stable(&value, &mut canonical);
    Ok(md5(canonical.as_bytes()))
}

fn md5(input: &[u8]) -> String {
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];
    const S: [u32; 16] = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&(input.len() as u64).wrapping_mul(8).to_le_bytes());
    let mut state = [0x67452301u32, 0xefcdab89, 0x98badcfe, 0x10325476];
    for chunk in padded.chunks_exact(64) {
        let mut words = [0u32; 16];
        for (word, bytes) in words.iter_mut().zip(chunk.chunks_exact(4)) {
            *word = u32::from_le_bytes(bytes.try_into().unwrap());
        }
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (f, index) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let next = b.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(K[i])
                    .wrapping_add(words[index])
                    .rotate_left(S[(i / 16) * 4 + i % 4]),
            );
            a = d;
            d = c;
            c = b;
            b = next;
        }
        for (word, add) in state.iter_mut().zip([a, b, c, d]) {
            *word = word.wrapping_add(add);
        }
    }
    state
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .map(|v| format!("{v:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_digest_vectors_and_stable_oem_payload_checksum() {
        assert_eq!(md5(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(md5(b"message digest"), "f96b697d7cb7938d525a2f31aaf161d0");
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("vendor/sidearm-synapse-parser/tests/fixtures/Sidearm_profile.synapse4");
        let raw = sidearm_synapse_parser::raw_source::read(
            &file,
            sidearm_synapse_parser::types::SourceKind::SynapseV4,
        )
        .unwrap();
        let sidearm_synapse_parser::raw_source::RawSource::V4 { profiles, .. } = raw else {
            panic!()
        };
        for env in profiles {
            assert_eq!(
                profile_hash(&env.decoded.unwrap().value).unwrap(),
                env.envelope["hash"].as_str().unwrap()
            );
        }
    }
}
