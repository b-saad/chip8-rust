use pixels::Pixels;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use winit::event::KeyEvent;

pub const DEFAULT_CYCLE_RATE: u16 = 700;
pub const DISPLAY_WIDTH: u8 = 64;
pub const DISPLAY_HEIGHT: u8 = 32;

// 4KB of ram
const RAM_SIZE: usize = 4096;

// CHIP-8 programs start at address 0x200 (512 in decimal)
const PC_START: u16 = 512;
const LOW_4_BITS_MASK: u16 = 0b0000_0000_0000_1111;
const LOW_8_BITS_MASK: u16 = 0b0000_0000_1111_1111;
const LOW_12_BITS_MASK: u16 = 0b0000_1111_1111_1111;

pub struct Emulator {
    key_event_rx: mpsc::Receiver<KeyEvent>,

    pixel_buffer: Arc<Mutex<Pixels<'static>>>,

    // should the frame be redrawn this cycle
    should_draw: bool,

    // cycle_rate is the number of instruction cycles to run per second
    // one cycle is defined as a full fetch/decode/execute loop
    // the standard rate is 700 per second
    cycle_rate: u16,

    memory: [u8; RAM_SIZE],

    // program counter, often called just “PC”, which points at the current instruction in memory
    pc: u16,

    // 16-bit index register called “I” which is used to point at locations in memory
    index_register: u16,

    // 16, 8-bit (one byte) general-purpose variable registers numbered 0 through F hexadecimal,
    // ie. 0 through 15 in decimal, called V0 through VF.
    //
    // VF is also used as a flag register; many instructions will set it to either 1 or 0
    // based on some rule, for example using it as a carry flag
    var_registers: [u8; 16],

    // An 8-bit delay timer which is decremented at a rate of 60 Hz (60 times per second) until it reaches 0
    delay_timer: u8,

    // An 8-bit sound timer which functions like the delay timer,
    // but which also gives off a beeping sound as long as it’s not 0
    sound_timer: u8,
}

impl Emulator {
    pub fn new(
        pixel_buffer: Arc<Mutex<Pixels<'static>>>,
        key_event_rx: mpsc::Receiver<KeyEvent>,
        cycle_rate: u16,
    ) -> Self {
        let mut mem: [u8; RAM_SIZE] = [0; RAM_SIZE];
        load_fonts(&mut mem);

        return Self {
            key_event_rx: key_event_rx,
            pixel_buffer: pixel_buffer,
            should_draw: false,
            cycle_rate: cycle_rate,
            memory: mem,
            pc: PC_START,
            index_register: 0,
            var_registers: [0; 16],
            delay_timer: 60,
            sound_timer: 60,
        };
    }

    pub fn run(&mut self) {
        let target_fps = 60;
        let frame_duration = Duration::from_secs_f64(1.0 / target_fps as f64);

        let mut last_frame_time = Instant::now();

        let cycle_start = Instant::now();
        let mut cycles_completed: u64 = 0;

        loop {
            // Calculate the time elapsed since the last frame
            let elapsed = last_frame_time.elapsed();

            // If the elapsed time is less than the desired frame duration, sleep
            if elapsed < frame_duration {
                thread::sleep(frame_duration - elapsed);
            }

            // Update the last frame time for the next iteration
            last_frame_time = Instant::now();

            self.handle_key_event();

            // Calculate how many cycles should have been completed by now
            // by comparing seconds elapsed * cycle_rate and cycles_completed
            let cycles_missing =
                (cycle_start.elapsed().as_secs() * self.cycle_rate as u64) - cycles_completed;

            self.execute_cycles(cycles_missing);

            cycles_completed += cycles_missing;

            self.update_sound_timer();
            self.update_delay_timer();

            if self.should_draw {
                self.render();
                self.should_draw = false
            }
        }
    }

    fn render(&self) {
        let locked_buffer = self.pixel_buffer.as_ref().lock().unwrap();

        if let Err(e) = locked_buffer.render() {
            eprintln!("failed to render to pixel buffer in emulator: {}", e);
        }
    }

    fn update_sound_timer(&mut self) {
        if self.sound_timer > 0 {
            self.sound_timer -= 1;
            // TODO: BEEP
        }
    }

    fn update_delay_timer(&mut self) {
        if self.delay_timer > 0 {
            self.delay_timer -= 1;
        }
    }

    fn handle_key_event(&self) {
        let event = match self.key_event_rx.try_recv() {
            Ok(e) => e,
            Err(_) => return, // no event in channel
        };

        println!("key event recieved and handled: {:?}", event);
    }

    fn execute_cycles(&mut self, cycles: u64) {
        for _ in 0..cycles {
            let instruction: u16 = self.fetch();
            self.decode_and_execute(instruction);
        }
    }

