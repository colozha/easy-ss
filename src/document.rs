use gtk::cairo::{Context, Error, FillRule, Format, ImageSurface, LineCap, LineJoin};
use gtk::gdk_pixbuf::Pixbuf;
use gtk::prelude::*;

pub type Point = (f64, f64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn bounds(points: &[Point]) -> Option<Self> {
        let &(x, y) = points.first()?;
        let (mut l, mut t, mut r, mut b) = (x, y, x, y);
        for &(x, y) in points {
            l = l.min(x);
            t = t.min(y);
            r = r.max(x);
            b = b.max(y);
        }
        if r <= l || b <= t {
            return None;
        }
        Some(Self {
            x: l.floor(),
            y: t.floor(),
            w: r.ceil() - l.floor(),
            h: b.ceil() - t.floor(),
        })
    }

    pub fn fit(self, width: i32, height: i32) -> View {
        let scale = (width as f64 / self.w).min(height as f64 / self.h);
        View {
            rect: self,
            scale,
            x: (width as f64 - self.w * scale) / 2.,
            y: (height as f64 - self.h * scale) / 2.,
        }
    }
}

pub struct View {
    pub rect: Rect,
    pub scale: f64,
    pub x: f64,
    pub y: f64,
}
impl View {
    pub fn point(&self, x: f64, y: f64, clamp: bool) -> Option<Point> {
        if self.scale <= 0. {
            return None;
        }
        let r = self.rect;
        let (x, y) = ((x - self.x) / self.scale, (y - self.y) / self.scale);
        if !clamp && (x < 0. || y < 0. || x > r.w || y > r.h) {
            return None;
        }
        Some((x.clamp(0., r.w) + r.x, y.clamp(0., r.h) + r.y))
    }
    pub fn apply(&self, c: &Context) {
        c.translate(self.x, self.y);
        c.scale(self.scale, self.scale);
        c.translate(-self.rect.x, -self.rect.y);
        c.rectangle(self.rect.x, self.rect.y, self.rect.w, self.rect.h);
        c.clip();
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Select,
    Pen,
    Highlight,
    Erase,
    Crop,
}

pub fn path(c: &Context, points: &[Point]) {
    if let Some((&first, rest)) = points.split_first() {
        c.move_to(first.0, first.1);
        for &(x, y) in rest {
            c.line_to(x, y);
        }
    }
}

fn smooth_ink(points: &[Point]) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut curve = vec![points[0]];
    let mut start = points[0];
    for i in 1..points.len() - 1 {
        let control = points[i];
        let next = points[i + 1];
        let end = ((control.0 + next.0) / 2., (control.1 + next.1) / 2.);
        // A quadratic's maximum deviation from its chord falls as 1 / steps².
        let bend = (start.0 - 2. * control.0 + end.0).hypot(start.1 - 2. * control.1 + end.1);
        let steps = (2. * bend).sqrt().ceil() as usize;
        for step in 1..steps {
            let t = step as f64 / steps as f64;
            let u = 1. - t;
            curve.push((
                u * u * start.0 + 2. * u * t * control.0 + t * t * end.0,
                u * u * start.1 + 2. * u * t * control.1 + t * t * end.1,
            ));
        }
        curve.push(end);
        start = end;
    }
    curve.push(points[points.len() - 1]);
    curve
}

pub fn stroke(c: &Context, tool: Tool, points: &[Point]) -> Result<(), Error> {
    draw_ink(c, tool, &smooth_ink(points), 1.)
}

fn radius(tool: Tool) -> f64 {
    if tool == Tool::Highlight { 8. } else { 1.5 }
}

