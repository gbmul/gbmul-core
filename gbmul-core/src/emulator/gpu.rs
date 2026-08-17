// Picture Processing Unit (PPU/GPU)
// Handles rendering tiles, sprites, and background

pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;

pub struct Gpu {
    pub mode: GpuMode,
    pub line: u8,       // Current scanline (LY register)
    pub cycles: u32,    // Cycles in current mode
    pub framebuffer: [u8; SCREEN_WIDTH * SCREEN_HEIGHT], // Grayscale values 0-3
    bg_color_ids: [u8; SCREEN_WIDTH * SCREEN_HEIGHT], // Original BG color IDs (0-3) for sprite priority
    scanline_lcdc: u8, // LCDC value cached at start of scanline
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GpuMode {
    HBlank,       // Mode 0
    VBlank,       // Mode 1
    OamSearch,    // Mode 2
    Drawing,      // Mode 3
}

impl GpuMode {
    pub fn to_u8(self) -> u8 {
        match self {
            GpuMode::HBlank    => 0,
            GpuMode::VBlank    => 1,
            GpuMode::OamSearch => 2,
            GpuMode::Drawing   => 3,
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => GpuMode::HBlank,
            1 => GpuMode::VBlank,
            3 => GpuMode::Drawing,
            _ => GpuMode::OamSearch,
        }
    }
}

impl Gpu {
    pub fn new() -> Self {
        Gpu {
            mode: GpuMode::OamSearch,
            line: 0,
            cycles: 0,
            framebuffer: [0; SCREEN_WIDTH * SCREEN_HEIGHT], // All white initially
            bg_color_ids: [0; SCREEN_WIDTH * SCREEN_HEIGHT], // All 0 (transparent) initially
            scanline_lcdc: 0x91, // Default LCDC value
        }
    }
    
    pub fn step(&mut self, cycles: u32, memory: &mut super::Memory) {
        // Check if LCD is enabled (LCDC bit 7)
        let lcdc = memory.read(0xFF40);
        let lcd_enabled = (lcdc & 0x80) != 0;
        
        if !lcd_enabled {
            // When LCD is disabled, GPU stops and LY is held at 0
            // When LCD is disabled, GPU stops and LY is held at 0
            // Reset state when LCD turns off
            if self.line != 0 {
                self.line = 0;
                memory.write(0xFF44, 0);
                self.cycles = 0;
                self.mode = GpuMode::HBlank;
                // Clear STAT mode bits but keep interrupt enable bits
                self.update_stat(memory);
            }
            return;
        }
        
        self.cycles += cycles;
        
        match self.mode {
            GpuMode::OamSearch => {
                if self.cycles >= 80 {
                    self.cycles -= 80;
                    self.mode = GpuMode::Drawing;
                    self.update_stat(memory);
                }
            }
            GpuMode::Drawing => {
                if self.cycles >= 172 {
                    self.cycles -= 172;
                    self.mode = GpuMode::HBlank;
                    self.update_stat(memory);
                    // Render the current scanline
                    // Debug logging disabled
                    // if self.line < 3 && (self.scanline_lcdc & 0x81) == 0x81 {
                    //     log::info!("[GPU] Line {} rendering with cached LCDC=0x{:02X}", self.line, self.scanline_lcdc);
                    // }
                    self.render_scanline(memory);
                }
            }
            GpuMode::HBlank => {
                if self.cycles >= 204 {
                    self.cycles -= 204;
                    self.line += 1;
                    memory.write(0xFF44, self.line); // Update LY register
                    self.update_stat(memory);
                    
                    if self.line == 144 {
                        self.mode = GpuMode::VBlank;
                        self.update_stat(memory);
                        // Request VBlank interrupt (set bit 0 of IF register at 0xFF0F)
                        let if_reg = memory.read(0xFF0F);
                        memory.write(0xFF0F, if_reg | 0x01);
                        //log::debug!("[GPU] VBlank interrupt requested! IF: 0x{:02X} -> 0x{:02X}", if_reg, if_reg | 0x01);
                    } else {
                        self.mode = GpuMode::OamSearch;
                        // Cache LCDC when entering OAM Search for this scanline
                        let lcdc = memory.read(0xFF40);
                        self.scanline_lcdc = lcdc;
                        // Debug logging disabled
                        // if self.line < 3 && (lcdc & 0x81) == 0x81 {  // LCD & BG both on
                        //     log::info!("[GPU] Line {} entering OAM Search, cached LCDC=0x{:02X} (LCD={}, BG={})",
                        //         self.line, lcdc,
                        //         if lcdc & 0x80 != 0 { "ON" } else { "OFF" },
                        //         if lcdc & 0x01 != 0 { "ON" } else { "OFF" });
                        // }
                        self.update_stat(memory);
                    }
                }
            }
            GpuMode::VBlank => {
                if self.cycles >= 456 {
                    self.cycles -= 456;
                    self.line += 1;
                    memory.write(0xFF44, self.line); // Update LY register
                    self.update_stat(memory);
                    
                    if self.line > 153 {
                        self.line = 0;
                        memory.write(0xFF44, self.line); // Update LY register
                        self.mode = GpuMode::OamSearch;
                        // Cache LCDC when entering OAM Search for new frame
                        let lcdc = memory.read(0xFF40);
                        self.scanline_lcdc = lcdc;
                        self.update_stat(memory);
                    }
                }
            }
        }
    }
    
