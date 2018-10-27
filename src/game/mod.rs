// This file is part of Carambolage.

// Carambolage is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Carambolage is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Carambolage.  If not, see <http://www.gnu.org/licenses/>.
mod camera;
mod car;
mod controller;
mod level;
mod scene;
mod transform;

use self::controller::{Controller, ControllerLayout};
use self::scene::Scene;
use grphx::{FrameBuffer, Shader};
use util::FrameLimiter;

use glfw::{Action, Context, Glfw, Key, Window};
use nalgebra::Perspective3;
use time::Duration;

use std::cell::Cell;
use std::mem::size_of;
use std::os::raw::c_void;
use std::ptr;
use std::sync::mpsc::Receiver;
use std::thread::sleep;

type Event = Receiver<(f64, glfw::WindowEvent)>;

pub(crate) struct Game {
    // Glfw and GL
    glfw: Glfw,
    window: Window,
    events: Event,
    frame_limiter: FrameLimiter,

    frame_buffer: FrameBuffer,
    post_proc_shader: Shader,
    post_proc_effect: i32,

    // Game
    settings: GameSettings,
    scene: Scene,
    controller: Vec<Controller>,
}

pub struct GameSettings {
    pub is_fullscreen: bool,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl Default for GameSettings {
    fn default() -> GameSettings {
        GameSettings {
            is_fullscreen: false,
            width: 640,
            height: 480,
            fps: 60,
        }
    }
}

impl Game {
    pub(crate) fn new(settings: GameSettings) -> Game {
        info!("Initializing game");
        let frame_limiter = FrameLimiter::new(settings.fps);

        debug!("Initializing glfw window");
        let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS).unwrap();
        glfw.window_hint(glfw::WindowHint::ContextVersion(3, 3));
        glfw.window_hint(glfw::WindowHint::OpenGlProfile(glfw::OpenGlProfileHint::Core));
        glfw.window_hint(glfw::WindowHint::SRgbCapable(true));
        glfw.set_error_callback(Some(glfw::Callback {
            f: error_callback,
            data: Cell::new(0),
        }));

        let (mut window, events) = glfw
            .with_primary_monitor(|glfw, m| {
                glfw.create_window(settings.width, settings.height, "Carambolage", {
                    if settings.is_fullscreen {
                        m.map_or(glfw::WindowMode::Windowed, |m| glfw::WindowMode::FullScreen(m))
                    } else {
                        glfw::WindowMode::Windowed
                    }
                })
            }).expect("Failed to create GLFW window");

        window.make_current();
        window.set_framebuffer_size_polling(true);
        window.set_cursor_pos_polling(true);
        window.set_scroll_polling(true);
        window.set_cursor_mode(glfw::CursorMode::Normal);

        debug!("Initializing openGL attributes");
        gl::load_with(|symbol| window.get_proc_address(symbol) as *const _);
        unsafe {
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::Enable(gl::DEPTH_TEST);
            gl::DepthFunc(gl::LESS);
        }

        let frame_buffer = FrameBuffer::new(settings.width as i32, settings.height as i32);
        let post_proc_shader = Shader::new("post_proc");

        let controller = vec![
            Controller::new(true, &ControllerLayout::WASD),
            Controller::new(true, &ControllerLayout::Arrows),
        ];
        let scene = Scene::new();

        Game {
            glfw,
            window,
            events,
            frame_limiter,

            frame_buffer,
            post_proc_shader,
            post_proc_effect: 0,

            settings,
            scene,
            controller,
        }
    }