fn draw_ink(c: &Context, tool: Tool, points: &[Point], scale: f64) -> Result<(), Error> {
    if tool == Tool::Highlight {
        c.set_source_rgba(1., 0.81, 0.05, 0.45);
    } else {
        c.set_source_rgb(0.9, 0.16, 0.19);
    };
    c.set_line_cap(LineCap::Round);
    c.set_line_join(LineJoin::Round);
    c.set_line_width(2. * radius(tool) * scale);
    path(c, points);
    if let [p] = points {
        c.line_to(p.0, p.1);
    }
    c.stroke()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub scale: f64,
    pub dx: f64,
    pub dy: f64,
}
impl Transform {
    pub const IDENTITY: Self = Self {
        scale: 1.,
        dx: 0.,
        dy: 0.,
    };
    pub fn point(self, p: Point) -> Point {
        (p.0 * self.scale + self.dx, p.1 * self.scale + self.dy)
    }
    pub fn gesture(start: Point, end: Point, anchor: Option<Point>) -> Self {
        if let Some((x, y)) = anchor {
            let (vx, vy) = (start.0 - x, start.1 - y);
            if vx == 0. && vy == 0. {
                return Self::IDENTITY;
            }
            let scale = (((end.0 - x) * vx + (end.1 - y) * vy) / (vx * vx + vy * vy)).max(0.05);
            Self {
                scale,
                dx: x * (1. - scale),
                dy: y * (1. - scale),
            }
        } else {
            Self {
                scale: 1.,
                dx: end.0 - start.0,
                dy: end.1 - start.1,
            }
        }
    }
}

enum Edit {
    Ink(Tool, Vec<Point>),
    Erase(Vec<usize>),
    Transform(usize, Vec<Point>, f64),
    Crop(Rect),
}
pub struct Document {
    base: ImageSurface,
    edits: Vec<Edit>,
    cursor: usize,
    saved: Option<usize>,
}

