use std::fmt::{self, Debug};

const HEX: &[u8] = b"0123456789abcdef";

pub struct Uuid([u8; 16]);

impl Uuid {
    pub fn new(uuid: [u8; 16]) -> Self {
        Uuid(uuid)
    }

    fn hi(&self, i: usize) -> u8 {
        HEX[((self.0[i] >> 4) & 0xF) as usize]
    }

    fn lo(&self, i: usize) -> u8 {
        HEX[(self.0[i] & 0xF) as usize]
    }
}

impl Debug for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let print_index = |f: &mut fmt::Formatter<'_>, k: usize| {
            // hi
            f.write_str(&(self.hi(k) as char).to_string())?;
            // lo
            f.write_str(&(self.lo(k) as char).to_string())?;
            Ok(())
        };

        for i in 0..4 {
            print_index(f, i)?;
        }
        f.write_str("-")?;
        for i in 4..6 {
            print_index(f, i)?;
        }
        f.write_str("-")?;
        for i in 6..8 {
            print_index(f, i)?;
        }
        f.write_str("-")?;
        for i in 8..10 {
            print_index(f, i)?;
        }
        f.write_str("-")?;
        for i in 10..16 {
            print_index(f, i)?;
        }

        Ok(())
    }
}
