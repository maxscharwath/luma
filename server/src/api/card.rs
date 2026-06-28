//! Smart Hub preview "cards": a 640×360 (16:9) landscape tile composited from a
//! backdrop + dark scrims + a category **badge** (NOUVEAUTÉ / REPRENDRE), the
//! **LUMA** brand lockup (top-right), and the title's **logo** artwork
//! (transparent PNG, drawn only when one exists — no text fallback), plus an
//! optional resume progress bar. Encoded as JPEG.
//!
//! The film/series title and meta line are shown by the carousel itself (the
//! tile's `title`/`subtitle`), so they are deliberately NOT baked in here.
//!
//! Dependency-light (no resvg / system fonts): tiny-skia rasterises shapes,
//! gradients and the pre-scaled PNG layers; fontdue draws the (Latin) badge +
//! brand wordmark; jpeg-encoder writes the output. The brand font is embedded.

use std::sync::OnceLock;

use fontdue::Font;
use jpeg_encoder::{ColorType, Encoder};
use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, PixmapPaint, Point,
    PremultipliedColorU8, Rect, SpreadMode, Stroke, Transform,
};

const W: u32 = 640;
const H: u32 = 360;
const MARGIN: f32 = 28.0;
const ACCENT: (u8, u8, u8) = (242, 180, 66); // LUMA amber
const WHITE: (u8, u8, u8) = (245, 245, 247);

// Top-left category pill geometry (shared so the brand lockup can align to it).
const BADGE_SIZE: f32 = 20.0;
const BADGE_TRACKING: f32 = 2.0;
const BADGE_PAD_X: f32 = 16.0;
const BADGE_PAD_Y: f32 = 11.0;
const BADGE_H: f32 = BADGE_SIZE + BADGE_PAD_Y * 2.0;

fn font() -> &'static Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    FONT.get_or_init(|| {
        let bytes = include_bytes!("../../assets/fonts/HankenGrotesk.ttf") as &[u8];
        Font::from_bytes(bytes, fontdue::FontSettings::default()).expect("bundled font parses")
    })
}

/// What to render onto a card.
pub struct Card<'a> {
    pub base_png: &'a [u8],
    /// Category badge text, e.g. "Nouveauté" / "Reprendre".
    pub label: &'a str,
    /// Title-treatment logo PNG (alpha), pre-scaled to fit. Drawn only when
    /// present — there is deliberately no text fallback.
    pub logo_png: Option<&'a [u8]>,
    /// Resume fraction 0.0–1.0 → draws a progress bar.
    pub progress: Option<f32>,
}

/// Composite a card and encode it to JPEG. `None` if the base PNG can't decode.
pub fn render(card: &Card) -> Option<Vec<u8>> {
    let mut pm = Pixmap::decode_png(card.base_png).ok()?;

    paint_scrims(&mut pm);

    // Title-treatment artwork (bottom-left). Drawn only when present — no text
    // fallback, by design: a card with no logo simply shows the bare backdrop.
    if let Some(logo) = card.logo_png.and_then(|b| Pixmap::decode_png(b).ok()) {
        let y = H as f32 - MARGIN - logo.height() as f32;
        pm.draw_pixmap(
            MARGIN as i32,
            y as i32,
            logo.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }

    if !card.label.is_empty() {
        paint_badge(&mut pm, &card.label.to_uppercase());
    }

    // LUMA brand lockup, top-right, vertically centred on the badge row.
    paint_brand(&mut pm, MARGIN + BADGE_H / 2.0);

    if let Some(p) = card.progress {
        paint_progress(&mut pm, p.clamp(0.0, 1.0));
    }

    Some(encode_jpeg(&pm))
}

// ---- layers ----------------------------------------------------------------

fn paint_scrims(pm: &mut Pixmap) {
    let (w, h) = (W as f32, H as f32);
    if let Some(shader) = LinearGradient::new(
        Point::from_xy(0.0, h * 0.4),
        Point::from_xy(0.0, h),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(0, 0, 0, 0)),
            GradientStop::new(1.0, Color::from_rgba8(0, 0, 0, 225)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    ) {
        let paint = Paint { shader, ..Default::default() };
        pm.fill_rect(Rect::from_xywh(0.0, 0.0, w, h).unwrap(), &paint, Transform::identity(), None);
    }
    // Soft top scrim so the badge stays legible over bright art.
    if let Some(shader) = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(0.0, h * 0.32),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(0, 0, 0, 150)),
            GradientStop::new(1.0, Color::from_rgba8(0, 0, 0, 0)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    ) {
        let paint = Paint { shader, ..Default::default() };
        pm.fill_rect(Rect::from_xywh(0.0, 0.0, w, h).unwrap(), &paint, Transform::identity(), None);
    }
}

/// Top-left category pill: translucent dark rounded rect + amber uppercase label.
fn paint_badge(pm: &mut Pixmap, text: &str) {
    let f = font();
    let tw = text_width(f, text, BADGE_SIZE, BADGE_TRACKING);
    let bw = tw + BADGE_PAD_X * 2.0;
    let bh = BADGE_H;
    let (x, y) = (MARGIN, MARGIN);

    if let Some(pill) = rounded_rect(x, y, bw, bh, bh / 2.0) {
        let mut bg = Paint::default();
        bg.set_color_rgba8(8, 8, 10, 200);
        bg.anti_alias = true;
        pm.fill_path(&pill, &bg, FillRule::Winding, Transform::identity(), None);
    }
    // Vertically centre the text in the pill (baseline ≈ top + pad + cap height).
    draw_text(
        pm,
        f,
        text,
        x + BADGE_PAD_X,
        y + BADGE_PAD_Y + BADGE_SIZE * 0.82,
        &TextStyle { size: BADGE_SIZE, color: ACCENT, tracking: BADGE_TRACKING },
    );
}