impl Document {
    pub fn from_image(image: &Pixbuf) -> Result<Self, Error> {
        let base = ImageSurface::create(Format::ARgb32, image.width(), image.height())?;
        let c = Context::new(&base)?;
        c.set_source_pixbuf(image, 0., 0.);
        c.paint()?;
        Ok(Self::new(base))
    }
    fn new(base: ImageSurface) -> Self {
        Self {
            base,
            edits: vec![],
            cursor: 0,
            saved: Some(0),
        }
    }
    pub fn rect(&self) -> Rect {
        self.edits[..self.cursor]
            .iter()
            .rev()
            .find_map(|edit| match edit {
                Edit::Crop(r) => Some(*r),
                _ => None,
            })
            .unwrap_or(Rect {
                x: 0.,
                y: 0.,
                w: self.base.width() as f64,
                h: self.base.height() as f64,
            })
    }
    pub fn dirty(&self) -> bool {
        self.saved != Some(self.cursor)
    }
    pub fn mark_saved(&mut self) {
        self.saved = Some(self.cursor);
    }
    pub fn undoable(&self) -> bool {
        self.cursor > 0
    }
    pub fn redoable(&self) -> bool {
        self.cursor < self.edits.len()
    }
    pub fn undo(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }
    pub fn redo(&mut self) {
        self.cursor = (self.cursor + 1).min(self.edits.len());
    }
    fn push(&mut self, edit: Edit) {
        if self.saved.is_some_and(|s| s > self.cursor) {
            self.saved = None;
        }
        self.edits.truncate(self.cursor);
        self.edits.push(edit);
        self.cursor += 1;
    }
    fn ink(&self) -> impl DoubleEndedIterator<Item = (usize, Tool, &[Point], f64)> {
        let edits = &self.edits[..self.cursor];
        let mut changed = std::collections::HashMap::new();
        for edit in edits {
            match edit {
                Edit::Transform(id, points, scale) => {
                    changed.insert(*id, Some((points.as_slice(), *scale)));
                }
                Edit::Erase(ids) => {
                    for id in ids {
                        changed.insert(*id, None);
                    }
                }
                _ => (),
            }
        }
        edits.iter().enumerate().filter_map(move |(id, edit)| {
            let Edit::Ink(tool, points) = edit else {
                return None;
            };
            let (points, scale) = changed.get(&id).copied().unwrap_or(Some((points, 1.)))?;
            Some((id, *tool, points, scale))
        })
    }
    pub fn hit(&self, point: Point, tolerance: f64) -> Option<usize> {
        self.ink()
            .rev()
            .find(|(_, tool, points, scale)| {
                touches(&[point], points, radius(*tool) * scale + tolerance)
            })
            .map(|ink| ink.0)
    }
    pub fn corners(&self, id: usize) -> Option<[Point; 4]> {
        let (_, tool, points, scale) = self.ink().find(|ink| ink.0 == id)?;
        let (mut left, mut top) = points[0];
        let (mut right, mut bottom) = points[0];
        for &(x, y) in points {
            left = left.min(x);
            right = right.max(x);
            top = top.min(y);
            bottom = bottom.max(y);
        }
        let pad = radius(tool) * scale;
        Some([
            (left - pad, top - pad),
            (right + pad, top - pad),
            (right + pad, bottom + pad),
            (left - pad, bottom + pad),
        ])
    }
    pub fn transform(&mut self, id: usize, transform: Transform) {
        if transform == Transform::IDENTITY {
            return;
        }
        let edit = self
            .ink()
            .find(|ink| ink.0 == id)
            .map(|(_, _, points, scale)| {
                Edit::Transform(
                    id,
                    points.iter().map(|&p| transform.point(p)).collect(),
                    scale * transform.scale,
                )
            });
        if let Some(edit) = edit {
            self.push(edit);
        }
    }
    pub fn remove(&mut self, id: usize) {
        self.push(Edit::Erase(vec![id]));
    }
    pub fn edit(&mut self, tool: Tool, points: Vec<Point>) {
        if points.is_empty() {
            return;
        }
        match tool {
            Tool::Crop => {
                if let Some(r) = Rect::bounds(&[points[0], *points.last().unwrap()])
                    && r != self.rect()
                {
                    self.push(Edit::Crop(r));
                }
            }
            Tool::Erase => {
                let hits: Vec<_> = self
                    .ink()
                    .filter(|(_, t, p, scale)| {
                        touches(
                            &points,
                            p,
                            radius(*t) * scale + if *t == Tool::Highlight { 4. } else { 4.5 },
                        )
                    })
                    .map(|(i, _, _, _)| i)
                    .collect();
                if !hits.is_empty() {
                    self.push(Edit::Erase(hits));
                }
            }
            Tool::Select => (),
            Tool::Pen | Tool::Highlight => self.push(Edit::Ink(tool, smooth_ink(&points))),
        }
    }
    pub fn paint(&self, c: &Context) -> Result<(), Error> {
        self.paint_preview(c, None)
    }
    pub fn paint_preview(
        &self,
        c: &Context,
        preview: Option<(usize, Transform)>,
    ) -> Result<(), Error> {
        c.set_source_surface(&self.base, 0., 0.)?;
        c.paint()?;
        for (id, t, p, scale) in self.ink() {
            c.save()?;
            if let Some((_, tr)) = preview.filter(|(selected, _)| *selected == id) {
                c.translate(tr.dx, tr.dy);
                c.scale(tr.scale, tr.scale);
            }
            draw_ink(c, t, p, scale)?;
            c.restore()?;
        }
        Ok(())
    }
    fn render(&self, r: Rect, mask: Option<&[Point]>) -> Result<ImageSurface, Error> {
        let s = ImageSurface::create(Format::ARgb32, r.w as i32, r.h as i32)?;
        let c = Context::new(&s)?;
        c.translate(-r.x, -r.y);
        if let Some(p) = mask {
            path(&c, p);
            c.close_path();
            c.set_fill_rule(FillRule::EvenOdd);
            c.clip();
        }
        self.paint(&c)?;
        Ok(s)
    }
    pub fn selection(&self, p: &[Point], free: bool) -> Result<Option<Self>, Error> {
        if p.len() < 2 {
            return Ok(None);
        }
        let r = if free {
            Rect::bounds(p)
        } else {
            Rect::bounds(&[p[0], p[p.len() - 1]])
        };
        let Some(r) = r else { return Ok(None) };
        if free && !p.windows(2).any(|s| cross(p[0], s[0], s[1]).abs() > 0.01) {
            return Ok(None);
        }
        Ok(Some(Self::new(self.render(r, free.then_some(p))?)))
    }
    pub fn pixels(
        &self,
        language: crate::settings::Language,
    ) -> Result<Pixbuf, Box<dyn std::error::Error>> {
        let s = self.render(self.rect(), None)?;
        gtk::gdk::pixbuf_get_from_surface(&s, 0, 0, s.width(), s.height()).ok_or_else(|| {
            language
                .text(
                    "Gagal membaca piksel gambar.",
                    "Could not read image pixels.",
                )
                .into()
        })
    }
}

