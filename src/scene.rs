use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};
use glam::{vec2, vec3, Vec2, Vec3};
use image::RgbaImage;
use crate::glutils::generate_pixel_data;
use crate::renderer::PixelData;

pub fn get_scene_data(scene_image: &RgbaImage, t: f32) -> Vec<PixelData>{
    let width = scene_image.width();
    let height = scene_image.height();
    let mut pixel_data = generate_pixel_data(scene_image);
    let mut noise = FastNoiseLite::with_seed(0);
    noise.set_noise_type(Some(NoiseType::OpenSimplex2));
    noise.set_fractal_type(Some(FractalType::FBm));
    noise.set_fractal_octaves(Some(4));
    noise.set_fractal_lacunarity(Some(2.0));
    noise.set_fractal_gain(Some(0.5)); // "Gain" is another word for persistence

    for x in 0..width-0 {
        for y in height-50..height-20 {
            let pixel = pixel_data.get_mut((x+(y*width)) as usize).unwrap();
            // let fbm_value = (fbm(vec2(x as f32, y as f32)/50.0, 1.0) / (num_octaves as f32*2.0f32))+0.5;
            let fbm_value = (noise.get_noise_2d(x as f32,y as f32)+1.0)/2.0;
                // pixel_data[(x+(y*width)) as usize].transmission=0.985;
            let cutoff = 0.5;
            if(fbm_value<cutoff){
                continue;
            }

            // pixel.transmission=0.96;
            pixel.transmission=1.0-(fbm_value/100.0);
            pixel.set_color(&vec3(1.0,1.0,1.0));
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