    fn fetch(&mut self) -> u16 {
        // Read the instruction that PC is currently pointing at from memory.
        // An instruction is two bytes, read two successive bytes from memory
        // and combine them into one 16-bit instruction.
        let mut inst: u16 = 0;
        let idx: usize = self.pc.into();

        inst |= self.memory[idx] as u16;
        inst = inst << 8;
        inst |= self.memory[idx + 1] as u16;

        self.pc += 2;

        return inst;
    }

    fn decode_and_execute(&mut self, instruction: u16) {
        // first nibble that tells you what kind of instruction it is
        let first_nibble: u16 = (instruction >> 12) & LOW_4_BITS_MASK;

        // The second nibble. Used to look up one of the 16 registers (VX) from V0 through VF
        let x: u16 = (instruction >> 8) & LOW_4_BITS_MASK;

        // The third nibble. Also used to look up one of the 16 registers (VY) from V0 through VF.
        let y: u16 = (instruction >> 4) & LOW_4_BITS_MASK;

        // The fourth nibble. A 4-bit number.
        let n: u16 = instruction & LOW_4_BITS_MASK;

        // The second byte (third and fourth nibbles). An 8-bit immediate number.
        let nn: u16 = instruction & LOW_8_BITS_MASK;

        // The second, third and fourth nibbles. A 12-bit immediate memory address.
        let nnn: u16 = instruction & LOW_12_BITS_MASK;

        match first_nibble {
            0 => self.exec_clear_screen(),

            1 => self.exec_jump(nnn),

            2 => {
                eprint!("unimplemented instruction: {}", first_nibble)
            }

            3 => {
                eprint!("unimplemented instruction: {}", first_nibble)
            }

            4 => {
                eprint!("unimplemented instruction: {}", first_nibble)
            }

            5 => {
                eprint!("unimplemented instruction: {}", first_nibble)
            }

            6 => self.exec_set_reg(x, nn),

            7 => self.exec_add_val_to_reg(x, nn),

            8 => {
                eprint!("unimplemented instruction: {}", first_nibble)
            }

            9 => {
                eprint!("unimplemented instruction: {}", first_nibble)
            }

            // 0xA
            10 => self.exec_set_index_reg(nnn),

            // 0xB
            11 => {
                eprint!("unimplemented instruction: {}", first_nibble)
            }

            // 0xC
            12 => {
                eprint!("unimplemented instruction: {}", first_nibble)
            }

            // 0xD
            13 => self.exec_draw(x, y, n),

            // 0xE
            14 => {
                eprint!("unimplemented instruction: {}", first_nibble)
            }

            // 0xF
            15 => {
                eprint!("unimplemented instruction: {}", first_nibble)
            }

            _ => eprint!("unknown instruction: {}", first_nibble),
        }
    }

    fn exec_clear_screen(&mut self) {
        let mut locked_buffer = self.pixel_buffer.as_ref().lock().unwrap();
        let frame = locked_buffer.frame_mut();
        for pixel in frame.chunks_exact_mut(4) {
            pixel[0] = 0x00; // R
            pixel[1] = 0x00; // G
            pixel[2] = 0x00; // B
            pixel[3] = 0xff; // A
        }

        self.should_draw = true
    }

    fn exec_jump(&mut self, to: u16) {}

    fn exec_set_reg(&mut self, reg: u16, val: u16) {}

    fn exec_add_val_to_reg(&mut self, reg: u16, val: u16) {}

    fn exec_set_index_reg(&mut self, val: u16) {}

    fn exec_draw(&mut self, hor_reg: u16, vert_reg: u16, tall: u16) {}
}

// The CHIP-8 emulator should have a built-in font, with sprite data representing the hexadecimal numbers from 0 through F.
// Each font character should be 4 pixels wide by 5 pixels tall.
// These font sprites are drawn just like regular sprites.
fn load_fonts(memory: &mut [u8; 4096]) {
    let font = [
        0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
        0x20, 0x60, 0x20, 0x20, 0x70, // 1
        0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
        0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
        0x90, 0x90, 0xF0, 0x10, 0x10, // 4
        0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
        0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
        0xF0, 0x10, 0x20, 0x40, 0x40, // 7
        0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
        0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
        0xF0, 0x90, 0xF0, 0x90, 0x90, // A
        0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
        0xF0, 0x80, 0x80, 0x80, 0xF0, // C
        0xE0, 0x90, 0x90, 0x90, 0xE0, // D
        0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
        0xF0, 0x80, 0xF0, 0x80, 0x80, // F
    ];

    // convention is to store fonts in memory in addresses 050 - 09F
    // 050 == 80 in decimal
    let mut index = 80;

    for val in font {
        memory[index] = val;
        index += 1;
    }
}
