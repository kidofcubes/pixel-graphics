use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};
use glam::{vec2, vec3, FloatExt, Vec2, Vec3};
use image::RgbaImage;
use crate::glutils::generate_pixel_data;


#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct PixelData {
    pub albedo: Vec3,
    pub density: f32,

    pub transmission: f32,
    pub emissiveness: f32,
    pub reflectiveness: f32,
    pub metallic: f32,

    pub scattering: Vec3,
    pub roughness: f32,

    pub absorption: Vec3,
    pub anisotropy: f32,
}
const AIR_SCATTERING_MULT: f32 = 1.0;
const AIR_SCATTERING: Vec3 = vec3(
    0.175 * AIR_SCATTERING_MULT,
    0.41 * AIR_SCATTERING_MULT,
    1.0 * AIR_SCATTERING_MULT
);
// const AIR_COLOR: Vec3 = vec3(0.3,0.3,0.35);
const AIR_COLOR: Vec3 = vec3(0.0,0.0,0.0);

fn remap_density(raw: f32, threshold: f32, sharpness: f32) -> f32 {
    // threshold: where the "surface" of the cloud is (0..1)
    // sharpness: how steep the edge is (1 = soft, 10 = very crisp)
    let x = (raw - threshold) * sharpness;
    (x / (1.0 + x.abs())).mul_add(0.5, 0.5).clamp(0.0, 1.0)
}

// Helper function to remap a value from one range to another, clamped between 0 and 1
fn remap(value: f32, original_min: f32, original_max: f32, new_min: f32, new_max: f32) -> f32 {
    let t = (value - original_min) / (original_max - original_min);
    (new_min + t * (new_max - new_min)).clamp(0.0, 1.0)
}

/// Calculates the cloud density at a given 2D coordinate.
pub fn sample_cloud_density1(p: Vec2, base_mult: f32, detail_mult: f32, base_noise: &FastNoiseLite, erosion_noise: &FastNoiseLite, cloud_coverage: f32) -> f32 {
    // 1. Position-based scale adjustments
    let base_coord = p * base_mult;
    let detail_coord = p * detail_mult;

    let base_fbm = (base_noise.get_noise_2d(base_coord.x, base_coord.y) + 1.0) / 2.0;

    let base_density = remap(base_fbm, 1.0 - cloud_coverage, 1.0, 0.0, 1.0);
    if base_density <= 0.0 {
        return 0.0;
    }

    let mut worley_noise = (erosion_noise.get_noise_2d(detail_coord.x, detail_coord.y) + 1.0) / 2.0;
    worley_noise = remap(worley_noise, 0.0, 1.0, 0.0, base_density);

    // 5. Erode the edges of the cloud
    // We only want to erode where the cloud is already thin (at the edges).
    // If base_density is high (inner core), erosion has less effect.
    let mut erosion_modifier = remap(base_density, 0.0, 0.3, 1.0, 0.0);
    erosion_modifier = 1.0;

    // Carve into the base density
    let final_density = base_density - (worley_noise * erosion_modifier);
    // final_density = f32::clamp(final_density,0.0, 1.0);

    // 6. Final Polish: Apply a tight contrast curve to sharpen the wispy boundaries
    // This stops it from looking like a blurry gradient.
    // final_density = remap(final_density, 0.1, 0.7, 0.0, 1.0);

    // if(final_density<0.0 || final_density>1.0){
    //     println!("wtf {:?}",final_density);
    // }
    // final_density = 0.1;

    final_density
}

