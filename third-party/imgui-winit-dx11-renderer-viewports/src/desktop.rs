//! The desktop in imgui's coordinate space.
//!
//! Each monitor maps physical pixels to imgui's logical units with its own
//! scale: `logical = origin + (physical - pos) / scale`. Scaling every monitor
//! toward the desktop origin instead (`physical / scale`) only keeps monitors
//! adjacent when their shared edge runs through x=0 or y=0; elsewhere it opens
//! gaps or overlaps imgui would see. So each monitor's logical origin is placed
//! against an already placed neighbour.

use winit::dpi::{PhysicalPosition, PhysicalSize};

pub(crate) struct Monitor {
    pub(crate) pos: PhysicalPosition<i32>,
    pub(crate) size: PhysicalSize<u32>,
    pub(crate) scale: f32,
    /// Logical position of `pos`.
    pub(crate) origin: [f32; 2],
}

impl Monitor {
    fn logical_size(&self) -> [f32; 2] {
        [
            self.size.width as f32 / self.scale,
            self.size.height as f32 / self.scale,
        ]
    }
}

pub(crate) struct Desktop {
    monitors: Vec<Monitor>,
}

/// Squared distance from `p` to the half-open range `[lo, lo + len)` on each
/// axis, zero inside.
fn axis_gap(p: f64, lo: f64, len: f64) -> f64 {
    if p < lo {
        lo - p
    } else if p >= lo + len {
        p - (lo + len) + 1.0
    } else {
        0.0
    }
}

impl Desktop {
    /// `monitors` are `(pos, size, scale)` in physical desktop coordinates.
    pub(crate) fn new(
        monitors: impl IntoIterator<Item = (PhysicalPosition<i32>, PhysicalSize<u32>, f32)>,
    ) -> Self {
        let mut monitors: Vec<Monitor> = monitors
            .into_iter()
            .map(|(pos, size, scale)| Monitor {
                pos,
                size,
                scale,
                origin: [0.0; 2],
            })
            .collect();
        layout(&mut monitors);
        Self { monitors }
    }

    /// The monitor containing `p`, or the nearest one.
    pub(crate) fn monitor_at(&self, p: PhysicalPosition<i32>) -> usize {
        self.nearest(|m| {
            let dx = axis_gap(p.x.into(), m.pos.x.into(), m.size.width.into());
            let dy = axis_gap(p.y.into(), m.pos.y.into(), m.size.height.into());
            dx * dx + dy * dy
        })
    }

    /// The monitor whose logical rect contains `p`, or the nearest one.
    pub(crate) fn monitor_at_logical(&self, p: [f32; 2]) -> usize {
        self.nearest(|m| {
            let [w, h] = m.logical_size();
            let dx = axis_gap(p[0].into(), m.origin[0].into(), w.into());
            let dy = axis_gap(p[1].into(), m.origin[1].into(), h.into());
            dx * dx + dy * dy
        })
    }

    fn nearest(&self, distance: impl Fn(&Monitor) -> f64) -> usize {
        self.monitors
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b)))
            .map_or(0, |(i, _)| i)
    }

    /// Scale of `monitor`; 1.0 if there are no monitors.
    pub(crate) fn scale(&self, monitor: usize) -> f32 {
        self.monitors.get(monitor).map_or(1.0, |m| m.scale)
    }

    pub(crate) fn to_logical(&self, monitor: usize, p: PhysicalPosition<i32>) -> [f32; 2] {
        let Some(m) = self.monitors.get(monitor) else {
            return [p.x as f32, p.y as f32];
        };
        [
            m.origin[0] + (p.x - m.pos.x) as f32 / m.scale,
            m.origin[1] + (p.y - m.pos.y) as f32 / m.scale,
        ]
    }

    pub(crate) fn to_physical(&self, monitor: usize, p: [f32; 2]) -> PhysicalPosition<f32> {
        let Some(m) = self.monitors.get(monitor) else {
            return PhysicalPosition::new(p[0], p[1]);
        };
        PhysicalPosition::new(
            m.pos.x as f32 + (p[0] - m.origin[0]) * m.scale,
            m.pos.y as f32 + (p[1] - m.origin[1]) * m.scale,
        )
    }

    pub(crate) fn platform_monitors(&self) -> Vec<imgui::PlatformMonitor> {
        self.monitors
            .iter()
            .map(|m| imgui::PlatformMonitor {
                main_pos: m.origin,
                main_size: m.logical_size(),
                work_pos: m.origin,
                work_size: m.logical_size(),
                dpi_scale: m.scale,
            })
            .collect()
    }
}

/// Places each monitor's logical origin. The monitor containing the desktop
/// origin (the primary) maps by plain scaling; every other monitor is placed
/// relative to its nearest already placed neighbour so that touching monitors
/// stay touching.
fn layout(monitors: &mut [Monitor]) {
    if monitors.is_empty() {
        return;
    }
    let root = monitors
        .iter()
        .position(|m| {
            m.pos.x <= 0
                && m.pos.y <= 0
                && m.pos.x + m.size.width as i32 > 0
                && m.pos.y + m.size.height as i32 > 0
        })
        .unwrap_or(0);
    let r = &mut monitors[root];
    r.origin = [r.pos.x as f32 / r.scale, r.pos.y as f32 / r.scale];

    let mut placed = vec![false; monitors.len()];
    placed[root] = true;
    while let Some((n, m)) = closest_unplaced(monitors, &placed) {
        monitors[n].origin = place(&monitors[m], &monitors[n]);
        placed[n] = true;
    }
}

