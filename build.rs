//! Puts the program icon, assets/limitpeeker.ico, into each EXE as icon
//! group 1, which File Explorer and Task Manager show, and a version
//! resource from Cargo.toml's version, which the file's Details tab and Task
//! Manager's name for it show. The resource files are written here, so no
//! resource compiler is needed: the build script runs cargo outside the
//! Visual Studio shell.

use std::path::PathBuf;

const ICON: &str = "assets/limitpeeker.ico";
const LICENSE: &str = "LICENSE";
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const RT_VERSION: u16 = 16;
/// U.S. English, the language of the version's strings.
const ENGLISH: u16 = 0x0409;
/// Each EXE and the description Task Manager shows for it.
const PROGRAMS: [(&str, &str); 2] = [
    ("limitpeeker", "LimitPeeker"),
    ("uninstall", "LimitPeeker uninstaller"),
];

fn main() {
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-changed={LICENSE}");
    println!("cargo:rerun-if-changed=build.rs");
    let ico = std::fs::read(ICON).ok();
    let copyright = std::fs::read_to_string(LICENSE)
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("Copyright"))
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    for (program, description) in PROGRAMS {
        let mut res = Vec::new();
        // Every resource file starts with an empty entry.
        entry(&mut res, 0, 0, 0, 0, &[]);
        if let Some(ico) = &ico {
            icons(&mut res, ico);
        }
        let version = version_info(program, description, &copyright);
        entry(&mut res, RT_VERSION, 1, 0x0030, ENGLISH, &version);
        let path = out.join(format!("{program}.res"));
        std::fs::write(&path, res).expect("the resource file");
        // The linker takes a resource file as it is.
        println!("cargo:rustc-link-arg-bin={program}={}", path.display());
    }
}

/// Each image of `ico` as an icon, numbered from 1, and all of them as icon
/// group 1.
fn icons(out: &mut Vec<u8>, ico: &[u8]) {
    let u16_at = |at: usize| u16::from_le_bytes([ico[at], ico[at + 1]]);
    let u32_at = |at: usize| u32::from_le_bytes([ico[at], ico[at + 1], ico[at + 2], ico[at + 3]]);
    assert_eq!((u16_at(0), u16_at(2)), (0, 1), "{ICON} is not an icon file");
    let count = u16_at(4);
    let mut group = vec![0, 0, 1, 0];
    group.extend(count.to_le_bytes());
    for index in 0..count {
        let at = 6 + 16 * usize::from(index);
        let (length, offset) = (u32_at(at + 8) as usize, u32_at(at + 12) as usize);
        let id = index + 1;
        // Moveable and discardable, as a resource compiler marks an icon.
        entry(out, RT_ICON, id, 0x1010, 0, &ico[offset..offset + length]);
        // The file's entry up to its length, then the icon's number.
        group.extend(&ico[at..at + 12]);
        group.extend(id.to_le_bytes());
    }
    entry(out, RT_GROUP_ICON, 1, 0x1030, 0, &group);
}

/// The version resource (VS_VERSIONINFO) for one EXE: the fixed part with
/// the numbers, then the strings in U.S. English and Unicode.
fn version_info(program: &str, description: &str, copyright: &str) -> Vec<u8> {
    let part = |name: &str| -> u16 {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    let (major, minor, patch) = (
        part("CARGO_PKG_VERSION_MAJOR"),
        part("CARGO_PKG_VERSION_MINOR"),
        part("CARGO_PKG_VERSION_PATCH"),
    );
    let text = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let high = (u32::from(major) << 16) | u32::from(minor);
    let low = u32::from(patch) << 16;
    let mut fixed = Vec::new();
    for word in [
        0xFEEF_04BD, // the signature
        0x0001_0000, // the structure's version
        high,
        low, // the file's version
        high,
        low,         // the product's version
        0x3F,        // which flags count
        0,           // no flags: not a debug or private build
        0x0004_0004, // for 32-bit Windows NT
        1,           // an application
        0,
        0,
        0, // no subtype and no date
    ] {
        fixed.extend(u32::to_le_bytes(word));
    }
    let file = format!("{program}.exe");
    let strings: Vec<Vec<u8>> = [
        ("FileDescription", description),
        ("FileVersion", text.as_str()),
        ("InternalName", program),
        ("LegalCopyright", copyright),
        ("OriginalFilename", file.as_str()),
        ("ProductName", "LimitPeeker"),
        ("ProductVersion", text.as_str()),
    ]
    .into_iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(key, value)| node(key, Value::Text(value), &[]))
    .collect();
    let table = node("040904B0", Value::None, &strings);
    let string_info = node("StringFileInfo", Value::None, &[table]);
    // The language and the code page (1200, Unicode) of the strings.
    let translation = node("Translation", Value::Binary(&[0x09, 0x04, 0xB0, 0x04]), &[]);
    let var_info = node("VarFileInfo", Value::None, &[translation]);
    node(
        "VS_VERSION_INFO",
        Value::Binary(&fixed),
        &[string_info, var_info],
    )
}

/// What a node of the version resource holds.
enum Value<'a> {
    None,
    Text(&'a str),
    Binary(&'a [u8]),
}

/// One node of the version resource: its length, its value's length (in
/// characters for text, bytes otherwise), whether the value is text, its
/// key, the value and the children, each starting on four bytes. The length
/// leaves out the padding after the node.
fn node(key: &str, value: Value, children: &[Vec<u8>]) -> Vec<u8> {
    let utf16 = |text: &str| -> Vec<u8> {
        text.encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    };
    let (bytes, length, kind) = match value {
        Value::None => (Vec::new(), 0, 1),
        Value::Text(text) => (utf16(text), text.encode_utf16().count() + 1, 1),
        Value::Binary(data) => (data.to_vec(), data.len(), 0),
    };
    let mut out = vec![0, 0];
    out.extend((length as u16).to_le_bytes());
    out.extend((kind as u16).to_le_bytes());
    out.extend(utf16(key));
    pad(&mut out);
    out.extend(bytes);
    for child in children {
        pad(&mut out);
        out.extend(child);
    }
    let total = (out.len() as u16).to_le_bytes();
    out[..2].copy_from_slice(&total);
    out
}

/// Zeros up to the next multiple of four bytes.
fn pad(out: &mut Vec<u8>) {
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

/// One resource: a header with a numbered type and name in `language`,
/// then the data, each padded to four bytes.
fn entry(out: &mut Vec<u8>, kind: u16, name: u16, flags: u16, language: u16, data: &[u8]) {
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(32u32.to_le_bytes());
    for id in [kind, name] {
        out.extend(0xFFFFu16.to_le_bytes());
        out.extend(id.to_le_bytes());
    }
    out.extend(0u32.to_le_bytes());
    out.extend(flags.to_le_bytes());
    out.extend(language.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(data);
    pad(out);
}
