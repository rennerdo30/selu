//! WDC5 fixed-record tables. Unsupported compression and sparse/encrypted sections fail closed.
use crate::{
    Result, Storage,
    local::{le, slice},
};
#[derive(Debug)]
pub struct Atlas {
    pub name: String,
    pub file_data_id: u32,
    pub left: u32,
    pub right: u32,
    pub top: u32,
    pub bottom: u32,
    pub width: u32,
    pub height: u32,
    pub sheet_width: u32,
    pub sheet_height: u32,
}
pub const TABLES: [(&str, u32); 2] = [("UiTextureAtlas", 897470), ("UiTextureAtlasMember", 897532)];
pub struct Table<'a> {
    bytes: &'a [u8],
    pub layout: u32,
    offset: usize,
    count: usize,
    size: usize,
    fields: Vec<(usize, usize)>,
    string_start: usize,
    string_end: usize,
}
impl<'a> Table<'a> {
    pub fn parse(b: &'a [u8]) -> Result<Self> {
        slice(b, 0, 244)?;
        if &b[..4] != b"WDC5" || le(b, 200)? != 1 {
            return Err("requires WDC5 with one section".into());
        }
        if le(b, 172)? & 0xffff != 0 || b[204..212].iter().any(|v| *v != 0) {
            return Err("sparse or encrypted DB2 unsupported".into());
        }
        let n = le(b, 140)? as usize;
        let offset = le(b, 212)? as usize;
        let count = le(b, 216)? as usize;
        let size = le(b, 144)? as usize;
        if size == 0
            || n > 256
            || count != le(b, 136)? as usize
            || le(b, 236)? != 0
            || le(b, 240)? != 0
        {
            return Err("unsupported DB2 section layout".into());
        }
        let string_start = offset
            .checked_add(count.checked_mul(size).ok_or("DB2 overflow")?)
            .ok_or("DB2 overflow")?;
        let string_end = string_start + le(b, 220)? as usize;
        slice(
            b,
            offset,
            string_end.checked_sub(offset).ok_or("DB2 overflow")?,
        )?;
        let mut fields = Vec::new();
        for i in 0..n {
            let p = 244 + n * 4 + i * 24;
            let c = slice(b, p, 24)?;
            let bit = u16::from_le_bytes(c[..2].try_into()?) as usize;
            let width = u16::from_le_bytes(c[2..4].try_into()?) as usize;
            if le(c, 8)? != 0
                || !bit.is_multiple_of(8)
                || ![8, 16, 32].contains(&width)
                || bit + width > size * 8
            {
                return Err("unsupported DB2 field storage".into());
            }
            fields.push((bit / 8, width / 8));
        }
        Ok(Self {
            bytes: b,
            layout: le(b, 156)?,
            offset,
            count,
            size,
            fields,
            string_start,
            string_end,
        })
    }
    pub fn count(&self) -> usize {
        self.count
    }
    pub fn value(&self, row: usize, column: usize) -> Result<u32> {
        if row >= self.count {
            return Err("row out of bounds".into());
        }
        let (p, n) = *self.fields.get(column).ok_or("column out of bounds")?;
        let mut out = [0; 4];
        out[..n].copy_from_slice(slice(self.bytes, self.offset + row * self.size + p, n)?);
        Ok(u32::from_le_bytes(out))
    }
    pub fn text(&self, row: usize, column: usize) -> Result<&str> {
        let rel = self.value(row, column)? as i32;
        let field = self.offset + row * self.size + self.fields[column].0;
        if rel == 0 {
            return Ok("");
        }
        let p = usize::try_from(field as i64 + rel as i64)?;
        if p < self.string_start || p >= self.string_end {
            return Err("DB2 string outside table".into());
        }
        let b = &self.bytes[p..self.string_end];
        let n = b
            .iter()
            .position(|c| *c == 0)
            .ok_or("unterminated DB2 string")?;
        Ok(std::str::from_utf8(&b[..n])?)
    }
}
impl Storage {
    pub fn atlas(&self, name: &str) -> Result<Atlas> {
        let a = self.read(TABLES[0].1)?;
        let m = self.read(TABLES[1].1)?;
        let atlas = Table::parse(&a)?;
        let members = Table::parse(&m)?;
        if atlas.layout != 0x91378408 || members.layout != 0x75e12f73 {
            return Err("unsupported atlas DB2 layout hash".into());
        }
        for row in 0..members.count {
            if members.text(row, 0)?.eq_ignore_ascii_case(name) {
                let id = members.value(row, 2)?;
                for sheet in 0..atlas.count {
                    if atlas.value(sheet, 0)? == id {
                        let left = members.value(row, 5)?;
                        let right = members.value(row, 6)?;
                        let top = members.value(row, 7)?;
                        let bottom = members.value(row, 8)?;
                        let sw = atlas.value(sheet, 3)?;
                        let sh = atlas.value(sheet, 4)?;
                        if right < left || bottom < top || right > sw || bottom > sh {
                            return Err("atlas rectangle outside sheet".into());
                        }
                        let ow = members.value(row, 10)?;
                        let oh = members.value(row, 11)?;
                        return Ok(Atlas {
                            name: members.text(row, 0)?.into(),
                            file_data_id: atlas.value(sheet, 1)?,
                            left,
                            right,
                            top,
                            bottom,
                            width: if ow > 0 { ow } else { right - left },
                            height: if oh > 0 { oh } else { bottom - top },
                            sheet_width: sw,
                            sheet_height: sh,
                        });
                    }
                }
                return Err("atlas sheet missing".into());
            }
        }
        Err("atlas name absent from installed build".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut b = vec![0; 280];
        b[..4].copy_from_slice(b"WDC5");
        for (p, v) in [
            (136, 1u32),
            (140, 1),
            (144, 4),
            (200, 1),
            (212, 272),
            (216, 1),
            (220, 4),
        ] {
            b[p..p + 4].copy_from_slice(&v.to_le_bytes());
        }
        b[250..252].copy_from_slice(&32u16.to_le_bytes());
        b[272..276].copy_from_slice(&4u32.to_le_bytes());
        b[276..280].copy_from_slice(b"abc\0");
        b
    }
    #[test]
    fn wdc5_relative_strings() {
        let b = fixture();
        let t = Table::parse(&b).unwrap();
        assert_eq!(t.count(), 1);
        assert_eq!(t.text(0, 0).unwrap(), "abc");
        assert!(t.value(1, 0).is_err());
    }
    #[test]
    fn refuses_truncated_and_compressed() {
        let b = fixture();
        for n in 0..b.len() {
            assert!(Table::parse(&b[..n]).is_err());
        }
        let mut b = b;
        b[256..260].copy_from_slice(&1u32.to_le_bytes());
        assert!(Table::parse(&b).is_err());
    }
}
