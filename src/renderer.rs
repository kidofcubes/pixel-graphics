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
use crate::glutils::{create_texture, generate_light_data, load_image};
use crate::scene::{get_scene_data, PixelData};

// Tile constants — must match assets/common.glsl
pub const TILE_W: u32 = 16;
pub const TILE_H: u32 = 16;
pub const MAX_RAYS: usize = 800_0000;
pub const RAYS_PER_FRAME: usize = 640_0000;
pub const SUN_RAYS_PER_FRAME: usize = 160_0000;
pub const MAX_WAVES: usize = 64;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct RayGpu {
    pub position: [f32; 2],   // offset 0
    pub direction: [f32; 2],  // offset 8
    pub energy: [f32; 3],     // offset 16 (vec3 std430 align 16)
    pub tile_id: u32,         // offset 28
}

#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
enum Bindings {
    Fb0Tex,
    Scene,
    ComputeAccuR,
    ComputeAccuG,
    ComputeAccuB,
    LightList,
    SortedRays,
    TileStart,
    TileCount,
    QueueA,
    QueueACount,
    QueueB,
    QueueBCount,
    TileWriteCursor,
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
}

// Fixed SSBO binding slots for the ping-pong ray queues. Shaders declare these
// with explicit `layout(binding = N)` and Rust calls BindBufferBase per wave to
// swap which physical buffer occupies each role.
const SLOT_INPUT_QUEUE:  GLuint = 10;
const SLOT_OUTPUT_QUEUE: GLuint = 11;
const SLOT_INPUT_COUNT:  GLuint = 12;
const SLOT_OUTPUT_COUNT: GLuint = 13;


