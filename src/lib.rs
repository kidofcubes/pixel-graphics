mod renderer;
mod glutils;
mod scene;

use std::error::Error;
use std::ffi::{CStr, CString};
use std::num::NonZeroU32;
use std::ops::Deref;
use std::sync::Arc;

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
use winit::dpi::PhysicalSize;
use winit::platform::x11::WindowAttributesExtX11;
use crate::gl::types::{GLenum, GLint};
use crate::renderer::{Configurable, Renderer, RAYS_PER_FRAME, SUN_RAYS_PER_FRAME};

pub mod gl {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));

    // pub use Gl as Gl;
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let (window, gl_config) = match &self.gl_display {
            // We just created the event loop, so initialize the display, pick the config, and
            // create the context.
            GlDisplayCreationState::Builder(display_builder) => {
                let (window, gl_config) = match display_builder.clone().build(
                    event_loop,
                    self.template.clone(),
                    gl_config_picker,
                ) {
                    Ok((window, gl_config)) => (window.unwrap(), gl_config),
                    Err(err) => {
                        self.exit_state = Err(err);
                        event_loop.exit();
                        return;
                    },
                };

                println!("Picked a config with {} samples", gl_config.num_samples());

                // Mark the display as initialized to not recreate it on resume, since the
                // display is valid until we explicitly destroy it.
                self.gl_display = GlDisplayCreationState::Init;

                // Create gl context.
                self.gl_context =
                    Some(create_gl_context(&window, &gl_config).treat_as_possibly_current());

                (window, gl_config)
            },
            GlDisplayCreationState::Init => {
                println!("Recreating window in `resumed`");
                // Pick the config which we already use for the context.
                let gl_config = self.gl_context.as_ref().unwrap().config();
                match glutin_winit::finalize_window(event_loop, window_attributes(), &gl_config) {
                    Ok(window) => (window, gl_config),
                    Err(err) => {
                        self.exit_state = Err(err.into());
                        event_loop.exit();
                        return;
                    },
                }
            },
        };

        let attrs = window
            .build_surface_attributes(Default::default())
            .expect("Failed to build surface attributes");
        let gl_surface =
            unsafe { gl_config.display().create_window_surface(&gl_config, &attrs).unwrap() };

        // The context needs to be current for the Renderer to set up shaders and
        // buffers. It also performs function loading, which needs a current context on
        // WGL.
        let gl_context = self.gl_context.as_ref().unwrap();
        gl_context.make_current(&gl_surface).unwrap();

        self.renderer.get_or_insert_with(|| Renderer::new(&gl_config.display()));
        println!("SETUP RENDERER");

        // Try setting vsync.
        if let Err(res) = gl_surface
            .set_swap_interval(gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()))
            // .set_swap_interval(gl_context, SwapInterval::DontWait)
        {
            eprintln!("Error setting vsync: {res:?}");
        }

        assert!(self.state.replace(AppState { gl_surface, window }).is_none());

        if self.egui_winit.is_none() {
            self.egui_winit = Some(egui_winit::State::new(
                self.egui_ctx.clone(),
                egui::ViewportId::ROOT,
                &self.state.as_ref().unwrap().window,
                None,
                None,
                None,
            ));
        }

        if self.egui_painter.is_none() {
            let glow_ctx = unsafe {
                egui_glow::glow::Context::from_loader_function(|sym| {
                    let cstr = CString::new(sym).unwrap();
                    gl_config.display().get_proc_address(cstr.as_c_str()).cast()
                })
            };
            self.egui_painter = Some(
                egui_glow::Painter::new(Arc::new(glow_ctx), "", None, false)
                    .expect("egui_glow::Painter failed"),
            );
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // This event is only raised on Android, where the backing NativeWindow for a GL
        // Surface can appear and disappear at any moment.
        println!("Android window removed");

        // Destroy the GL Surface and un-current the GL Context before ndk-glue releases
        // the window back to the system.
        self.state = None;

        // Make context not current.
        self.gl_context = Some(
            self.gl_context.take().unwrap().make_not_current().unwrap().treat_as_possibly_current(),
        );
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let egui_consumed = if let (Some(ew), Some(state)) =
            (self.egui_winit.as_mut(), self.state.as_ref())
        {
            ew.on_window_event(&state.window, &event).consumed
        } else {
            false
        };

        match event {
            WindowEvent::RedrawRequested => {
                
            },
            WindowEvent::Resized(size) if size.width != 0 && size.height != 0 => {
                // Some platforms like EGL require resizing GL surface to update the size
                // Notable platforms here are Wayland and macOS, other don't require it
                // and the function is no-op, but it's wise to resize it for portability
                // reasons.
                if let Some(AppState { gl_surface, window: _ }) = self.state.as_ref() {
                    let gl_context = self.gl_context.as_ref().unwrap();
                    gl_surface.resize(
                        gl_context,
                        NonZeroU32::new(size.width).unwrap(),
                        NonZeroU32::new(size.height).unwrap(),
                    );

                    let renderer = self.renderer.as_ref().unwrap();
                    renderer.resize(size.width as i32, size.height as i32);
                }
            },
            WindowEvent::CloseRequested
             => event_loop.exit(),
            WindowEvent::KeyboardInput {
                event: KeyEvent { logical_key: Key::Named(NamedKey::Escape), .. },
                ..
            } => event_loop.exit(),
            WindowEvent::KeyboardInput {
                event: key_event,
                ..
            } if !egui_consumed => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.on_key_press(&key_event);
                }
            },
            _ => (),
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Destroy egui GL resources (buffers, textures) while the context is still
        // current. If we let Painter drop after the context is gone, it segfaults.
        if let Some(mut painter) = self.egui_painter.take() {
            painter.destroy();
        }
        self.egui_winit = None;

        // NOTE: The handling below is only needed due to nvidia on Wayland to not crash
        // on exit due to nvidia driver touching the Wayland display from on
        // `exit` hook.
        let _gl_display = self.gl_context.take().unwrap().display();

        // Clear the window.
        self.state = None;
        #[cfg(egl_backend)]
        #[allow(irrefutable_let_patterns)]
        if let glutin::display::Display::Egl(display) = _gl_display {
            unsafe {
                display.terminate();
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(AppState { gl_surface, window }) = self.state.as_ref() else { return };
        let gl_context = self.gl_context.as_ref().unwrap();

        // Gather egui input
        let raw_input = self.egui_winit
            .as_mut()
            .map(|ew| ew.take_egui_input(window))
            .unwrap_or_default();

        // Extract cloud configurable values into locals so they can be captured
        // by the egui closure without holding a borrow on self.renderer.
        macro_rules! cf32 { ($k:expr) => { self.renderer.as_ref().unwrap().get_f32_range($k) } }
        macro_rules! ci32 { ($k:expr) => { self.renderer.as_ref().unwrap().get_i32_range($k) } }
        let (mut c_coverage,   c_coverage_lo,   c_coverage_hi)   = cf32!(Configurable::CloudCoverage);
        let (mut c_base_mult,  c_base_mult_lo,  c_base_mult_hi)  = cf32!(Configurable::CloudBaseMult);
        let (mut c_detail,     c_detail_lo,     c_detail_hi)     = cf32!(Configurable::CloudDetailMult);
        let (mut c_erosion,    c_erosion_lo,    c_erosion_hi)    = cf32!(Configurable::CloudErosionModifier);
        let (mut c_cutoff,     c_cutoff_lo,     c_cutoff_hi)     = cf32!(Configurable::CloudCutoff);
        let (mut c_x_off,      c_x_off_lo,      c_x_off_hi)     = cf32!(Configurable::CloudXOffset);
        let (mut c_y_off,      c_y_off_lo,      c_y_off_hi)     = cf32!(Configurable::CloudYOffset);
        let (mut c_depth,      c_depth_lo,      c_depth_hi)      = cf32!(Configurable::Cloud3DDepth);
        let (mut c_fbm_lac,    c_fbm_lac_lo,    c_fbm_lac_hi)   = cf32!(Configurable::CloudFBMLacunarity);
        let (mut c_fbm_gain,   c_fbm_gain_lo,   c_fbm_gain_hi)  = cf32!(Configurable::CloudFBMGain);
        let (mut c_wrl_lac,    c_wrl_lac_lo,    c_wrl_lac_hi)   = cf32!(Configurable::CloudWorleyLacunarity);
        let (mut c_wrl_gain,   c_wrl_gain_lo,   c_wrl_gain_hi)  = cf32!(Configurable::CloudWorleyGain);
        let (mut c_fbm_oct,    c_fbm_oct_lo,    c_fbm_oct_hi)   = ci32!(Configurable::CloudFBMOctaves);
        let (mut c_wrl_oct,    c_wrl_oct_lo,    c_wrl_oct_hi)   = ci32!(Configurable::CloudWorleyOctaves);
        let (mut c_samples,    c_samples_lo,    c_samples_hi)   = ci32!(Configurable::Cloud3DSamples);

        // Build egui frame — split field borrows so the closure can capture
        // &mut gui_state while egui_ctx is borrowed for the run() call.
        let mut accum_dirty = false;
        let mut cloud_dirty = false;
        let gui = &mut self.gui_state;
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            egui::Window::new("Scene Controls")
                .resizable(false)
                .show(ctx, |ui| {
                    if ui.checkbox(&mut gui.manual_time, "Manual time").changed() {
                        accum_dirty = true;
                    }
                    if ui.add_enabled(
                        gui.manual_time,
                        egui::Slider::new(&mut gui.time_of_day, 0.0f32..=1.0)
                            .text("Time of Day"),
                    ).changed() {
                        accum_dirty = true;
                    }
                    ui.add(egui::Slider::new(&mut gui.sun_rays,     0u32..=80_000).text("Sun rays"));
                    ui.add(egui::Slider::new(&mut gui.emissive_rays, 0u32..=80_000).text("Emissive rays"));

                    ui.separator();
                    ui.collapsing("Cloud", |ui| {
                        let mut ch = |r: bool| { if r { cloud_dirty = true; } };
                        ch(ui.add(egui::Slider::new(&mut c_coverage,  c_coverage_lo..=c_coverage_hi)  .text("Coverage")).changed());
                        ch(ui.add(egui::Slider::new(&mut c_cutoff,    c_cutoff_lo..=c_cutoff_hi)      .text("Cutoff")).changed());
                        ch(ui.add(egui::Slider::new(&mut c_base_mult, c_base_mult_lo..=c_base_mult_hi).text("Base mult")).changed());
                        ch(ui.add(egui::Slider::new(&mut c_detail,    c_detail_lo..=c_detail_hi)      .text("Detail mult")).changed());
                        ch(ui.add(egui::Slider::new(&mut c_erosion,   c_erosion_lo..=c_erosion_hi)    .text("Erosion")).changed());
                        ch(ui.add(egui::Slider::new(&mut c_x_off,     c_x_off_lo..=c_x_off_hi)       .text("X offset")).changed());
                        ch(ui.add(egui::Slider::new(&mut c_y_off,     c_y_off_lo..=c_y_off_hi)       .text("Y offset")).changed());
                        ch(ui.add(egui::Slider::new(&mut c_depth,     c_depth_lo..=c_depth_hi)        .text("3D depth")).changed());
                        ch(ui.add(egui::Slider::new(&mut c_samples,   c_samples_lo..=c_samples_hi)    .text("3D samples")).changed());
                        ui.label("FBM noise");
                        ch(ui.add(egui::Slider::new(&mut c_fbm_oct,   c_fbm_oct_lo..=c_fbm_oct_hi)   .text("Octaves")).changed());
                        ch(ui.add(egui::Slider::new(&mut c_fbm_lac,   c_fbm_lac_lo..=c_fbm_lac_hi)   .text("Lacunarity")).changed());
                        ch(ui.add(egui::Slider::new(&mut c_fbm_gain,  c_fbm_gain_lo..=c_fbm_gain_hi)  .text("Gain")).changed());
                        ui.label("Worley noise");
                        ch(ui.add(egui::Slider::new(&mut c_wrl_oct,   c_wrl_oct_lo..=c_wrl_oct_hi)   .text("Octaves")).changed());
                        ch(ui.add(egui::Slider::new(&mut c_wrl_lac,   c_wrl_lac_lo..=c_wrl_lac_hi)   .text("Lacunarity")).changed());
                        ch(ui.add(egui::Slider::new(&mut c_wrl_gain,  c_wrl_gain_lo..=c_wrl_gain_hi)  .text("Gain")).changed());
                    });
                });
        });
        let _ = accum_dirty;

        // Push current GUI values to renderer
        let renderer = self.renderer.as_mut().unwrap();
        renderer.set_time_of_day(gui.manual_time.then_some(gui.time_of_day));
        renderer.set_sun_rays(Some(gui.sun_rays));
        renderer.set_emissive_rays(Some(gui.emissive_rays));

        if cloud_dirty {
            renderer.set_configurable_f32(Configurable::CloudCoverage,         c_coverage);
            renderer.set_configurable_f32(Configurable::CloudBaseMult,          c_base_mult);
            renderer.set_configurable_f32(Configurable::CloudDetailMult,        c_detail);
            renderer.set_configurable_f32(Configurable::CloudErosionModifier,   c_erosion);
            renderer.set_configurable_f32(Configurable::CloudCutoff,            c_cutoff);
            renderer.set_configurable_f32(Configurable::CloudXOffset,           c_x_off);
            renderer.set_configurable_f32(Configurable::CloudYOffset,           c_y_off);
            renderer.set_configurable_f32(Configurable::Cloud3DDepth,           c_depth);
            renderer.set_configurable_f32(Configurable::CloudFBMLacunarity,     c_fbm_lac);
            renderer.set_configurable_f32(Configurable::CloudFBMGain,           c_fbm_gain);
            renderer.set_configurable_f32(Configurable::CloudWorleyLacunarity,  c_wrl_lac);
            renderer.set_configurable_f32(Configurable::CloudWorleyGain,        c_wrl_gain);
            renderer.set_configurable_i32(Configurable::CloudFBMOctaves,        c_fbm_oct);
            renderer.set_configurable_i32(Configurable::CloudWorleyOctaves,     c_wrl_oct);
            renderer.set_configurable_i32(Configurable::Cloud3DSamples,         c_samples);
            renderer.regenerate_scene();
        }

        // Run the OpenGL render
        renderer.draw();

        // Paint egui overlay on top (after renderer.draw so GL state conflict is moot)
        if let (Some(painter), Some(ew)) =
            (self.egui_painter.as_mut(), self.egui_winit.as_mut())
        {
            let ppp = window.scale_factor() as f32;
            let size = window.inner_size();
            let prims = self.egui_ctx.tessellate(full_output.shapes, ppp);
            painter.paint_and_update_textures(
                [size.width, size.height],
                ppp,
                &prims,
                &full_output.textures_delta,
            );
            ew.handle_platform_output(window, full_output.platform_output);
        }

        window.request_redraw();
        gl_surface.swap_buffers(gl_context).unwrap();
    }
}

