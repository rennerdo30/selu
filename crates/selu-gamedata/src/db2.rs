//! WDC5 fixed-record tables. Unsupported compression and sparse/encrypted sections fail closed.
use crate::{
    Result, Storage,
    local::{le, slice},
};
use std::collections::HashMap;
#[derive(Clone, Debug)]
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
    pub raw_width: u32,
    pub raw_height: u32,
    pub tiles_horizontally: bool,
    pub tiles_vertically: bool,
}
pub const TABLES: [(&str, u32); 3] = [
    ("UiTextureAtlas", 897470),
    ("UiTextureAtlasMember", 897532),
    ("UiTextureAtlasElement", 1989276),
];
pub struct Table<'a> {
    bytes: &'a [u8],
    pub layout: u32,
    offset: usize,
    count: usize,
    size: usize,
    fields: Vec<(usize, usize)>,
    string_start: usize,
    string_end: usize,
    source_rows: Vec<usize>,
    ids: Vec<u32>,
    inline_id: Option<usize>,
}
impl<'a> Table<'a> {
    pub fn parse(b: &'a [u8]) -> Result<Self> {
        slice(b, 0, 244)?;
        if &b[..4] != b"WDC5" || le(b, 200)? != 1 {
            return Err("requires WDC5 with one section".into());
        }
        let flags = le(b, 172)? & 0xffff;
        if flags & !4 != 0 || b[204..212].iter().any(|v| *v != 0) {
            return Err("sparse or encrypted DB2 unsupported".into());
        }
        let n = le(b, 140)? as usize;
        let offset = le(b, 212)? as usize;
        let count = le(b, 216)? as usize;
        let size = le(b, 144)? as usize;
        if size == 0 || n > 256 || count != le(b, 136)? as usize || le(b, 236)? != 0 {
            return Err("unsupported DB2 section layout".into());
        }
        let string_start = offset
            .checked_add(count.checked_mul(size).ok_or("DB2 overflow")?)
            .ok_or("DB2 overflow")?;
        let string_end = string_start + le(b, 220)? as usize;
        let ids = le(b, 228)? as usize;
        if (flags & 4 != 0 && ids != count.checked_mul(4).ok_or("DB2 id list overflow")?)
            || (flags & 4 == 0 && ids != 0)
        {
            return Err("unsupported DB2 id list".into());
        }
        slice(b, string_end, ids)?;
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
            // A full-width signed bitpacked scalar has the same 32 raw bits
            // as an uncompressed scalar (GlobalColor.Color in build 70790).
            let full_width_signed = le(c, 8)? == 5
                && width == 32
                && le(c, 4)? == 0
                && le(c, 12)? == 0
                && le(c, 16)? == 32
                && le(c, 20)? == 0;
            if (le(c, 8)? != 0 && !full_width_signed)
                || !bit.is_multiple_of(8)
                || ![8, 16, 32].contains(&width)
                || bit + width > size * 8
            {
                return Err("unsupported DB2 field storage".into());
            }
            fields.push((bit / 8, width / 8));
        }
        let inline_id = if flags & 4 == 0 {
            Some((le(b, 172)? >> 16) as usize)
        } else {
            None
        };
        let mut ids = Vec::with_capacity(count);
        for row in 0..count {
            let id = if let Some(column) = inline_id {
                let (column_offset, width) =
                    *fields.get(column).ok_or("invalid inline ID column")?;
                let mut value = [0; 4];
                value[..width].copy_from_slice(slice(
                    b,
                    offset + row * size + column_offset,
                    width,
                )?);
                u32::from_le_bytes(value)
            } else {
                le(b, string_end + row * 4)?
            };
            ids.push(id);
        }
        let mut source_rows: Vec<_> = (0..count).collect();
        let mut by_id: HashMap<_, _> = ids.iter().enumerate().map(|(row, id)| (*id, row)).collect();
        let copies = le(b, 240)? as usize;
        let copy_start = string_end
            .checked_add(le(b, 228)? as usize)
            .ok_or("DB2 copy offset overflow")?;
        slice(
            b,
            copy_start,
            copies.checked_mul(8).ok_or("DB2 copy count overflow")?,
        )?;
        for index in 0..copies {
            let at = copy_start + index * 8;
            let id = le(b, at)?;
            let source = le(b, at + 4)?;
            let row = *by_id
                .get(&source)
                .ok_or("DB2 copy refers to missing record")?;
            if by_id.insert(id, row).is_some() {
                return Err("duplicate DB2 copy ID".into());
            }
            source_rows.push(row);
            ids.push(id);
        }
        Ok(Self {
            bytes: b,
            layout: le(b, 156)?,
            offset,
            count: source_rows.len(),
            size,
            fields,
            string_start,
            string_end,
            source_rows,
            ids,
            inline_id,
        })
    }
    pub fn count(&self) -> usize {
        self.count
    }
    pub fn id(&self, row: usize) -> Result<u32> {
        self.ids
            .get(row)
            .copied()
            .ok_or_else(|| "row out of bounds".into())
    }
    pub fn value(&self, row: usize, column: usize) -> Result<u32> {
        if row >= self.count {
            return Err("row out of bounds".into());
        }
        if self.inline_id == Some(column) {
            return Ok(self.ids[row]);
        }
        let (p, n) = *self.fields.get(column).ok_or("column out of bounds")?;
        let mut out = [0; 4];
        out[..n].copy_from_slice(slice(
            self.bytes,
            self.offset + self.source_rows[row] * self.size + p,
            n,
        )?);
        Ok(u32::from_le_bytes(out))
    }
    pub fn text(&self, row: usize, column: usize) -> Result<&str> {
        let rel = self.value(row, column)? as i32;
        let field = self.offset + self.source_rows[row] * self.size + self.fields[column].0;
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
/// Owned decoded records, scoped to Storage's immutable product/build snapshot.
pub(crate) struct AtlasIndex {
    records: HashMap<String, Atlas>,
    unavailable: HashMap<String, String>,
}
impl AtlasIndex {
    fn load(storage: &Storage) -> Result<Self> {
        let a = storage.read(TABLES[0].1)?;
        let m = storage.read(TABLES[1].1)?;
        let e = storage.read(TABLES[2].1)?;
        Self::decode(&Table::parse(&a)?, &Table::parse(&m)?, &Table::parse(&e)?)
    }
    fn decode(atlases: &Table<'_>, members: &Table<'_>, elements: &Table<'_>) -> Result<Self> {
        // Verified against the installed 1.60.1.70790 tables; field orders are
        // documented by wowdev/WoWDBDefs definitions/UiTextureAtlas*.dbd.
        if atlases.layout != 0x91378408
            || members.layout != 0x75e12f73
            || elements.layout != 0x7ccbd18f
        {
            return Err("unsupported atlas DB2 layout hash".into());
        }
        let mut sheets = HashMap::new();
        for row in 0..atlases.count {
            sheets.insert(
                atlases.value(row, 0)?,
                (
                    atlases.value(row, 1)?,
                    atlases.value(row, 3)?,
                    atlases.value(row, 4)?,
                ),
            );
        }
        let mut names = HashMap::new();
        for row in 0..elements.count {
            names.insert(elements.value(row, 1)?, elements.text(row, 0)?);
        }
        let mut index = Self {
            records: HashMap::new(),
            unavailable: HashMap::new(),
        };
        for row in 0..members.count {
            let committed = members.text(row, 0)?;
            let element = names.get(&members.value(row, 9)?).copied().unwrap_or("");
            let name = if element.is_empty() {
                committed
            } else {
                element
            };
            if name.is_empty() {
                continue;
            }
            let record = (|| -> Result<Atlas> {
                let (file, sw, sh) = sheets
                    .get(&members.value(row, 2)?)
                    .copied()
                    .ok_or("atlas sheet missing")?;
                let (left, right, top, bottom) = (
                    members.value(row, 5)?,
                    members.value(row, 6)?,
                    members.value(row, 7)?,
                    members.value(row, 8)?,
                );
                if sw == 0 || sh == 0 || right < left || bottom < top || right > sw || bottom > sh {
                    return Err("atlas rectangle outside sheet".into());
                }
                let (ow, oh, flags) = (
                    members.value(row, 10)?,
                    members.value(row, 11)?,
                    members.value(row, 12)?,
                );
                let raw_width = members.value(row, 3)?;
                let raw_height = members.value(row, 4)?;
                Ok(Atlas {
                    name: name.into(),
                    file_data_id: file,
                    left,
                    right,
                    top,
                    bottom,
                    width: if ow > 0 {
                        ow
                    } else if raw_width > 0 {
                        raw_width
                    } else {
                        right - left
                    },
                    height: if oh > 0 {
                        oh
                    } else if raw_height > 0 {
                        raw_height
                    } else {
                        bottom - top
                    },
                    sheet_width: sw,
                    sheet_height: sh,
                    raw_width,
                    raw_height,
                    tiles_horizontally: flags & 4 != 0 && flags & 16 == 0,
                    tiles_vertically: flags & 2 != 0 && flags & 16 == 0,
                })
            })();
            for alias in [name, committed]
                .into_iter()
                .filter(|name| !name.is_empty())
            {
                let key = alias.to_ascii_lowercase();
                match &record {
                    Ok(record) => {
                        index.records.entry(key).or_insert_with(|| record.clone());
                    }
                    Err(error) => {
                        index.unavailable.insert(key, error.to_string());
                    }
                }
            }
        }
        Ok(index)
    }
}
impl Storage {
    pub fn atlas(&self, name: &str) -> Result<Atlas> {
        let index = self
            .atlas_cache
            .get_or_init(|| AtlasIndex::load(self).map_err(|error| error.to_string()));
        let index = index.as_ref().map_err(|error| error.to_owned())?;
        let key = name.to_ascii_lowercase();
        if let Some(atlas) = index.records.get(&key) {
            return Ok(atlas.clone());
        }
        Err(index
            .unavailable
            .get(&key)
            .cloned()
            .unwrap_or_else(|| "atlas name absent from installed build".into())
            .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_width_signed_color_storage_preserves_argb_bits() {
        let mut bytes=records(0,&[vec![1,0xffffd200]],&[]);
        let at=244+2*4+24;
        bytes[at+8..at+12].copy_from_slice(&5u32.to_le_bytes());
        bytes[at+16..at+20].copy_from_slice(&32u32.to_le_bytes());
        assert_eq!(Table::parse(&bytes).unwrap().value(0,1).unwrap(),0xffffd200);
        bytes[at+16..at+20].copy_from_slice(&31u32.to_le_bytes());
        assert!(Table::parse(&bytes).is_err());
    }
    #[test]
    fn copied_rows_keep_source_string_offsets_and_alias_ids() {
        let mut bytes = records(0, &[vec![7, 0]], &[(0, 1, "Copied label")]);
        bytes[240..244].copy_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&9u32.to_le_bytes());
        bytes.extend_from_slice(&7u32.to_le_bytes());
        let table = Table::parse(&bytes).unwrap();
        assert_eq!(table.count(), 2);
        assert_eq!(table.value(1, 0).unwrap(), 9);
        assert_eq!(table.text(1, 1).unwrap(), "Copied label");
        bytes.pop();
        assert!(Table::parse(&bytes).is_err());
        let mut bytes = records(0, &[vec![0]], &[(0, 0, "Noninline label")]);
        bytes[172..176].copy_from_slice(&4u32.to_le_bytes());
        bytes[228..232].copy_from_slice(&4u32.to_le_bytes());
        bytes[240..244].copy_from_slice(&1u32.to_le_bytes());
        for value in [7u32, 9, 7] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        let table = Table::parse(&bytes).unwrap();
        assert_eq!(table.id(1).unwrap(), 9);
        assert_eq!(table.text(1, 0).unwrap(), "Noninline label");
    }
    fn records(layout: u32, rows: &[Vec<u32>], names: &[(usize, usize, &str)]) -> Vec<u8> {
        let columns = rows[0].len();
        let offset = 244 + columns * 28;
        let size = columns * 4;
        let mut bytes = vec![0; offset + rows.len() * size];
        bytes[..4].copy_from_slice(b"WDC5");
        for (position, value) in [
            (136, rows.len() as u32),
            (140, columns as u32),
            (144, size as u32),
            (156, layout),
            (200, 1),
            (212, offset as u32),
            (216, rows.len() as u32),
        ] {
            bytes[position..position + 4].copy_from_slice(&value.to_le_bytes());
        }
        for column in 0..columns {
            let position = 244 + columns * 4 + column * 24;
            bytes[position..position + 2].copy_from_slice(&((column * 32) as u16).to_le_bytes());
            bytes[position + 2..position + 4].copy_from_slice(&32u16.to_le_bytes());
        }
        for (row, values) in rows.iter().enumerate() {
            for (column, value) in values.iter().enumerate() {
                let position = offset + row * size + column * 4;
                bytes[position..position + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        let string_start = bytes.len();
        for &(row, column, name) in names {
            let position = offset + row * size + column * 4;
            let relative = (bytes.len() - position) as u32;
            bytes[position..position + 4].copy_from_slice(&relative.to_le_bytes());
            bytes.extend_from_slice(name.as_bytes());
            bytes.push(0);
        }
        let length = (bytes.len() - string_start) as u32;
        bytes[220..224].copy_from_slice(&length.to_le_bytes());
        bytes
    }
    #[test]
    fn atlas_names_join_elements_and_invalid_members_stay_local() {
        let a = records(0x91378408, &[vec![12, 1234, 0, 256, 256, 0]], &[]);
        let m = records(
            0x75e12f73,
            &[
                vec![0, 7, 12, 32, 16, 10, 42, 20, 36, 9, 64, 0, 6, 0],
                vec![0, 8, 12, 32, 16, 100, 350, 20, 36, 10, 0, 0, 0, 0],
            ],
            &[(0, 0, "LegacyAlias")],
        );
        let e = records(
            0x7ccbd18f,
            &[vec![0, 9], vec![0, 10]],
            &[(0, 0, "HUD-Test"), (1, 0, "Broken-HUD")],
        );
        let index = AtlasIndex::decode(
            &Table::parse(&a).unwrap(),
            &Table::parse(&m).unwrap(),
            &Table::parse(&e).unwrap(),
        )
        .unwrap();
        let atlas = &index.records["hud-test"];
        assert_eq!(
            (
                atlas.file_data_id,
                atlas.left,
                atlas.top,
                atlas.width,
                atlas.height
            ),
            (1234, 10, 20, 64, 16)
        );
        assert_eq!((atlas.raw_width, atlas.raw_height), (32, 16));
        assert!(atlas.tiles_horizontally && atlas.tiles_vertically);
        assert_eq!(index.records["legacyalias"].name, "HUD-Test");
        assert!(!index.records.contains_key("broken-hud"));
        assert!(index.unavailable["broken-hud"].contains("outside sheet"));
    }
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