    fn render_scanline(&mut self, memory: &super::Memory) {
        if self.line >= 144 {
            return;
        }
        
        // Simple background rendering
        // Read background palette from IO register (0xFF47)
        let bgp = memory.read(0xFF47);
        // Use cached LCDC from start of scanline, not current value
        let lcdc = self.scanline_lcdc;
        
        // Check if LCD is enabled (bit 7)
        if (lcdc & 0x80) == 0 {
            // LCD is off, fill with white
            for x in 0..SCREEN_WIDTH {
                let offset = (self.line as usize) * SCREEN_WIDTH + x;
                self.framebuffer[offset] = 0;
            }
            return;
        }
        
        // Check if background is enabled (bit 0)
        if (lcdc & 0x01) == 0 {
            // Background disabled, fill with white  
            for x in 0..SCREEN_WIDTH {
                let offset = (self.line as usize) * SCREEN_WIDTH + x;
                self.framebuffer[offset] = 0;
            }
            return;
        }
        
        // Read scroll registers
        let scy = memory.read(0xFF42);
        let scx = memory.read(0xFF43);
        
        // Debug logging disabled - game is working now
        // if self.line <= 1 {
        //     // Sample first 8 tilemap entries
        //     let samples: Vec<String> = (0..8).map(|i| format!("{:02X}", memory.read(0x9800 + i))).collect();
        //     log::info!("[GPU] Rendering line {}: LCDC=0x{:02X}, BGP=0x{:02X}, SCX={}, SCY={}, TileMap[0-7]: {}", 
        //         self.line, lcdc, bgp, scx, scy, samples.join(" "));
        //     
        //     // Sample tilemap at multiple rows to see where content is
        //     for row in [0, 5, 10, 15] {
        //         let addr_start = 0x9800 + (row * 32);
        //         let samples: Vec<String> = (0..8).map(|i| format!("{:02X}", memory.read(addr_start + i))).collect();
        //         let non_blank = (0..32).filter(|&i| memory.read(addr_start + i) != 0x2F).count();
        //         log::info!("[GPU]   TileMap row {}: {} non-blank tiles, samples: {}", row, non_blank, samples.join(" "));
        //     }
        //     
        //     // Check where non-zero VRAM data is
        //     let vram_ranges = [
        //         ("Tile 0", 0x8000, 0x8010),  // Tile 0 specifically
        //         ("Tile 0-7", 0x8000, 0x8080),
        //         ("Tiles around 0x2F", 0x82E0, 0x8320),
        //         ("Last tiles", 0x8F80, 0x9000),
        //     ];
        //     for (name, start, end) in &vram_ranges {
        //         let non_zero = ((*start)..(*end)).filter(|&addr| memory.read(addr) != 0).count();
        //         let samples: Vec<String> = ((*start)..(start+16).min(*end)).map(|a| format!("{:02X}", memory.read(a))).collect();
        //         log::info!("[GPU]   {}: {}/{} non-zero, data: {}", name, non_zero, end - start, samples.join(" "));
        //     }
        // }
        
        // Render each pixel of the scanline
        for x in 0..SCREEN_WIDTH {
            let pixel_x = (x as u8).wrapping_add(scx);
            let pixel_y = self.line.wrapping_add(scy);
            
            // Get tile coordinates
            let tile_x = (pixel_x / 8) as usize;
            let tile_y = (pixel_y / 8) as usize;
            
            // Get tile from background map; LCDC bit 3 selects $9C00 vs $9800
            let bg_map_base: usize = if (lcdc & 0x08) != 0 { 0x9C00 } else { 0x9800 };
            let tile_map_addr = bg_map_base + ((tile_y % 32) * 32) + (tile_x % 32);
            let tile_num = memory.read(tile_map_addr as u16);
            
            // Get tile data from VRAM - addressing mode depends on LCDC bit 4
            let tile_addr = if (lcdc & 0x10) != 0 {
                // Set1: Unsigned addressing [0x8000-0x8FFF]
                0x8000 + (tile_num as u16 * 16)
            } else {
                // Set0: Signed addressing [0x8800-0x97FF]
                // Tile 0 is at 0x9000, negative indices go to 0x8800-0x8FFF
                if tile_num < 128 {
                    0x9000 + (tile_num as u16 * 16)
                } else {
                    0x8800 + ((tile_num - 128) as u16 * 16)
                }
            };
            
            // Get pixel within tile
            let tile_pixel_y = (pixel_y % 8) as u16;
            let tile_pixel_x = pixel_x % 8;
            
            // Each tile row is 2 bytes
            let byte1 = memory.read(tile_addr + tile_pixel_y * 2);
            let byte2 = memory.read(tile_addr + tile_pixel_y * 2 + 1);
            
            // Extract color ID (2 bits)
            let bit_pos = 7 - tile_pixel_x;
            let color_id = (((byte2 >> bit_pos) & 1) << 1) | ((byte1 >> bit_pos) & 1);
            
            // Apply palette
            let color = (bgp >> (color_id * 2)) & 0x03;
            
            // Write to framebuffer and store original color ID for sprite priority
            let offset = (self.line as usize) * SCREEN_WIDTH + x;
            self.framebuffer[offset] = color;
            self.bg_color_ids[offset] = color_id;
            
            // Debug logging disabled
            // if self.line <= 1 && x < 8 {
            //     log::info!("[GPU]   Line {} Pixel {}: tile_map_addr=0x{:04X}, tile_num=0x{:02X}, tile_addr=0x{:04X}, byte1=0x{:02X}, byte2=0x{:02X}, color_id={}, color={}, FB[{}]={}",
            //         self.line, x, tile_map_addr, tile_num, tile_addr, byte1, byte2, color_id, color, offset, self.framebuffer[offset]);
            // }
        }
        
        // Render window layer if enabled (LCDC bit 5); window map: LCDC bit 6
        if (lcdc & 0x20) != 0 {
            let wy = memory.read(0xFF4A);
            let wx = memory.read(0xFF4B).wrapping_sub(7);
            if self.line >= wy {
                let win_map_base: usize = if (lcdc & 0x40) != 0 { 0x9C00 } else { 0x9800 };
                let win_y = (self.line - wy) as usize;
                let tile_row = win_y / 8;
                let tile_pixel_y = (win_y % 8) as u16;
                for x in 0..SCREEN_WIDTH {
                    let screen_x = wx as usize + x;
                    if screen_x >= SCREEN_WIDTH {
                        break;
                    }
                    let tile_col = x / 8;
                    let tile_pixel_x = (x % 8) as u8;
                    let tile_map_addr = win_map_base + (tile_row % 32) * 32 + (tile_col % 32);
                    let tile_num = memory.read(tile_map_addr as u16);
                    let tile_addr = if (lcdc & 0x10) != 0 {
                        0x8000 + (tile_num as u16 * 16)
                    } else if tile_num < 128 {
                        0x9000 + (tile_num as u16 * 16)
                    } else {
                        0x8800 + ((tile_num - 128) as u16 * 16)
                    };
                    let byte1 = memory.read(tile_addr + tile_pixel_y * 2);
                    let byte2 = memory.read(tile_addr + tile_pixel_y * 2 + 1);
                    let bit_pos = 7 - tile_pixel_x;
                    let color_id = (((byte2 >> bit_pos) & 1) << 1) | ((byte1 >> bit_pos) & 1);
                    let color = (bgp >> (color_id * 2)) & 0x03;
                    let offset = self.line as usize * SCREEN_WIDTH + screen_x;
                    self.framebuffer[offset] = color;
                    self.bg_color_ids[offset] = color_id;
                }
            }
        }

        // Render sprites for this scanline if sprites are enabled (LCDC bit 1)
        if (lcdc & 0x02) != 0 {
            self.render_sprites(memory, lcdc);
        }
    }
    
