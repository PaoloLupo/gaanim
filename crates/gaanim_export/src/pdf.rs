//! Slide decks exported to PDF, laid out from the pages [`crate::deck`]
//! plans.
//!
//! Each frame becomes a one-page PDF of its own, its pixels compressed as
//! soon as it is captured, and Typst lays out the deck around them: it embeds
//! those pages without decoding their pixels again, so a long deck does not
//! hold every frame in memory, and it sets the notes as real text.

use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};

use crate::config::ExportTelemetry;
use crate::deck::{Handout, Paper};
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
    /// A frame, as a one-page PDF from [`frame_pdf`], and the links on it.
    Frame { pdf: Vec<u8>, links: Vec<PdfLink> },
    /// A slide's name over its speaker notes.
    Notes { title: String, notes: String },
}

/// A link on a frame page: its box, 0 to 1 across the page with y down,
/// and where it goes.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfLink {
    pub rect: [f32; 4],
    pub to: PdfDestination,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PdfDestination {
    /// A page of the deck, counted from 1.
    Page(usize),
    Url(String),
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
            PdfPage::Frame { pdf, links } => {
                let path = format!("/page-{}.pdf", index + 1);
                source.push_str(&format!(
                    "#page[#image({}, width: 100%, height: 100%)",
                    typst_string(&path)
                ));
                // Each link is a box over its drawable that the PDF reader
                // follows.
                for link in links {
                    let [left, top, right, bottom] = link.rect.map(f64::from);
                    let to = match &link.to {
                        PdfDestination::Page(page) => {
                            format!("(page: {page}, x: 0pt, y: 0pt)")
                        }
                        PdfDestination::Url(url) => typst_string(url),
                    };
                    source.push_str(&format!(
                        "#place(top + left, dx: {:.3}pt, dy: {:.3}pt, link({to}, box(width: {:.3}pt, height: {:.3}pt)))",
                        left.max(0.0) * PAGE_WIDTH,
                        top.max(0.0) * page_height,
                        (right.min(1.0) - left.max(0.0)).max(0.0) * PAGE_WIDTH,
                        (bottom.min(1.0) - top.max(0.0)).max(0.0) * page_height,
                    ));
                }
                source.push_str("]\n");
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

/// A slide of a handout: its frame, its number and name, and its notes.
#[derive(Debug, Clone, PartialEq)]
pub struct HandoutSlide {
    /// The frame, as a one-page PDF from [`frame_pdf`].
    pub frame: Vec<u8>,
    /// 1-based position of the slide in the deck.
    pub number: usize,
    pub name: String,
    /// Printable notes; empty when the slide has none.
    pub notes: String,
}

/// Margin around a handout sheet, in points.
const SHEET_MARGIN: f64 = 36.0;
/// Room under a sheet for its footer, in points.
const SHEET_FOOTER: f64 = 24.0;
/// Space between the slides of a sheet, in points.
const SHEET_GUTTER: f64 = 18.0;
/// Height of a slide's caption on a sheet, in points.
const SHEET_CAPTION: f64 = 18.0;
/// Writing lines a handout leaves under a slide without notes.
const BLANK_NOTE_LINES: usize = 10;

/// Lay out `slides` as a handout titled `title` on `paper`: `handout`'s
/// grid of slides to a sheet, each over its number and name, or a slide to
/// a sheet with its notes under it as text (writing lines when it has
/// none). A grid with more columns than rows turns the sheet sideways. Each
/// sheet's footer shows the title and the sheet number.
pub fn handout_pdf(
    slides: Vec<HandoutSlide>,
    (width, height): (u32, u32),
    title: &str,
    handout: Handout,
    paper: Paper,
) -> Result<Vec<u8>> {
    if width == 0 || height == 0 {
        return Err(ExportError::General(
            "a deck's frames must be at least one pixel wide and tall".to_string(),
        ));
    }
    let aspect = f64::from(width) / f64::from(height);
    let (columns, rows) = match handout {
        Handout::Grid { columns, rows } => (usize::from(columns), usize::from(rows)),
        Handout::Notes => (1, 1),
    };
    let flipped = columns > rows;
    let (sheet_width, sheet_height) = match (paper.size(), flipped) {
        ((width, height), true) => (height, width),
        (size, false) => size,
    };
    let inner_width = sheet_width - 2.0 * SHEET_MARGIN;
    // A little short of the sheet, so a grid that fills it does not spill
    // onto the next one by a rounding error.
    let inner_height = sheet_height - 2.0 * SHEET_MARGIN - SHEET_FOOTER - 2.0;
    let mut source = format!(
        "#set document(title: {title})\n\
         #set page(width: {sheet_width:.3}pt, height: {sheet_height:.3}pt, margin: (x: {SHEET_MARGIN}pt, top: {SHEET_MARGIN}pt, bottom: {bottom}pt), \
         footer: context {{ set text(size: 8pt, fill: luma(110)); {title}; h(1fr); counter(page).display(\"1 / 1\", both: true) }})\n\
         #set text(size: 10pt)\n",
        title = typst_string(title),
        bottom = SHEET_MARGIN + SHEET_FOOTER,
    );
    let caption = |slide: &HandoutSlide| {
        format!(
            "text(size: 9pt)[#strong({}) #h(4pt) #({})]",
            typst_string(&format!("{}.", slide.number)),
            typst_string(&slide.name)
        )
    };
    let mut files = Vec::new();
    let mut image = |slide: &HandoutSlide, width: f64| {
        let path = format!("/slide-{}.pdf", files.len() + 1);
        let image = format!(
            "box(stroke: 0.5pt + luma(170), image({}, width: {width:.3}pt, height: {:.3}pt))",
            typst_string(&path),
            width / aspect
        );
        files.push((path, slide.frame.clone()));
        image
    };
    match handout {
        Handout::Grid { .. } => {
            let cell_width = (inner_width - SHEET_GUTTER * (columns - 1) as f64) / columns as f64;
            let cell_height = (inner_height - SHEET_GUTTER * (rows - 1) as f64) / rows as f64;
            let image_width = cell_width
                .min((cell_height - SHEET_CAPTION) * aspect)
                .max(1.0);
            for (index, sheet) in slides.chunks(columns * rows).enumerate() {
                if index > 0 {
                    source.push_str("#pagebreak()\n");
                }
                source.push_str(&format!(
                    "#grid(columns: ({cell_width:.3}pt,) * {columns}, rows: {cell_height:.3}pt, \
                     column-gutter: {SHEET_GUTTER}pt, row-gutter: {SHEET_GUTTER}pt,\n"
                ));
                for slide in sheet {
                    source.push_str(&format!(
                        "  stack(spacing: 5pt, {}, {}),\n",
                        image(slide, image_width),
                        caption(slide)
                    ));
                }
                source.push_str(")\n");
            }
        }
        Handout::Notes => {
            let image_width = inner_width.min(inner_height * 0.5 * aspect);
            for (index, slide) in slides.iter().enumerate() {
                if index > 0 {
                    source.push_str("#pagebreak()\n");
                }
                source.push_str(&format!(
                    "#align(center, {})\n#v(6pt)\n#{}\n#v(14pt)\n",
                    image(slide, image_width),
                    caption(slide)
                ));
                if slide.notes.trim().is_empty() {
                    source.push_str(&format!(
                        "#for _ in range({BLANK_NOTE_LINES}) {{ v(20pt); line(length: 100%, stroke: 0.5pt + luma(190)) }}\n"
                    ));
                } else {
                    source.push_str(&format!(
                        "#{{ set text(size: 12pt); {}.split(\"\\n\").join(linebreak()) }}\n",
                        typst_string(&slide.notes)
                    ));
                }
            }
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
            PdfPage::Frame {
                pdf: solid([200, 30, 30]),
                links: vec![
                    PdfLink {
                        rect: [0.1, 0.1, 0.4, 0.3],
                        to: PdfDestination::Page(3),
                    },
                    PdfLink {
                        rect: [0.5, 0.5, 0.9, 0.7],
                        to: PdfDestination::Url("https://doi.org/10.1000/1".to_string()),
                    },
                ],
            },
            PdfPage::Notes {
                title: "Señales \"de\" prueba".to_string(),
                notes: "Primera línea\\ con barra.\nSegunda: ñandú, 東京.".to_string(),
            },
            PdfPage::Frame {
                pdf: solid([30, 30, 200]),
                links: Vec::new(),
            },
        ];
        let bytes = deck_pdf(pages, (32, 18), "Deck").unwrap();
        let pdf = hayro_syntax::Pdf::new(bytes).unwrap();
        // The links became annotations on the first page: one to a page,
        // one to the address.
        use hayro_syntax::object::{Array, Dict, dict::keys::ANNOTS};
        let annotations = pdf.pages()[0]
            .raw()
            .get::<Array<'_>>(ANNOTS)
            .map_or(0, |annotations| annotations.iter::<Dict<'_>>().count());
        assert_eq!(annotations, 2, "link annotations");
        let pages = pdf.pages();
        assert_eq!(pages.len(), 3);
        for page in pages.iter() {
            let (width, height) = page.render_dimensions();
            assert!((width - 960.0).abs() < 0.01, "{width}");
            assert!((height - 540.0).abs() < 0.01, "{height}");
        }
    }

    fn handout_slides(count: usize) -> Vec<HandoutSlide> {
        let pixels = [40, 90, 200, 255].repeat(32 * 18);
        (1..=count)
            .map(|number| HandoutSlide {
                frame: frame_pdf(&pixels, 32, 18),
                number,
                name: format!("Slide \"{number}\""),
                notes: if number % 2 == 0 {
                    "Say this.\nThen ñandú.".to_string()
                } else {
                    String::new()
                },
            })
            .collect()
    }

    fn sheets(bytes: Vec<u8>) -> Vec<(f32, f32)> {
        let pdf = hayro_syntax::Pdf::new(bytes).unwrap();
        pdf.pages()
            .iter()
            .map(|page| page.render_dimensions())
            .collect()
    }

    #[test]
    fn handouts_lay_out_slides_on_sheets_of_paper() {
        let grid = Handout::Grid {
            columns: 2,
            rows: 3,
        };
        let bytes = handout_pdf(handout_slides(10), (32, 18), "Talk", grid, Paper::A4).unwrap();
        let pages = sheets(bytes);
        assert_eq!(pages.len(), 2);
        for (width, height) in pages {
            assert!((width - 595.276).abs() < 0.1 && (height - 841.89).abs() < 0.1);
        }
        // More columns than rows turn the sheet sideways.
        let wide = Handout::Grid {
            columns: 3,
            rows: 2,
        };
        let bytes = handout_pdf(handout_slides(6), (32, 18), "Talk", wide, Paper::Letter).unwrap();
        assert_eq!(sheets(bytes), [(792.0, 612.0)]);
        // A slide to a sheet with its notes, or lines to write them.
        let bytes = handout_pdf(
            handout_slides(3),
            (32, 18),
            "Talk",
            Handout::Notes,
            Paper::A4,
        )
        .unwrap();
        assert_eq!(sheets(bytes).len(), 3);
    }
}