/// Top-right LUMA brand lockup: an amber aperture mark (ring + centre dot)
/// followed by the "LUMA" wordmark, right-aligned to the margin and vertically
/// centred on `cy`. Mirrors the on-screen `<Logo>` lockup, drawn natively (no
/// SVG/asset) from tiny-skia primitives + the embedded brand font.
fn paint_brand(pm: &mut Pixmap, cy: f32) {
    const WORD: &str = "LUMA";
    let f = font();
    let size = 20.0;
    let tracking = size * 0.16; // matches the brand's .16em letter-spacing
    let ring_r = 9.5;
    let stroke_w = 2.4;
    let dot_r = 3.4;
    let gap = 11.0; // mark → wordmark

    let mark_w = ring_r * 2.0 + stroke_w; // visual diameter incl. the stroke
    let word_w = text_width(f, WORD, size, tracking);
    let x0 = W as f32 - MARGIN - (mark_w + gap + word_w);
    let mark_cx = x0 + mark_w / 2.0;

    let mut amber = Paint::default();
    amber.set_color_rgba8(ACCENT.0, ACCENT.1, ACCENT.2, 255);
    amber.anti_alias = true;

    // Aperture ring.
    if let Some(ring) = PathBuilder::from_circle(mark_cx, cy, ring_r) {
        let stroke = Stroke { width: stroke_w, ..Default::default() };
        pm.stroke_path(&ring, &amber, &stroke, Transform::identity(), None);
    }
    // Centre dot.
    if let Some(dot) = PathBuilder::from_circle(mark_cx, cy, dot_r) {
        pm.fill_path(&dot, &amber, FillRule::Winding, Transform::identity(), None);
    }
    // Wordmark, vertically centred on the mark.
    draw_text(
        pm,
        f,
        WORD,
        x0 + mark_w + gap,
        cy + size * 0.32,
        &TextStyle { size, color: WHITE, tracking },
    );
}

fn paint_progress(pm: &mut Pixmap, frac: f32) {
    let (w, y) = (W as f32, H as f32 - 10.0);
    let (x0, x1) = (MARGIN, w - MARGIN);
    let mut tp = Paint::default();
    tp.set_color_rgba8(255, 255, 255, 70);
    pm.fill_rect(Rect::from_xywh(x0, y, x1 - x0, 4.0).unwrap(), &tp, Transform::identity(), None);
    if frac > 0.0 {
        let mut fp = Paint::default();
        fp.set_color_rgba8(ACCENT.0, ACCENT.1, ACCENT.2, 255);
        pm.fill_rect(Rect::from_xywh(x0, y, (x1 - x0) * frac, 4.0).unwrap(), &fp, Transform::identity(), None);
    }
}

// ---- primitives ------------------------------------------------------------

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

/// Glyph rendering style: size, RGB colour, and inter-letter tracking.
#[derive(Clone, Copy)]
struct TextStyle {
    size: f32,
    color: (u8, u8, u8),
    tracking: f32,
}

fn draw_text(pm: &mut Pixmap, font: &Font, text: &str, x: f32, baseline: f32, style: &TextStyle) {
    let TextStyle { size, color, tracking } = *style;
    let mut pen = x;
    for ch in text.chars() {
        let (m, bitmap) = font.rasterize(ch, size);
        if m.width > 0 && m.height > 0 {
            if let Some(mut glyph) = Pixmap::new(m.width as u32, m.height as u32) {
                let px = glyph.pixels_mut();
                for (i, &c) in bitmap.iter().enumerate() {
                    let r = (color.0 as u16 * c as u16 / 255) as u8;
                    let g = (color.1 as u16 * c as u16 / 255) as u8;
                    let b = (color.2 as u16 * c as u16 / 255) as u8;
                    if let Some(p) = PremultipliedColorU8::from_rgba(r, g, b, c) {
                        px[i] = p;
                    }
                }
                let gx = (pen + m.xmin as f32).round() as i32;
                let gy = (baseline - m.height as f32 - m.ymin as f32).round() as i32;
                pm.draw_pixmap(gx, gy, glyph.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
            }
        }
        pen += m.advance_width + tracking;
    }
}

fn text_width(font: &Font, text: &str, size: f32, tracking: f32) -> f32 {
    let mut w = 0.0;
    for ch in text.chars() {
        w += font.metrics(ch, size).advance_width + tracking;
    }
    (w - tracking).max(0.0)
}

// ---- output ----------------------------------------------------------------

fn encode_jpeg(pm: &Pixmap) -> Vec<u8> {
    let data = pm.data();
    let mut rgb = Vec::with_capacity((W * H * 3) as usize);
    for px in data.chunks_exact(4) {
        rgb.extend_from_slice(&px[..3]);
    }
    let mut out = Vec::new();
    Encoder::new(&mut out, 82)
        .encode(&rgb, W as u16, H as u16, ColorType::Rgb)
        .expect("jpeg encode");
    out
}