fn create_gl_context(window: &Window, gl_config: &Config) -> NotCurrentContext {
    let raw_window_handle = window.window_handle().ok().map(|wh| wh.as_raw());

    // The context creation part.
    // let context_attributes = ContextAttributesBuilder::new().build(raw_window_handle);
    let context_attributes = ContextAttributesBuilder::new().with_debug(true).build(raw_window_handle);

    // Since glutin by default tries to create OpenGL core context, which may not be
    // present we should try gles.
    let fallback_context_attributes = ContextAttributesBuilder::new().with_debug(true)
        .with_context_api(ContextApi::Gles(None))
        .build(raw_window_handle);

    // There are also some old devices that support neither modern OpenGL nor GLES.
    // To support these we can try and create a 2.1 context.
    let legacy_context_attributes = ContextAttributesBuilder::new().with_debug(true)
        .with_context_api(ContextApi::OpenGl(Some(Version::new(2, 1))))
        .build(raw_window_handle);

    // Reuse the uncurrented context from a suspended() call if it exists, otherwise
    // this is the first time resumed() is called, where the context still
    // has to be created.
    let gl_display = gl_config.display();

    unsafe {
        gl_display.create_context(gl_config, &context_attributes).unwrap_or_else(|_| {
            gl_display.create_context(gl_config, &fallback_context_attributes).unwrap_or_else(
                |_| {
                    gl_display
                        .create_context(gl_config, &legacy_context_attributes)
                        .expect("failed to create context")
                },
            )
        })
    }
}

