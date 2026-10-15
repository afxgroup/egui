//! Software coverage for glyph outlines.
//!
//! `vello_cpu` paints no curves and only part of the straight edges on big-endian
//! targets such as AmigaOS4 (PowerPC). This module fills the outline with plain `f32`
//! arithmetic instead, so it does not depend on the SIMD code path or on byte order.
//!
//! Per pixel row it takes [`SUB_SCANLINES`] samples. On each sample it finds the edge
//! crossings, fills the spans that the nonzero rule marks as inside, and adds the exact
//! horizontal coverage of each span to the pixels it covers.

use vello_cpu::kurbo::{BezPath, PathEl, Point, Vec2};

/// Samples per pixel row. Horizontal coverage is exact, vertical coverage is sampled.
const SUB_SCANLINES: usize = 4;

/// Line segments used for each quadratic or cubic curve.
const CURVE_SEGMENTS: usize = 8;

/// Coverage in `0..=255` for every pixel of a `width` x `height` box whose top-left
/// corner is at `(origin_x, origin_y)` in the coordinates of `path`.
pub(super) fn rasterize(
    path: &BezPath,
    origin_x: f64,
    origin_y: f64,
    width: usize,
    height: usize,
) -> Vec<u8> {
    let lines = flatten(path, Point::new(origin_x, origin_y));

    let mut out = vec![0u8; width * height];
    let mut acc = vec![0.0f32; width];
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    let weight = 1.0 / SUB_SCANLINES as f32;

    for row in 0..height {
        acc.iter_mut().for_each(|a| *a = 0.0);
        for sub in 0..SUB_SCANLINES {
            let y = row as f64 + (sub as f64 + 0.5) / SUB_SCANLINES as f64;

            crossings.clear();
            for &(a, b) in &lines {
                if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
                    let x = a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y);
                    let direction = if b.y > a.y { 1 } else { -1 };
                    crossings.push((x as f32, direction));
                }
            }
            crossings.sort_by(|p, q| p.0.total_cmp(&q.0));

            let mut winding = 0;
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                if winding != 0 {
                    add_span(&mut acc, pair[0].0, pair[1].0, weight);
                }
            }
        }

        for (x, &a) in acc.iter().enumerate() {
            let coverage = a.min(1.0);
            out[row * width + x] = (coverage * 255.0 + 0.5) as u8;
        }
    }
    out
}

/// Straight segments of `path`, moved so that `origin` becomes `(0, 0)`.
fn flatten(path: &BezPath, origin: Point) -> Vec<(Vec2, Vec2)> {
    let mut lines = Vec::new();
    let mut current = Point::new(0.0, 0.0);
    let mut start = Point::new(0.0, 0.0);

    for el in path.iter() {
        match el {
            PathEl::MoveTo(p) => {
                current = p;
                start = p;
            }
            PathEl::LineTo(p) => {
                lines.push((current - origin, p - origin));
                current = p;
            }
            PathEl::QuadTo(p1, p2) => {
                let p0 = current;
                let mut prev = p0;
                for i in 1..=CURVE_SEGMENTS {
                    let t = i as f64 / CURVE_SEGMENTS as f64;
                    let mt = 1.0 - t;
                    let p = Point::new(
                        mt * mt * p0.x + 2.0 * mt * t * p1.x + t * t * p2.x,
                        mt * mt * p0.y + 2.0 * mt * t * p1.y + t * t * p2.y,
                    );
                    lines.push((prev - origin, p - origin));
                    prev = p;
                }
                current = p2;
            }
            PathEl::CurveTo(p1, p2, p3) => {
                let p0 = current;
                let mut prev = p0;
                for i in 1..=CURVE_SEGMENTS {
                    let t = i as f64 / CURVE_SEGMENTS as f64;
                    let mt = 1.0 - t;
                    let p = Point::new(
                        mt * mt * mt * p0.x
                            + 3.0 * mt * mt * t * p1.x
                            + 3.0 * mt * t * t * p2.x
                            + t * t * t * p3.x,
                        mt * mt * mt * p0.y
                            + 3.0 * mt * mt * t * p1.y
                            + 3.0 * mt * t * t * p2.y
                            + t * t * t * p3.y,
                    );
                    lines.push((prev - origin, p - origin));
                    prev = p;
                }
                current = p3;
            }
            PathEl::ClosePath => {
                lines.push((current - origin, start - origin));
                current = start;
            }
        }
    }
    lines
}

/// Adds `weight` times the horizontal coverage of `[x0, x1)` to each pixel of `acc`.
fn add_span(acc: &mut [f32], x0: f32, x1: f32, weight: f32) {
    let width = acc.len() as f32;
    let x0 = x0.clamp(0.0, width);
    let x1 = x1.clamp(0.0, width);
    if x1 <= x0 {
        return;
    }
    let first = x0.floor() as usize;
    let last = (x1.ceil() as usize).min(acc.len());
    for (px, cell) in acc.iter_mut().enumerate().take(last).skip(first) {
        let left = x0.max(px as f32);
        let right = x1.min(px as f32 + 1.0);
        if right > left {
            *cell += (right - left) * weight;
        }
    }
}
