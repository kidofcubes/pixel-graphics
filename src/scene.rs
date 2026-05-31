use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};
use glam::{vec3, Vec3};
use image::RgbaImage;
use crate::glutils::generate_pixel_data;


#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct PixelData {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub density: f32,

    pub transmission: f32,
    pub emissiveness: f32,
    pub reflectiveness: f32,
    pub metallic: f32,

    pub scattering: Vec3,
    pub roughness: f32,

    pub anisotropy: f32,
    pub absorption: f32,
    pub padding_0: f32, //ior later
    pub padding_1: f32,
}
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
        // for y in height-200..height-00 {
        for y in 0..height-000 {
            let pixel = pixel_data.get_mut((x+(y*width)) as usize).unwrap();
            if(pixel.density>=1.0){
                continue;
            }
            // let fbm_value = (fbm(vec2(x as f32, y as f32)/50.0, 1.0) / (num_octaves as f32*2.0f32))+0.5;
            let pos_mult: f32 = 1.5;
            let fbm_value = (noise.get_noise_2d(x as f32 * pos_mult,y as f32 * pos_mult)+1.0)/2.0;
                // pixel_data[(x+(y*width)) as usize].transmission=0.985;
            let cutoff = 0.7;
            if(fbm_value<cutoff){
                continue;
            }else{
                // continue;
            }
            let normalized_post_cutoff = ((fbm_value-cutoff)/(1.0-cutoff));

            // pixel.transmission=0.96;
            pixel.density=0.0+(normalized_post_cutoff/2.0);
            pixel.scattering = vec3(1.0,1.0,1.0);
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

pub fn generate_single_pixel(r: u8, g: u8, b: u8, a: u8) -> PixelData{
    let mut pixel_data: PixelData = PixelData {
        r: r as f32 / 255.,
        g: g as f32 / 255.,
        b: b as f32 / 255.,
        density : 1.0,

        transmission: 0.0,
        // a: a as f32 / 255.,
        emissiveness: 0.0,
        reflectiveness: 0.0,
        metallic: 0.0,

        roughness: 0.8,
        scattering: vec3(0.0,0.0,0.0),
        anisotropy: 1.0,
        absorption: 0.0,

        padding_0: 0.0,
        padding_1: 0.0,
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
        pixel_data.scattering = vec3(0.175, 0.41, 1.0);
        pixel_data.r = 0.3;
        pixel_data.g = 0.3;
        pixel_data.b = 0.35;
    }else{
        // pixel_data.transmission = 0.1;
        // pixel_data.reflectiveness= 0.99;
        pixel_data.density = 1.0;
        pixel_data.transmission = 0.6;
        pixel_data.reflectiveness= 0.0;
    }
    return pixel_data;
}


impl PixelData {
    pub fn set_color(&mut self, vec: &glam::Vec3){
        self.r = vec.x;
        self.g = vec.y;
        self.b = vec.z;
    }
}