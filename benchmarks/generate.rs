//! Byte-compatible replacement for generate.mjs. No Cargo or npm dependencies.
use std::{
    env, fs,
    os::raw::{c_int, c_ulong},
    path::Path,
};
#[link(name = "z")]
extern "C" {
    fn compressBound(source_len: c_ulong) -> c_ulong;
    fn compress2(
        dest: *mut u8,
        dest_len: *mut c_ulong,
        source: *const u8,
        source_len: c_ulong,
        level: c_int,
    ) -> c_int;
}
fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc = 0xffff_ffffu32;
    for byte in tag.iter().chain(data) {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    out.extend_from_slice(&(!crc).to_be_bytes());
}
fn png(width: u32, height: u32, mut value: u32) -> Vec<u8> {
    let mut raw = vec![0; (height * (width * 3 + 1)) as usize];
    for row in raw.chunks_mut((width * 3 + 1) as usize) {
        for byte in &mut row[1..] {
            value ^= value << 13;
            value ^= value >> 17;
            value ^= value << 5;
            *byte = value as u8;
        }
    }
    // Node's deflateSync default uses zlib level -1, one zlib stream.
    let compressed = unsafe {
        let mut size = compressBound(raw.len() as c_ulong);
        let mut bytes = vec![0; size as usize];
        assert_eq!(
            compress2(
                bytes.as_mut_ptr(),
                &mut size,
                raw.as_ptr(),
                raw.len() as c_ulong,
                -1
            ),
            0
        );
        bytes.truncate(size as usize);
        bytes
    };
    let mut out = vec![137, 80, 78, 71, 13, 10, 26, 10];
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &compressed);
    chunk(&mut out, b"IEND", &[]);
    out
}
fn write(root: &Path, name: &str, text: String) {
    fs::write(root.join(format!("{name}.md")), text).unwrap();
}
fn main() {
    let arg = env::args()
        .nth(1)
        .unwrap_or_else(|| "benchmarks/generated".into());
    let root = Path::new(&arg);
    fs::create_dir_all(root).unwrap();
    for (name, size) in [
        ("small", 100 * 1024),
        ("medium", 1024 * 1024),
        ("large", 10 * 1024 * 1024),
    ] {
        let mut text = String::from("# Hashline Benchmark\n");
        let mut i = 0;
        while text.len() < size {
            text.push_str(&format!("\n## Abschnitt {i}\n\nLesbarer Text mit **Hervorhebung**, [einem Link](#abschnitt-{i}) und Unicode: Grüße 日本語. Dieser Absatz enthält eine Suchnadel und bleibt ein zusammenhängender Text.\n\n- Erster Punkt\n- Zweiter Punkt\n  - Verschachtelt\n\n| Name | Wert |\n| --- | --- |\n| Beispiel | {i} |\n\n```typescript\nconst section = {i};\nconsole.log(section);\n```\n"));
            i += 1;
        }
        write(root, name, text);
    }
    write(
        root,
        "long-line",
        format!("# Lange Zeile\n\n{}", "abcdefghij".repeat(100_000)),
    );
    write(
        root,
        "deep-list",
        format!(
            "# Tiefe Liste\n\n{}",
            (0..100)
                .map(|i| format!("{}- Ebene {i}", "  ".repeat(i)))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    );
    write(
        root,
        "wide-table",
        format!(
            "# Breite Tabelle\n\n|{}\n|{}\n{}",
            " Spalte |".repeat(100),
            " --- |".repeat(100),
            format!("|{}\n", " Inhalt |".repeat(100)).repeat(100)
        ),
    );
    write(
        root,
        "many-blocks",
        format!("# Kleine Blöcke\n\n{}", "Absatz.\n\n".repeat(20_000)),
    );
    for (name, count, width, height) in [
        ("many-images", 100, 256, 256),
        ("large-images", 4, 2048, 2048),
    ] {
        let mut lines = vec![format!("# {name}\n")];
        for i in 0..count {
            let filename = format!("{name}-{i}.png");
            fs::write(root.join(&filename), png(width, height, i + 1)).unwrap();
            lines.push(format!("## Bild {i}\n\n![Bild {i}]({filename})\n"));
        }
        write(root, name, lines.join("\n"));
    }
    write(
        root,
        "large-code",
        format!(
            "# Großer Codeblock\n\n```text\n{}```\n",
            "long code line\n".repeat(70_000)
        ),
    );
    fs::write(
        root.join("metadata.json"),
        include_bytes!("fixtures/metadata.json"),
    )
    .unwrap();
}
