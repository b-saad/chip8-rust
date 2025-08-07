use pixels::Pixels;
use rand::Rng;
use rodio::Decoder;
use std::collections::HashSet;
use std::io::Cursor;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use winit::event::KeyEvent;

pub const DEFAULT_CYCLE_RATE: u16 = 700;
pub const DISPLAY_WIDTH: u8 = 64;
pub const DISPLAY_HEIGHT: u8 = 32;

// convention is to store fonts in memory in addresses 050 - 09F
const FONT_PC: usize = 0x50;

const LOW_4_BITS_MASK: u16 = 0x000F;
const LOW_8_BITS_MASK: u16 = 0x00FF;
const LOW_12_BITS_MASK: u16 = 0x0FFF;

// CHIP-8 programs start at address 0x200 (512 in decimal)
const PC_START: u16 = 512;

// 4KB of ram
const RAM_SIZE: usize = 4096;

pub struct Emulator {
    key_event_rx: mpsc::Receiver<KeyEvent>,

    pixel_buffer: Arc<Mutex<Pixels<'static>>>,

    // Should the frame be redrawn this cycle
    should_draw: bool,

    // cycle_rate is the number of instruction cycles to run per second
    // one cycle is defined as a full fetch/decode/execute loop
    // the standard rate is 700 per second
    cycle_rate: u16,

    memory: [u8; RAM_SIZE],

    // Program counter, often called just “PC”, which points at the current instruction in memory
    pc: u16,

    // A stack for 16-bit addresses, which is used to call subroutines/functions and return from them
    stack: Vec<u16>,

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

    // whether to run the SHIFT instructions as per the original spec or not
    op_shift_original: bool,

    // whether to run the JUMP WITH OFFSET instructions as per the original spec or not
    // sensible default: true
    op_jump_with_offset_original: bool,

    // whether to run the STORE AND LOAD instructions as per the original spec or not
    // sensible default: false
    op_store_and_load_original: bool,

    // keep track of which keys are currently pressed, each key is a single hex character
    pressed_keys: HashSet<u16>,

    audio_sink: rodio::Sink,

    audio_sink_initialized: bool,

    beep_audio_bytes: Vec<u8>,
}

impl Emulator {
    pub fn new(
        pixel_buffer: Arc<Mutex<Pixels<'static>>>,
        key_event_rx: mpsc::Receiver<KeyEvent>,
        cycle_rate: u16,
        op_shift_original: bool,
        op_jump_with_offset_original: bool,
        op_store_and_load_original: bool,
        audio_sink: rodio::Sink,
        beep_audio_bytes: Vec<u8>,
    ) -> Self {
        let mut mem: [u8; RAM_SIZE] = [0; RAM_SIZE];
        load_fonts(&mut mem);

        // let sink = rodio::Sink::connect_new(&output_stream.mixer());

        return Self {
            key_event_rx: key_event_rx,
            pixel_buffer: pixel_buffer,
            should_draw: false,
            cycle_rate: cycle_rate,
            memory: mem,
            pc: PC_START,
            stack: Vec::new(),
            index_register: 0,
            var_registers: [0; 16],
            delay_timer: 60,
            sound_timer: 60,
            op_shift_original: op_shift_original,
            op_jump_with_offset_original: op_jump_with_offset_original,
            op_store_and_load_original: op_store_and_load_original,
            pressed_keys: HashSet::new(),
            audio_sink: audio_sink,
            audio_sink_initialized: false,
            beep_audio_bytes: beep_audio_bytes,
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
            if !self.audio_sink_initialized {
                let cursor = Cursor::new(self.beep_audio_bytes.clone());
                let source = Decoder::new_looped(cursor).unwrap();
                self.audio_sink.append(source);
                self.audio_sink_initialized = true;
            }

            self.sound_timer -= 1;
            self.audio_sink.play();
        } else {
            self.audio_sink.pause();
        }
    }

    fn update_delay_timer(&mut self) {
        if self.delay_timer > 0 {
            self.delay_timer -= 1;
        }
    }

