use std::collections::HashMap;
use std::ffi::CString;
use std::ops::Deref;
use std::os::raw::c_void;
use std::path::{Path, PathBuf};
use gl::types::GLfloat;
use glutin::config::GetGlConfig;
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use crate::{gl, glutils};
use crate::gl::types::{GLint, GLsizeiptr, GLuint};
use image::RgbaImage;
use std::mem;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use winit::event::KeyEvent;
use crate::glutils::{create_texture, generate_light_data, load_image};
use crate::renderer::Configurable::{
    Cloud3DDepth, Cloud3DSamples, CloudBaseMult, CloudCoverage, CloudCutoff,
    CloudDetailMult, CloudErosionModifier, CloudFBMGain, CloudFBMLacunarity,
    CloudFBMOctaves, CloudWorleyGain, CloudWorleyLacunarity, CloudWorleyOctaves,
    CloudXOffset, CloudYOffset,
};
use crate::renderer::ConfigurableValue::{F32, I32};
use crate::scene::PixelData;

// Tile constants — must match assets/common.glsl
pub const TILE_W: u32 = 16;
pub const TILE_H: u32 = 16;
pub const MAX_RAYS: usize = 800_00;
pub const RAYS_PER_FRAME: usize = 640_00;
pub const SUN_RAYS_PER_FRAME: usize = 160_00;
// MUST BE EVEN TO KEEP PING PONG PONGING
pub const MAX_WAVES: usize = 64;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct RayGpu {
    pub position: [f32; 2],   // offset 0
    pub direction: [f32; 2],  // offset 8
    pub energy: [f32; 3],     // offset 16 (vec3 std430 align 16)
    pub type_and_tile_id: u32,         // offset 28
}

#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
enum Bindings {
    Fb0Tex,
    ComputeAccuR,
    ComputeAccuG,
    ComputeAccuB,

    Scene,
    LightList,
    SortedRays,
    TileStart,
    TileCount,
    TileWriteCursor,
    QueueA,
    QueueACount,
    QueueB,
    QueueBCount,
}
impl Bindings {
    fn get_id(&self) -> String {
        match self{
            Bindings::Fb0Tex => "fb0tex",
            Bindings::Scene => "scene",
            Bindings::ComputeAccuR => "computeAccuR",
            Bindings::ComputeAccuG => "computeAccuG",
            Bindings::ComputeAccuB => "computeAccuB",
            Bindings::LightList => "lightList",
            Bindings::SortedRays => "sortedRays",
            Bindings::TileStart => "tileStart",
            Bindings::TileCount => "tileCount",
            Bindings::QueueA => "queueA",
            Bindings::QueueACount => "queueACount",
            Bindings::QueueB => "queueB",
            Bindings::QueueBCount => "queueBCount",
            Bindings::TileWriteCursor => "tileWriteCursor",
        }.to_owned()
    }
    fn get_binding(&self) -> GLuint {
        match self{
            Bindings::Fb0Tex => 0,
            Bindings::ComputeAccuR => 1,
            Bindings::ComputeAccuG => 2,
            Bindings::ComputeAccuB => 3,

            Bindings::Scene => 0,
            Bindings::LightList => 1,
            Bindings::SortedRays => 2,
            Bindings::TileStart => 3,
            Bindings::TileCount => 4,
            Bindings::TileWriteCursor => 5,

            Bindings::QueueA => 0,          //binded dynamically ping pong
            Bindings::QueueACount => 0,     //binded dynamically ping pong
            Bindings::QueueB => 0,          //binded dynamically ping pong
            Bindings::QueueBCount => 0,     //binded dynamically ping pong
        }
    }
}

// Fixed SSBO binding slots for the ping-pong ray queues. Shaders declare these
// with explicit `layout(binding = N)` and Rust calls BindBufferBase per wave to
// swap which physical buffer occupies each role.
const SLOT_INPUT_QUEUE:  GLuint = 6;
const SLOT_INPUT_COUNT:  GLuint = 7;
const SLOT_OUTPUT_QUEUE: GLuint = 8;
const SLOT_OUTPUT_COUNT: GLuint = 9;


#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
pub enum Uniform {
    Frame,
    LightCount,
    TimeOfDay,
    SunMode,
    RealTime,
    RaysToEmit,
}
impl Uniform {
    fn get_id(&self) -> &str {
        match self{
            Uniform::Frame => "frame",
            Uniform::LightCount => "lightCount",
            Uniform::TimeOfDay => "timeOfDay",
            Uniform::SunMode => "sunMode",
            Uniform::RealTime => "realTime",
            Uniform::RaysToEmit => "raysToEmit",
        }
    }

