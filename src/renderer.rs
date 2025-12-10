use std::collections::HashMap;
use std::ffi::CString;
use std::ops::Deref;
use std::os::raw::c_void;
use std::path::{Path, PathBuf};
use gl::types::GLfloat;
use glutin::config::GetGlConfig;
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use half::f16;
use crate::{gl, glutils, pack2f16, pack4u8};
use crate::gl::types::{GLint, GLsizei, GLsizeiptr, GLuint};
use image::{ImageReader, RgbaImage};
use std::mem;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::glutils::{create_texture, generate_light_data, generate_pixel_data, load_image};

#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
enum Bindings {
    Fb0Tex,
    Scene,
    ComputeAccuR,
    ComputeAccuG,
    ComputeAccuB,
    LightList,
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
        }.to_owned()
    }
}


#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
enum Uniform {
    Frame,
    LightCount
}
impl Uniform {
    fn get_id(&self) -> &str {
        match self{
            Uniform::Frame => "frame",
            Uniform::LightCount => "lightCount",
        }
    }
}

#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
enum Program {
    Raytracing,
    StretchedDisplay,
    ComputeRaytracing,
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
    pub emissiveness: f32,

    // Padding to ensure Vec4 alignment (16 bytes total)
    pub padding: f32,
}

// A helper type for the list of lights
pub const MAX_LIGHTS: usize = 1024;
pub type LightList = [LightPoint; MAX_LIGHTS];


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
    scene: RgbaImage,
    scene_mapped_ptr: *mut c_void,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct PixelData {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub opacity: f32,

    pub emissiveness: f32,
    pub reflectiveness: f32,
    pub metallic: f32,
    pub roughness: f32,

    pub transmission: f32,
    pub padding_1: f32,
    pub padding_2: f32,
    pub padding_3: f32,
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


            let scene = load_image(Path::new("assets/scene640x360.png"));
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



            let compute_shader = glutils::create_shader(&gl, gl::COMPUTE_SHADER, &PathBuf::from("assets/raytrace.comp"));


            let compute_program = gl.CreateProgram();
            gl.AttachShader(compute_program, compute_shader);
            println!("the original-4 is {}",gl.GetError());
            gl.LinkProgram(compute_program);
            println!("the original-2 is {}",gl.GetError());
            programs.insert(Program::ComputeRaytracing, compute_program);





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
                    Bindings::Scene => {
                        gl.BindBufferBase(gl::SHADER_STORAGE_BUFFER, shader_storage_block_unit, value.1.1);
                        types.insert(gl::SHADER_STORAGE_BLOCK, shader_storage_block_unit);
                        shader_storage_block_unit+=1;
                    },
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
                for uniform in [Uniform::Frame, Uniform::LightCount] {
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

            Self {
                programs,
                uniform_locations,
                bindings: bindings,
                vao,
                fb0,
                gl,
                frame: 0,
                last_frame: 0,
                scene,
                scene_mapped_ptr
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

    pub fn draw(&mut self) {
        self.draw_with_clear_color(0.0, 0.0, 0.0, 0.7);
        self.frame=self.frame+1;
        let new_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        let fps = 1000.0/((new_time-self.last_frame) as f64);
        println!("fps is {:?}",fps);
        self.last_frame=new_time;
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
            let pixel_data = generate_pixel_data(&self.scene);
            let light_data = generate_light_data(self.scene.width(),&pixel_data);

            let ubo_binding = self.bindings.get(&Bindings::LightList.get_id()).unwrap().to_owned().1;
            self.gl.BindBuffer(gl::UNIFORM_BUFFER, ubo_binding);
            // self.gl.BufferSubData(gl::UNIFORM_BUFFER, 0, (light_data.len() * mem::size_of::<LightPoint>()) as GLsizeiptr, light_data.as_ptr() as *const c_void);
            self.gl.BufferData(gl::UNIFORM_BUFFER, (light_data.len() * mem::size_of::<LightPoint>()) as GLsizeiptr, light_data.as_ptr() as *const c_void, gl::DYNAMIC_DRAW);
            self.gl.BindBuffer(gl::UNIFORM_BUFFER, 0);

            self.update_uniforms(light_data.len() as u32);
            // println!("lightdata len is {:?}",light_data.len());


            // Copy the new Rust data directly into the mapped GPU memory pointer
            std::ptr::copy_nonoverlapping(
                pixel_data.as_ptr() as *const c_void,
                self.scene_mapped_ptr,
                total_size,
            );

            // println!("the thing0 is {}",self.gl.GetError());
            // println!("the thing1 is {}",self.gl.GetError());


            self.gl.BindFramebuffer(gl::FRAMEBUFFER, 0);
            self.gl.ClearColor(red, green, blue, alpha);
            self.gl.Clear(gl::COLOR_BUFFER_BIT);

            self.gl.ClearTexImage(self.bindings.get(&Bindings::ComputeAccuR.get_id()).unwrap().1, 0, 0+gl::RED_INTEGER, 0+gl::UNSIGNED_INT, std::ptr::null());
            self.gl.ClearTexImage(self.bindings.get(&Bindings::ComputeAccuG.get_id()).unwrap().1, 0, 0+gl::RED_INTEGER, 0+gl::UNSIGNED_INT, std::ptr::null());
            self.gl.ClearTexImage(self.bindings.get(&Bindings::ComputeAccuB.get_id()).unwrap().1, 0, 0+gl::RED_INTEGER, 0+gl::UNSIGNED_INT, std::ptr::null());
            self.gl.UseProgram(*self.programs.get(&Program::ComputeRaytracing).unwrap());
            // self.gl.DispatchCompute(1,360,1);
            self.gl.DispatchCompute(128,1,1);
            self.gl.MemoryBarrier(gl::SHADER_IMAGE_ACCESS_BARRIER_BIT);

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