pub fn window_attributes() -> WindowAttributes {
    Window::default_attributes()
        .with_transparent(true)
        .with_inner_size(PhysicalSize::new(640, 360))
        .with_title("Glutin triangle gradient example (press Escape to exit)")
        .with_name("pixel-graphics", "pixel-graphics")
}

enum GlDisplayCreationState {
    /// The display was not build yet.
    Builder(DisplayBuilder),
    /// The display was already created for the application.
    Init,
}

pub struct GuiState {
    pub time_of_day: f32,
    pub manual_time: bool,
    pub sun_rays: u32,
    pub emissive_rays: u32,
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            time_of_day: 0.0,
            manual_time: false,
            sun_rays: SUN_RAYS_PER_FRAME as u32,
            emissive_rays: RAYS_PER_FRAME as u32,
        }
    }
}

pub struct App {
    template: ConfigTemplateBuilder,
    renderer: Option<Renderer>,
    // NOTE: `AppState` carries the `Window`, thus it should be dropped after everything else.
    state: Option<AppState>,
    gl_context: Option<PossiblyCurrentContext>,
    gl_display: GlDisplayCreationState,
    pub exit_state: Result<(), Box<dyn Error>>,
    egui_ctx: egui::Context,
    egui_winit: Option<egui_winit::State>,
    egui_painter: Option<egui_glow::Painter>,
    gui_state: GuiState,
}