fn cross(a: Point, b: Point, p: Point) -> f64 {
    (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
}
fn distance(p: Point, a: Point, b: Point) -> f64 {
    let (x, y) = (b.0 - a.0, b.1 - a.1);
    let t = if x == 0. && y == 0. {
        0.
    } else {
        (((p.0 - a.0) * x + (p.1 - a.1) * y) / (x * x + y * y)).clamp(0., 1.)
    };
    (p.0 - a.0 - t * x).hypot(p.1 - a.1 - t * y)
}
fn touches(a: &[Point], b: &[Point], radius: f64) -> bool {
    let segments = |len: usize| (0..len).map(move |i| (i, (i + 1).min(len - 1)));
    segments(a.len()).any(|(i, j)| {
        segments(b.len()).any(|(k, l)| {
            let (a, b, c, d) = (a[i], a[j], b[k], b[l]);
            cross(a, b, c) * cross(a, b, d) < 0. && cross(c, d, a) * cross(c, d, b) < 0.
                || [
                    distance(a, c, d),
                    distance(b, c, d),
                    distance(c, a, b),
                    distance(d, a, b),
                ]
                .into_iter()
                .any(|x| x <= radius)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> Document {
        let surface = ImageSurface::create(Format::ARgb32, 64, 32).unwrap();
        let ctx = Context::new(&surface).unwrap();
        ctx.set_source_rgb(1., 1., 1.);
        ctx.paint().unwrap();
        Document::new(surface)
    }
    fn pixel(surface: &mut ImageSurface, x: usize, y: usize) -> u32 {
        let i = y * surface.stride() as usize + x * 4;
        u32::from_ne_bytes(surface.data().unwrap()[i..i + 4].try_into().unwrap())
    }

    #[test]
    fn selections_preserve_pixel_bounds_and_mask_alpha() {
        let d = image();
        let reversed = d
            .selection(&[(64., 32.), (10.5, 4.25)], false)
            .unwrap()
            .unwrap();
        assert_eq!((reversed.base.width(), reversed.base.height()), (54, 28));
        let mut triangle = d
            .selection(&[(2., 2.), (22., 2.), (2., 22.)], true)
            .unwrap()
            .unwrap();
        assert_eq!(pixel(&mut triangle.base, 1, 1), 0xffffffff);
        assert_eq!(pixel(&mut triangle.base, 19, 19), 0);
        assert!(d.selection(&[(1., 1.), (3., 3.)], true).unwrap().is_none());
        assert!(
            d.selection(&[(5., 2.), (5., 10.)], false)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn scaled_letterbox_maps_into_cropped_source() {
        let rect = Rect {
            x: 100.,
            y: 40.,
            w: 400.,
            h: 100.,
        };
        let view = rect.fit(500, 300);
        assert_eq!(view.point(250., 150., false), Some((300., 90.)));
        assert_eq!(view.point(250., 0., false), None);
        assert_eq!(view.point(600., 0., true), Some((500., 40.)));
    }

    #[test]
    fn eraser_follows_the_visible_curve() {
        let mut d = image();
        d.edit(Tool::Pen, vec![(0., 30.), (0., 0.), (60., 0.)]);
        // Halfway along the first quadratic, the centerline is at (7.5, 7.5).
        assert_ne!(
            pixel(&mut d.render(d.rect(), None).unwrap(), 7, 7),
            0xffffffff
        );
        d.edit(Tool::Erase, vec![(0., 0.)]);
        assert_eq!(d.ink().count(), 1);
        d.edit(Tool::Erase, vec![(7.5, 7.5)]);
        assert_eq!(d.ink().count(), 0);
    }

    #[test]
    fn spiral_and_short_strokes_match_preview_after_commit() {
        let spiral = (0..24)
            .map(|i| {
                let angle = i as f64 * 0.55;
                (32. + angle * angle.cos(), 16. + angle * angle.sin())
            })
            .collect();
        for points in [spiral, vec![(10., 10.)], vec![(10., 10.), (15., 10.)]] {
            for tool in [Tool::Pen, Tool::Highlight] {
                let mut d = image();
                let mut preview = d.render(d.rect(), None).unwrap();
                stroke(&Context::new(&preview).unwrap(), tool, &points).unwrap();
                let &(x, y) = points.last().unwrap();
                assert_ne!(pixel(&mut preview, x as usize, y as usize), 0xffffffff);
                d.edit(tool, points.clone());
                let mut rendered = d.render(d.rect(), None).unwrap();
                assert_eq!(&*preview.data().unwrap(), &*rendered.data().unwrap());
            }
        }
    }

    #[test]
    fn transformed_strokes_keep_order_and_history() {
        let mut d = image();
        d.edit(Tool::Pen, vec![(5., 10.), (15., 10.)]);
        d.edit(Tool::Highlight, vec![(10., 6.), (10., 14.)]);
        assert_eq!(d.hit((10., 10.), 0.), Some(1));
        d.transform(0, Transform::gesture((5., 10.), (30., 20.), None));
        assert_eq!(d.hit((35., 20.), 0.), Some(0));
        let corners = d.corners(0).unwrap();
        let (x, y) = corners[0];
        let end = (x + 2. * (corners[2].0 - x), y + 2. * (corners[2].1 - y));
        let resize = Transform::gesture(corners[2], end, Some(corners[0]));
        let mut preview = ImageSurface::create(Format::ARgb32, 64, 32).unwrap();
        d.paint_preview(&Context::new(&preview).unwrap(), Some((0, resize)))
            .unwrap();
        d.transform(0, resize);
        assert_eq!(d.corners(0).unwrap()[0], corners[0]);
        assert_eq!(d.corners(0).unwrap()[2], end);
        assert_eq!(
            &*preview.data().unwrap(),
            &*d.render(d.rect(), None).unwrap().data().unwrap()
        );
        d.remove(0);
        assert_eq!(d.hit((45., 20.), 0.), None);
        d.undo();
        assert_eq!(d.hit((45., 20.), 0.), Some(0));
        d.undo();
        assert_eq!(d.corners(0).unwrap(), corners);
        d.undo();
        d.redo();
        assert_eq!(d.corners(0).unwrap(), corners);
        d.redo();
        d.redo();
        assert!(d.corners(0).is_none());
    }

    #[test]
    fn erasing_crossed_ink_and_cropping_share_undo_history() {
        let mut d = image();
        d.edit(Tool::Pen, vec![(2., 16.), (60., 16.)]);
        d.edit(Tool::Highlight, vec![(2., 16.), (60., 16.)]);
        d.edit(Tool::Crop, vec![(40., 24.), (20., 8.)]);
        d.mark_saved();
        let r = d.rect();
        let mut painted = d.render(r, None).unwrap();
        assert_ne!(pixel(&mut painted, 10, 8), 0xffffffff);
        d.edit(Tool::Erase, vec![(30., 8.), (30., 24.)]);
        assert_eq!(pixel(&mut d.render(r, None).unwrap(), 10, 8), 0xffffffff);
        d.undo();
        assert!(!d.dirty());
        assert_eq!(
            pixel(&mut d.render(r, None).unwrap(), 10, 8),
            pixel(&mut painted, 10, 8)
        );
        d.redo();
        assert!(d.dirty());
        d.undo();
        d.undo();
        assert_eq!(d.rect().w, 64.);
        d.edit(Tool::Pen, vec![(12., 12.)]);
        assert!(!d.redoable());
        assert!(d.dirty());
    }
}