#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
pub enum Uniform {
    Frame,
    LightCount,
    TimeOfDay,
    SunMode,
    RealTime,
}
impl Uniform {
    fn get_id(&self) -> &str {
        match self{
            Uniform::Frame => "frame",
            Uniform::LightCount => "lightCount",
            Uniform::TimeOfDay => "timeOfDay",
            Uniform::SunMode => "sunMode",
            Uniform::RealTime => "realTime",
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


pub struct Renderer {
    vao: gl::types::GLuint,
    // vbo: gl::types::GLuint,
    fb0: gl::types::GLuint,
    gl: gl::Gl,
    programs: HashMap<Program, GLuint>,
    uniform_locations: HashMap<Program, HashMap<Uniform, GLint>>,
    bindings: HashMap<String, (Bindings,GLuint)>,
    frame: i32,
    last_frame: u64,
    start_time: Instant,
    scene: RgbaImage,
    scene_mapped_ptr: *mut c_void,
    pixel_data: Vec<PixelData>,
    light_data: Vec<LightPoint>,
    tiles_per_row: u32,
    tile_count: usize,
}

impl Renderer {
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


            let mut binded_bindings = HashMap::new();

            let mut texture_unit: GLuint = 0;
            let mut image_unit: GLuint = 0;
            let mut buffer_base_unit: GLuint = 0;
            let mut shader_storage_block_unit: GLuint = 0;

            for (unit, value) in bindings.iter().enumerate() {
                let mut types = HashMap::new();
                match value.1.0 {
                    Bindings::Fb0Tex | Bindings::ComputeAccuR | Bindings::ComputeAccuG | Bindings::ComputeAccuB => {
                        gl.BindImageTexture(texture_unit, value.1.1, 0, gl::FALSE, 0, gl::READ_WRITE, 0+gl::R32UI);
                        gl.BindTextureUnit(texture_unit, value.1.1);
                        gl.BindSampler(texture_unit, nearest_sampler);
                        types.insert(gl::TEXTURE_2D, texture_unit);
                        texture_unit += 1;
                    },
                    _ => {}
                }
                match value.1.0 {
                    Bindings::LightList => {
                        gl.BindBufferBase(gl::UNIFORM_BUFFER, buffer_base_unit, value.1.1);
                        types.insert(gl::UNIFORM_BLOCK, buffer_base_unit);
                        buffer_base_unit+=1;
                    },
                    _ => {}
                }
                match value.1.0 {
                    Bindings::Scene
                    | Bindings::SortedRays
                    | Bindings::TileStart
                    | Bindings::TileCount
                    | Bindings::TileWriteCursor => {
                        gl.BindBufferBase(gl::SHADER_STORAGE_BUFFER, shader_storage_block_unit, value.1.1);
                        types.insert(gl::SHADER_STORAGE_BLOCK, shader_storage_block_unit);
                        shader_storage_block_unit+=1;
                    },
                    // QueueA/B and their counts use fixed binding slots (SLOT_*_QUEUE/COUNT)
                    // managed by per-wave BindBufferBase calls below; skip them here.
                    Bindings::QueueA | Bindings::QueueB | Bindings::QueueACount | Bindings::QueueBCount => {},
                    _ => {}
                }
                println!("BINDED {:?} to {:?}",value.1.0, types);
                binded_bindings.insert(value.0.clone(), types);
            }



            let mut uniform_locations = HashMap::new();

            for (program_name,program) in programs.iter() {
                println!("=========CURRENTLY ON PROGRAM {:?} WHO IS AT {}",program_name,program);
                gl.UseProgram(*program);
                for (id, value) in binded_bindings.iter() {
                    for (gl_type, binding_point) in value.iter() {
                        // println!("the {} is in {:?} at {}", id, program_name, *binding_point);
                        match *gl_type {
                            gl::TEXTURE_2D | gl::IMAGE_2D => {
                                let loc = gl.GetUniformLocation(*program, CString::new(id.as_bytes().to_vec()).unwrap().as_ptr());
                                if (loc != -1) {
                                    println!("found a {:?} in program {:?} at location {}", id, program_name, loc);
                                    gl.Uniform1i(loc, *binding_point as GLint);
                                }else{
                                    // println!("the {} is NOT in {:?}", id, program_name);
                                }
                            },
                            gl::UNIFORM_BLOCK | gl::SHADER_STORAGE_BLOCK => {
                                let loc = gl.GetProgramResourceIndex(*program, *gl_type, CString::new(id.as_bytes().to_vec()).unwrap().as_ptr());
                                if (loc != gl::INVALID_INDEX) {
                                    println!("found a {:?} in program {:?} at location {}", id, program_name, loc);
                                    match *gl_type {
                                        gl::UNIFORM_BLOCK => {gl.UniformBlockBinding(*program, loc, *binding_point)},
                                        gl::SHADER_STORAGE_BLOCK => {gl.ShaderStorageBlockBinding(*program, loc, *binding_point)}
                                        _ => {panic!("not implemented for {:?} ???", gl_type)}
                                    }
                                }
                            },
                            _ => {panic!("not implemented for {:?}", gl_type)}
                        }
                    }
                }


                let mut uniforms = HashMap::new();
                for uniform in [Uniform::Frame, Uniform::LightCount, Uniform::RealTime, Uniform::SunMode, Uniform::TimeOfDay] {
                    // gl.GetUniforms
                    // set this up to string and all uniforms later maybe idk
                    let loc = gl.GetUniformLocation(*program, CString::new(uniform.get_id().clone().as_bytes().to_vec()).unwrap().as_ptr());
                    if(loc != -1) {
                        println!("found a {:?} in program {:?} at location {}", uniform.get_id(), program_name, loc);
                        uniforms.insert(uniform, loc as GLint);
                    }
                }
                uniform_locations.insert(*program_name, uniforms);
            }

            println!("the original3 is {}",gl.GetError());

            let pixel_data = get_scene_data(&scene, 0.0);

            let light_data = generate_light_data(scene.width(), scene.height(), &pixel_data);

            Self {
                programs,
                uniform_locations,
                bindings: bindings,
                vao,
                fb0,
                gl,
                frame: 0,
                last_frame: 0,
                start_time: Instant::now(),
                scene,
                scene_mapped_ptr,
                pixel_data,
                light_data,
                tiles_per_row: tiles_per_row as u32,
                tile_count,
            }
            // Self { program, gl }
        }
    }


    pub unsafe fn update_uniforms(&self, light_count: u32){
        for (program_name, program) in self.programs.iter() {
            self.gl.UseProgram(*program);
            if let Some(loc) = self.uniform_locations.get(program_name).unwrap().get(&Uniform::Frame) {
                self.gl.Uniform1i(*loc, self.frame);
            }
            if let Some(loc) = self.uniform_locations.get(program_name).unwrap().get(&Uniform::LightCount) {
                self.gl.Uniform1i(*loc, light_count as i32);
            }
        }
    }

    pub unsafe fn update_uniformi(&self, uniform: &Uniform, value: i32){
        for (program_name, program) in self.programs.iter() {
            self.gl.UseProgram(*program);
            if let Some(loc) = self.uniform_locations.get(program_name).unwrap().get(uniform) {
                self.gl.Uniform1i(*loc, value);
            }
        }
    }
    pub unsafe fn update_uniformf(&self, uniform: &Uniform, value: f32){
        for (program_name, program) in self.programs.iter() {
            self.gl.UseProgram(*program);
            if let Some(loc) = self.uniform_locations.get(program_name).unwrap().get(uniform) {
                self.gl.Uniform1f(*loc, value);
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

    pub fn draw_with_clear_color(
        &self,
        red: GLfloat,
        green: GLfloat,
        blue: GLfloat,
        alpha: GLfloat,
    ) {
        unsafe {
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
            self.update_uniformf(&Uniform::TimeOfDay, (time_elapsed.as_millis() as f32 / (1000.0 * 24.0)).fract());
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

    /// Set a `uint` uniform on `program` without binding it (DSA).
    unsafe fn set_uniform_u(&self, program: GLuint, name: &str, value: u32) {
        let cname = CString::new(name).unwrap();
        let loc = self.gl.GetUniformLocation(program, cname.as_ptr());
        if loc != -1 {
            self.gl.ProgramUniform1ui(program, loc, value);
        }
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
        // Ray emission: append into queue A on top of any carryover from the
        // previous frame. queueA_count is NOT zeroed here — raygenerator's
        // atomicAdd extends the existing count.
        // ------------------------------------------------------------------
        self.bind_slot(SLOT_OUTPUT_QUEUE, queue_a_id);
        self.bind_slot(SLOT_OUTPUT_COUNT, queue_a_cnt);
        self.gl.MemoryBarrier(gl::BUFFER_UPDATE_BARRIER_BIT | gl::SHADER_STORAGE_BARRIER_BIT);

        let raygen_prog = *self.programs.get(&Program::RayGenerator).unwrap();

        // Emissive rays (sunMode = 0). update_uniformi loops and clobbers the active
        // program, so re-UseProgram immediately before dispatch.
        self.update_uniformi(&Uniform::SunMode, 0);
        let emissive_rays = RAYS_PER_FRAME as u32;
        self.set_uniform_u(raygen_prog, "raysToEmit", emissive_rays);
        self.gl.UseProgram(raygen_prog);
        let wgs = (emissive_rays + 255) / 256;
        self.gl.DispatchCompute(wgs, 1, 1);

        // Sun rays (sunMode = 1).
        self.update_uniformi(&Uniform::SunMode, 1);
        let sun_rays = SUN_RAYS_PER_FRAME as u32;
        self.set_uniform_u(raygen_prog, "raysToEmit", sun_rays);
        self.gl.UseProgram(raygen_prog);
        let wgs = (sun_rays + 255) / 256;
        self.gl.DispatchCompute(wgs, 1, 1);

        self.gl.MemoryBarrier(gl::SHADER_STORAGE_BARRIER_BIT);

        // Clamp queue_a_count to MAX_RAYS. Raygenerator's slot check stops writes
        // past MAX_RAYS but the atomic counter keeps growing — left unclamped,
        // count.comp/scatter.comp would iterate over stale slots.
        let clamp_prog = *self.programs.get(&Program::ClampCount).unwrap();
        self.gl.UseProgram(clamp_prog);
        self.gl.DispatchCompute(1, 1, 1);
        self.gl.MemoryBarrier(gl::SHADER_STORAGE_BARRIER_BIT);

        // ------------------------------------------------------------------
        // Wave loop: ping-pong queueA <-> queueB.
        // ------------------------------------------------------------------
        let count_prog    = *self.programs.get(&Program::Count).unwrap();
        let scan_prog     = *self.programs.get(&Program::Scan).unwrap();
        let scatter_prog  = *self.programs.get(&Program::Scatter).unwrap();
        let tilepass_prog = *self.programs.get(&Program::TilePass).unwrap();

        let sort_wgs = ((MAX_RAYS + 255) / 256) as u32;
        let tile_wgs = self.tile_count as u32;

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

    pub fn resize(&self, width: i32, height: i32) {
        unsafe {
            self.gl.Viewport(0, 0, width, height);
        }
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


