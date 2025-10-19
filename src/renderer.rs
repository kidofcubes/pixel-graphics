use std::error::Error;
use std::ffi::{CStr, CString};
use std::fs;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::os::raw::c_void;
use std::path::{Path, PathBuf};
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
use crate::{gl, pack4u8};
use crate::gl::types::{GLenum, GLint, GLsizei, GLuint};
use image::ImageReader;


pub struct Renderer {
    pass1: gl::types::GLuint,
    pass2: gl::types::GLuint,
    pass3: gl::types::GLuint,
    vao: gl::types::GLuint,
    // vbo: gl::types::GLuint,
    fb0: gl::types::GLuint,
    fb0tex: gl::types::GLuint,
    scene_tex: gl::types::GLuint,
    gl: gl::Gl,
}


impl Renderer {
    pub fn new<D: GlDisplay>(gl_display: &D) -> Self {
        unsafe {
            let gl = gl::Gl::load_with(|symbol| {
                let symbol = CString::new(symbol).unwrap();
                gl_display.get_proc_address(symbol.as_c_str()).cast()
            });

            if let Some(renderer) = get_gl_string(&gl, gl::RENDERER) {
                println!("Running on {}", renderer.to_string_lossy());
            }
            if let Some(version) = get_gl_string(&gl, gl::VERSION) {
                println!("OpenGL Version {}", version.to_string_lossy());
            }

            if let Some(shaders_version) = get_gl_string(&gl, gl::SHADING_LANGUAGE_VERSION) {
                println!("Shaders version on {}", shaders_version.to_string_lossy());
            }

            let pass1 = create_program(&gl, &PathBuf::from("assets/stub_fullscreen.glsl"), &PathBuf::from("assets/frag.glsl"));
            // let pass2 = create_program(&gl, &PathBuf::from("assets/stub_fullscreen.glsl"), &PathBuf::from("assets/frag.glsl"));
            // let pass3 = create_program(&gl, &PathBuf::from("assets/stub_fullscreen.glsl"), &PathBuf::from("assets/frag.glsl"));
            let pass2 = pass1;
            let pass3 = pass1;

            // println!("the original0 is {}",gl.GetError());
            let mut vao = std::mem::zeroed();
            // println!("the original1 is {}",gl.GetError());
            gl.GenVertexArrays(1, &mut vao);
            // println!("the original2 is {}",gl.GetError());
            gl.BindVertexArray(vao);


            let mut nearest_sampler = std::mem::zeroed();
            gl.GenSamplers(1, &mut nearest_sampler);
            gl.SamplerParameteri(nearest_sampler, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl.SamplerParameteri(nearest_sampler, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            // gl.SamplerParameteri(nearest_sampler, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);


            // let img = ImageReader::open("assets/rgbanoise320x180.png").unwrap().decode().unwrap();
            let img = ImageReader::open("assets/rgbanoise640x360.png").unwrap().decode().unwrap();
            let img = img.flipv();
            let width = img.width();
            let height = img.height();
            // let rgba = img.to_rgba8();
            let rgba = img.to_rgba8();
            let rgba_unprocessed: Vec<u32> = img.to_rgba8().iter().map(|x| {*x as u32}).collect();
            let rgba_processed: Vec<u32> =
                rgba.chunks_exact(4)
                .flat_map(|px| {
                    // let origMultiplied: Vec<f32> = px.iter().map(|v| (*v as f32) / 255.).collect();
                    let mut r: u8 = px[0];
                    let mut g: u8 = px[1];
                    let mut b: u8 = px[2];
                    let mut a: u8 = px[3];
                    let mut emissiveness: f32 = 0.0;
                    let mut opacity: f32 = 0.0;
                    // if px[3] < 253 {
                    //     opacity = 0.0;
                    // }else{
                    //     opacity = 1.0;
                    //     if f32::sqrt(((px[0] as f32)*(px[0] as f32))+((px[1] as f32)*(px[1] as f32))+((px[2] as f32)*(px[2] as f32)))  > 358.0 {
                    //         emissiveness = 100.0;
                    //     }else{
                    //         r = 255;
                    //         g = 0;
                    //         b = 255;
                    //     }
                    // }
                    a = 255;


                    // return Vec::from([pack4u8(r,g,b,a), emissiveness.to_bits(), opacity.to_bits(), 0]);
                    return Vec::from([r as u32,g as u32,b as u32,a as u32]);
                })
                .collect();


            let mut scene_tex= std::mem::zeroed();
            gl.GenTextures(1, &mut scene_tex);
            gl.BindTexture(gl::TEXTURE_2D, scene_tex);
            gl.TexImage2D(gl::TEXTURE_2D, 0,
                          0+gl::RGBA32UI as GLint,
                          // 0+gl::RGBA8UI as GLint,
                          width as GLsizei,height as GLsizei,
                          0,
                          0+gl::RGBA_INTEGER,
                          0+gl::UNSIGNED_INT, //???
                          rgba_unprocessed.as_ptr() as *const c_void);
            gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);






            let mut fb0 = std::mem::zeroed();
            gl.GenFramebuffers(1, &mut fb0);
            gl.BindFramebuffer(gl::FRAMEBUFFER, fb0);

            let mut fb0tex = std::mem::zeroed();
            gl.GenTextures(1, &mut fb0tex);
            gl.BindTexture(gl::TEXTURE_2D, fb0tex);
            gl.TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA32F as GLint, width as GLsizei, height as GLsizei,
                          0,
                          0+gl::RGBA,
                          0+gl::FLOAT,
                          std::ptr::null());
            gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl.FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, fb0tex, 0);


            gl.BindTextureUnit(0, scene_tex);
            gl.BindSampler(0, nearest_sampler);
            gl.BindTextureUnit(1, fb0tex);
            gl.BindSampler(1, nearest_sampler);





            for (i,program) in [pass1, pass2, pass3].iter().enumerate() {
                let scene_loc = gl.GetUniformLocation(*program, CString::new("scene").unwrap().as_ptr());
                println!("is the sceneloc -1? {}", (scene_loc == -1));
                println!("current program is {}",i);
                if (scene_loc != -1) {
                    gl.Uniform1i(scene_loc, 0);
                }
            }

            // println!("the original3 is {}",gl.GetError());

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
            Self { pass1, pass2, pass3, vao, fb0, fb0tex, scene_tex, gl }
            // Self { program, gl }
        }
    }

    pub fn draw(&self) {
        self.draw_with_clear_color(0.0, 0.0, 0.0, 0.7)
    }

    pub fn draw_with_clear_color(
        &self,
        red: GLfloat,
        green: GLfloat,
        blue: GLfloat,
        alpha: GLfloat,
    ) {
        unsafe {
            // println!("the thing0 is {}",self.gl.GetError());
            // println!("the thing1 is {}",self.gl.GetError());


            self.gl.ClearColor(red, green, blue, alpha);
            self.gl.Clear(gl::COLOR_BUFFER_BIT);

            self.gl.UseProgram(self.pass1);
            self.gl.BindVertexArray(self.vao);
            // self.gl.BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            // self.gl.BindFramebuffer(gl::FRAMEBUFFER, self.fb0);
            self.gl.BindFramebuffer(gl::FRAMEBUFFER, 0);
            self.gl.DrawArrays(gl::TRIANGLES, 0, 3);


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
            self.gl.DeleteProgram(self.pass1);
            self.gl.DeleteProgram(self.pass2);
            self.gl.DeleteProgram(self.pass3);
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
    // println!("the original-8 is {}",gl.GetError());
    // let vertex_shader = create_shader(&gl, gl::VERTEX_SHADER, &PathBuf::from("assets/stub_fullscreen.glsl"));
    let vertex_shader = create_shader(&gl, gl::VERTEX_SHADER, vertex_shader);
    // println!("the original-7 is {}",gl.GetError());
    let fragment_shader = create_shader(&gl, gl::FRAGMENT_SHADER, fragment_shader);
    // println!("the original-6 is {}",gl.GetError());

    let pass1 = gl.CreateProgram();
    // println!("the original-5 is {}",gl.GetError());

    gl.AttachShader(pass1, vertex_shader);
    // println!("the original-4 is {}",gl.GetError());
    gl.AttachShader(pass1, fragment_shader);
    // println!("the original-3 is {}",gl.GetError());

    gl.LinkProgram(pass1);
    // println!("the original-2 is {}",gl.GetError());

    // println!("the original-1 is {}",gl.GetError());

    let mut logs: Vec<u8> = Vec::with_capacity(12345);
    let mut info_used_length: GLint = 0;
    gl.GetShaderInfoLog(fragment_shader,12345, &mut info_used_length, logs.as_mut_ptr() as *mut _);
    logs.set_len(info_used_length as usize);
    println!("the error is {}",String::from_utf8(logs).unwrap());

    gl.DeleteShader(vertex_shader);
    gl.DeleteShader(fragment_shader);
    return pass1;
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
    let mut shader_source: String = fs::read_to_string("assets/common.glsl").unwrap();
    shader_source.push_str("\n");
    shader_source.push_str(&fs::read_to_string(path).unwrap());

    println!("the shader source is {}", shader_source);
    gl.ShaderSource(shader, 1, [shader_source.as_bytes().as_ptr().cast()].as_ptr(), std::ptr::null());
    // gl.ShaderSource(shader, 1, [source.as_ptr().cast()].as_ptr(), std::ptr::null());
    // println!("the compiling2 is {}",gl.GetError());
    gl.CompileShader(shader);
    // println!("the compiling3 is {}",gl.GetError());
    shader
}

fn get_gl_string(gl: &gl::Gl, variant: gl::types::GLenum) -> Option<&'static CStr> {
    unsafe {
        let s = gl.GetString(variant);
        (!s.is_null()).then(|| CStr::from_ptr(s.cast()))
    }
}

