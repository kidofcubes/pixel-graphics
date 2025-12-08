use std::collections::HashMap;
use std::error::Error;
use std::ffi::{CStr, CString};
use std::fs;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::os::raw::c_void;
use std::path::{Path, PathBuf};
use std::string::ToString;
use gl::types::GLfloat;
use raw_window_handle::HasWindowHandle;
use winit::application::ApplicationHandler;
use winit::event::{KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes};

use glutin::config::{Config, ConfigTemplateBuilder, GetGlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentContext, PossiblyCurrentContext, Version,
};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{Surface, SwapInterval, WindowSurface};

use glutin_winit::{DisplayBuilder, GlWindow};
use half::f16;
use crate::{gl, pack2f16, pack4u8};
use crate::gl::types::{GLchar, GLenum, GLint, GLsizei, GLuint};
use image::ImageReader;

#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
enum Texture {
    Fb0Tex,
    Scene,
    ComputeAccu,
}
impl Texture {
    fn get_id(&self) -> &str {
        match self{
            Texture::Fb0Tex => "fb0tex",
            Texture::Scene => "scene",
            Texture::ComputeAccu => "computeAccu",
        }
    }
}


#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
enum Uniform {
    Frame
}
impl Uniform {
    fn get_id(&self) -> &str {
        match self{
            Uniform::Frame => "frame",
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


pub struct Renderer {
    vao: gl::types::GLuint,
    // vbo: gl::types::GLuint,
    fb0: gl::types::GLuint,
    gl: gl::Gl,
    programs: HashMap<Program, GLuint>,
    uniform_locations: HashMap<Program, HashMap<Uniform, GLint>>,
    textures: HashMap<Texture, GLuint>,
    frame: i32,
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
            gl.DebugMessageCallback(Some(debug_callback), std::ptr::null());
            gl.DebugMessageControl(
                gl::DONT_CARE,
                gl::DONT_CARE,
                gl::DONT_CARE,
                0,
                std::ptr::null(),
                gl::TRUE,
            );

            if let Some(renderer) = get_gl_string(&gl, gl::RENDERER) {
                println!("Running on {}", renderer.to_string_lossy());
            }
            if let Some(version) = get_gl_string(&gl, gl::VERSION) {
                println!("OpenGL Version {}", version.to_string_lossy());
            }

            if let Some(shaders_version) = get_gl_string(&gl, gl::SHADING_LANGUAGE_VERSION) {
                println!("Shaders version on {}", shaders_version.to_string_lossy());
            }

            let mut programs: HashMap<Program, GLuint> = HashMap::new();
            programs.insert(Program::Raytracing, create_program(&gl, &PathBuf::from("assets/stub_fullscreen.glsl"), &PathBuf::from("assets/raytracingfrag.glsl")) as GLuint);
            programs.insert(Program::StretchedDisplay, create_program(&gl, &PathBuf::from("assets/stub_fullscreen.glsl"), &PathBuf::from("assets/texturedisplayer.glsl")) as GLuint);
            programs.insert(Program::FragTest, create_program(&gl, &PathBuf::from("assets/stub_fullscreen.glsl"), &PathBuf::from("assets/frag.glsl")) as GLuint);
            programs.insert(Program::Accumulation, create_program(&gl, &PathBuf::from("assets/stub_fullscreen.glsl"), &PathBuf::from("assets/accumulation.frag")) as GLuint);

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


            // let img = ImageReader::open("assets/rgbanoise320x180.png").unwrap().decode().unwrap();
            // let img = ImageReader::open("assets/rgbanoise640x360.png").unwrap().decode().unwrap();
            let img = ImageReader::open("assets/scene640x360.png").unwrap().decode().unwrap();
            let img = img.flipv();
            let width = img.width();
            let height = img.height();
            // let rgba = img.to_rgba8();
            let rgba = img.to_rgba8();
            let rgba_unprocessed: Vec<u32> = img.to_rgba8().iter().map(|x| {*x as u32}).collect();
            let mut count1 = 0;
            let mut count2 = 0;
            let rgba_processed: Vec<u32> =
                rgba.chunks_exact(4)
                .flat_map(|px| {
                    // let origMultiplied: Vec<f32> = px.iter().map(|v| (*v as f32) / 255.).collect();
                    let mut r: u8 = px[0];
                    let mut g: u8 = px[1];
                    let mut b: u8 = px[2];
                    let mut a: u8 = px[3];
                    let mut emissiveness: f32 = 0.0;
                    let mut reflectiveness: f32 = 1.0;
                    let mut diffusionness: f32 = 0.0;
                    let mut opacity: f32 = 0.5;
                    let mut passthrough_chance:f32 = 0.1;
                    if(r==255 && g==255 && b==255){
                        emissiveness = 1.0;
                    }
                    if(a==0){
                        opacity = 0.001;
                        passthrough_chance = 0.9;
                        reflectiveness = 0.0;
                        diffusionness = 1.0;
                        r = 32;
                        g = 32;
                        b = 32;
                    }


                    // if a < 253 {
                    //     opacity = 0.0;
                    //     count1+=1;
                    // }else{
                    //     count2+=1;
                    //     opacity = 1.0;
                    //     // if f32::sqrt(((px[0] as f32)*(px[0] as f32))+((px[1] as f32)*(px[1] as f32))+((px[2] as f32)*(px[2] as f32)))  > 358.0 {
                    //     if f32::sqrt(((r as f32)*(r as f32))+((g as f32)*(g as f32))+((b as f32)*(b as f32)))  > 358.0 {
                    //         emissiveness = 100.0;
                    //     }else{
                    //         r = 255;
                    //         g = 0;
                    //         b = 255;
                    //     }
                    // }
                    a = 128;


                    // return Vec::from([pack4u8(r,g,b,a), pack4u8((emissiveness*255.0).round() as u8, (opacity*255.0).round() as u8, (reflectiveness*255.0).round() as u8, (0.0* 255.0f32).round() as u8), 0.0f32.to_bits(), 0.0f32.to_bits()]);
                    return Vec::from([
                        pack4u8(r,g,b,a),
                        pack2f16(f16::from_f32(emissiveness), f16::from_f32(opacity)),
                        pack2f16(f16::from_f32(reflectiveness), f16::from_f32(1.0-reflectiveness)),
                        pack2f16(f16::from_f32(opacity), f16::from_f32(passthrough_chance)),
                    ]);
                    // return Vec::from([r as u32,g as u32,b as u32,a as u32]);
                })
                .collect();
            // println!("count1 and 2 {} {}",count1, count2);
            println!("the original2 is {}",gl.GetError());


            let mut textures: HashMap<Texture, GLuint> = HashMap::new();

            textures.insert(Texture::Scene, std::mem::zeroed());
            gl.GenTextures(1, textures.get_mut(&Texture::Scene).unwrap());
            gl.BindTexture(gl::TEXTURE_2D, *textures.get(&Texture::Scene).unwrap());
            gl.TexImage2D(gl::TEXTURE_2D, 0,
                          0+gl::RGBA32UI as GLint,
                          // 0+gl::RGBA8UI as GLint,
                          width as GLsizei,height as GLsizei,
                          0,
                          0+gl::RGBA_INTEGER,
                          0+gl::UNSIGNED_INT, //???
                          rgba_processed.as_ptr() as *const c_void);
            gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);

            println!("the original2.1 is {}",gl.GetError());






            let mut fb0 = std::mem::zeroed();
            gl.GenFramebuffers(1, &mut fb0);
            gl.BindFramebuffer(gl::FRAMEBUFFER, fb0);

            textures.insert(Texture::Fb0Tex, std::mem::zeroed());
            gl.GenTextures(1, textures.get_mut(&Texture::Fb0Tex).unwrap());
            gl.BindTexture(gl::TEXTURE_2D, *textures.get(&Texture::Fb0Tex).unwrap());
            gl.TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA32F as GLint, width as GLsizei, height as GLsizei,
                          0,
                          0+gl::RGBA,
                          0+gl::FLOAT,
                          std::ptr::null());
            gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl.FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, *textures.get(&Texture::Fb0Tex).unwrap(), 0);