    /// Render sprites for the current scanline
    fn render_sprites(&mut self, memory: &super::Memory, lcdc: u8) {
        let sprite_height = if (lcdc & 0x04) != 0 { 16 } else { 8 };
        let obp0 = memory.read(0xFF48); // Sprite palette 0
        let obp1 = memory.read(0xFF49); // Sprite palette 1
        
        // Scan OAM for sprites on this line (0xFE00-0xFE9F, 40 sprites max)
        let mut sprites_on_line = Vec::new();
        
        for sprite_num in 0..40 {
            let oam_addr = 0xFE00 + (sprite_num * 4);
            let oam_y = memory.read(oam_addr);
            let oam_x = memory.read(oam_addr + 1);
            let tile_num = memory.read(oam_addr + 2);
            let attrs = memory.read(oam_addr + 3);
            
            // Sprite is only visible if Y is in range [16, 160)
            // Screen Y = OAM Y - 16 (so OAM Y=16 means top of screen)
            if oam_y == 0 || oam_y >= 160 {
                continue;
            }
            
            let y = oam_y.wrapping_sub(16);
            let x = oam_x.wrapping_sub(8);
            
            // Check if sprite is on this scanline
            if self.line >= y && self.line < y.wrapping_add(sprite_height) {
                sprites_on_line.push((x, y, tile_num, attrs, sprite_num));
            }
            
            // Game Boy only renders 10 sprites per scanline
            if sprites_on_line.len() >= 10 {
                break;
            }
        }
        
        // Render sprites in reverse order (lowest X has priority)
        for (sprite_x, sprite_y, tile_num, attrs, _sprite_num) in sprites_on_line.iter().rev() {
            let palette = if (attrs & 0x10) != 0 { obp1 } else { obp0 };
            let flip_x = (attrs & 0x20) != 0;
            let flip_y = (attrs & 0x40) != 0;
            let behind_bg = (attrs & 0x80) != 0;
            
            // Calculate which line of the sprite we're rendering
            let mut sprite_line = self.line.wrapping_sub(*sprite_y);
            if flip_y {
                sprite_line = (sprite_height - 1).wrapping_sub(sprite_line);
            }
            
            // Get tile data
            let tile_addr = 0x8000 + (*tile_num as u16 * 16) + (sprite_line as u16 * 2);
            let byte1 = memory.read(tile_addr);
            let byte2 = memory.read(tile_addr + 1);
            
            // Render 8 pixels of the sprite
            for px in 0..8 {
                let screen_x = sprite_x.wrapping_add(px);
                
                // Skip if off screen
                if screen_x >= SCREEN_WIDTH as u8 {
                    continue;
                }
                
                // Get pixel bit position
                let bit_pos = if flip_x { px } else { 7 - px };
                let color_id = (((byte2 >> bit_pos) & 1) << 1) | ((byte1 >> bit_pos) & 1);
                
                // Color 0 is transparent for sprites
                if color_id == 0 {
                    continue;
                }
                
                let offset = (self.line as usize) * SCREEN_WIDTH + (screen_x as usize);
                
                // Check priority - sprite is behind BG if priority bit set and BG color ID is not 0
                if behind_bg && self.bg_color_ids[offset] != 0 {
                    continue;
                }
                
                // Apply palette and write pixel
                let color = (palette >> (color_id * 2)) & 0x03;
                self.framebuffer[offset] = color;
            }
        }
    }
    
