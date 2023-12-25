use bevy::{
    prelude::*,
    render::{
        render_resource::{PrimitiveTopology, TextureFormat},
    },
};
use image::Pixel;

use crate::{noise::generate_noise_map, noise::Noise, util::export_terrain};

#[derive(Component)]
pub struct Terrain {
    pub noise: Noise,
    // Size of the terrain
    pub size: [u32; 2],
    pub resolution: u32,
    pub wireframe: bool,
    pub height_exponent: f32,
    pub sea_percent: f32,
    pub export: bool,
}

impl Default for Terrain {
    fn default() -> Self {
        Self {
            noise: Noise::default(),
            size: [2; 2],
            resolution: 10,
            wireframe: false,
            height_exponent: 1.0,
            sea_percent: 10.0,
            export: false,
        }
    }
}

#[derive(Bundle, Default)]
pub struct TerrainBundle {
    pub terrain: Terrain,
    pub pbr_bundle: PbrBundle,
}

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, generate_terrain);
    }
}

fn generate_terrain(
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut query: Query<(&mut Terrain, &mut Handle<Mesh>, &Handle<StandardMaterial>)>,
) {
    for (mut terrain, mut mesh_handle, material) in &mut query {
        if let Some(material) = materials.get_mut(material) {
            *material = StandardMaterial::default()
        }
        terrain.noise.size = [
            terrain.size[0] * terrain.resolution,
            terrain.size[1] * terrain.resolution,
        ];
        let noise_values = generate_noise_map(&terrain.noise);

        let mut colors: Vec<colorgrad::Color> = Vec::with_capacity(terrain.noise.regions.len());
        let mut domain: Vec<f64> = Vec::with_capacity(terrain.noise.regions.len());
        for region in &terrain.noise.regions {
            colors.push(colorgrad::Color {
                r: region.color[0] as f64 / 255.0,
                g: region.color[1] as f64 / 255.0,
                b: region.color[2] as f64 / 255.0,
                a: region.color[3] as f64 / 255.0,
            });
            domain.push(region.position);
        }
        let mut grad = colorgrad::CustomGradient::new()
            .colors(&colors)
            .domain(&domain)
            .build()
            .unwrap_or(
                colorgrad::CustomGradient::new()
                    .colors(&colors)
                    .build()
                    .expect("Gradient generation failed"),
            );

        if terrain.noise.gradient.segments != 0 {
            grad = grad.sharp(
                terrain.noise.gradient.segments,
                terrain.noise.gradient.smoothness,
            );
        }

        let mut gradient_buffer = image::ImageBuffer::from_pixel(
            terrain.noise.gradient.size[0],
            terrain.noise.gradient.size[1],
            image::Rgba(terrain.noise.base_color),
        );

        for (x, _, pixel) in gradient_buffer.enumerate_pixels_mut() {
            let rgba = grad
                .at(x as f64 * 100.0 / terrain.noise.gradient.size[0] as f64)
                .to_rgba8();
            pixel.blend(&image::Rgba(rgba));
        }

        terrain.noise.gradient.image = images.add(
            Image::from_dynamic(gradient_buffer.into(), true)
                .convert(TextureFormat::Rgba8UnormSrgb)
                .expect("Could not convert to Rgba8UnormSrgb"),
        );

        let vertices_count: usize =
            ((terrain.noise.size[0] + 1) * (terrain.noise.size[1] + 1)) as usize;
        let triangle_count: usize =
            (terrain.noise.size[0] * terrain.noise.size[1] * 2 * 3) as usize;

        let mut positions: Vec<[f32; 3]> = Vec::with_capacity(vertices_count);
        let mut normals: Vec<[f32; 3]> = Vec::with_capacity(vertices_count);
        let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(vertices_count);
        let mut indices: Vec<u32> = Vec::with_capacity(triangle_count);
        let mut colors: Vec<[f32; 4]> = Vec::with_capacity(vertices_count);

        let rows = terrain.size[0] * terrain.resolution + 1;
        let cols = terrain.size[1] * terrain.resolution + 1;
        let width = terrain.size[0] as f32 + 1.0;
        let depth = terrain.size[1] as f32 + 1.0;
        for i in 0..rows {
            for j in 0..cols {
                let i = i as f32;
                let j = j as f32;
                let noise_value = noise_values[i as usize][j as usize] as f32;
                let height_value = (0_f32.max(noise_value - terrain.sea_percent)) / 100.0;
                let x = (i / terrain.resolution as f32 - width / 2.0) + 0.5;
                let y = ((height_value * 1.2).powf(terrain.height_exponent) - 0.5) * 2.0;
                let z = (j / terrain.resolution as f32 - depth / 2.0) + 0.5;

                let color = grad.at(noise_values[i as usize][j as usize]);
                let color = [
                    color.r as f32,
                    color.g as f32,
                    color.b as f32,
                    color.a as f32,
                ];

                positions.push([x, y, z]);
                normals.push([0.0, 1.0, 0.0]);
                uvs.push([i, j]);
                colors.push(color);
            }
        }

        for i in 0..(rows - 1) {
            for j in 0..(cols - 1) {
                let current = i * cols + j;
                let next_row = (i + 1) * cols + j;

                // Triangle 1
                indices.push(current);
                indices.push(current + 1);
                indices.push(next_row);

                // Triangle 2
                indices.push(next_row);
                indices.push(current + 1);
                indices.push(next_row + 1);
            }
        }

        if terrain.wireframe {
            let triangle_number = indices.len() / 3;
            let cloned_indices = indices.clone();
            indices = vec![];
            for i in 0..triangle_number {
                for j in &[0, 1, 1, 2, 2, 0] {
                    indices.push(cloned_indices[i * 3 + j]);
                }
            }
        }

        let mut mesh = if terrain.wireframe {
            Mesh::new(PrimitiveTopology::LineList)
        } else {
            Mesh::new(PrimitiveTopology::TriangleList)
        };
        mesh.set_indices(Some(bevy::render::mesh::Indices::U32(indices.clone())));
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        *mesh_handle = meshes.add(mesh);

        if terrain.export {
            export_terrain(positions, indices, colors);
            terrain.export = false;
        }
    }
}