impl App {
    pub fn new(template: ConfigTemplateBuilder, display_builder: DisplayBuilder) -> Self {
        Self {
            template,
            gl_display: GlDisplayCreationState::Builder(display_builder),
            exit_state: Ok(()),
            gl_context: None,
            state: None,
            renderer: None,
            egui_ctx: egui::Context::default(),
            egui_winit: None,
            egui_painter: None,
            gui_state: GuiState::default(),
        }
    }
}

struct AppState {
    gl_surface: Surface<WindowSurface>,
    // NOTE: Window should be dropped after all resources created using its
    // raw-window-handle.
    window: Window,
}

// Find the config with the maximum number of samples, so our triangle will be
// smooth.
pub fn gl_config_picker(configs: Box<dyn Iterator<Item = Config> + '_>) -> Config {
    configs
        .reduce(|accum, config| {
            let transparency_check = config.supports_transparency().unwrap_or(false)
                & !accum.supports_transparency().unwrap_or(false);

            if transparency_check || config.num_samples() > accum.num_samples() {
                config
            } else {
                accum
            }
        })
        .unwrap()
}

pub fn pack4u8(r: u8, g: u8, b: u8, a: u8) -> u32 {
    return ((((((a as u32) << 8) | (b as u32)) << 8) | (g as u32)) << 8) | (r as u32);
}
pub fn pack2f16(x: f16, y: f16) -> u32 {
    return ((y.to_bits() as u32) << 16) | (x.to_bits() as u32);
}
