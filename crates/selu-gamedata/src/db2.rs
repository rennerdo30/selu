//! WDC5 fixed records with scalar bitpacking/palettes. Other compression and
//! sparse/encrypted sections fail closed.
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
    pub width: f64,
    pub height: f64,
    pub sheet_width: u32,
    pub sheet_height: u32,
    pub raw_width: u32,
    pub raw_height: u32,
    pub tiles_horizontally: bool,
    pub tiles_vertically: bool,
}
/// FileDataIDs are assigned once by Blizzard and shared by every product.
/// Checked 2026-11-07 against the installed wow, wowf, wow_classic,
/// wow_classic_era and wow_anniversary clients: where a client's root still
/// names `DBFilesClient/<table>.db2` (the Classic clients), the name resolves
/// to these IDs; retail and Forever roots carry no names for these tables, so
/// the ID is the only lookup there.
pub const TABLES: [(&str, u32); 3] = [
    ("UiTextureAtlas", 897470),
    ("UiTextureAtlasMember", 897532),
    ("UiTextureAtlasElement", 1989276),
];
fn scalar(bytes: &[u8], record: usize, (bit, width, signed): (usize, usize, bool)) -> Result<u32> {
    let shift = bit % 8;
    let source = slice(bytes, record + bit / 8, (shift + width).div_ceil(8))?;
    let packed = source
        .iter()
        .enumerate()
        .fold(0u64, |value, (index, byte)| {
            value | (u64::from(*byte) << (index * 8))
        });
    let mut value = ((packed >> shift) & ((1u64 << width) - 1)) as u32;
    if signed && width < 32 && value & (1u32 << (width - 1)) != 0 {
        value |= u32::MAX << width;
    }
    Ok(value)
}

