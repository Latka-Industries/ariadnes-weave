//! Shared helpers for examples that write PDFs under `tmp/`.

pub fn write_pdf(name: &str, bytes: &[u8]) {
    let path = format!("tmp/{name}");
    std::fs::create_dir_all("tmp").ok();
    std::fs::write(&path, bytes).unwrap();
    println!("wrote {path} ({} bytes)", bytes.len());
}
