//! The KROMA brand lockup painted onto a preview card: the wordmark with the
//! chromatic-wheel O ("KR" + wheel + "MA").
//!
//! Mirrors the official lockup export (kroma-lockup-*.svg) but is drawn natively
//! from tiny-skia primitives + an embedded font, so a card costs no SVG
//! rasteriser and no system fonts. Everything here only touches the [`Pixmap`]
//! and the text primitives owned by the parent module.

use std::sync::OnceLock;

use fontdue::Font;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

use super::{draw_text, text_width, TextStyle, MARGIN, W, WHITE};

/// The wordmark face: Bricolage Grotesque ExtraBold, per the "1b Chroma" logo spec.
fn font() -> &'static Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    FONT.get_or_init(|| {
        let bytes = include_bytes!("../../../assets/fonts/BricolageGrotesque-ExtraBold.ttf") as &[u8];
        Font::from_bytes(bytes, fontdue::FontSettings::default()).expect("bundled font parses")
    })
}

/// The chromatic wheel's six segment colours, clockwise from 12 o'clock
/// (corail, ambre, menthe, azur, indigo, violet).
const WHEEL: [(u8, u8, u8); 6] = [
    (242, 104, 92),
    (244, 182, 66),
    (95, 191, 143),
    (79, 157, 224),
    (99, 102, 241),
    (168, 85, 247),
];

/// Paint the lockup right-aligned to the margin and vertically centred on `cy`:
/// wheel ~.91em, optically centred on the caps, asymmetric side gaps, -.014em
/// tracking (all from the official export).
pub(super) fn paint(pm: &mut Pixmap, cy: f32) {
    let f = font();
    let size = 20.0;
    let tracking = size * -0.014;
    let wheel = size * 0.91;
    let (gap_l, gap_r) = (size * 0.055, size * 0.09);

    let w_kr = text_width(f, "KR", size, tracking);
    let w_ma = text_width(f, "MA", size, tracking);
    let total = w_kr + gap_l + wheel + gap_r + w_ma;
    let x0 = W as f32 - MARGIN - total;
    let baseline = cy + size * 0.32;

    let style = TextStyle { size, color: WHITE, tracking };
    draw_text(pm, f, "KR", x0, baseline, &style);
    let wx = x0 + w_kr + gap_l;
    let wy = baseline + size * 0.095 - wheel;
    paint_wheel(pm, wx, wy, wheel);
    draw_text(pm, f, "MA", wx + wheel + gap_r, baseline, &style);
}

/// Draw the chromatic wheel: six annular sectors (hub/outer radius ratio
/// 15/44 per the official mark, hub left transparent) at top-left `x,y`,
/// `size` = wheel diameter in px. Each 60-degree arc is one cubic Bezier
/// (k = 4/3 tan(15 deg), exact to sub-pixel at card scale).
fn paint_wheel(pm: &mut Pixmap, x: f32, y: f32, size: f32) {
    const K: f32 = 0.357_264_3;
    let s = size / 88.0;
    let (cx, cy) = (x + size / 2.0, y + size / 2.0);
    let (r_out, r_in) = (44.0 * s, 15.0 * s);
    // 0 rad at 12 o'clock, clockwise in screen coords (y down).
    let dir = |t: f32| (t.sin(), -t.cos());
    let tan = |t: f32| (t.cos(), t.sin());

    for (i, &(cr, cg, cb)) in WHEEL.iter().enumerate() {
        let t1 = (i as f32) * 60.0_f32.to_radians();
        let t2 = t1 + 60.0_f32.to_radians();
        let (d1, d2) = (dir(t1), dir(t2));
        let (g1, g2) = (tan(t1), tan(t2));
        let po1 = (cx + r_out * d1.0, cy + r_out * d1.1);
        let po2 = (cx + r_out * d2.0, cy + r_out * d2.1);
        let pi1 = (cx + r_in * d1.0, cy + r_in * d1.1);
        let pi2 = (cx + r_in * d2.0, cy + r_in * d2.1);
        let (ko, ki) = (K * r_out, K * r_in);

        let mut pb = PathBuilder::new();
        pb.move_to(pi1.0, pi1.1);
        pb.line_to(po1.0, po1.1);
        pb.cubic_to(
            po1.0 + ko * g1.0,
            po1.1 + ko * g1.1,
            po2.0 - ko * g2.0,
            po2.1 - ko * g2.1,
            po2.0,
            po2.1,
        );
        pb.line_to(pi2.0, pi2.1);
        pb.cubic_to(
            pi2.0 - ki * g2.0,
            pi2.1 - ki * g2.1,
            pi1.0 + ki * g1.0,
            pi1.1 + ki * g1.1,
            pi1.0,
            pi1.1,
        );
        pb.close();
        if let Some(path) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(cr, cg, cb, 255);
            paint.anti_alias = true;
            pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::card::H;

    /// The lockup draws, and it stays inside the card: right-aligned to the
    /// margin, on the badge row.
    #[test]
    fn lockup_paints_in_the_top_right_corner() {
        let mut pm = Pixmap::new(W, H).expect("pixmap");
        paint(&mut pm, MARGIN + 21.0);
        let lit = |x: u32, y: u32| pm.pixel(x, y).is_some_and(|p| p.alpha() > 0);
        assert!((W / 2..W).any(|x| (0..80).any(|y| lit(x, y))), "the lockup drew nothing");
        // Right-aligned to the margin, so the edge column is never touched.
        assert!(!(0..H).any(|y| lit(W - 1, y)), "the lockup spilled past the margin");
        // Nor does it reach the bottom half (that belongs to the title artwork).
        assert!(!(0..W).any(|x| (H / 2..H).any(|y| lit(x, y))), "the lockup leaked downward");
    }
}
