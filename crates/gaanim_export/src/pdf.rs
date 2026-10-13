//! Slide decks exported to PDF, laid out from the pages [`crate::deck`]
//! plans.
//!
//! Each frame becomes a one-page PDF of its own, its pixels compressed as
//! soon as it is captured, and Typst lays out the deck around them: it embeds
//! those pages without decoding their pixels again, so a long deck does not
//! hold every frame in memory, and it sets the notes as real text.

use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};

use crate::config::ExportTelemetry;
use crate::encoder::{ExportError, Result};

/// Report that `done` of a deck's `total` frames are captured: to
/// `telemetry`, and to the editor watching an export worker.
pub fn report_frames(telemetry: &Option<ExportTelemetry>, done: u64, total: u64) {
    crate::exporter::export_progress(telemetry, done, total);
}

/// A one-page PDF of an opaque `width` × `height` RGBA frame, a point per
/// pixel, with its pixels compressed losslessly.
pub fn frame_pdf(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let rgb: Vec<u8> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|[red, green, blue, _]| [*red, *green, *blue])
        .collect();
    let pixels = miniz_oxide::deflate::compress_to_vec_zlib(&rgb, 6);
    let (catalog, tree, page, image, content) = (
        Ref::new(1),
        Ref::new(2),
        Ref::new(3),
        Ref::new(4),
        Ref::new(5),
    );
    let name = Name(b"Frame");
    let (w, h) = (width as f32, height as f32);
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).kids([page]).count(1);
    let mut page_writer = pdf.page(page);
    page_writer.media_box(Rect::new(0.0, 0.0, w, h));
    page_writer.parent(tree);
    page_writer.contents(content);
    page_writer.resources().x_objects().pair(name, image);
    page_writer.finish();
    let mut image_writer = pdf.image_xobject(image, &pixels);
    image_writer.filter(Filter::FlateDecode);
    image_writer.width(width as i32);
    image_writer.height(height as i32);
    image_writer.color_space().device_rgb();
    image_writer.bits_per_component(8);
    image_writer.finish();
    let mut draw = Content::new();
    draw.save_state();
    draw.transform([w, 0.0, 0.0, h, 0.0, 0.0]);
    draw.x_object(name);
    draw.restore_state();
    pdf.stream(content, &draw.finish());
    pdf.finish()
}

/// A page of a deck, ready to lay out.
#[derive(Debug, Clone, PartialEq)]
pub enum PdfPage {
    /// A frame, as a one-page PDF from [`frame_pdf`].
    Frame(Vec<u8>),
    /// A slide's name over its speaker notes.
    Notes { title: String, notes: String },
}

/// Width of a deck's pages in points, that of a widescreen slide in
/// presentation software.
const PAGE_WIDTH: f64 = 960.0;

/// Lay out `pages` as one PDF titled `title`, its pages in the aspect of a
/// `width` × `height` frame: a frame fills its page, and a notes page sets
/// the slide's name over its notes, a line for each line of them.
pub fn deck_pdf(pages: Vec<PdfPage>, (width, height): (u32, u32), title: &str) -> Result<Vec<u8>> {
    if width == 0 || height == 0 {
        return Err(ExportError::General(
            "a deck's frames must be at least one pixel wide and tall".to_string(),
        ));
    }
    let page_height = PAGE_WIDTH * f64::from(height) / f64::from(width);
    let mut source = format!(
        "#set document(title: {})\n\
         #set page(width: {PAGE_WIDTH}pt, height: {page_height:.4}pt, margin: 0pt)\n\
         #set text(size: 20pt)\n",
        typst_string(title)
    );
    let mut files = Vec::new();
    for (index, page) in pages.into_iter().enumerate() {
        match page {
            PdfPage::Frame(pdf) => {
                let path = format!("/page-{}.pdf", index + 1);
                source.push_str(&format!(
                    "#page(image({}, width: 100%, height: 100%))\n",
                    typst_string(&path)
                ));
                files.push((path, pdf));
            }
            PdfPage::Notes { title, notes } => source.push_str(&format!(
                "#page(margin: 56pt)[\n\
                 #text(size: 30pt, weight: \"bold\", {})\n\
                 #v(16pt)\n\
                 #{{ {}.split(\"\\n\").join(linebreak()) }}\n\
                 ]\n",
                typst_string(&title),
                typst_string(&notes),
            )),
        }
    }
    let fonts = gaanim_text::font::FontRegistry::without_system_fonts();
    gaanim_text::typst_compiler::compile_typst_pdf(&source, files, &fonts).map_err(|errors| {
        ExportError::General(format!("could not lay out the PDF: {}", errors.join("; ")))
    })
}

/// `text` as a Typst string literal.
fn typst_string(text: &str) -> String {
    let mut literal = String::with_capacity(text.len() + 2);
    literal.push('"');
    for character in text.chars() {
        match character {
            '\\' => literal.push_str("\\\\"),
            '"' => literal.push_str("\\\""),
            '\n' => literal.push_str("\\n"),
            '\r' => {}
            '\t' => literal.push_str("\\t"),
            other => literal.push(other),
        }
    }
    literal.push('"');
    literal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decks_lay_out_frames_and_notes_as_pdf_pages() {
        let solid = |rgb: [u8; 3]| {
            let pixels = [rgb[0], rgb[1], rgb[2], 255].repeat(32 * 18);
            frame_pdf(&pixels, 32, 18)
        };
        let pages = vec![
            PdfPage::Frame(solid([200, 30, 30])),
            PdfPage::Notes {
                title: "Señales \"de\" prueba".to_string(),
                notes: "Primera línea\\ con barra.\nSegunda: ñandú, 東京.".to_string(),
            },
            PdfPage::Frame(solid([30, 30, 200])),
        ];
        let bytes = deck_pdf(pages, (32, 18), "Deck").unwrap();
        let pdf = hayro_syntax::Pdf::new(bytes).unwrap();
        let pages = pdf.pages();
        assert_eq!(pages.len(), 3);
        for page in pages.iter() {
            let (width, height) = page.render_dimensions();
            assert!((width - 960.0).abs() < 0.01, "{width}");
            assert!((height - 540.0).abs() < 0.01, "{height}");
        }
    }
}
