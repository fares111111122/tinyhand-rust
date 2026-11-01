use crate::model::HandGeometry;
use eframe::egui::{
    self, Color32, Pos2, Rect, Vec2,
    epaint::{Mesh, Vertex},
};

pub struct Hand {
    texture: egui::TextureHandle,
    geometry: HandGeometry,
    aspect: f32,
}
impl Hand {
    pub fn load(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Result<Self, String> {
        let image = image::load_from_memory(bytes)
            .map_err(|e| format!("{name}: {e}"))?
            .into_rgba8();
        let geometry = HandGeometry::measure(&image).map_err(|e| format!("{name}: {e}"))?;
        let (w, h) = image.dimensions();
        let color =
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], image.as_raw());
        Ok(Self {
            texture: ctx.load_texture(name, color, egui::TextureOptions::LINEAR),
            geometry,
            aspect: w as f32 / h as f32,
        })
    }
    pub fn paint(
        &self,
        painter: &egui::Painter,
        tip: Pos2,
        screen: Rect,
        size: f32,
        pressed: bool,
        shadow: bool,
    ) {
        let g = &self.geometry;
        let grip = Pos2::new(
            (tip.x + screen.width() * 0.3).min(screen.right() + screen.width() * 0.05),
            screen.bottom() + screen.height() * 0.12,
        );
        let down = (grip - tip).normalized();
        let right = Vec2::new(down.y, -down.x);
        let scale = screen.height() * size / (g.wrist - g.top) * if pressed { 0.94 } else { 1.0 };
        let width = scale * self.aspect;
        let wrist =
            tip + down * ((g.wrist - g.top) * scale) + right * ((g.rod_x - g.tip_x) * width);
        if shadow {
            let offset = Vec2::new(8.0, 12.0) * if pressed { 0.3 } else { 1.0 };
            painter.line_segment(
                [wrist + offset, grip + offset],
                egui::Stroke::new(12.0_f32, Color32::from_black_alpha(45)),
            );
        }
        let mut mesh = Mesh::with_texture(self.texture.id());
        // Stretch only the narrow rod slice; the hand retains its original proportions.
        let rod_left = right * (g.rod_x * width);
        let corners = [
            wrist - rod_left,
            wrist - rod_left + right * width,
            grip - rod_left + right * width,
            grip - rod_left,
        ];
        quad(
            &mut mesh,
            corners,
            Rect::from_min_max(
                Pos2::new(0.0, g.rod_y),
                Pos2::new(1.0, (g.rod_y + 0.002).min(1.0)),
            ),
        );
        let top_left = tip - right * (g.tip_x * width);
        let height = (g.wrist - g.top) * scale;
        quad(
            &mut mesh,
            [
                top_left,
                top_left + right * width,
                top_left + right * width + down * height,
                top_left + down * height,
            ],
            Rect::from_min_max(Pos2::new(0.0, g.top), Pos2::new(1.0, g.wrist)),
        );
        painter.add(egui::Shape::mesh(mesh));
    }
}
fn quad(mesh: &mut Mesh, corners: [Pos2; 4], uv: Rect) {
    let start = mesh.vertices.len() as u32;
    for (pos, uv) in corners.into_iter().zip([
        uv.left_top(),
        uv.right_top(),
        uv.right_bottom(),
        uv.left_bottom(),
    ]) {
        mesh.vertices.push(Vertex {
            pos,
            uv,
            color: Color32::WHITE,
        });
    }
    mesh.indices
        .extend([start, start + 1, start + 2, start, start + 2, start + 3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn texture_quad_has_two_valid_triangles() {
        let mut mesh = Mesh::default();
        quad(
            &mut mesh,
            [
                Pos2::ZERO,
                Pos2::new(1.0, 0.0),
                Pos2::new(1.0, 1.0),
                Pos2::new(0.0, 1.0),
            ],
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        );
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices.len(), 6);
        assert!(mesh.is_valid());
    }
}
