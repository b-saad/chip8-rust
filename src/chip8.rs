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
const LOW_4_BITS_MASK: u16 = 0x000F;
const LOW_8_BITS_MASK: u16 = 0x00FF;
const LOW_12_BITS_MASK: u16 = 0x0FFF;

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

    pub fn load_rom(&mut self, rom: Vec<u8>) {
        for (idx, instruction) in rom.iter().enumerate() {
            let pc: usize = PC_START as usize + idx;
            self.memory[pc] = *instruction;
        }
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

        inst |= self.memory[self.pc as usize] as u16;
        inst = inst << 8;
        inst |= self.memory[(self.pc + 1) as usize] as u16;

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

        let nibbles = (first_nibble, x, y, n);

        // The second byte (third and fourth nibbles). An 8-bit immediate number.
        let nn: u16 = instruction & LOW_8_BITS_MASK;

        // The second, third and fourth nibbles. A 12-bit immediate memory address.
        let nnn: u16 = instruction & LOW_12_BITS_MASK;

        match nibbles {
            (0x0, 0x0, 0xE, 0x0) => self.exec_00e0(),
            (0x1, _, _, _) => self.exec_1nnn(nnn),
            (0x6, _, _, _) => self.exec_6xnn(x, nn),
            (0x7, _, _, _) => self.exec_7xnn(x, nn),
            (0xA, _, _, _) => self.exec_annn(nnn),
            (0xD, _, _, _) => self.exec_dxyn(x, y, n),
            _ => eprint!("unknown instruction: {}", first_nibble),
        }
    }
}

impl Emulator {
    // clear screen
    fn exec_00e0(&mut self) {
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

    // jump, set program counter to nnn
    fn exec_1nnn(&mut self, nnn: u16) {
        self.pc = nnn;
    }

    // set vx reg to nn
    fn exec_6xnn(&mut self, x: u16, nn: u16) {
        self.var_registers[x as usize] = nn as u8;
    }

    // add value nn to vx reg
    fn exec_7xnn(&mut self, x: u16, nn: u16) {
        self.var_registers[x as usize] += nn as u8;
    }

    // set index register to nnn
    fn exec_annn(&mut self, nnn: u16) {
        self.index_register = nnn;
    }

    // draw an "n" pixels tall sprite from the memory location that the I index register
    // is holding to the screen, at the horizontal X coordinate in vx and the Y coordinate in vy.
    // All the pixels that are “on” in the sprite will flip the pixels on the screen that it is drawn to
    // (from left to right, from most to least significant bit).
    // If any pixels on the screen were turned “off” by this, the VF flag register is set to 1. Otherwise, it’s set to 0.
    fn exec_dxyn(&mut self, x: u16, y: u16, n: u16) {
        self.should_draw = true;

        let mut locked_buffer = self.pixel_buffer.as_ref().lock().unwrap();
        let frame = locked_buffer.frame_mut();

        // The starting position of the sprite will wrap. Another way of saying it is that the coordinates are modulo
        // (or binary AND) the size of the display (when counting from 0).
        //
        // However, the actual drawing of the sprite should not wrap. If a sprite is drawn near the edge of the screen,
        // it should be clipped, and not wrap. The sprite should be partly drawn near the edge,
        // and the other part should not reappear on the opposite side of the screen.
        let vx: u16 = self.var_registers[x as usize] as u16 % 64;
        let vy: u16 = self.var_registers[y as usize] as u16 % 32;

        self.var_registers[0xF] = 0;

        // how many rows tall
        for i in 0..n {
            // stop drawing if we reached the bottom row
            if vy + i == DISPLAY_HEIGHT as u16 {
                break;
            }

            let sprite_data = self.memory[(self.index_register + i) as usize];

            for j in 0..8 {
                // stop drawing if we reached the right edge
                if vx + j == DISPLAY_WIDTH as u16 {
                    break;
                }

                // go from most significant bit to least
                let sprite_pixel_on = ((sprite_data >> (7 - j)) & 1) == 1;

                // The frame buffer is a 1D array representing a 2D space
                let frame_x = (vx + j) as usize;
                let frame_y = (vy + i) as usize;
                let mut frame_pixel_idx = frame_x + (frame_y * DISPLAY_WIDTH as usize);

                // The frame buffer is of length W x L x 4. 4 because each pixel is an RGBA value,
                // i.e each "pixel" is 4 consecutive elements in the buffer. So we must multiple our index by 4
                // to get the correct starting index of the pixel.
                frame_pixel_idx *= 4;
                let display_pixel_on = frame[frame_pixel_idx] != 0;

                if display_pixel_on && sprite_pixel_on {
                    // turn pixel off (R, G, B)
                    frame[frame_pixel_idx] = 0x00;
                    frame[frame_pixel_idx + 1] = 0x00;
                    frame[frame_pixel_idx + 2] = 0x00;
                    self.var_registers[0xF] = 1;
                } else if !display_pixel_on && sprite_pixel_on {
                    // turn pixel on (R, G, B)
                    frame[frame_pixel_idx] = 0xFF;
                    frame[frame_pixel_idx + 1] = 0xFF;
                    frame[frame_pixel_idx + 2] = 0xFF;
                }
            }
        }
    }
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
    let mut index = 0x50;

    for val in font {
        memory[index] = val;
        index += 1;
    }
}
