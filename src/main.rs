mod app;
mod chip8;

use std::sync::mpsc;
use std::thread;
use winit::event_loop::{ControlFlow, EventLoop};

const EMULATOR_TITLE: &str = "Chip-8";
//
// #[allow(dead_code)]
// #[derive(Debug, Clone, Copy)]
// enum UserEvent {
//     Render,
// }

fn main() {
    let (key_event_tx, key_event_rx) = mpsc::channel();
    let (frame_buffer_tx, frame_buffer_rx): (
        mpsc::Sender<std::sync::Arc<std::sync::Mutex<pixels::Pixels<'static>>>>,
        mpsc::Receiver<std::sync::Arc<std::sync::Mutex<pixels::Pixels<'static>>>>,
    ) = mpsc::channel();

    let mut app = app::App::new(
        chip8::DISPLAY_WIDTH.into(),
        chip8::DISPLAY_HEIGHT.into(),
        EMULATOR_TITLE.to_string(),
        key_event_tx,
        frame_buffer_tx,
    );

    let event_loop = EventLoop::new().unwrap();

    // ControlFlow::Poll continuously runs the event loop, even if the OS hasn't
    // dispatched any events. This is ideal for games and similar applications.
    event_loop.set_control_flow(ControlFlow::Poll);

    thread::spawn(move || {
        let frame_buffer = frame_buffer_rx.recv().unwrap();
        let mut emulator =
            chip8::Emulator::new(frame_buffer, key_event_rx, chip8::DEFAULT_CYCLE_RATE);
        emulator.run();
    });

    let _ = event_loop.run_app(&mut app);

    println!("DONE")
}