fn closest_unplaced(monitors: &[Monitor], placed: &[bool]) -> Option<(usize, usize)> {
    let gap = |a: &Monitor, b: &Monitor| -> i64 {
        let gx = (i64::from(b.pos.x) - (i64::from(a.pos.x) + i64::from(a.size.width)))
            .max(i64::from(a.pos.x) - (i64::from(b.pos.x) + i64::from(b.size.width)))
            .max(0);
        let gy = (i64::from(b.pos.y) - (i64::from(a.pos.y) + i64::from(a.size.height)))
            .max(i64::from(a.pos.y) - (i64::from(b.pos.y) + i64::from(b.size.height)))
            .max(0);
        gx * gx + gy * gy
    };
    (0..monitors.len())
        .filter(|&n| !placed[n])
        .flat_map(|n| {
            (0..monitors.len())
                .filter(|&m| placed[m])
                .map(move |m| (n, m))
        })
        .min_by_key(|&(n, m)| gap(&monitors[m], &monitors[n]))
}

/// Logical origin for `n`, placed against the already placed `m`. On the axis
/// where they sit side by side, `n` starts where `m` ends (plus any physical
/// gap, in `n`'s scale); on the other axis the physical offset is kept, in
/// `n`'s scale.
fn place(m: &Monitor, n: &Monitor) -> [f32; 2] {
    let axis = |m_pos: i32, m_len: u32, m_origin: f32, n_pos: i32, n_len: u32| -> f32 {
        let m_end = m_pos + m_len as i32;
        if n_pos >= m_end {
            m_origin + m_len as f32 / m.scale + (n_pos - m_end) as f32 / n.scale
        } else if n_pos + n_len as i32 <= m_pos {
            m_origin - (m_pos - n_pos) as f32 / n.scale
        } else {
            m_origin + (n_pos - m_pos) as f32 / n.scale
        }
    };
    [
        axis(m.pos.x, m.size.width, m.origin[0], n.pos.x, n.size.width),
        axis(m.pos.y, m.size.height, m.origin[1], n.pos.y, n.size.height),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `((x, y), (width, height), scale)`
    type Spec = ((i32, i32), (u32, u32), f32);

    fn desktop(monitors: &[Spec]) -> Desktop {
        Desktop::new(
            monitors.iter().map(|&((x, y), (w, h), s)| {
                (PhysicalPosition::new(x, y), PhysicalSize::new(w, h), s)
            }),
        )
    }

    fn approx(a: [f32; 2], b: [f32; 2]) {
        assert!(
            (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3,
            "{a:?} != {b:?}"
        );
    }

    /// The layout this crate was developed on: a 125% portrait monitor to the
    /// left of a 175% 4K primary. Placement must match plain origin scaling.
    #[test]
    fn left_of_primary_matches_origin_scaling() {
        let d = desktop(&[
            ((0, 0), (3840, 2160), 1.75),
            ((-1440, -79), (1440, 2560), 1.25),
        ]);
        approx(d.monitors[1].origin, [-1440.0 / 1.25, -79.0 / 1.25]);
        let p = PhysicalPosition::new(-700, 600);
        approx(d.to_logical(1, p), [-700.0 / 1.25, 600.0 / 1.25]);
    }

    /// Origin scaling would put this monitor's left edge at 3840/1.25 = 3072
    /// while the primary ends at 3840/1.75 = 2194.3, leaving a gap.
    #[test]
    fn right_of_primary_is_adjacent() {
        let d = desktop(&[
            ((0, 0), (3840, 2160), 1.75),
            ((3840, 0), (1920, 1080), 1.25),
        ]);
        let primary_right = d.to_logical(0, PhysicalPosition::new(3840, 0))[0];
        approx(d.monitors[1].origin, [primary_right, 0.0]);
    }

    #[test]
    fn below_primary_is_adjacent() {
        let d = desktop(&[
            ((0, 0), (3840, 2160), 1.75),
            ((500, 2160), (1920, 1080), 1.0),
        ]);
        approx(d.monitors[1].origin, [500.0, 2160.0 / 1.75]);
    }

    #[test]
    fn round_trip() {
        let d = desktop(&[
            ((0, 0), (3840, 2160), 1.75),
            ((3840, 0), (1920, 1080), 1.25),
        ]);
        for (m, p) in [(0, (100, 200)), (1, (4000, 300))] {
            let p = PhysicalPosition::new(p.0, p.1);
            let back = d.to_physical(m, d.to_logical(m, p));
            assert!((back.x - p.x as f32).abs() < 1e-3 && (back.y - p.y as f32).abs() < 1e-3);
        }
    }

    #[test]
    fn lookups_pick_containing_or_nearest() {
        let d = desktop(&[
            ((0, 0), (3840, 2160), 1.75),
            ((3840, 0), (1920, 1080), 1.25),
        ]);
        assert_eq!(d.monitor_at(PhysicalPosition::new(3839, 10)), 0);
        assert_eq!(d.monitor_at(PhysicalPosition::new(3840, 10)), 1);
        assert_eq!(d.monitor_at(PhysicalPosition::new(9000, 10)), 1);
        let edge = d.monitors[1].origin;
        assert_eq!(d.monitor_at_logical([edge[0] - 0.5, 10.0]), 0);
        assert_eq!(d.monitor_at_logical([edge[0] + 0.5, 10.0]), 1);
    }
}