            println!("the original2.2 is {}",gl.GetError());


            textures.insert(Texture::ComputeAccu, std::mem::zeroed());
            gl.GenTextures(1, textures.get_mut(&Texture::ComputeAccu).unwrap());
            gl.BindTexture(gl::TEXTURE_2D, *textures.get(&Texture::ComputeAccu).unwrap());
            gl.TexImage2D(gl::TEXTURE_2D, 0,
                          0+gl::R32UI as GLint,
                          width as GLsizei,height as GLsizei,
                          0,
                          0+gl::RED_INTEGER,
                          0+gl::UNSIGNED_INT, //???
                          std::ptr::null());
            gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);



            let compute_shader = create_shader(&gl, gl::COMPUTE_SHADER, &PathBuf::from("assets/raytrace.comp"));


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


            let mut one_time = HashMap::new();

            for (unit, value) in textures.iter().enumerate() {
                gl.BindTextureUnit(unit as GLuint, *value.1);
                gl.BindSampler(unit as GLuint, nearest_sampler);
                if *value.0 == Texture::ComputeAccu {
                    gl.BindImageTexture(unit as GLuint, *value.1, 0, gl::FALSE, 0, gl::READ_WRITE, 0+gl::R32UI);
                }
                one_time.insert(*value.0, unit as GLint);

            }



            let mut uniform_locations = HashMap::new();

            for (program_name,program) in programs.iter() {
                println!("=========CURRENTLY ON PROGRAM {:?} WHO IS AT {}",program_name,program);
                gl.UseProgram(*program);
                for (texture, value) in textures.iter() {
                    // if *program_name==Program::ComputeRaytracing {
                    //     continue;
                    // }
                    // println!("the before before thing is {}",gl.GetError());
                    // println!("the before thing is {}",gl.GetError());
                    let loc = gl.GetUniformLocation(*program, CString::new(texture.get_id().clone().as_bytes().to_vec()).unwrap().as_ptr());
                    if (loc != -1) {
                        // println!("the {} is in {:?} at {}", key, program_name, loc);
                        println!("found a {:?} in program {:?} at location {}", texture, program_name, loc);
                        gl.Uniform1i(loc, *one_time.get(texture).unwrap());
                    }else{
                        // println!("the {} is NOT in {:?}", key, program_name);
                    }
                    // println!("the after thing is {}",gl.GetError());
                }


                let mut uniforms = HashMap::new();
                for uniform in [Uniform::Frame] {
                    // gl.GetUniforms
                    // set this up to string and all uniforms later maybe idk
                    let loc = gl.GetUniformLocation(*program, CString::new(uniform.get_id().clone().as_bytes().to_vec()).unwrap().as_ptr());
                    if(loc != -1) {
                        uniforms.insert(uniform, loc as GLint);
                    }
                }
                uniform_locations.insert(*program_name, uniforms);
            }

            println!("the original3 is {}",gl.GetError());

            // let mut vbo = std::mem::zeroed();
            // gl.GenBuffers(1, &mut vbo);
            // gl.BindBuffer(gl::ARRAY_BUFFER, vbo);
            // gl.BufferData(
            //     gl::ARRAY_BUFFER,
            //     (VERTEX_DATA.len() * std::mem::size_of::<f32>()) as gl::types::GLsizeiptr,
            //     VERTEX_DATA.as_ptr() as *const _,
            //     gl::STATIC_DRAW,
            // );

            // let pos_attrib = gl.GetAttribLocation(program, b"position\0".as_ptr() as *const _);
            // let color_attrib = gl.GetAttribLocation(program, b"color\0".as_ptr() as *const _);
            // gl.VertexAttribPointer(
            //     pos_attrib as gl::types::GLuint,
            //     2,
            //     gl::FLOAT,
            //     0,
            //     5 * std::mem::size_of::<f32>() as gl::types::GLsizei,
            //     std::ptr::null(),
            // );
            // gl.VertexAttribPointer(
            //     color_attrib as gl::types::GLuint,
            //     3,
            //     gl::FLOAT,
            //     0,
            //     5 * std::mem::size_of::<f32>() as gl::types::GLsizei,
            //     (2 * std::mem::size_of::<f32>()) as *const () as *const _,
            // );
            // gl.EnableVertexAttribArray(pos_attrib as gl::types::GLuint);
            // gl.EnableVertexAttribArray(color_attrib as gl::types::GLuint);

            // Self { program, vao, vbo, gl }
            // Self { program, vao, gl }
            // Self { pass1, pass2, pass3, vao, fb0, fb0tex, scene_tex, gl, frame: 0 }
            Self {
                programs,
                uniform_locations,
                textures,
                vao,
                fb0,
                gl,
                frame: 0
            }
            // Self { program, gl }
        }
    }

    pub unsafe fn update_uniforms(&self){
        for (program_name, program) in self.programs.iter() {
            self.gl.UseProgram(*program);
            if let Some(loc) = self.uniform_locations.get(program_name).unwrap().get(&Uniform::Frame) {
                self.gl.Uniform1i(*loc, self.frame);
            }
        }
    }

    pub fn draw(&mut self) {
        self.draw_with_clear_color(0.0, 0.0, 0.0, 0.7);
        self.frame=self.frame+1;
    }

    pub fn draw_with_clear_color(
        &self,
        red: GLfloat,
        green: GLfloat,
        blue: GLfloat,
        alpha: GLfloat,
    ) {
        unsafe {
            self.update_uniforms();
            // println!("the thing0 is {}",self.gl.GetError());
            // println!("the thing1 is {}",self.gl.GetError());


            self.gl.BindFramebuffer(gl::FRAMEBUFFER, 0);
            self.gl.ClearColor(red, green, blue, alpha);
            self.gl.Clear(gl::COLOR_BUFFER_BIT);

            self.gl.ClearTexImage(*self.textures.get(&Texture::ComputeAccu).unwrap(), 0, 0+gl::RED_INTEGER, 0+gl::UNSIGNED_INT, std::ptr::null());
            self.gl.UseProgram(*self.programs.get(&Program::ComputeRaytracing).unwrap());
            self.gl.DispatchCompute(1,360,1);
            self.gl.MemoryBarrier(gl::SHADER_IMAGE_ACCESS_BARRIER_BIT);

            self.gl.UseProgram(*self.programs.get(&Program::Accumulation).unwrap());
            self.gl.BindVertexArray(self.vao);
            self.gl.BindFramebuffer(gl::FRAMEBUFFER, self.fb0);
            self.gl.DrawArrays(gl::TRIANGLES, 0, 3);


            self.gl.UseProgram(*self.programs.get(&Program::FragTest).unwrap());
            self.gl.BindVertexArray(self.vao);
            self.gl.BindFramebuffer(gl::FRAMEBUFFER, 0);
            self.gl.DrawArrays(gl::TRIANGLES, 0, 3);

            // // raytrace
            // self.gl.UseProgram(*self.programs.get(&Program::Raytracing).unwrap());
            // self.gl.BindVertexArray(self.vao);
            // // self.gl.BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            // // self.gl.BindFramebuffer(gl::FRAMEBUFFER, self.fb0);
            // self.gl.BindFramebuffer(gl::FRAMEBUFFER, self.fb0);
            // self.gl.DrawArrays(gl::TRIANGLES, 0, 3);
            // // raytrace END
            //
            //
            // // display final
            // self.gl.UseProgram(*self.programs.get(&Program::StretchedDisplay).unwrap());
            // self.gl.BindVertexArray(self.vao);
            // self.gl.BindFramebuffer(gl::FRAMEBUFFER, 0);
            // self.gl.DrawArrays(gl::TRIANGLES, 0, 3);
            // // display final END


            // println!("the thing3 is {}",self.gl.GetError());
            // self.gl.DrawArrays(gl::POINTS, 0, 1);
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

unsafe fn create_program(
    gl: &gl::Gl,
    vertex_shader: &Path,
    fragment_shader: &Path,
) -> GLuint {
    println!("the original-8 is {}",gl.GetError());
    // let vertex_shader = create_shader(&gl, gl::VERTEX_SHADER, &PathBuf::from("assets/stub_fullscreen.glsl"));
    let vertex_shader = create_shader(&gl, gl::VERTEX_SHADER, vertex_shader);
    println!("the original-7 is {}",gl.GetError());
    let fragment_shader = create_shader(&gl, gl::FRAGMENT_SHADER, fragment_shader);
    println!("the original-6 is {}",gl.GetError());

    let program = gl.CreateProgram();
    println!("the original-5 is {}",gl.GetError());

    gl.AttachShader(program, vertex_shader);
    println!("the original-4 is {}",gl.GetError());
    gl.AttachShader(program, fragment_shader);
    println!("the original-3 is {}",gl.GetError());

    gl.LinkProgram(program);
    println!("the original-2 is {}",gl.GetError());

    // println!("the original-1 is {}",gl.GetError());

    let mut logs: Vec<u8> = Vec::with_capacity(12345);
    let mut info_used_length: GLint = 0;
    gl.GetShaderInfoLog(fragment_shader,12345, &mut info_used_length, logs.as_mut_ptr() as *mut _);
    logs.set_len(info_used_length as usize);
    println!("the error is {}",String::from_utf8(logs).unwrap());
    println!("the original-1 is {}",gl.GetError());

    // gl.DeleteShader(vertex_shader);
    // gl.DeleteShader(fragment_shader);
    return program;
}

unsafe fn create_shader(
    gl: &gl::Gl,
    shader: gl::types::GLenum,
    // source: &[u8],
    path: &Path,
) -> gl::types::GLuint {
    // println!("compiling {}",str::from_utf8(source).unwrap());
    // println!("the compiling0 is {}",gl.GetError());
    let shader = gl.CreateShader(shader);
    // println!("the compiling1 is {}",gl.GetError());


    // println!("the shader source is {}", shader_source);
    gl.ShaderSource(shader, 1, [get_shader_source(path).as_bytes().as_ptr().cast()].as_ptr(), std::ptr::null());
    // gl.ShaderSource(shader, 1, [source.as_ptr().cast()].as_ptr(), std::ptr::null());
    // println!("the compiling2 is {}",gl.GetError());
    gl.CompileShader(shader);

    let mut logs: Vec<u8> = Vec::with_capacity(12345);
    let mut info_used_length: GLint = 0;
    gl.GetShaderInfoLog(shader,12345, &mut info_used_length, logs.as_mut_ptr() as *mut _);
    logs.set_len(info_used_length as usize);
    println!("the postcreateshader error for {:?} is {}",path,String::from_utf8(logs).unwrap());
    println!("the postcreateshader+1 is {}",gl.GetError());
    // println!("the compiling3 is {}",gl.GetError());
    shader
}

fn get_shader_source(
    path: &Path,
) -> String {
    let mut shader_source: String = fs::read_to_string("assets/common.glsl").unwrap();
    shader_source.push_str("\n");
    shader_source.push_str(&fs::read_to_string(path).unwrap());
    shader_source = shader_source.replace(char::from(0),"");
    shader_source.push('\0');
    return shader_source;
}

fn get_gl_string(gl: &gl::Gl, variant: gl::types::GLenum) -> Option<&'static CStr> {
    unsafe {
        let s = gl.GetString(variant);
        (!s.is_null()).then(|| CStr::from_ptr(s.cast()))
    }
}


extern "system" fn debug_callback(
    source: GLenum,
    _type: GLenum,
    id: GLuint,
    severity: GLenum,
    _length: GLsizei,
    message: *const GLchar,
    _user_param: *mut c_void,
) {
    unsafe {
        let msg = CStr::from_ptr(message).to_string_lossy();

        let src = match source {
            gl::DEBUG_SOURCE_API => "API",
            gl::DEBUG_SOURCE_WINDOW_SYSTEM => "WINDOW SYSTEM",
            gl::DEBUG_SOURCE_SHADER_COMPILER => "SHADER COMPILER",
            gl::DEBUG_SOURCE_THIRD_PARTY => "THIRD PARTY",
            gl::DEBUG_SOURCE_APPLICATION => "APPLICATION",
            gl::DEBUG_SOURCE_OTHER => "OTHER",
            _ => "UNKNOWN",
        };

        let sev = match severity {
            gl::DEBUG_SEVERITY_HIGH => "HIGH",
            gl::DEBUG_SEVERITY_MEDIUM => "MEDIUM",
            gl::DEBUG_SEVERITY_LOW => "LOW",
            gl::DEBUG_SEVERITY_NOTIFICATION => "NOTIFICATION",
            _ => "UNKNOWN",
        };

        println!(
            "[GL DEBUG][{src}][{sev}] (id={id}): {msg}"
        );
    }
}