pub struct Table<'a> {
    bytes: &'a [u8],
    pub layout: u32,
    offset: usize,
    count: usize,
    size: usize,
    fields: Vec<(usize, usize, bool)>,
    palettes: Vec<Option<Vec<u32>>>,
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
        let mut palettes = Vec::new();
        let mut pallet_at = 244 + n * 4 + le(b, 188)? as usize;
        let pallet_end = pallet_at
            .checked_add(le(b, 196)? as usize)
            .ok_or("DB2 palette overflow")?;
        if le(b, 196)? != 0 && (le(b, 188)? as usize != n * 24 || pallet_end > offset) {
            return Err("invalid DB2 palette metadata layout".into());
        }
        slice(b, pallet_at, pallet_end - pallet_at)?;
        for i in 0..n {
            let p = 244 + n * 4 + i * 24;
            let c = slice(b, p, 24)?;
            let bit = u16::from_le_bytes(c[..2].try_into()?) as usize;
            let width = u16::from_le_bytes(c[2..4].try_into()?) as usize;
            // WDC5 signed scalar bitpacking (UiCanvas dimensions and colors).
            // Common and array encodings still fail closed.
            let signed = le(c, 8)? == 5
                && le(c, 4)? == 0
                && (le(c, 12)? as usize == bit || (width == 32 && le(c, 12)? == 0))
                && le(c, 16)? as usize == width
                && le(c, 20)? == 0;
            let plain = le(c, 8)? == 0 && bit.is_multiple_of(8) && [8, 16, 32].contains(&width);
            let pallet = le(c, 8)? == 3
                && le(c, 4)?.is_multiple_of(4)
                && le(c, 16)? as usize == width
                && le(c, 20)? == 0;
            if !(plain || signed || pallet) || width == 0 || width > 32 || bit + width > size * 8 {
                return Err("unsupported DB2 field storage".into());
            }
            palettes.push(if pallet {
                let len = le(c, 4)? as usize;
                let end = pallet_at.checked_add(len).ok_or("DB2 palette overflow")?;
                if end > pallet_end {
                    return Err("DB2 palette outside metadata".into());
                }
                let values = (pallet_at..end)
                    .step_by(4)
                    .map(|at| le(b, at))
                    .collect::<Result<Vec<_>>>()?;
                pallet_at = end;
                Some(values)
            } else {
                None
            });
            fields.push((bit, width, signed));
        }
        if pallet_at != pallet_end {
            return Err("unconsumed DB2 palette metadata".into());
        }
        let inline_id = if flags & 4 == 0 {
            Some((le(b, 172)? >> 16) as usize)
        } else {
            None
        };
        let mut ids = Vec::with_capacity(count);
        for row in 0..count {
            let id = if let Some(column) = inline_id {
                let field = *fields.get(column).ok_or("invalid inline ID column")?;
                let raw = scalar(b, offset + row * size, field)?;
                match &palettes[column] {
                    Some(values) => *values
                        .get(raw as usize)
                        .ok_or("DB2 ID palette index out of bounds")?,
                    None => raw,
                }
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
            palettes,
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
        let field = *self.fields.get(column).ok_or("column out of bounds")?;
        let raw = scalar(
            self.bytes,
            self.offset + self.source_rows[row] * self.size,
            field,
        )?;
        match &self.palettes[column] {
            Some(values) => values
                .get(raw as usize)
                .copied()
                .ok_or_else(|| "DB2 palette index out of bounds".into()),
            None => Ok(raw),
        }
    }

    pub fn text(&self, row: usize, column: usize) -> Result<&str> {
        let rel = self.value(row, column)? as i32;
        let field = self.offset + self.source_rows[row] * self.size + self.fields[column].0 / 8;
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
/// GlobalStrings D40F6D96 stores non-inline ID, BaseTag, TagText_lang, Flags.
/// The strings are runtime client content; only the schema lives here.
pub fn global_strings(bytes: &[u8]) -> Result<Vec<(String, String)>> {
    let table = Table::parse(bytes)?;
    if table.layout != 0xd40f6d96 || table.fields.len() != 3 || table.inline_id.is_some() {
        return Err("unsupported GlobalStrings schema".into());
    }
    let mut strings = Vec::with_capacity(table.count());
    for row in 0..table.count() {
        // Flags bit 0 selects FrameXML/game globals, bit 1 GlueXML.
        if table.value(row, 2)? & 1 == 0 {
            continue;
        }
        let key = table.text(row, 0)?;
        let text = table.text(row, 1)?;
        if !key.is_empty() {
            strings.push((key.to_owned(), text.to_owned()));
        }
    }
    if strings.is_empty() {
        return Err("GlobalStrings has no game strings".into());
    }
    Ok(strings)
}
pub(crate) struct GlobalStrings {
    strings: Vec<(String, String)>,
    content_hash: String,
}
impl Storage {
    /// Decoded text and its verified CASC content identity share one snapshot.
    pub fn global_strings(&self) -> Result<&[(String, String)]> {
        Ok(&self.global_strings_snapshot()?.strings)
    }
    pub fn global_strings_content_hash(&self) -> Result<&str> {
        Ok(&self.global_strings_snapshot()?.content_hash)
    }
    fn global_strings_snapshot(&self) -> Result<&GlobalStrings> {
        let result = self.global_strings_cache.get_or_init(|| {
            let id = self
                .resolve("DBFilesClient/GlobalStrings.db2")
                .unwrap_or(1394440);
            self.read(id)
                .and_then(|bytes| {
                    use md5::{Digest, Md5};
                    Ok(GlobalStrings {
                        content_hash: hex::encode(Md5::digest(&bytes)),
                        strings: global_strings(&bytes)?,
                    })
                })
                .map_err(|error| error.to_string())
        });
        result.as_ref().map_err(|error| error.clone().into())
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
        let canvases = storage.read(1989363)?;
        let mut available = HashMap::new();
        Self::decode_available(
            &Table::parse(&a)?,
            &Table::parse(&m)?,
            &Table::parse(&e)?,
            Some(&Table::parse(&canvases)?),
            |file| {
                *available
                    .entry(file)
                    .or_insert_with(|| storage.has_local_file(file))
            },
        )
    }
    #[cfg(test)]
    fn decode(atlases: &Table<'_>, members: &Table<'_>, elements: &Table<'_>) -> Result<Self> {
        Self::decode_available(atlases, members, elements, None, |_| false)
    }
    fn decode_available(
        atlases: &Table<'_>,
        members: &Table<'_>,
        elements: &Table<'_>,
        canvases: Option<&Table<'_>>,
        mut available: impl FnMut(u32) -> bool,
    ) -> Result<Self> {
        // Verified against the installed 1.60.1.70790 tables; field orders are
        // documented by wowdev/WoWDBDefs definitions/UiTextureAtlas*.dbd.
        if atlases.layout != 0x91378408
            || members.layout != 0x75e12f73
            || elements.layout != 0x7ccbd18f
        {
            return Err("unsupported atlas DB2 layout hash".into());
        }
        let mut canvas_sizes = HashMap::new();
        if let Some(canvases) = canvases {
            if canvases.layout != 0x9ca2cfb4 {
                return Err("unsupported UiCanvas DB2 layout hash".into());
            }
            for row in 0..canvases.count() {
                let size = (canvases.value(row, 0)?, canvases.value(row, 1)?);
                if size.0 == 0 || size.1 == 0 {
                    return Err("invalid atlas canvas dimensions".into());
                }
                canvas_sizes.insert(canvases.id(row)?, size);
            }
        }
        let base = canvas_sizes
            .values()
            .copied()
            .min_by_key(|(width, height)| u64::from(*width) * u64::from(*height));
        let mut sheets = HashMap::new();
        for row in 0..atlases.count {
            sheets.insert(
                atlases.value(row, 0)?,
                (
                    atlases.value(row, 1)?,
                    atlases.value(row, 3)?,
                    atlases.value(row, 4)?,
                    atlases.value(row, 5)?,
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
                let (file, sw, sh, canvas) = sheets
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
                let density = match (base, canvas_sizes.get(&canvas)) {
                    (Some((base_width, base_height)), Some((width, height))) => (
                        f64::from(*width) / f64::from(base_width),
                        f64::from(*height) / f64::from(base_height),
                    ),
                    _ => (1.0, 1.0),
                };
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
                        f64::from(ow)
                    } else if raw_width > 0 {
                        f64::from(raw_width) / density.0
                    } else {
                        f64::from(right - left) / density.0
                    },
                    height: if oh > 0 {
                        f64::from(oh)
                    } else if raw_height > 0 {
                        f64::from(raw_height) / density.1
                    } else {
                        f64::from(bottom - top) / density.1
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
                        let replace = index.records.get(&key).is_none_or(|previous| {
                            previous.file_data_id != record.file_data_id
                                && !available(previous.file_data_id)
                                && available(record.file_data_id)
                        });
                        if replace {
                            index.records.insert(key, record.clone());
                        }
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
    fn scalar_palettes_and_runtime_global_string_columns() {
        let mut bytes = records(
            0xd40f6d96,
            &[vec![0, 0, 0], vec![0, 0, 1]],
            &[
                (0, 0, "SELF_WRITTEN_TAG"),
                (0, 1, "Synthetic %s — fixture"),
                (1, 0, "SELF_WRITTEN_GLUE"),
                (1, 1, "Glue fixture"),
            ],
        );
        let original = le(&bytes, 212).unwrap() as usize;
        bytes.splice(
            original..original,
            [1u32.to_le_bytes(), 2u32.to_le_bytes()].concat(),
        );
        bytes[212..216].copy_from_slice(&((original + 8) as u32).to_le_bytes());
        for (at, value) in [(172, 4u32), (188, 72), (196, 8), (228, 8)] {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        let field = 244 + 3 * 4 + 2 * 24;
        bytes[field + 2..field + 4].copy_from_slice(&2u16.to_le_bytes());
        for (at, value) in [(4, 8u32), (8, 3), (16, 2)] {
            bytes[field + at..field + at + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&7u32.to_le_bytes());
        bytes.extend_from_slice(&9u32.to_le_bytes());
        let table = Table::parse(&bytes).unwrap();
        assert_eq!(table.value(0, 2).unwrap(), 1);
        assert_eq!(table.value(1, 2).unwrap(), 2);
        assert_eq!(
            global_strings(&bytes).unwrap(),
            vec![("SELF_WRITTEN_TAG".into(), "Synthetic %s — fixture".into())]
        );
        bytes[original + 8 + 8] = 3;
        assert!(Table::parse(&bytes).unwrap().value(0, 2).is_err());
        assert!(global_strings(&bytes).is_err());
        bytes[field + 4..field + 8].copy_from_slice(&12u32.to_le_bytes());
        assert!(Table::parse(&bytes).is_err());
    }
    #[test]
    fn full_width_signed_color_storage_preserves_argb_bits() {
        let mut bytes = records(0, &[vec![1, 0xffffd200]], &[]);
        let at = 244 + 2 * 4 + 24;
        bytes[at + 8..at + 12].copy_from_slice(&5u32.to_le_bytes());
        bytes[at + 16..at + 20].copy_from_slice(&32u32.to_le_bytes());
        assert_eq!(
            Table::parse(&bytes).unwrap().value(0, 1).unwrap(),
            0xffffd200
        );
        bytes[at + 16..at + 20].copy_from_slice(&31u32.to_le_bytes());
        assert!(Table::parse(&bytes).is_err());
    }
    #[test]
    fn signed_bitpacked_scalars_cross_byte_boundaries_and_sign_extend() {
        let mut bytes = records(0, &[vec![1, 0]], &[]);
        let at = 244 + 2 * 4 + 24;
        bytes[at..at + 2].copy_from_slice(&35u16.to_le_bytes());
        bytes[at + 2..at + 4].copy_from_slice(&13u16.to_le_bytes());
        for (offset, value) in [(8, 5u32), (12, 35), (16, 13)] {
            bytes[at + offset..at + offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        let row = 244 + 2 * 28;
        bytes[row + 4..row + 8].copy_from_slice(&(2048u32 << 3).to_le_bytes());
        assert_eq!(Table::parse(&bytes).unwrap().value(0, 1).unwrap(), 2048);
        bytes[row + 4..row + 8].copy_from_slice(&(8191u32 << 3).to_le_bytes());
        assert_eq!(Table::parse(&bytes).unwrap().value(0, 1).unwrap(), u32::MAX);
        bytes[at + 16..at + 20].copy_from_slice(&14u32.to_le_bytes());
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
    fn atlas_canvas_density_changes_logical_size_without_changing_uvs() {
        let a = records(0x91378408, &[vec![12, 1234, 0, 256, 256, 2]], &[]);
        let m = records(
            0x75e12f73,
            &[
                vec![0, 7, 12, 78, 78, 10, 88, 20, 98, 9, 0, 0, 0, 0],
                vec![0, 8, 12, 78, 78, 10, 88, 20, 98, 10, 26, 26, 0, 0],
            ],
            &[],
        );
        let e = records(
            0x7ccbd18f,
            &[vec![0, 9], vec![0, 10]],
            &[(0, 0, "Density-Test"), (1, 0, "Explicit-Size")],
        );
        let mut c = records(0x9ca2cfb4, &[vec![1024, 768], vec![2048, 1536]], &[]);
        c[172..176].copy_from_slice(&4u32.to_le_bytes());
        c[228..232].copy_from_slice(&8u32.to_le_bytes());
        for id in [1u32, 2] {
            c.extend_from_slice(&id.to_le_bytes());
        }
        let index = AtlasIndex::decode_available(
            &Table::parse(&a).unwrap(),
            &Table::parse(&m).unwrap(),
            &Table::parse(&e).unwrap(),
            Some(&Table::parse(&c).unwrap()),
            |_| true,
        )
        .unwrap();
        let atlas = &index.records["density-test"];
        assert_eq!((atlas.width, atlas.height), (39.0, 39.0));
        assert_eq!(
            (atlas.left, atlas.right, atlas.top, atlas.bottom),
            (10, 88, 20, 98)
        );
        assert_eq!((atlas.raw_width, atlas.raw_height), (78, 78));
        assert_eq!(
            (
                index.records["explicit-size"].width,
                index.records["explicit-size"].height
            ),
            (26.0, 26.0)
        );
    }
    #[test]
    fn atlas_variants_prefer_installed_payload_and_keep_aliases() {
        let a = records(
            0x91378408,
            &[
                vec![12, 1234, 0, 256, 256, 0],
                vec![13, 5678, 1, 256, 256, 0],
            ],
            &[],
        );
        let m = records(
            0x75e12f73,
            &[
                vec![0, 7, 12, 32, 16, 10, 42, 20, 36, 9, 0, 0, 0, 0],
                vec![0, 8, 13, 32, 16, 50, 82, 60, 76, 9, 0, 0, 0, 0],
            ],
            &[(0, 0, "Alias"), (1, 0, "Alias")],
        );
        let e = records(0x7ccbd18f, &[vec![0, 9]], &[(0, 0, "Micro-Test")]);
        let (a, m, e) = (
            Table::parse(&a).unwrap(),
            Table::parse(&m).unwrap(),
            Table::parse(&e).unwrap(),
        );
        let available = AtlasIndex::decode_available(&a, &m, &e, None, |id| id == 5678).unwrap();
        for name in ["micro-test", "alias"] {
            assert_eq!(available.records[name].file_data_id, 5678);
            assert_eq!(available.records[name].left, 50);
        }
        let unavailable = AtlasIndex::decode(&a, &m, &e).unwrap();
        assert_eq!(unavailable.records["micro-test"].file_data_id, 1234);
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
            (1234, 10, 20, 64.0, 16.0)
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