    /// Update STAT register (0xFF41) with current mode and LY==LYC flag
    fn update_stat(&self, memory: &mut super::Memory) {
        let stat = memory.read_direct(0xFF41);
        let lyc = memory.read(0xFF45);
        
        // Keep bits 6-3 (interrupt enables), update bits 2-0
        let mut new_stat = stat & 0xF8;
        
        // Set LY==LYC flag (bit 2)
        if self.line == lyc {
            new_stat |= 0x04;
        }
        
        // Set mode bits (1-0)
        let mode_bits = match self.mode {
            GpuMode::HBlank => 0,
            GpuMode::VBlank => 1,
            GpuMode::OamSearch => 2,
            GpuMode::Drawing => 3,
        };
        new_stat |= mode_bits;
        
        memory.write_direct(0xFF41, new_stat);
    }
    
    /// Get reference to the framebuffer
    pub fn get_framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }
    
    /// Render a test pattern (for debugging)
    pub fn render_test_pattern(&mut self) {
        for y in 0..SCREEN_HEIGHT {
            for x in 0..SCREEN_WIDTH {
                let idx = y * SCREEN_WIDTH + x;
                // Create a gradient pattern
                let shade = ((x + y) / 80) as u8 % 4;
                self.framebuffer[idx] = shade;
            }
        }
    }
}