    pub(crate) fn run(&mut self) {
        // Play game music (sorry just testing)
        //let device = rodio::default_output_device().unwrap();
        //let file = File::open("res/sounds/music/Rolemusic-01-Bacterial-Love.mp3").unwrap();
        //let source = rodio::Decoder::new(BufReader::new(file)).unwrap().repeat_infinite();
        //rodio::play_raw(&device, source.convert_samples());

        let nano_sec = Duration::nanoseconds(1).to_std().unwrap();

        let screen_vertices: [f32; 24] = [
            -1.0, 1.0, 0.0, 1.0, -1.0, -1.0, 0.0, 0.0, 1.0, -1.0, 1.0, 0.0, -1.0, 1.0, 0.0, 1.0, 1.0, -1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0,
        ];
        let mut screen_vao = 0;
        let mut screen_vbo = 0;

        unsafe {
            gl::GenVertexArrays(1, &mut screen_vao);
            gl::BindVertexArray(screen_vao);

            gl::GenBuffers(1, &mut screen_vbo);
            gl::BindBuffer(gl::ARRAY_BUFFER, screen_vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (screen_vertices.len() * size_of::<f32>()) as isize,
                &screen_vertices[0] as *const f32 as *const c_void,
                gl::STATIC_DRAW,
            );

            let stride = 4 * size_of::<f32>() as i32;
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, stride, ptr::null());
            gl::EnableVertexAttribArray(1);
            gl::VertexAttribPointer(1, 2, gl::FLOAT, gl::FALSE, stride, (2 * size_of::<f32>()) as *const c_void);
        }

        while !self.window.should_close() {
            let dt = self.frame_limiter.start();
            self.window.make_current();
            self.glfw.poll_events();
            self.process_events();
            self.process_input(dt);

            self.scene.update(dt, &self.controller);

            unsafe {
                self.frame_buffer.bind();
                gl::Enable(gl::DEPTH_TEST);
                gl::ClearColor(0.2, 0.2, 0.2, 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

                let projection = Perspective3::new(self.settings.width as f32 / self.settings.height as f32, 70., 0.1, 100.).unwrap();
                self.scene.draw(&projection);

                self.frame_buffer.unbind();

                gl::Disable(gl::DEPTH_TEST);
                gl::ClearColor(1.0, 1.0, 1.0, 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);

                self.post_proc_shader.bind();
                self.post_proc_shader.set_uniform_int(0, self.post_proc_effect);
                gl::BindVertexArray(screen_vao);
                gl::ActiveTexture(5);
                gl::BindTexture(gl::TEXTURE_2D, self.frame_buffer.color_buffer);
                gl::DrawArrays(gl::TRIANGLES, 0, 6);
            }

            self.window.swap_buffers();
            while self.frame_limiter.stop() {
                self.glfw.poll_events();
                sleep(nano_sec);
            }
        }
    }

    #[cfg_attr(feature = "cargo-clippy", allow(single_match))]
    pub fn process_events(&mut self) {
        for (_, event) in glfw::flush_messages(&self.events) {
            match event {
                glfw::WindowEvent::FramebufferSize(width, height) => unsafe {
                    gl::Viewport(0, 0, width, height);
                    self.settings.width = width as u32;
                    self.settings.height = height as u32;
                    self.frame_buffer.resize(width, height);
                },
                _ => {}
            }
        }
    }

    pub fn process_input(&mut self, dt: f32) {
        if self.window.get_key(Key::Escape) == Action::Press {
            self.window.set_should_close(true)
        }

        if self.window.get_key(Key::F1) == Action::Press {
            self.post_proc_effect = 1;
        }
        if self.window.get_key(Key::F2) == Action::Press {
            self.post_proc_effect = 2;
        }
        if self.window.get_key(Key::F3) == Action::Press {
            self.post_proc_effect = 3;
        }
        if self.window.get_key(Key::F4) == Action::Press {
            self.post_proc_effect = 4;
        }
        if self.window.get_key(Key::F5) == Action::Press {
            self.post_proc_effect = 5;
        }
        if self.window.get_key(Key::F6) == Action::Press {
            self.post_proc_effect = 6;
        }
        if self.window.get_key(Key::F7) == Action::Press {
            self.post_proc_effect = 7;
        }
        if self.window.get_key(Key::F8) == Action::Press {
            self.post_proc_effect = 8;
        }
        if self.window.get_key(Key::F9) == Action::Press {
            self.post_proc_effect = 9;
        }
        if self.window.get_key(Key::F10) == Action::Press {
            self.post_proc_effect = 10;
        }

        for ctrl in &mut self.controller.iter_mut() {
            ctrl.process_input(&self.window, dt);
        }
    }
}

#[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
fn error_callback(_: glfw::Error, description: String, error_count: &Cell<usize>) {
    println!("GLFW error {}: {}", error_count.get(), description);
    error_count.set(error_count.get() + 1);
}
