use pixels::{Pixels, SurfaceTexture};
use std::sync::{Arc, Mutex, mpsc};
use winit::application::ApplicationHandler;
use winit::event::{KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

pub struct App {
    width: u32,
    height: u32,
    window_title: String,
    key_event_tx: mpsc::Sender<KeyEvent>,
    pixel_buffer_tx: mpsc::Sender<Arc<Mutex<Pixels<'static>>>>,
    pixel_buffer: Option<Arc<Mutex<Pixels<'static>>>>,
}

impl App {
    pub fn new(
        width: u32,
        height: u32,
        window_title: String,
        key_event_tx: mpsc::Sender<KeyEvent>,
        pixel_buffer_tx: mpsc::Sender<Arc<Mutex<Pixels<'static>>>>,
    ) -> Self {
        Self {
            width: width,
            height: height,
            window_title: window_title,
            key_event_tx: key_event_tx,
            pixel_buffer_tx: pixel_buffer_tx, 
            pixel_buffer: None,
        }
    }
}

impl ApplicationHandler for App {
    // We create our window and frame_buffer on resume because the docs say:
    // "It’s recommended that applications should only initialize their graphics context and create a window after they have received
    // their first Resumed event. Some systems (specifically Android) won’t allow applications to create a render surface until they are resumed."
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // if self.window.is_some() {
        //     return;
        // }

        let window_attributes = Window::default_attributes()
            .with_title(self.window_title.clone())
            .with_inner_size(winit::dpi::PhysicalSize::new(self.width, self.height));
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

        let surface_texture = SurfaceTexture::new(self.width, self.height, window.clone());
        let pixels: Pixels<'static> =
            Pixels::new(self.width, self.height, surface_texture).unwrap();

        let thread_safe_pixels = Arc::new(Mutex::new(pixels));
        self.pixel_buffer = Some(thread_safe_pixels.clone());

        if let Err(e) = self.pixel_buffer_tx.send(thread_safe_pixels.clone()) {
            eprintln!("failed to send pixel_buffer to channel: {}", e);
        }
    }

    // fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: crate::UserEvent) {
    //     println!("User event received: {event:?}");
    // }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }

            WindowEvent::RedrawRequested => {
                // Redraw the application.
                //
                // It's preferable for applications that do not render continuously to render in
                // this event rather than in AboutToWait, since rendering in here allows
                // the program to gracefully handle redraws requested by the OS.

                // Draw.
                // let frames = self.pixel_buffer.as_ref().unwrap().frame_mut();
                // println!("frame length")

                // after buffer goes out of scope, mutex will be unlocked again
                let buffer = self.pixel_buffer.as_ref().unwrap().lock().unwrap();
                if let Err(e) = buffer.render() {
                    eprintln!("failed to render to pixel buffer: {}", e);
                }

                // Queue a RedrawRequested event.
                //
                // You only need to call this if you've determined that you need to redraw in
                // applications which do not always need to. Applications that redraw continuously
                // can render here instead.
            }

            WindowEvent::KeyboardInput { event, .. } => {
                println!("key event recieved: {:?}", event);
                if let Err(e) = self.key_event_tx.send(event) {
                    eprintln!("failed to send device event to channel: {}", e);
                }
            }
            _ => (),
        }
    }

}
