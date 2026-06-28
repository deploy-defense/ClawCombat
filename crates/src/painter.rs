use std::collections::{HashMap, LinkedList};

use ggez::graphics::{self, BlendComponent, BlendFactor, BlendMode, BlendOperation};

#[derive(Default, Clone)]
pub struct Painter {
    pub(crate) shapes: Vec<egui::ClippedPrimitive>,
    pub(crate) textures_delta: LinkedList<egui::TexturesDelta>,
    paint_jobs: Vec<(egui::TextureId, graphics::Mesh, graphics::Rect)>,
    textures: HashMap<egui::TextureId, graphics::Image>,
    cpu_textures: HashMap<egui::TextureId, egui::ColorImage>,
}

impl Painter {
	pub fn draw(&mut self, canvas: &mut graphics::Canvas, scale_factor: f32) {
		let prev_blend = canvas.blend_mode();
		canvas.set_blend_mode(BlendMode {
			color: BlendComponent {
				src_factor: BlendFactor::One,
				dst_factor: BlendFactor::OneMinusSrcAlpha,
				operation: BlendOperation::Add,
			},
			alpha: BlendComponent {
				src_factor: BlendFactor::OneMinusDstAlpha,
				dst_factor: BlendFactor::One,
				operation: BlendOperation::Add,
			},
		});
		for (id, mesh, clip) in self.paint_jobs.iter() {
			canvas.set_scissor_rect(*clip).unwrap();
			canvas.draw_textured_mesh(
				mesh.clone(),
				self.textures[id].clone(),
				graphics::DrawParam::default().scale([scale_factor, scale_factor]),
			);
		}
		canvas.set_default_scissor_rect();
		self.paint_jobs.clear();
		canvas.set_blend_mode(prev_blend);
	}

	pub fn update(&mut self, ctx: &mut ggez::Context, scale_factor: f32) {
		// Create and free textures
		while let Some(textures_delta) = self.textures_delta.pop_front() {
			self.update_textures(ctx, textures_delta);
		}

		// generating meshes
		for egui::ClippedPrimitive {
			primitive,
			clip_rect,
		} in self.shapes.iter()
		{
			match primitive {
				egui::epaint::Primitive::Mesh(mesh) => {
					if mesh.vertices.len() < 3 {
						continue;
					}

					let vertices = mesh
						.vertices
						.iter()
						.map(|v| graphics::Vertex {
							position: [v.pos.x, v.pos.y],
							uv: [v.uv.x, v.uv.y],
							color: egui::Rgba::from(v.color).to_array(),
						})
						.collect::<Vec<_>>();

					self.paint_jobs.push((
						mesh.texture_id,
						graphics::Mesh::from_data(
							ctx,
							graphics::MeshData {
								vertices: vertices.as_slice(),
								indices: mesh.indices.as_slice(),
							},
						),
						graphics::Rect::new(
							clip_rect.min.x * scale_factor,
							clip_rect.min.y * scale_factor,
							(clip_rect.max.x - clip_rect.min.x) * scale_factor,
							(clip_rect.max.y - clip_rect.min.y) * scale_factor,
						),
					));
				}
				egui::epaint::Primitive::Callback(_) => {
					panic!("Custom rendering callbacks are not implemented yet");
				}
			}
		}
	}

	pub fn update_textures(
        &mut self,
        ctx: &mut ggez::Context,
        textures_delta: egui::TexturesDelta,
    ) {
        // set textures
        for (id, delta) in &textures_delta.set {
            let delta_color_image = match &delta.image {
                // 타입 불일치 에러(E0308) 해결: Arc 포인터 내부의 실제 ColorImage 구조체를 깊은 복사합니다.
                egui::ImageData::Color(image) => (**image).clone(),
                egui::ImageData::Font(image) => {
                    let pixels: Vec<egui::Color32> = image.srgba_pixels(None).collect();
                    egui::ColorImage {
                        size: image.size,
                        pixels,
                    }
                }
            };

            if let Some([x, y]) = delta.pos {
                // Partial Update (폰트에 새로운 글리프가 추가될 때)
                if let Some(existing_image) = self.cpu_textures.get_mut(id) {
                    let delta_width = delta_color_image.size[0];
                    for (dy, row) in delta_color_image.pixels.chunks(delta_width).enumerate() {
                        let dest_y = y + dy;
                        let dest_start = dest_y * existing_image.size[0] + x;
                        let dest_end = dest_start + delta_width;
                        existing_image.pixels[dest_start..dest_end].copy_from_slice(row);
                    }
                }
            } else {
                // Full Update (새로운 텍스처 아틀라스가 완전히 생성될 때)
                self.cpu_textures.insert(*id, delta_color_image);
            }

            // CPU 캐시에서 완성된 전체 픽셀 데이터를 뽑아 GPU로 재전송
            if let Some(updated_color_image) = self.cpu_textures.get(id) {
                let mut pixels: Vec<u8> = Vec::with_capacity(updated_color_image.pixels.len() * 4);
                for pixel in &updated_color_image.pixels {
                    pixels.extend(pixel.to_array());
                }

                let ggez_image = graphics::Image::from_pixels(
                    ctx,
                    pixels.as_slice(),
                    graphics::ImageFormat::Rgba8UnormSrgb,
                    updated_color_image.size[0] as u32,
                    updated_color_image.size[1] as u32,
                );
                self.textures.insert(*id, ggez_image);
            }
        }

        // free textures
        for id in &textures_delta.free {
            self.textures.remove(id);
            self.cpu_textures.remove(id);
        }
    }
}

// Generate Image from egui ColorImage
fn color_to_image(color: &egui::ColorImage, ctx: &mut ggez::Context) -> graphics::Image {
	assert_eq!(
		color.width() * color.height(),
		color.pixels.len(),
		"Mismatch between texture size and texel count"
	);

	let mut pixels: Vec<u8> = Vec::with_capacity(color.pixels.len() * 4);

	for pixel in &color.pixels {
		pixels.extend(pixel.to_array());
	}

	graphics::Image::from_pixels(
		ctx,
		pixels.as_slice(),
		graphics::ImageFormat::Rgba8UnormSrgb,
		color.width() as u32,
		color.height() as u32,
	)
}

// Generate Image from egui FontImage
fn font_to_image(font: &egui::FontImage, ctx: &mut ggez::Context) -> graphics::Image {
	assert_eq!(
		font.width() * font.height(),
		font.pixels.len(),
		"Mismatch between texture size and texel count"
	);

	let mut pixels: Vec<u8> = Vec::with_capacity(font.pixels.len() * 4);

	for pixel in font.srgba_pixels(None) {
		pixels.extend(pixel.to_array());
	}

	graphics::Image::from_pixels(
		ctx,
		pixels.as_slice(),
		graphics::ImageFormat::Rgba8UnormSrgb,
		font.width() as u32,
		font.height() as u32,
	)
}