    // Chip8 keypad     QWERTY Keyboard mapping
    // 1 | 2 | 3 | C        1 | 2 | 3 | 4
    // 4 | 5 | 6 | D  <=>   Q | W | E | R
    // 7 | 8 | 9 | E  <=>   A | S | D | F
    // A | 0 | B | F        Z | X | C | V
    fn handle_key_event(&mut self) {
        let event = match self.key_event_rx.try_recv() {
            Ok(e) => e,
            Err(_) => return, // no event in channel
        };

        let chip8_key: u16 = match event.physical_key {
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Digit1) => 0x1,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Digit2) => 0x2,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Digit3) => 0x3,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Digit4) => 0xC,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyQ) => 0x4,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyW) => 0x5,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyE) => 0x6,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyR) => 0xD,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyA) => 0x7,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyS) => 0x8,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyD) => 0x9,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyF) => 0xE,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ) => 0xA,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyX) => 0x0,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyC) => 0xB,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV) => 0xF,
            _ => return, // not a key on the keyboard
        };

        match event.state {
            winit::event::ElementState::Released => {
                self.pressed_keys.remove(&chip8_key);
            }
            winit::event::ElementState::Pressed => {
                self.pressed_keys.insert(chip8_key);
            }
        }
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
            (0x0, 0x0, 0xE, 0xE) => self.exec_00ee(),
            (0x1, _, _, _) => self.exec_1nnn(nnn),
            (0x2, _, _, _) => self.exec_2nnn(nnn),
            (0x3, _, _, _) => self.exec_3xnn(x, nn),
            (0x4, _, _, _) => self.exec_4xnn(x, nn),
            (0x5, _, _, 0x0) => self.exec_5xy0(x, y),
            (0x6, _, _, _) => self.exec_6xnn(x, nn),
            (0x7, _, _, _) => self.exec_7xnn(x, nn),
            (0x8, _, _, 0x0) => self.exec_8xy0(x, y),
            (0x8, _, _, 0x1) => self.exec_8xy1(x, y),
            (0x8, _, _, 0x2) => self.exec_8xy2(x, y),
            (0x8, _, _, 0x3) => self.exec_8xy3(x, y),
            (0x8, _, _, 0x4) => self.exec_8xy4(x, y),
            (0x8, _, _, 0x5) => self.exec_8xy5(x, y),
            (0x8, _, _, 0x6) => self.exec_8xy6(x, y),
            (0x8, _, _, 0x7) => self.exec_8xy7(x, y),
            (0x8, _, _, 0xE) => self.exec_8xye(x, y),
            (0x9, _, _, 0x0) => self.exec_9xy0(x, y),
            (0xA, _, _, _) => self.exec_annn(nnn),
            (0xB, _, _, _) => self.exec_bnnn(x, nnn),
            (0xC, _, _, _) => self.exec_cxnn(x, nn),
            (0xD, _, _, _) => self.exec_dxyn(x, y, n),
            (0xE, _, 0x9, 0xE) => self.exec_ex9e(x),
            (0xE, _, 0xA, 0x1) => self.exec_exa1(x),
            (0xF, _, 0x0, 0x7) => self.exec_fx07(x),
            (0xF, _, 0x1, 0x5) => self.exec_fx15(x),
            (0xF, _, 0x1, 0x8) => self.exec_fx18(x),
            (0xF, _, 0x1, 0xE) => self.exec_fx1e(x),
            (0xF, _, 0x0, 0xA) => self.exec_fx0a(x),
            (0xF, _, 0x2, 0x9) => self.exec_fx29(x),
            (0xF, _, 0x3, 0x3) => self.exec_fx33(x),
            (0xF, _, 0x5, 0x5) => self.exec_fx55(x),
            (0xF, _, 0x6, 0x5) => self.exec_fx65(x),
            _ => eprint!("unknown instruction: {:x}", first_nibble),
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

    // return from a subroutine
    // i.e pop the last address from the stack and set the pc to it
    fn exec_00ee(&mut self) {
        self.pc = self.stack.pop().unwrap();
    }

    // jump, set program counter to nnn
    fn exec_1nnn(&mut self, nnn: u16) {
        self.pc = nnn;
    }

    // call the subroutine at memory location nnn
    // push current pc to stack first so we can return
    fn exec_2nnn(&mut self, nnn: u16) {
        self.stack.push(self.pc);
        self.pc = nnn;
    }

    // skip one instruction if the value in vx is equal to nn
    fn exec_3xnn(&mut self, x: u16, nn: u16) {
        if self.var_registers[x as usize] as u16 == nn {
            self.pc += 2;
        }
    }

    // skip one instruction if the value in vx is NOT equal to nn
    fn exec_4xnn(&mut self, x: u16, nn: u16) {
        if self.var_registers[x as usize] as u16 != nn {
            self.pc += 2;
        }
    }

    // skip one instruction if the values in vx and vy are equal
    fn exec_5xy0(&mut self, x: u16, y: u16) {
        if self.var_registers[x as usize] == self.var_registers[y as usize] {
            self.pc += 2;
        }
    }

    // set vx reg to nn
    fn exec_6xnn(&mut self, x: u16, nn: u16) {
        self.var_registers[x as usize] = nn as u8;
    }

    // add value nn to vx reg
    fn exec_7xnn(&mut self, x: u16, nn: u16) {
        let val: u8 = self.var_registers[x as usize].wrapping_add(nn as u8);
        self.var_registers[x as usize] = val;
    }

    // set vx to value of vy
    fn exec_8xy0(&mut self, x: u16, y: u16) {
        self.var_registers[x as usize] = self.var_registers[y as usize];
    }

    // set vx to the binary OR of vx and vy
    fn exec_8xy1(&mut self, x: u16, y: u16) {
        self.var_registers[x as usize] |= self.var_registers[y as usize];
    }

    // set vx to the binary AND of vx and vy
    fn exec_8xy2(&mut self, x: u16, y: u16) {
        self.var_registers[x as usize] &= self.var_registers[y as usize];
    }

    // set vx to the binary XOR of vx and vy
    fn exec_8xy3(&mut self, x: u16, y: u16) {
        self.var_registers[x as usize] ^= self.var_registers[y as usize];
    }

    // set vx to the sume of vx and vy
    // if it overflows, set vf to 1 otherwise set it to 0
    fn exec_8xy4(&mut self, x: u16, y: u16) {
        let vx = self.var_registers[x as usize];
        let vy = self.var_registers[y as usize];
        let (result, overflow) = vx.overflowing_add(vy);
        self.var_registers[x as usize] = result;
        self.var_registers[0xf] = 0;
        if overflow {
            self.var_registers[0xf] = 1;
        }
    }

    // sets vx to result of vx - vy
    // If the minuend (the first operand) is larger than the subtrahend (second operand),
    // VF will be set to 1. If the subtrahend is larger, and we “underflow” the result, VF is set to 0
    fn exec_8xy5(&mut self, x: u16, y: u16) {
        let vx = self.var_registers[x as usize];
        let vy = self.var_registers[y as usize];
        let (result, overflow) = vx.overflowing_sub(vy);
        self.var_registers[x as usize] = result;
        if overflow {
            self.var_registers[0xf] = 0;
        } else {
            self.var_registers[0xf] = 1;
        }
    }

    // put the value of VY into VX, and then shift the value in VX 1 bit to the right
    // the flag register VF would be set to the bit that was shifted out.
    // However, starting with CHIP-48 and SUPER-CHIP in the early 1990s,
    // this instruction was changed so that they shifted VX in place, and ignored the Y completely.
    fn exec_8xy6(&mut self, x: u16, y: u16) {
        let mut val = self.var_registers[y as usize];
        if !self.op_shift_original {
            val = self.var_registers[x as usize];
        }
        let new_val = val >> 1;
        self.var_registers[x as usize] = new_val;
        self.var_registers[0xf] = val & 1;
    }

    // sets vx to result of vy - vx
    // If the minuend (the first operand) is larger than the subtrahend (second operand),
    // VF will be set to 1. If the subtrahend is larger, and we “underflow” the result, VF is set to 0
    fn exec_8xy7(&mut self, x: u16, y: u16) {
        let vx = self.var_registers[x as usize];
        let vy = self.var_registers[y as usize];
        let (result, overflow) = vy.overflowing_sub(vx);
        self.var_registers[x as usize] = result;
        if overflow {
            self.var_registers[0xf] = 0;
        } else {
            self.var_registers[0xf] = 1;
        }
    }

    // put the value of VY into VX, and then shift the value in VX 1 bit to the left
    // the flag register VF would be set to the bit that was shifted out.
    // However, starting with CHIP-48 and SUPER-CHIP in the early 1990s,
    // this instruction was changed so that they shifted VX in place, and ignored the Y completely.
    fn exec_8xye(&mut self, x: u16, y: u16) {
        let mut val = self.var_registers[y as usize];
        if !self.op_shift_original {
            val = self.var_registers[x as usize];
        }
        let new_val = val << 1;
        self.var_registers[x as usize] = new_val;
        self.var_registers[0xf] = (val & 0b1000_0000) >> 7;
    }

    // skip one instruction if the values in vx and vy are NOT equal
    fn exec_9xy0(&mut self, x: u16, y: u16) {
        if self.var_registers[x as usize] != self.var_registers[y as usize] {
            self.pc += 2;
        }
    }

    // set index register to nnn
    fn exec_annn(&mut self, nnn: u16) {
        self.index_register = nnn;
    }

    // jump to the address nnn plus the value in the register v0
    //
    // Starting with CHIP-48 and SUPER-CHIP, it was changed to work as bxnn:
    // It will jump to the address xnn, plus the value in the register vx
    fn exec_bnnn(&mut self, x: u16, nnn: u16) {
        let mut val = self.var_registers[0] as u16;
        if !self.op_jump_with_offset_original {
            val = self.var_registers[x as usize] as u16;
        }
        val += nnn;
        self.pc = val;
    }

    // generates a random number, binary ANDs it with the value nn, and puts the result in x
    fn exec_cxnn(&mut self, x: u16, nn: u16) {
        let mut rng = rand::rng();
        let val: u16 = rng.random();
        self.var_registers[x as usize] = (val & nn) as u8;
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

    // skip one instruction (increment PC by 2) if the key corresponding to the value in vx is pressed
    fn exec_ex9e(&mut self, x: u16) {
        if self.pressed_keys.contains(&x) {
            self.pc += 2;
        }
    }

    // skip one instruction (increment PC by 2) if the key corresponding to the value in vx NOT is pressed
    fn exec_exa1(&mut self, x: u16) {
        if !self.pressed_keys.contains(&x) {
            self.pc += 2;
        }
    }

    // set vx to the current value of the delay timer
    fn exec_fx07(&mut self, x: u16) {
        self.var_registers[x as usize] = self.delay_timer;
    }

    // set the delay timer to the value in vx
    fn exec_fx15(&mut self, x: u16) {
        self.delay_timer = self.var_registers[x as usize] as u8;
    }

    // set the sound timer to the value in vx
    fn exec_fx18(&mut self, x: u16) {
        self.sound_timer = self.var_registers[x as usize] as u8;
    }

    // add to index, add the value of vx to the index register
    fn exec_fx1e(&mut self, x: u16) {
        self.index_register += self.var_registers[x as usize] as u16;
    }

    // “blocks”; it stops executing instructions and waits for key input
    // (or loops forever, unless a key is pressed).
    // PC is decremented here since it is incremented in the fetch phase
    fn exec_fx0a(&mut self, x: u16) {
        if let Some(key) = self.pressed_keys.iter().next() {
            self.var_registers[x as usize] = *key as u8;
        } else {
            self.pc -= 2;
        }
    }

    // The index register is set to the address of the hexadecimal character in vx
    fn exec_fx29(&mut self, x: u16) {
        let vx = self.var_registers[x as usize];
        let char_address = font_digit_address(vx);
        self.index_register = char_address as u16;
    }

    // Binary-coded decimal conversion,
    // It takes the number in vx (which is one byte, so it can be any number from 0 to 255)
    // and converts it to three decimal digits, storing these digits in memory at
    // the address in the index register
    fn exec_fx33(&mut self, x: u16) {
        let vx = self.var_registers[x as usize];
        let three_digit_vx = format!("{:03}", vx);
        let radix: u32 = 10;
        for (idx, c) in three_digit_vx.chars().enumerate() {
            let address = self.index_register + idx as u16;
            let digit: u8 = c.to_digit(radix).unwrap() as u8;
            self.memory[address as usize] = digit;
        }
    }

    // store and load, the value of each variable register from V0 to VX inclusive
    // (if X is 0, then only V0) will be stored in successive memory addresses,
    // starting with the one that’s stored in index_register
    fn exec_fx55(&mut self, x: u16) {
        for i in 0..=x {
            let val = self.var_registers[i as usize];
            let address: usize = (self.index_register + i) as usize;
            self.memory[address] = val;
        }

        if self.op_store_and_load_original {
            self.index_register += x + 1;
        }
    }

    // store and load, opposite of fx55
    // it takes the value stored at the memory addresses and
    // loads them into the variable registers instead.
    fn exec_fx65(&mut self, x: u16) {
        for i in 0..=x {
            let address: usize = (self.index_register + i) as usize;
            self.var_registers[i as usize] = self.memory[address];
        }

        if self.op_store_and_load_original {
            self.index_register += x + 1;
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

    let mut index = FONT_PC;

    for val in font {
        memory[index] = val;
        index += 1;
    }
}

// returns the starting address of a hex character in the emulator memory
// each digit is 5 bytes long
fn font_digit_address(digit: u8) -> u8 {
    return FONT_PC as u8 + (digit * 5);
}
