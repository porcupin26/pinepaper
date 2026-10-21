use crate::{
    model::{Document, Page, Payload, Store, PAGE_H, PAGE_W},
    render::{Box2, Surface, View},
};
use anyhow::{bail, Context, Result};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

pub fn pdf_count(path: &Path) -> Result<usize> {
    let result = Command::new("pdfinfo")
        .env("LC_ALL", "C")
        .arg(path)
        .output()
        .context("Install Poppler utilities (pdfinfo and pdftoppm) to read PDFs")?;
    if !result.status.success() {
        bail!(
            "PDF could not be opened: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let text = String::from_utf8_lossy(&result.stdout);
    let count = text
        .lines()
        .find_map(|s| s.strip_prefix("Pages:"))
        .context("PDF page count missing")?
        .trim()
        .parse::<usize>()?;
    if count == 0 || count > 10000 {
        bail!("PDF must contain between 1 and 10,000 pages");
    }
    Ok(count)
}
pub fn pdf_page(path: &Path, index: usize) -> Result<image::RgbImage> {
    let tmp = tempfile::tempdir()?;
    let prefix = tmp.path().join("page");
    let result = Command::new("pdftoppm")
        .args([
            "-f",
            &(index + 1).to_string(),
            "-l",
            &(index + 1).to_string(),
            "-singlefile",
            "-scale-to",
            "2080",
            "-png",
        ])
        .arg(path)
        .arg(&prefix)
        .output()
        .context("Install Poppler utilities to render PDFs")?;
    if !result.status.success() {
        bail!(
            "PDF rendering failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    Ok(image::open(prefix.with_extension("png"))?.to_rgb8())
}
pub fn paginate(body: &str, title: &str) -> Vec<Payload> {
    // Fixed typography/geometry; rotation never changes page boundaries.
    let font = fontdue::Font::from_bytes(
        include_bytes!("../assets/DejaVuSerif.ttf") as &[u8],
        fontdue::FontSettings::default(),
    )
    .unwrap();
    let mut pages = vec![];
    let mut current = String::new();
    let mut y = 0.;
    let mut line_w = 0.;
    let mut lines = Vec::new();
    let mut line = String::new();
    for paragraph in body.split('\n') {
        for word in paragraph.split_whitespace() {
            let w = word
                .chars()
                .map(|c| font.metrics(c, 28.).advance_width as f64)
                .sum::<f64>();
            let space = font.metrics(' ', 28.).advance_width as f64;
            if !line.is_empty() && line_w + space + w > 624. {
                lines.push(std::mem::take(&mut line));
                line_w = 0.;
            }
            // Extremely long words are split at character boundaries.
            if w > 624. {
                for c in word.chars() {
                    let cw = font.metrics(c, 28.).advance_width as f64;
                    if line_w + cw > 624. {
                        lines.push(std::mem::take(&mut line));
                        line_w = 0.;
                    }
                    line.push(c);
                    line_w += cw;
                }
            } else {
                if !line.is_empty() {
                    line.push(' ');
                    line_w += space;
                }
                line.push_str(word);
                line_w += w;
            }
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            line_w = 0.;
        }
        lines.push(String::new());
    }
    for line in lines {
        if y + 42. > 800. && !current.trim().is_empty() {
            let mut p = Payload::blank("Blank");
            p.body = Some(current.trim_end().into());
            p.heading = Some(title.into());
            pages.push(p);
            current.clear();
            y = 0.;
        }
        if !line.is_empty() || !current.is_empty() {
            current.push_str(&line);
            current.push('\n');
            y += 42.;
        }
    }
    if !current.trim().is_empty() {
        let mut p = Payload::blank("Blank");
        p.body = Some(current.trim_end().into());
        p.heading = Some(title.into());
        pages.push(p);
    }
    pages
}
fn zip_text<R: Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>, name: &str) -> Result<String> {
    let mut file = zip.by_name(name)?;
    if file.size() > 16 * 1024 * 1024 {
        bail!("EPUB section exceeds 16 MB");
    }
    let mut out = String::new();
    file.read_to_string(&mut out)?;
    Ok(out)
}
fn zip_path(base: &Path, href: &str) -> Result<String> {
    let mut parts = Vec::new();
    let input = href.split('#').next().unwrap_or(href);
    let mut decoded = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&input[i + 1..i + 3], 16) {
                decoded.push(v);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    let decoded = String::from_utf8(decoded)?;
    if decoded.starts_with('/') {
        bail!("Invalid absolute EPUB path");
    }
    for p in base.join(decoded).components() {
        match p {
            std::path::Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            std::path::Component::ParentDir => {
                if parts.pop().is_none() {
                    bail!("Invalid EPUB path");
                }
            }
            std::path::Component::CurDir => (),
            _ => bail!("Invalid EPUB path"),
        }
    }
    Ok(parts.join("/"))
}
pub fn epub(path: &Path) -> Result<(String, Vec<Payload>)> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
    let mut total = 0u64;
    for i in 0..zip.len() {
        total = total
            .checked_add(zip.by_index(i)?.size())
            .context("EPUB size overflow")?;
    }
    if total > 200 * 1024 * 1024 {
        bail!("EPUB exceeds the 200 MB unpacked limit");
    }
    let container = zip_text(&mut zip, "META-INF/container.xml")?;
    let xml = roxmltree::Document::parse(&container)?;
    let opf = xml
        .descendants()
        .find(|n| n.has_tag_name("rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .context("EPUB has no manifest")?
        .to_string();
    let raw = zip_text(&mut zip, &opf)?;
    let xml = roxmltree::Document::parse(&raw)?;
    let title = xml
        .descendants()
        .find(|n| n.has_tag_name("title"))
        .and_then(|n| n.text())
        .unwrap_or("Book")
        .to_string();
    let manifest: std::collections::HashMap<_, _> = xml
        .descendants()
        .filter(|n| n.has_tag_name("item"))
        .filter_map(|n| {
            Some((
                n.attribute("id")?,
                (n.attribute("href")?, n.attribute("media-type")?),
            ))
        })
        .collect();
    let mut pages = vec![];
    let mut text_size = 0;
    for n in xml.descendants().filter(|n| n.has_tag_name("itemref")) {
        if n.attribute("linear") == Some("no") {
            continue;
        }
        if let Some(&(href, kind)) = n.attribute("idref").and_then(|id| manifest.get(id)) {
            if !["application/xhtml+xml", "text/html"].contains(&kind) {
                continue;
            }
            let name = zip_path(Path::new(&opf).parent().unwrap_or(Path::new("")), href)?;
            let text = zip_text(&mut zip, &name)?;
            text_size += text.len();
            if text_size > 16 * 1024 * 1024 {
                bail!("EPUB text exceeds 16 MB");
            }
            let body = html2text::from_read(text.as_bytes(), 1000)?;
            pages.extend(paginate(&body, &title));
        }
    }
    if pages.is_empty() {
        bail!("No readable text; encrypted or image-only EPUBs are unsupported");
    }
    Ok((title, pages))
}
pub fn import(store: &mut Store, path: &Path, parent: Option<&str>) -> Result<String> {
    let path = path.canonicalize().context("File was not found")?;
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let title = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let (title, kind, pages) = match ext.as_str() {
        "pdf" => {
            let n = pdf_count(&path)?;
            (
                title,
                "pdf",
                (0..n).map(|_| Payload::blank("Blank")).collect(),
            )
        }
        "epub" => {
            let (t, p) = epub(&path)?;
            (t, "book", p)
        }
        "txt" | "md" => {
            if fs::metadata(&path)?.len() > 16 * 1024 * 1024 {
                bail!("Text file exceeds 16 MB");
            }
            let p = paginate(&fs::read_to_string(&path)?, &title);
            if p.is_empty() {
                bail!("Text file is empty");
            }
            (title, "book", p)
        }
        _ => bail!("Choose a PDF, EPUB or UTF-8 text file"),
    };
    let source = store.copy_source(&path)?;
    match store.create(&title, kind, parent, pages, Some(&source)) {
        Ok(id) => Ok(id),
        Err(e) => {
            let _ = fs::remove_file(store.root.join(source));
            Err(e)
        }
    }
}
pub fn export_pdf(destination: &Path, pages: &[Page], source: Option<&Path>) -> Result<()> {
    use flate2::{write::ZlibEncoder, Compression};
    let mut objects: Vec<Vec<u8>> = vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(), vec![]];
    let mut kids = vec![];
    let mut surface = Surface::new();
    surface.resize(760, 1040, 2);
    for (index, p) in pages.iter().enumerate() {
        let bg = source.map(|s| pdf_page(s, index)).transpose()?;
        surface.page(
            View {
                x: 0.,
                y: 0.,
                zoom: 1.,
                clip: Box2::new(0., 0., PAGE_W, PAGE_H),
            },
            &p.payload,
            bg.as_ref(),
            true,
        );
        let id = objects.len() + 1;
        let image_id = id + 2;
        kids.push(format!("{id} 0 R"));
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 760 1040] /Resources << /XObject << /Im0 {image_id} 0 R >> >> /Contents {} 0 R >>",id+1).into_bytes());
        let data = b"q 760 0 0 1040 0 0 cm /Im0 Do Q";
        objects.push(stream("", data));
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        for c in &surface.pixels {
            z.write_all(&[
                ((c >> 16) & 255) as u8,
                ((c >> 8) & 255) as u8,
                (c & 255) as u8,
            ])?;
        }
        objects.push(stream("/Type /XObject /Subtype /Image /Width 1520 /Height 2080 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode",&z.finish()?));
    }
    objects[1] = format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        pages.len()
    )
    .into_bytes();
    let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = vec![0];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        writeln!(out, "{} 0 obj", i + 1)?;
        out.extend(obj);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    write!(out, "xref\n0 {}\n0000000000 65535 f \n", offsets.len())?;
    for o in offsets.iter().skip(1) {
        writeln!(out, "{o:010} 00000 n ")?;
    }
    write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        offsets.len(),
        xref
    )?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(&out)?;
    tmp.as_file().sync_all()?;
    tmp.persist(destination)?;
    Ok(())
}
fn stream(attrs: &str, data: &[u8]) -> Vec<u8> {
    let mut out = format!("<< {attrs} /Length {} >>\nstream\n", data.len()).into_bytes();
    out.extend(data);
    out.extend(b"\nendstream");
    out
}
pub fn pdf_source(store: &Store, doc: &Document) -> Result<Option<PathBuf>> {
    if doc.kind == "pdf" {
        Ok(Some(store.source_path(doc)?))
    } else {
        Ok(None)
    }
}
