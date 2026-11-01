use eframe::egui::{Color32, Pos2};
use std::time::Instant;

pub struct Stroke {
    pub points: Vec<Pos2>,
    pub color: Color32,
    pub width: f32,
    pub ended: Option<Instant>,
}

#[derive(Default)]
pub struct Drawing {
    pub strokes: Vec<Stroke>,
    pub drawing: bool,
}

impl Drawing {
    pub fn begin(&mut self, p: Pos2, color: Color32, width: f32) {
        self.end();
        if self.strokes.len() >= 256 {
            self.strokes.remove(0);
        }
        self.strokes.push(Stroke {
            points: vec![p],
            color,
            width,
            ended: None,
        });
        self.drawing = true;
    }
    pub fn push(&mut self, p: Pos2) {
        if self.drawing
            && let Some(s) = self.strokes.last_mut()
            && s.points.len() < 50_000
            && s.points.last().is_none_or(|q| q.distance(p) >= 2.0)
        {
            s.points.push(p);
        }
    }
    pub fn end(&mut self) {
        if self.drawing
            && let Some(s) = self.strokes.last_mut()
        {
            s.ended = Some(Instant::now());
        }
        self.drawing = false;
    }
    pub fn clear(&mut self) {
        self.strokes.clear();
        self.drawing = false;
    }
    pub fn expire(&mut self, now: Instant, seconds: f32) {
        self.strokes
            .retain(|s| opacity(s.ended, now, seconds) > 0.0);
    }
}

pub fn opacity(ended: Option<Instant>, now: Instant, seconds: f32) -> f32 {
    if seconds <= 0.0 {
        return 1.0;
    }
    ended.map_or(1.0, |t| {
        ((seconds - now.saturating_duration_since(t).as_secs_f32()) / 0.5).clamp(0.0, 1.0)
    })
}

pub struct HandGeometry {
    pub top: f32,
    pub wrist: f32,
    pub tip_x: f32,
    pub rod_x: f32,
    pub rod_y: f32,
}
impl HandGeometry {
    pub fn measure(image: &image::RgbaImage) -> Result<Self, String> {
        let (w, h) = image.dimensions();
        let rows: Vec<Vec<u32>> = (0..h)
            .map(|y| (0..w).filter(|&x| image.get_pixel(x, y)[3] > 128).collect())
            .collect();
        let top = rows
            .iter()
            .position(|r| !r.is_empty())
            .ok_or("Empty hand image")?;
        let end = (top + (h as f32 * 0.45) as usize).min(h as usize);
        let max = rows[top..end]
            .iter()
            .map(Vec::len)
            .max()
            .ok_or("Invalid hand")? as f32;
        let fist = (top..h as usize)
            .find(|&y| rows[y].len() as f32 > max * 0.6)
            .ok_or("Missing fist")?;
        let wrist = (fist..h as usize)
            .find(|&y| (rows[y].len() as f32) < max * 0.35)
            .ok_or("Missing wrist")?;
        let center = |y: usize| -> Result<f32, String> {
            let row = &rows[y];
            Ok(
                (*row.first().ok_or("Empty measurement row")? + *row.last().unwrap()) as f32
                    / 2.0
                    / w as f32,
            )
        };
        let rod_y = (wrist + h as usize / 50).min(h as usize - 1);
        Ok(Self {
            top: top as f32 / h as f32,
            wrist: wrist as f32 / h as f32,
            tip_x: center((top + h as usize / 200).min(h as usize - 1))?,
            rod_x: center(rod_y)?,
            rod_y: rod_y as f32 / h as f32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn clear_while_pressed_does_not_continue_old_stroke() {
        let mut d = Drawing::default();
        d.begin(Pos2::ZERO, Color32::RED, 3.0);
        d.clear();
        d.push(Pos2::new(4.0, 4.0));
        assert!(d.strokes.is_empty());
        assert!(!d.drawing);
    }
    #[test]
    fn active_and_permanent_ink_survive_but_finished_ink_expires() {
        let now = Instant::now();
        assert_eq!(opacity(None, now + Duration::from_secs(10), 3.0), 1.0);
        assert_eq!(opacity(Some(now), now + Duration::from_secs(3), 3.0), 0.0);
        assert_eq!(opacity(Some(now), now + Duration::from_secs(10), 0.0), 1.0);
        assert!((opacity(Some(now), now + Duration::from_millis(2750), 3.0) - 0.5).abs() < 0.01);
    }
    #[test]
    fn transparent_image_is_rejected() {
        assert!(HandGeometry::measure(&image::RgbaImage::new(8, 8)).is_err());
    }
}
