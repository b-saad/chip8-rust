mod app;
mod chip8;

use clap::Parser;
use rodio::OutputStreamBuilder;
use std::fs;
use std::sync::mpsc;
use std::thread;
use winit::event_loop::{ControlFlow, EventLoop};

const EMULATOR_TITLE: &str = "Chip-8";

static BEEP_SOUND_DATA: &[u8] = include_bytes!("../assets/beep_short.mp3");

/// A Chip-8 Emulator
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the Chip-8 ROM
    #[arg(long, required = true)]
    rom: String,

    /// Original behaviour of the shift instruction (default: false)
    #[arg(long, default_value_t = false)]
    shift_instruction_original: bool,

    /// Original behaviour of jump with offset instruction (default: false)
    #[arg(long, default_value_t = false)]
    jump_with_offset_original: bool,

    /// Original behaviour of store and load instruction (default: false)
    #[arg(long, default_value_t = false)]
    store_and_load_original: bool,
}

fn main() {
    let args = Args::parse();

    // default output stream
    let audio_output =
        OutputStreamBuilder::open_default_stream().expect("open default audio stream");

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
        let rom: Vec<u8> = fs::read(args.rom).unwrap();

        let audio_sink = rodio::Sink::connect_new(&audio_output.mixer());
        let beep_data: Vec<u8> = BEEP_SOUND_DATA.to_vec();

        let frame_buffer = frame_buffer_rx.recv().unwrap();
        let mut emulator = chip8::Emulator::new(
            frame_buffer,
            key_event_rx,
            chip8::DEFAULT_CYCLE_RATE,
            args.shift_instruction_original,
            args.jump_with_offset_original,
            args.store_and_load_original,
            audio_sink,
            beep_data,
        );

        emulator.load_rom(rom);
        emulator.run();
    });

    let _ = event_loop.run_app(&mut app);
}
