use std::error::Error;
use std::ffi::{CStr, CString};
use std::fs;
use std::num::NonZeroU32;
use std::ops::Deref;
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
use crate::gl;
use crate::gl::types::{GLenum, GLint};


pub struct Renderer {
    program: gl::types::GLuint,
    vao: gl::types::GLuint,
    // vbo: gl::types::GLuint,
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

            // println!("the original-8 is {}",gl.GetError());
            let vertex_shader = create_shader(&gl, gl::VERTEX_SHADER, &PathBuf::from("assets/stub_fullscreen.glsl"));
            // println!("the original-7 is {}",gl.GetError());
            let fragment_shader = create_shader(&gl, gl::FRAGMENT_SHADER, &PathBuf::from("assets/frag.glsl"));
            // println!("the original-6 is {}",gl.GetError());

            let program = gl.CreateProgram();
            // println!("the original-5 is {}",gl.GetError());

            gl.AttachShader(program, vertex_shader);
            // println!("the original-4 is {}",gl.GetError());
            gl.AttachShader(program, fragment_shader);
            // println!("the original-3 is {}",gl.GetError());

            gl.LinkProgram(program);
            // println!("the original-2 is {}",gl.GetError());

            gl.UseProgram(program);
            // println!("the original-1 is {}",gl.GetError());

            let mut logs: Vec<u8> = Vec::with_capacity(12345);
            let mut info_used_length: GLint = 0;
            gl.GetShaderInfoLog(fragment_shader,12345, &mut info_used_length, logs.as_mut_ptr() as *mut _);
            logs.set_len(info_used_length as usize);
            println!("the error is {}",String::from_utf8(logs).unwrap());

            gl.DeleteShader(vertex_shader);
            gl.DeleteShader(fragment_shader);

            // println!("the original0 is {}",gl.GetError());
            let mut vao = std::mem::zeroed();
            // println!("the original1 is {}",gl.GetError());
            gl.GenVertexArrays(1, &mut vao);
            // println!("the original2 is {}",gl.GetError());
            gl.BindVertexArray(vao);
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
            Self { program, vao, gl }
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
            self.gl.UseProgram(self.program);
            // println!("the thing1 is {}",self.gl.GetError());


            self.gl.ClearColor(red, green, blue, alpha);
            self.gl.Clear(gl::COLOR_BUFFER_BIT);

            self.gl.BindVertexArray(self.vao);
            // println!("the thing2 is {}",self.gl.GetError());
            // self.gl.BindBuffer(gl::ARRAY_BUFFER, self.vbo);

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
            self.gl.DeleteProgram(self.program);
            // self.gl.DeleteBuffers(1, &self.vbo);
            self.gl.DeleteVertexArrays(1, &self.vao);
        }
    }
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
    let shader_source: String = fs::read_to_string(path).unwrap();
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