    fn get_location(&self) -> GLuint {
        match self{
            Uniform::Frame => 0,
            Uniform::LightCount => 1,
            Uniform::TimeOfDay => 2,
            Uniform::SunMode => 3,
            Uniform::RealTime => 4,
            Uniform::RaysToEmit => 5,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
enum Program {
    Raytracing,
    StretchedDisplay,
    ComputeRaytracing,
    TilePass,
    RayGenerator,
    Count,
    Scan,
    Scatter,
    ClampCount,
    FragTest,
    Accumulation,
}

// Define the light point data structure
#[repr(C)] // Ensure C-style memory layout for OpenGL compatibility
#[derive(Debug, Clone, Copy)]
pub struct LightPoint {
    // We are using a simple 2D grid, so we only need x, y, and intensity.
    // However, GLSL std140 layout often requires Vec4 for alignment.
    // For simplicity, we'll use 4-byte aligned components.

    // Position (2D coordinates)
    pub x: f32,
    pub y: f32,

    // Intensity/Emissivity
    pub padding_0: f32,

    // Padding to ensure Vec4 alignment (16 bytes total)
    pub padding_1: f32,
}

// A helper type for the list of lights
pub const MAX_LIGHTS: usize = 1024;


/// value, default, min, max
pub enum ConfigurableValue{
    F32(f32, f32, f32),
    I32(i32, i32, i32)
}

#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
pub enum Configurable {
    CloudXOffset,
    CloudYOffset,
    CloudBaseMult,
    CloudDetailMult,
    Cloud3DSamples,
    Cloud3DDepth,
    CloudCoverage,
    CloudErosionModifier,
    CloudFBMOctaves,
    CloudFBMLacunarity,
    CloudFBMGain,
    CloudWorleyOctaves,
    CloudWorleyLacunarity,
    CloudWorleyGain,
    CloudCutoff,
}

pub struct Renderer {
    vao: gl::types::GLuint,
    // vbo: gl::types::GLuint,
    fb0: gl::types::GLuint,
    gl: gl::Gl,
    programs: HashMap<Program, GLuint>,
    program_uniforms: HashMap<Program, std::collections::HashSet<Uniform>>,
    bindings: HashMap<String, (Bindings,GLuint)>,
    frame: i32,
    last_frame: u64,
    start_time: Instant,
    scene: RgbaImage,
    scene_mapped_ptr: *mut c_void,
    pixel_data: Vec<PixelData>,
    light_data: Vec<LightPoint>,
    nearest_sampler: gl::types::GLuint,
    tiles_per_row: u32,
    tile_count: usize,
    pub configurables: HashMap<Configurable,ConfigurableValue>,
    pub time_of_day_override: Option<f32>,
    pub sun_rays_override: Option<u32>,
    pub emissive_rays_override: Option<u32>,
}

impl Renderer {
    pub fn get_f32(&self, configurable: Configurable) -> f32 {
        match self.configurables.get(&configurable) {
            Some(ConfigurableValue::F32(value, _, _)) => *value,
            _ => panic!("Invalid configurable value"),
        }
    }
    pub fn get_i32(&self, configurable: Configurable) -> i32 {
        match self.configurables.get(&configurable) {
            Some(ConfigurableValue::I32(value, _, _)) => *value,
            _ => panic!("Invalid configurable value"),
        }
    }

    pub fn get_f32_range(&self, key: Configurable) -> (f32, f32, f32) {
        match self.configurables.get(&key) {
            Some(ConfigurableValue::F32(v, min, max)) => (*v, *min, *max),
            _ => panic!("not an f32 configurable"),
        }
    }

    pub fn get_i32_range(&self, key: Configurable) -> (i32, i32, i32) {
        match self.configurables.get(&key) {
            Some(ConfigurableValue::I32(v, min, max)) => (*v, *min, *max),
            _ => panic!("not an i32 configurable"),
        }
    }

    pub fn set_configurable_f32(&mut self, key: Configurable, value: f32) {
        if let Some(ConfigurableValue::F32(v, _, _)) = self.configurables.get_mut(&key) {
            *v = value;
        }
    }

    pub fn set_configurable_i32(&mut self, key: Configurable, value: i32) {
        if let Some(ConfigurableValue::I32(v, _, _)) = self.configurables.get_mut(&key) {
            *v = value;
        }
    }

    pub fn regenerate_scene(&mut self) {
        let scene = self.scene.clone();
        self.pixel_data = self.get_scene_data(&scene, 0.0);
        unsafe {
            let total_size = scene.width() as usize * scene.height() as usize * mem::size_of::<PixelData>();
            std::ptr::copy_nonoverlapping(
                self.pixel_data.as_ptr() as *const c_void,
                self.scene_mapped_ptr,
                total_size,
            );
        }
        self.light_data = generate_light_data(scene.width(), scene.height(), &self.pixel_data);
    }

    pub fn new<D: GlDisplay>(gl_display: &D) -> Self {
        unsafe {
            let gl = gl::Gl::load_with(|symbol| {
                let symbol = CString::new(symbol).unwrap();
                gl_display.get_proc_address(symbol.as_c_str()).cast()
            });
            gl.Enable(gl::DEBUG_OUTPUT);
            gl.Enable(gl::DEBUG_OUTPUT_SYNCHRONOUS);
            gl.DebugMessageCallback(Some(glutils::debug_callback), std::ptr::null());
            gl.DebugMessageControl(
                gl::DONT_CARE,
                gl::DONT_CARE,
                gl::DONT_CARE,
                0,
                std::ptr::null(),
                gl::TRUE,
            );

            if let Some(renderer) = glutils::get_gl_string(&gl, gl::RENDERER) {
                println!("Running on {}", renderer.to_string_lossy());
            }
            if let Some(version) = glutils::get_gl_string(&gl, gl::VERSION) {
                println!("OpenGL Version {}", version.to_string_lossy());
            }

            if let Some(shaders_version) = glutils::get_gl_string(&gl, gl::SHADING_LANGUAGE_VERSION) {
                println!("Shaders version on {}", shaders_version.to_string_lossy());
            }

            let mut programs: HashMap<Program, GLuint> = HashMap::new();
            // programs.insert(Program::Raytracing, glutils::create_program(&gl, &PathBuf::from("assets/stub_fullscreen.glsl"), &PathBuf::from("assets/raytracingfrag.glsl")) as GLuint);
            programs.insert(Program::StretchedDisplay, glutils::create_program(&gl, &PathBuf::from("assets/stub_fullscreen.glsl"), &PathBuf::from("assets/texturedisplayer.glsl")) as GLuint);
            programs.insert(Program::FragTest, glutils::create_program(&gl, &PathBuf::from("assets/stub_fullscreen.glsl"), &PathBuf::from("assets/frag.glsl")) as GLuint);
            programs.insert(Program::Accumulation, glutils::create_program(&gl, &PathBuf::from("assets/stub_fullscreen.glsl"), &PathBuf::from("assets/accumulation.frag")) as GLuint);

            // println!("the original0 is {}",gl.GetError());
            let mut vao = std::mem::zeroed();
            // println!("the original1 is {}",gl.GetError());
            gl.GenVertexArrays(1, &mut vao);
            // println!("the original2 is {}",gl.GetError());
            gl.BindVertexArray(vao);

            println!("the original1 is {}",gl.GetError());


            let mut nearest_sampler = std::mem::zeroed();
            gl.GenSamplers(1, &mut nearest_sampler);
            gl.SamplerParameteri(nearest_sampler, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl.SamplerParameteri(nearest_sampler, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            // gl.SamplerParameteri(nearest_sampler, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);


            let scene = load_image(Path::new("assets/scene640x360test.png"));
            // let scene = load_image(Path::new("assets/blank3840x2160.png"));
            let width = scene.width();
            let height = scene.height();
            // println!("count1 and 2 {} {}",count1, count2);
            println!("the original2 is {}",gl.GetError());


            let mut bindings: HashMap<String, (Bindings,GLuint)> = HashMap::new();

            // bindings.insert(Bindings::Scene, create_texture(&gl, width, height, gl::RGBA32UI, Some(rgba_processed.as_ptr() as *const c_void)));
            println!("the original2.1 is {}",gl.GetError());

            let mut fb0 = std::mem::zeroed();
            gl.GenFramebuffers(1, &mut fb0);
            gl.BindFramebuffer(gl::FRAMEBUFFER, fb0);

            bindings.insert(Bindings::Fb0Tex.get_id(), (Bindings::Fb0Tex, create_texture(&gl, width, height, gl::RGBA32F, None)));
            gl.FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, bindings.get(&Bindings::Fb0Tex.get_id()).unwrap().1, 0);

            println!("the original2.2 is {}",gl.GetError());

            bindings.insert(Bindings::ComputeAccuR.get_id(), (Bindings::ComputeAccuR,create_texture(&gl, width, height, gl::R32UI, None)));
            bindings.insert(Bindings::ComputeAccuG.get_id(), (Bindings::ComputeAccuG,create_texture(&gl, width, height, gl::R32UI, None)));
            bindings.insert(Bindings::ComputeAccuB.get_id(), (Bindings::ComputeAccuB,create_texture(&gl, width, height, gl::R32UI, None)));

            println!("the original2.3 is {}",gl.GetError());




            let mut ubo_id: GLuint = 0;
            gl.GenBuffers(1, &mut ubo_id);
            gl.BindBuffer(gl::UNIFORM_BUFFER, ubo_id);
            // 2. Allocate the storage once (MAX_LIGHTS * size_of<LightPoint>)
            let size = (MAX_LIGHTS * mem::size_of::<LightPoint>()) as GLsizeiptr;
            gl.BufferData(gl::UNIFORM_BUFFER, size, std::ptr::null(), gl::DYNAMIC_DRAW);
            bindings.insert(Bindings::LightList.get_id(), (Bindings::LightList,ubo_id));

            println!("the original2.4 is {}",gl.GetError());

            let mut ssbo_id: GLuint = 0;
            let total_size_bytes = (width * height * mem::size_of::<PixelData>() as u32) as GLsizeiptr;
            let mut scene_mapped_ptr: *mut c_void = std::ptr::null_mut();

            // 1. Generate the SSBO
            gl.GenBuffers(1, &mut ssbo_id);
            gl.BindBuffer(gl::SHADER_STORAGE_BUFFER, ssbo_id);

            // 2. Allocate the storage and set mapping flags (Persistent/Coherent)
            // This is the fastest update method if supported (OpenGL 4.4+)
            let flags = gl::MAP_WRITE_BIT | gl::MAP_PERSISTENT_BIT | gl::MAP_COHERENT_BIT;

            gl.BufferStorage(
                gl::SHADER_STORAGE_BUFFER,
                total_size_bytes,
                std::ptr::null(), // Allocate without initial data
                flags
            );

            // 3. Persistently map the buffer memory
            scene_mapped_ptr = gl.MapBufferRange(
                gl::SHADER_STORAGE_BUFFER,
                0,
                total_size_bytes,
                flags
            );
            println!("the original2.5 is {}",gl.GetError());

            if scene_mapped_ptr.is_null() {
                eprintln!("ERROR: Failed to persistently map SSBO memory.");
                // Handle error
            }

            bindings.insert(Bindings::Scene.get_id(), (Bindings::Scene,ssbo_id));

            // === Wavefront tile-bucket SSBOs ===
            let tiles_per_row = ((width + TILE_W - 1) / TILE_W) as usize;
            let tiles_per_col = ((height + TILE_H - 1) / TILE_H) as usize;
            let tile_count = tiles_per_row * tiles_per_col;

            let make_ssbo = |size_bytes: GLsizeiptr| -> GLuint {
                let mut id: GLuint = 0;
                gl.GenBuffers(1, &mut id);
                gl.BindBuffer(gl::SHADER_STORAGE_BUFFER, id);
                gl.BufferData(gl::SHADER_STORAGE_BUFFER, size_bytes, std::ptr::null(), gl::DYNAMIC_DRAW);
                id
            };

            let sorted_rays_id       = make_ssbo((MAX_RAYS * mem::size_of::<RayGpu>()) as GLsizeiptr);
            let tile_start_id        = make_ssbo((tile_count * mem::size_of::<u32>()) as GLsizeiptr);
            let tile_count_id        = make_ssbo((tile_count * mem::size_of::<u32>()) as GLsizeiptr);
            let tile_write_cursor_id = make_ssbo((tile_count * mem::size_of::<u32>()) as GLsizeiptr);
            let queue_a_id           = make_ssbo((MAX_RAYS * mem::size_of::<RayGpu>()) as GLsizeiptr);
            let queue_a_count_id     = make_ssbo(mem::size_of::<u32>() as GLsizeiptr);
            let queue_b_id           = make_ssbo((MAX_RAYS * mem::size_of::<RayGpu>()) as GLsizeiptr);
            let queue_b_count_id     = make_ssbo(mem::size_of::<u32>() as GLsizeiptr);

            bindings.insert(Bindings::SortedRays.get_id(),      (Bindings::SortedRays,      sorted_rays_id));
            bindings.insert(Bindings::TileStart.get_id(),       (Bindings::TileStart,       tile_start_id));
            bindings.insert(Bindings::TileCount.get_id(),       (Bindings::TileCount,       tile_count_id));
            bindings.insert(Bindings::TileWriteCursor.get_id(), (Bindings::TileWriteCursor, tile_write_cursor_id));
            bindings.insert(Bindings::QueueA.get_id(),          (Bindings::QueueA,          queue_a_id));
            bindings.insert(Bindings::QueueACount.get_id(),     (Bindings::QueueACount,     queue_a_count_id));
            bindings.insert(Bindings::QueueB.get_id(),          (Bindings::QueueB,          queue_b_id));
            bindings.insert(Bindings::QueueBCount.get_id(),     (Bindings::QueueBCount,     queue_b_count_id));

            println!("tile_count = {} ({}x{} tiles)", tile_count, tiles_per_row, tiles_per_col);

            // let compute_shader = glutils::create_shader(&gl, gl::COMPUTE_SHADER, &PathBuf::from("assets/raytrace.comp"));


            // let compute_program = gl.CreateProgram();
            // gl.AttachShader(compute_program, compute_shader);
            println!("the original-4 is {}",gl.GetError());
            // gl.LinkProgram(compute_program);
            println!("the original-2 is {}",gl.GetError());
            // programs.insert(Program::ComputeRaytracing, compute_program);

            // === wavefront compute programs ===
            let load_compute = |path: &str| -> GLuint {
                let sh = glutils::create_shader(&gl, gl::COMPUTE_SHADER, &PathBuf::from(path));
                let p = gl.CreateProgram();
                gl.AttachShader(p, sh);
                gl.LinkProgram(p);
                p
                // return glutils::create_compute_shader_as_program(&gl, &PathBuf::from(path));
            };
            programs.insert(Program::TilePass,     load_compute("assets/tile_pass.comp"));
            programs.insert(Program::RayGenerator, load_compute("assets/raygenerator.comp"));
            programs.insert(Program::Count,        load_compute("assets/count.comp"));
            programs.insert(Program::Scan,         load_compute("assets/scan.comp"));
            programs.insert(Program::Scatter,      load_compute("assets/scatter.comp"));
            programs.insert(Program::ClampCount,   load_compute("assets/clamp_count.comp"));





            // gl.BindTextureUnit(0, scene_tex);
            // gl.BindSampler(0, nearest_sampler);
            // gl.BindTextureUnit(1, fb0tex);
            // gl.BindSampler(1, nearest_sampler);

            println!("the original2.2 is {}",gl.GetError());

            // Bind every resource to its hardcoded slot from Bindings::get_binding().
            // The shaders declare `layout(binding = N)` against the matching constant
            // in common.glsl, so no reflection / per-program patching is needed.
            for (_, value) in bindings.iter() {
                let kind = value.0;
                let id = value.1;
                let slot = kind.get_binding();
                match kind {
                    Bindings::Fb0Tex => {
                        // Sampler-only access from fragment shaders.
                        gl.BindTextureUnit(slot, id);
                        gl.BindSampler(slot, nearest_sampler);
                    }
                    Bindings::ComputeAccuR | Bindings::ComputeAccuG | Bindings::ComputeAccuB => {
                        // Used as image2D in compute + accumulation passes.
                        gl.BindImageTexture(slot, id, 0, gl::FALSE, 0, gl::READ_WRITE, gl::R32UI);
                    }
                    Bindings::LightList => {
                        gl.BindBufferBase(gl::UNIFORM_BUFFER, slot, id);
                    }
                    Bindings::Scene
                    | Bindings::SortedRays
                    | Bindings::TileStart
                    | Bindings::TileCount
                    | Bindings::TileWriteCursor => {
                        gl.BindBufferBase(gl::SHADER_STORAGE_BUFFER, slot, id);
                    }
                    // Ping-pong queues bound per wave via SLOT_* constants.
                    Bindings::QueueA | Bindings::QueueB | Bindings::QueueACount | Bindings::QueueBCount => {}
                }
            }

            // For each program, record which named uniforms it actually declares.
            // ProgramUniform* with a location not present in the target program is
            // a GL error, so we filter on this set when broadcasting uniforms.
            let mut program_uniforms: HashMap<Program, std::collections::HashSet<Uniform>> = HashMap::new();
            for (program_name, program) in programs.iter() {
                let mut set = std::collections::HashSet::new();
                for u in [Uniform::Frame, Uniform::LightCount, Uniform::TimeOfDay, Uniform::SunMode, Uniform::RealTime, Uniform::RaysToEmit] {
                    let cname = CString::new(u.get_id()).unwrap();
                    let loc = gl.GetUniformLocation(*program, cname.as_ptr());
                    if loc != -1 { set.insert(u); }
                }
                program_uniforms.insert(*program_name, set);
            }

            println!("the original3 is {}",gl.GetError());

            // Suppress low-priority notifications (e.g. buffer memory placement hints
            // from the driver). Errors, warnings, and performance messages still pass.
            gl.DebugMessageControl(
                gl::DONT_CARE,
                gl::DONT_CARE,
                gl::DEBUG_SEVERITY_NOTIFICATION,
                0,
                std::ptr::null(),
                gl::FALSE,
            );

            let mut configurables = HashMap::new();
            configurables.insert(CloudFBMOctaves,        I32(4,    1,    12));
            configurables.insert(CloudFBMLacunarity,     F32(2.0,  0.5,  8.0));
            configurables.insert(CloudFBMGain,           F32(0.5,  0.0,  1.0));
            configurables.insert(CloudWorleyOctaves,     I32(4,    1,    8));
            configurables.insert(CloudWorleyLacunarity,  F32(2.0,  0.5,  8.0));
            configurables.insert(CloudWorleyGain,        F32(0.75, 0.0,  1.0));
            configurables.insert(CloudCoverage,          F32(0.5,  0.0,  1.0));
            configurables.insert(CloudBaseMult,          F32(0.25, 0.01, 5.0));
            configurables.insert(CloudDetailMult,        F32(6.0,  0.1,  20.0));
            configurables.insert(Cloud3DSamples,         I32(64,   1,    128));
            configurables.insert(Cloud3DDepth,           F32(2.0,  0.1,  10.0));
            configurables.insert(CloudErosionModifier,   F32(0.25, 0.0,  2.0));
            configurables.insert(CloudXOffset,           F32(0.0,  -500.0, 500.0));
            configurables.insert(CloudYOffset,           F32(0.0,  -500.0, 500.0));
            configurables.insert(CloudCutoff,            F32(0.5,  0.0,  1.0));

            let mut renderer = Self {
                programs,
                program_uniforms,
                bindings: bindings,
                vao,
                fb0,
                gl,
                frame: 0,
                last_frame: 0,
                start_time: Instant::now(),
                scene,
                scene_mapped_ptr,
                // pixel_data,
                pixel_data: Vec::new(),
                // light_data,
                light_data: Vec::new(),
                nearest_sampler,
                tiles_per_row: tiles_per_row as u32,
                tile_count,
                configurables,
                time_of_day_override: None,
                sun_rays_override: None,
                emissive_rays_override: None,
            };


            renderer.pixel_data = renderer.get_scene_data(&renderer.scene, 0.0);

            renderer.light_data = generate_light_data(renderer.scene.width(), renderer.scene.height(), &renderer.pixel_data);


            return renderer;
            // Self { program, gl }
        }
    }


    pub unsafe fn update_uniforms(&self, light_count: u32){
        self.update_uniformi(&Uniform::Frame, self.frame);
        self.update_uniformi(&Uniform::LightCount, light_count as i32);
    }

    pub unsafe fn update_uniformi(&self, uniform: &Uniform, value: i32){
        let loc = uniform.get_location() as GLint;
        for (name, program) in self.programs.iter() {
            if self.program_uniforms.get(name).map_or(false, |s| s.contains(uniform)) {
                self.gl.ProgramUniform1i(*program, loc, value);
            }
        }
    }
    pub unsafe fn update_uniformf(&self, uniform: &Uniform, value: f32){
        let loc = uniform.get_location() as GLint;
        for (name, program) in self.programs.iter() {
            if self.program_uniforms.get(name).map_or(false, |s| s.contains(uniform)) {
                self.gl.ProgramUniform1f(*program, loc, value);
            }
        }
    }
    pub unsafe fn update_uniformu(&self, uniform: &Uniform, value: u32){
        let loc = uniform.get_location() as GLint;
        for (name, program) in self.programs.iter() {
            if self.program_uniforms.get(name).map_or(false, |s| s.contains(uniform)) {
                self.gl.ProgramUniform1ui(*program, loc, value);
            }
        }
    }

    pub fn draw(&mut self) {
        let new_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        let fps = 1000.0/((new_time-self.last_frame) as f64);
        println!("fps is {:?} frame is {:?}",fps, self.frame);
        self.last_frame=new_time;
        //if(fps <= 120.0){
            self.draw_with_clear_color(0.0, 0.0, 0.0, 0.7);
            self.frame=self.frame+1;
        //}
    }
    pub fn on_key_press(&mut self, event: &KeyEvent) {
        match event {
            KeyEvent { physical_key: winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowUp), .. } => {
                // self.
            },
            _ => {}
        }
    }

    pub fn draw_with_clear_color(
        &self,
        red: GLfloat,
        green: GLfloat,
        blue: GLfloat,
        alpha: GLfloat,
    ) {
        unsafe {
            // Restore texture unit 0 binding that egui_glow clobbers each frame via
            // glBindTexture(). Without this, FragTest samples the egui font atlas.
            let fb0tex_id = self.bindings.get(&Bindings::Fb0Tex.get_id()).unwrap().1;
            self.gl.BindTextureUnit(Bindings::Fb0Tex.get_binding(), fb0tex_id);
            self.gl.BindSampler(Bindings::Fb0Tex.get_binding(), self.nearest_sampler);

            let total_size = self.scene.width() as usize * self.scene.height() as usize * mem::size_of::<PixelData>();

            let ubo_binding = self.bindings.get(&Bindings::LightList.get_id()).unwrap().to_owned().1;
            self.gl.BindBuffer(gl::UNIFORM_BUFFER, ubo_binding);
            // self.gl.BufferSubData(gl::UNIFORM_BUFFER, 0, (light_data.len() * mem::size_of::<LightPoint>()) as GLsizeiptr, light_data.as_ptr() as *const c_void);
            self.gl.BufferData(gl::UNIFORM_BUFFER, (self.light_data.len() * mem::size_of::<LightPoint>()) as GLsizeiptr, self.light_data.as_ptr() as *const c_void, gl::DYNAMIC_DRAW);
            self.gl.BindBuffer(gl::UNIFORM_BUFFER, 0);

            self.update_uniforms(self.light_data.len() as u32);
            // println!("lightdata len is {:?}",light_data.len());


            // Copy the new Rust data directly into the mapped GPU memory pointer
            if(self.frame%100==0) {
                std::ptr::copy_nonoverlapping(
                    self.pixel_data.as_ptr() as *const c_void,
                    self.scene_mapped_ptr,
                    total_size,
                );
            }

            let time_elapsed = Instant::now().duration_since(self.start_time);
            self.update_uniformi(&Uniform::RealTime, time_elapsed.as_millis() as i32);
            // self.update_uniformf(&Uniform::TimeOfDay, time_elapsed.as_millis() as f32 / (24.0 * 60.0 * 60.0));
            let tod = self.time_of_day_override
                .unwrap_or_else(|| (time_elapsed.as_millis() as f32 / (10000.0 * 24.0)).fract());
            self.update_uniformf(&Uniform::TimeOfDay, tod);
            // println!("time of day is now {:?}",(time_elapsed.as_millis() as f32 / (1000.0 * 24.0)).fract());
            self.update_uniformi(&Uniform::SunMode, 0);

            // println!("the thing0 is {}",self.gl.GetError());
            // println!("the thing1 is {}",self.gl.GetError());


            self.gl.BindFramebuffer(gl::FRAMEBUFFER, 0);
            self.gl.ClearColor(red, green, blue, alpha);
            self.gl.Clear(gl::COLOR_BUFFER_BIT);

            self.gl.ClearTexImage(self.bindings.get(&Bindings::ComputeAccuR.get_id()).unwrap().1, 0, 0+gl::RED_INTEGER, 0+gl::UNSIGNED_INT, std::ptr::null());
            self.gl.ClearTexImage(self.bindings.get(&Bindings::ComputeAccuG.get_id()).unwrap().1, 0, 0+gl::RED_INTEGER, 0+gl::UNSIGNED_INT, std::ptr::null());
            self.gl.ClearTexImage(self.bindings.get(&Bindings::ComputeAccuB.get_id()).unwrap().1, 0, 0+gl::RED_INTEGER, 0+gl::UNSIGNED_INT, std::ptr::null());
            // === Wavefront pipeline: GPU raygen + GPU counting sort + tile_pass ===
            self.run_wavefront_frame();

            self.gl.UseProgram(*self.programs.get(&Program::Accumulation).unwrap());
            self.gl.BindVertexArray(self.vao);
            self.gl.BindFramebuffer(gl::FRAMEBUFFER, self.fb0);
            self.gl.DrawArrays(gl::TRIANGLES, 0, 3);


            self.gl.UseProgram(*self.programs.get(&Program::FragTest).unwrap());
            self.gl.BindVertexArray(self.vao);
            self.gl.BindFramebuffer(gl::FRAMEBUFFER, 0);
            self.gl.DrawArrays(gl::TRIANGLES, 0, 3);
        }
    }

    /// Zero out an entire SSBO using glClearBufferData (no CPU upload needed).
    unsafe fn zero_ssbo(&self, buf: GLuint) {
        self.gl.BindBuffer(gl::SHADER_STORAGE_BUFFER, buf);
        self.gl.ClearBufferData(
            gl::SHADER_STORAGE_BUFFER,
            gl::R32UI,
            gl::RED_INTEGER,
            gl::UNSIGNED_INT,
            std::ptr::null(),
        );
        self.gl.BindBuffer(gl::SHADER_STORAGE_BUFFER, 0);
    }

    /// Bind a buffer to a fixed SSBO slot.
    unsafe fn bind_slot(&self, slot: GLuint, buf: GLuint) {
        self.gl.BindBufferBase(gl::SHADER_STORAGE_BUFFER, slot, buf);
    }

    unsafe fn run_wavefront_frame(&self) {
        // Cross-frame persistence assumes the final wave's output lands back in queueA.
        // Wave i has input=A,output=B when i is even; with an even MAX_WAVES the last
        // wave (MAX_WAVES-1, odd) outputs to queueA. If this ever becomes odd, swap
        // the queue IDs at end of frame instead of zeroing.
        debug_assert!(MAX_WAVES % 2 == 0, "MAX_WAVES must be even for cross-frame ray persistence");

        let queue_a_id   = self.bindings.get(&Bindings::QueueA.get_id()).unwrap().1;
        let queue_a_cnt  = self.bindings.get(&Bindings::QueueACount.get_id()).unwrap().1;
        let queue_b_id   = self.bindings.get(&Bindings::QueueB.get_id()).unwrap().1;
        let queue_b_cnt  = self.bindings.get(&Bindings::QueueBCount.get_id()).unwrap().1;
        let tile_count_id = self.bindings.get(&Bindings::TileCount.get_id()).unwrap().1;


        // ------------------------------------------------------------------
        // Wave loop: ping-pong queueA <-> queueB.
        // ------------------------------------------------------------------
        let count_prog    = *self.programs.get(&Program::Count).unwrap();
        let scan_prog     = *self.programs.get(&Program::Scan).unwrap();
        let scatter_prog  = *self.programs.get(&Program::Scatter).unwrap();
        let tilepass_prog = *self.programs.get(&Program::TilePass).unwrap();

        let sort_wgs = ((MAX_RAYS + 255) / 256) as u32;
        let tile_wgs = self.tile_count as u32;
        self.generate_rays();

        for wave in 0..MAX_WAVES {

            let (in_q, in_c, out_q, out_c) = if wave % 2 == 0 {
                (queue_a_id, queue_a_cnt, queue_b_id, queue_b_cnt)
            } else {
                (queue_b_id, queue_b_cnt, queue_a_id, queue_a_cnt)
            };

            self.bind_slot(SLOT_INPUT_QUEUE,  in_q);
            self.bind_slot(SLOT_INPUT_COUNT,  in_c);
            self.bind_slot(SLOT_OUTPUT_QUEUE, out_q);
            self.bind_slot(SLOT_OUTPUT_COUNT, out_c);

            self.zero_ssbo(out_c);
            self.zero_ssbo(tile_count_id);
            self.gl.MemoryBarrier(gl::BUFFER_UPDATE_BARRIER_BIT | gl::SHADER_STORAGE_BARRIER_BIT);

            // Count rays per tile.
            self.gl.UseProgram(count_prog);
            self.gl.DispatchCompute(sort_wgs, 1, 1);
            self.gl.MemoryBarrier(gl::SHADER_STORAGE_BARRIER_BIT);

            // Exclusive prefix scan -> tile_start (also zeroes tile_write_cursor).
            self.gl.UseProgram(scan_prog);
            self.gl.DispatchCompute(1, 1, 1);
            self.gl.MemoryBarrier(gl::SHADER_STORAGE_BARRIER_BIT);

            // Scatter rays into sortedRays.
            self.gl.UseProgram(scatter_prog);
            self.gl.DispatchCompute(sort_wgs, 1, 1);
            self.gl.MemoryBarrier(gl::SHADER_STORAGE_BARRIER_BIT);

            // March one tile-width per ray, depositing light and emitting survivors.
            self.gl.UseProgram(tilepass_prog);
            self.gl.DispatchCompute(tile_wgs, 1, 1);
            self.gl.MemoryBarrier(
                gl::SHADER_IMAGE_ACCESS_BARRIER_BIT | gl::SHADER_STORAGE_BARRIER_BIT,
            );

            let _ = wave;
        }
    }
    unsafe fn generate_rays(&self) {
        // ------------------------------------------------------------------
        // Ray emission: append into queue A on top of any carryover from the
        // previous frame. queueA_count is NOT zeroed here — raygenerator's
        // atomicAdd extends the existing count.
        // ------------------------------------------------------------------
        self.bind_slot(SLOT_OUTPUT_QUEUE, self.bindings.get(&Bindings::QueueA.get_id()).unwrap().1);
        self.bind_slot(SLOT_OUTPUT_COUNT, self.bindings.get(&Bindings::QueueACount.get_id()).unwrap().1);
        self.gl.MemoryBarrier(gl::BUFFER_UPDATE_BARRIER_BIT | gl::SHADER_STORAGE_BARRIER_BIT);

        let raygen_prog = *self.programs.get(&Program::RayGenerator).unwrap();
        self.gl.UseProgram(raygen_prog);


        // Sun rays (sunMode = 1).
        self.update_uniformi(&Uniform::SunMode, 1);
        let sun_rays = self.sun_rays_override.unwrap_or(SUN_RAYS_PER_FRAME as u32);
        self.update_uniformu(&Uniform::RaysToEmit, sun_rays);
        let wgs = (sun_rays + 255) / 256;
        self.gl.DispatchCompute(wgs, 1, 1);

        self.gl.MemoryBarrier(gl::SHADER_STORAGE_BARRIER_BIT);

        // Emissive rays (sunMode = 0).
        self.update_uniformi(&Uniform::SunMode, 0);
        let emissive_rays = self.emissive_rays_override.unwrap_or(RAYS_PER_FRAME as u32);
        self.update_uniformu(&Uniform::RaysToEmit, emissive_rays);
        let wgs = (emissive_rays + 255) / 256;
        self.gl.DispatchCompute(wgs, 1, 1);



        // Clamp queue_a_count to MAX_RAYS. Raygenerator's slot check stops writes
        // past MAX_RAYS but the atomic counter keeps growing — left unclamped,
        // count.comp/scatter.comp would iterate over stale slots.
        let clamp_prog = *self.programs.get(&Program::ClampCount).unwrap();
        self.gl.UseProgram(clamp_prog);
        self.gl.DispatchCompute(1, 1, 1);
        self.gl.MemoryBarrier(gl::SHADER_STORAGE_BARRIER_BIT);
    }

    pub fn resize(&self, width: i32, height: i32) {
        unsafe {
            self.gl.Viewport(0, 0, width, height);
        }
    }

    pub fn set_time_of_day(&mut self, v: Option<f32>) {
        self.time_of_day_override = v;
    }

    pub fn set_sun_rays(&mut self, v: Option<u32>) {
        self.sun_rays_override = v;
    }

    pub fn set_emissive_rays(&mut self, v: Option<u32>) {
        self.emissive_rays_override = v;
    }
}

impl Deref for Renderer {
    type Target = gl::Gl;

    fn deref(&self) -> &Self::Target {
        &self.gl
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            // self.gl.DeleteProgram(self.pass1);
            // self.gl.DeleteProgram(self.pass2);
            // self.gl.DeleteProgram(self.pass3);
            // self.gl.DeleteBuffers(1, &self.vbo);
            self.gl.DeleteVertexArrays(1, &self.vao);
        }
    }
}


