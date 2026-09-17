//! Plain text shaping with HarfRust.

use crate::fonts::Faces;
use crate::{FontId, GlyphRun, PositionedGlyph, SegmentLayout, TextItem};

/// Shapes `text` in STIX Two Text Regular at `size_pt` points.
///
/// Glyphs are positioned from HarfRust's advances and offsets, so kerning and
/// ligatures are applied. Each glyph's text range starts at its cluster and
/// ends at the next larger cluster, so a ligature claims every character it
/// replaces; when several glyphs share one cluster, the first claims the
/// characters and the others receive an empty range at the cluster's end.
///
/// The extents are the face ascender and the magnitude of its descender,
/// independent of the characters, so that labels set at one size align.
pub(crate) fn shape(
    shaper_data: &harfrust::ShaperData,
    faces: &Faces,
    text: &str,
    size_pt: f64,
) -> SegmentLayout {
    let face = faces.get(FontId::TextRegular);
    let scale = size_pt / f64::from(face.units_per_em());
    let height = (f64::from(face.ascender()) * scale).max(0.0);
    let depth = (-f64::from(face.descender()) * scale).max(0.0);
    if text.is_empty() {
        return SegmentLayout {
            items: Vec::new(),
            width: 0.0,
            height,
            depth,
        };
    }

    let bytes = crate::fonts::bytes(FontId::TextRegular);
    let Ok(font_ref) = harfrust::FontRef::new(bytes) else {
        // The bundled face is verified by tests; an unreadable face yields
        // nothing to draw rather than a panic.
        return SegmentLayout::default();
    };
    let shaper = shaper_data.shaper(&font_ref).build();
    let mut buffer = harfrust::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let shaped = shaper.shape(buffer, harfrust::ShapeOptions::new());

    let infos = shaped.glyph_infos();
    let positions = shaped.glyph_positions();
    let mut cluster_starts: Vec<usize> = infos.iter().map(|info| info.cluster as usize).collect();
    cluster_starts.sort_unstable();
    cluster_starts.dedup();

    let mut glyphs = Vec::with_capacity(infos.len());
    let mut claimed = vec![false; cluster_starts.len()];
    let mut pen = 0.0;
    for (info, position) in infos.iter().zip(positions) {
        let start = (info.cluster as usize).min(text.len());
        let slot = cluster_starts.partition_point(|&c| c < start);
        let end = cluster_starts
            .get(slot + 1)
            .copied()
            .unwrap_or(text.len())
            .min(text.len());
        let text_range = match claimed.get_mut(slot) {
            Some(done) if !*done => {
                *done = true;
                start..end
            }
            _ => end..end,
        };
        glyphs.push(PositionedGlyph {
            id: u16::try_from(info.glyph_id).unwrap_or(0),
            x: pen + f64::from(position.x_offset) * scale,
            // Subtracting from zero avoids a negative zero on the baseline.
            y: 0.0 - f64::from(position.y_offset) * scale,
            text_range,
        });
        pen += f64::from(position.x_advance) * scale;
    }

    SegmentLayout {
        items: vec![TextItem::Glyphs(GlyphRun {
            font: FontId::TextRegular,
            size_pt,
            text: text.to_owned(),
            glyphs,
        })],
        width: pen,
        height,
        depth,
    }
}
