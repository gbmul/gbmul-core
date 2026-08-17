// Cartridge handler - ROM loading and MBC support

pub struct Cartridge {
    data: Vec<u8>,
    mbc_type: MbcType,
}

#[derive(Debug, Clone, Copy)]
pub enum MbcType {
    None,
    Mbc1,
    Mbc3,
    Mbc5,
}

impl Cartridge {
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, String> {
        if data.len() < 0x150 {
            return Err("ROM too small".to_string());
        }
        
        // Read cartridge type from header
        let cart_type = data[0x0147];
        let mbc_type = match cart_type {
            0x00 => MbcType::None,
            0x01..=0x03 => MbcType::Mbc1,
            0x0F..=0x13 => MbcType::Mbc3,
            0x19..=0x1E => MbcType::Mbc5,
            _ => MbcType::None,
        };
        
        Ok(Cartridge { data, mbc_type })
    }
    
    pub fn get_rom_data(&self) -> &[u8] {
        &self.data
    }
}