pub fn get_scene_data(scene_image: &RgbaImage, t: f32) -> Vec<PixelData>{
    let width = scene_image.width();
    let height = scene_image.height();
    println!("width {:?} height {:?}",width,height);
    let mut pixel_data = generate_pixel_data(scene_image);
    // let mut pixel_data = Vec::with_capacity((width*height) as usize);
    // let mut pixel_data = vec![generate_single_pixel(0,0,0,0); (width*height) as usize];
    // pixel_data.fill();
    let mut fbm_noise = FastNoiseLite::with_seed(0);
    fbm_noise.set_noise_type(Some(NoiseType::Perlin));
    fbm_noise.set_fractal_type(Some(FractalType::FBm));
    fbm_noise.set_fractal_octaves(Some(4));
    fbm_noise.set_fractal_lacunarity(Some(2.0));
    fbm_noise.set_fractal_gain(Some(0.5)); // "Gain" is another word for persistence
    let mut worley_noise = FastNoiseLite::with_seed(0);
    worley_noise.set_noise_type(Some(NoiseType::Cellular));
    worley_noise.set_fractal_type(Some(FractalType::FBm));
    worley_noise.set_fractal_octaves(Some(4));
    worley_noise.set_fractal_lacunarity(Some(2.0));
    worley_noise.set_fractal_gain(Some(0.5)); // "Gain" is another word for persistence


    for x in 0..width-0 {
        // for y in height-000..height-00 {
        for y in height-100..height-00 {
        // for y in 0..height-000 {
            let pixel = pixel_data.get_mut((x+(y*width)) as usize).unwrap();
            if(pixel.density>=1.0){
                continue;
            }
            // let fbm_value = (fbm(vec2(x as f32, y as f32)/50.0, 1.0) / (num_octaves as f32*2.0f32))+0.5;
            let pos_mult: f32 = 0.5;
            // let fbm_value = (noise.get_noise_2d(x as f32 * pos_mult,y as f32 * pos_mult)+1.0)/2.0;
            // let x_mult = f32::lerp(1.0,2.0,(height-y) as f32/100.0);
            let x_mult = 1.0;
            let fbm_value = (fbm_noise.get_noise_2d((x as f32 * x_mult) * pos_mult, y as f32 * pos_mult)+1.0)/2.0;
                // pixel_data[(x+(y*width)) as usize].transmission=0.985;
            let cutoff = 0.5;
            if(fbm_value<cutoff){
                continue;
            }else{
                // continue;
            }
            let mut normalized_post_cutoff = ((fbm_value-cutoff)/(1.0-cutoff));
            // normalized_post_cutoff = sample_cloud_density1(vec2(x as f32,y as f32), 0.25, 8.0, &fbm_noise, &worley_noise, 0.4);
            // let mut normalized_post_cutoff = remap_density(fbm_value, cutoff, 3.0);
            // normalized_post_cutoff = (normalized_post_cutoff * 16.0).round()/16.0;

            // pixel.transmission=0.96;
            pixel.density=0.0+(normalized_post_cutoff/10.0);

            // pixel.scattering = Vec3::lerp(AIR_SCATTERING,vec3(1.0,1.0,1.0), normalized_post_cutoff);
            pixel.scattering = vec3(1.0,1.0,1.0);
            pixel.albedo = Vec3::lerp(AIR_COLOR,vec3(1.0,1.0,1.0),normalized_post_cutoff);
        }
    }
    return pixel_data;
}

trait EachMaxMin {
    fn each_max(&self, rhs: f32) -> Self;
    fn each_min(&self, rhs: f32) -> Self;
}
impl EachMaxMin for Vec3 {
    fn each_max(&self, rhs: f32) -> Self {
        return vec3(self.x.max(rhs),self.y.max(rhs),self.z.max(rhs));
    }
    fn each_min(&self, rhs: f32) -> Self {
        return vec3(self.x.min(rhs),self.y.min(rhs),self.z.min(rhs));
    }
}

pub fn generate_single_pixel(r: u8, g: u8, b: u8, a: u8) -> PixelData{
    let mut pixel_data: PixelData = PixelData {
        albedo: vec3(r as f32, g as f32, b as f32) / 255.0,
        density : 1.0,

        transmission: 0.0,
        // a: a as f32 / 255.,
        emissiveness: 0.0,
        reflectiveness: 0.0,
        metallic: 0.0,

        roughness: 0.8,
        scattering: vec3(0.0,0.0,0.0),

        anisotropy: 1.0,
        absorption: vec3(0.0,0.0,0.0),
    };
    if(a==128){
        pixel_data.emissiveness = 30.0;
        pixel_data.density = 1.0;
        pixel_data.transmission = 1.0;
        // pixel_data.reflectiveness = 0.0;
        // pixel_data.r = 1.0;
        // pixel_data.g = 1.0;
        // pixel_data.b = 1.0;
        // pixel_data.transmission = 0.0;
    }else if(a==0){
        pixel_data.density = 0.01;
        pixel_data.transmission = 1.0;
        pixel_data.reflectiveness = 0.0;
        pixel_data.scattering = AIR_SCATTERING;
        pixel_data.albedo = AIR_COLOR;
    }else{
        // pixel_data.transmission = 0.1;
        // pixel_data.reflectiveness= 0.99;
        pixel_data.density = 1.0;
        pixel_data.transmission = 0.6;
        pixel_data.reflectiveness= 0.0;
    }
    return pixel_data;
}