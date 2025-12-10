use std::ffi::CStr;
use std::fs;
use std::os::raw::c_void;
use std::path::Path;
use image::{ImageReader, RgbaImage};
use crate::gl;
use crate::gl::types::{GLchar, GLenum, GLint, GLsizei, GLuint};
use crate::renderer::{LightPoint, PixelData};

pub fn load_image(path: &Path) -> RgbaImage {
    // let img = ImageReader::open("assets/rgbanoise320x180.png").unwrap().decode().unwrap();
    // let img = ImageReader::open("assets/rgbanoise640x360.png").unwrap().decode().unwrap();
    // let img = ImageReader::open("assets/scene640x360.png").unwrap().decode().unwrap();
    let img = ImageReader::open(path).unwrap().decode().unwrap();
    let img = img.flipv();
    let width = img.width();
    let height = img.height();
    // let rgba = img.to_rgba8();
    let rgba = img.to_rgba8();
    let rgba_unprocessed: Vec<u32> = img.to_rgba8().iter().map(|x| { *x as u32 }).collect();
    let mut count1 = 0;
    let mut count2 = 0;

    rgba
}
pub fn generate_light_data(width: u32, pixel_data: &Vec<PixelData>) -> Vec<LightPoint> {
    let mut x:u32 = 0;
    let mut y:u32 = 0;
    let mut light_points = Vec::new();
    for pixel in pixel_data {
        if(pixel.emissiveness>0.0){
            light_points.push(LightPoint {
                x: x as f32,
                y: y as f32,
                emissiveness: pixel.emissiveness,
                padding: 0.0,
            });
        }
        x+=1;
        if(x==width){
            x=0;
            y+=1;
        }
    }
    return light_points;
}

pub fn generate_pixel_data(rgba: &RgbaImage) -> Vec<PixelData> {

    rgba.pixels()
        .map(|px| {
            // let origMultiplied: Vec<f32> = px.iter().map(|v| (*v as f32) / 255.).collect();
            let mut r: u8 = px.0[0];
            let mut g: u8 = px.0[1];
            let mut b: u8 = px.0[2];
            let mut a: u8 = px.0[3];

            return generate_single_pixel(r,g,b,a);
        }).collect()
}
pub fn generate_single_pixel(r: u8,g: u8,b: u8,a: u8) -> PixelData{
    let mut pixel_data: PixelData = PixelData {
        r: r as f32 / 255.,
        g: g as f32 / 255.,
        b: b as f32 / 255.,
        opacity: 1.0,
        // a: a as f32 / 255.,
        emissiveness: 0.0,
        reflectiveness: 0.04,
        metallic: 0.0,
        roughness: 0.8,
        
        transmission: 0.0,
        padding_1: 0.0,
        padding_2: 0.0,
        padding_3: 0.0,
    };
    if(r==255 && g==255 && b==255){
        pixel_data.emissiveness = 10.0;
        pixel_data.opacity = 0.0;
        pixel_data.r = 1.0;
        pixel_data.g = 1.0;
        pixel_data.b = 1.0;
        // pixel_data.transmission = 0.0;
    }
    if(a==0){
        pixel_data.opacity = 0.001;
        pixel_data.transmission = 1.0;
        pixel_data.reflectiveness = 0.0;
        pixel_data.r = 1.0;
        pixel_data.g = 1.0;
        pixel_data.b = 1.0;
    }else{
        // pixel_data.transmission = 0.1;
        // pixel_data.reflectiveness= 0.99;
        pixel_data.transmission = 0.5;

    }
    return pixel_data;
}

pub unsafe fn create_texture(gl: &gl::Gl, width: u32, height: u32, internal_format: gl::types::GLenum, data: Option<*const c_void>) -> GLuint {
    let mut texture: GLuint = 0;

    gl.GenTextures(1, &mut texture);
    gl.BindTexture(gl::TEXTURE_2D, texture);
    let (format, type_) = map_internal_format(internal_format);
    gl.TexImage2D(gl::TEXTURE_2D, 0, internal_format as GLint, width as GLsizei, height as GLsizei,
                  0,
                  format+0,
                  type_+0,
                  if let Some(x) = data {x}else{std::ptr::null()});
    gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
    gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
    return texture;
}

pub fn map_internal_format(internal_format: GLenum) -> (GLenum, GLenum) {
    match internal_format {
        gl::R8 => (gl::RED, gl::UNSIGNED_BYTE),
        gl::R8UI => (gl::RED_INTEGER, gl::UNSIGNED_INT),
        gl::R16UI => (gl::RED_INTEGER, gl::UNSIGNED_INT),
        gl::R16F => (gl::RED, gl::FLOAT),
        gl::R16I => (gl::RED_INTEGER, gl::INT),
        gl::R32I => (gl::RED_INTEGER, gl::INT),
        gl::R32UI => (gl::RED_INTEGER, gl::UNSIGNED_INT),
        gl::RG8UI => (gl::RG_INTEGER, gl::UNSIGNED_INT),
        gl::RG8 => (gl::RG, gl::UNSIGNED_BYTE),
        gl::RGB8 => (gl::RGB, gl::UNSIGNED_BYTE),
        gl::RGBA8 => (gl::RGBA, gl::UNSIGNED_BYTE),
        gl::R32F => (gl::RED, gl::FLOAT),
        gl::RG32F => (gl::RG, gl::FLOAT),
        gl::RGB32F => (gl::RGB, gl::FLOAT),
        gl::RGBA32F => (gl::RGBA, gl::FLOAT),
        gl::RGBA8UI => (gl::RGBA_INTEGER, gl::UNSIGNED_INT),
        gl::RGBA32UI => (gl::RGBA_INTEGER, gl::UNSIGNED_INT),
        gl::DEPTH_COMPONENT => (gl::DEPTH_COMPONENT, gl::UNSIGNED_INT),
        gl::DEPTH_COMPONENT16 => (gl::DEPTH_COMPONENT, gl::UNSIGNED_SHORT),
        gl::DEPTH_COMPONENT24 => (gl::DEPTH_COMPONENT, gl::UNSIGNED_INT),
        gl::DEPTH_COMPONENT32F => (gl::DEPTH_COMPONENT, gl::FLOAT),
        _ => (panic!("wtf is format {internal_format}")), // Default fallback
    }
}




pub unsafe fn create_program(
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

pub unsafe fn create_shader(
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

pub fn get_gl_string(gl: &gl::Gl, variant: gl::types::GLenum) -> Option<&'static CStr> {
    unsafe {
        let s = gl.GetString(variant);
        (!s.is_null()).then(|| CStr::from_ptr(s.cast()))
    }
}

pub extern "system" fn debug_callback(
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