use std::error::Error;
use std::ffi::{CStr, CString};
use std::num::NonZeroU32;
use std::ops::Deref;

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
use pixel_graphics::{gl, window_attributes, App};
use pixel_graphics::gl::types::{GLfloat, GLint};

pub fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new().unwrap();
    // The template will match only the configurations supporting rendering
    // to windows.
    //
    // XXX We force transparency only on macOS, given that EGL on X11 doesn't
    // have it, but we still want to show window. The macOS situation is like
    // that, because we can query only one config at a time on it, but all
    // normal platforms will return multiple configs, so we can find the config
    // with transparency ourselves inside the `reduce`.
    let template =
        ConfigTemplateBuilder::new().with_alpha_size(8).with_transparency(cfg!(cgl_backend));

    let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attributes()));

    let mut app = App::new(template, display_builder);
    event_loop.run_app(&mut app)?;

    app.exit_state
}

const VERTEX_SHADER_SOURCE: &[u8] = b"
#version 460

out vec2 texcoords;
void main() {
    vec2 vertices[3]=vec2[3](vec2(-1,-1), vec2(3,-1), vec2(-1, 3));
    gl_Position = vec4(vertices[gl_VertexID],0,1);
    texcoords = 0.5 * gl_Position.xy + vec2(0.5);
}
\0";

const FRAGMENT_SHADER_SOURCE: &[u8] = b"

\0";