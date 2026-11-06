//! Read-only SavedVariables data importer. This module never evaluates Lua.
use crate::saved::SavedValue as V;
use std::{collections::BTreeMap, path::Path};

pub fn parse(file: &str, bytes: &[u8]) -> Result<BTreeMap<String, V>, String> {
    let mut p = Parser {
        b: bytes,
        i: 0,
        file,
    };
    let mut out = BTreeMap::new();
    while {
        p.space()?;
        p.i < bytes.len()
    } {
        let name = p.name()?;
        p.expect(b'=')?;
        out.insert(name, p.value(0)?);
        let _ = p.eat(b';')?;
    }
    Ok(out)
}
struct Parser<'a> {
    b: &'a [u8],
    i: usize,
    file: &'a str,
}
impl Parser<'_> {
    fn err(&self, message: &str) -> String {
        format!(
            "{}:{}: {message}",
            self.file,
            1 + self.b[..self.i].iter().filter(|&&c| c == b'\n').count()
        )
    }
    fn long(&mut self) -> Result<Option<Vec<u8>>, String> {
        if self.b.get(self.i) != Some(&b'[') {
            return Ok(None);
        }
        let start = self.i;
        let mut j = start + 1;
        while self.b.get(j) == Some(&b'=') {
            j += 1;
        }
        if self.b.get(j) != Some(&b'[') {
            return Ok(None);
        }
        let end = format!("]{}]", "=".repeat(j - start - 1));
        self.i = j + 1;
        if self.b.get(self.i) == Some(&b'\r') {
            self.i += 1;
        }
        if self.b.get(self.i) == Some(&b'\n') {
            self.i += 1;
        }
        let content = self.i;
        while self.i < self.b.len() {
            if self.b[self.i..].starts_with(end.as_bytes()) {
                let mut v = Vec::new();
                let mut cursor = content;
                while cursor < self.i {
                    let ch = self.b[cursor];
                    cursor += 1;
                    if ch == b'\r' {
                        if self.b.get(cursor) == Some(&b'\n') {
                            cursor += 1;
                        }
                        v.push(b'\n');
                    } else {
                        v.push(ch);
                    }
                }
                self.i += end.len();
                return Ok(Some(v));
            }
            self.i += 1;
        }
        Err(self.err("unterminated long string/comment"))
    }
    fn space(&mut self) -> Result<(), String> {
        loop {
            while self.b.get(self.i).is_some_and(u8::is_ascii_whitespace) {
                self.i += 1;
            }
            if !self.b[self.i..].starts_with(b"--") {
                return Ok(());
            }
            self.i += 2;
            if self.long()?.is_none() {
                while self.b.get(self.i).is_some_and(|c| *c != b'\n') {
                    self.i += 1;
                }
            }
        }
    }
    fn eat(&mut self, c: u8) -> Result<bool, String> {
        self.space()?;
        if self.b.get(self.i) == Some(&c) {
            self.i += 1;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.eat(c)? {
            Ok(())
        } else {
            Err(self.err(&format!("expected {}", c as char)))
        }
    }
    fn name(&mut self) -> Result<String, String> {
        self.space()?;
        let start = self.i;
        if !self
            .b
            .get(self.i)
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        {
            return Err(self.err("expected name"));
        }
        self.i += 1;
        while self
            .b
            .get(self.i)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
        {
            self.i += 1;
        }
        Ok(String::from_utf8_lossy(&self.b[start..self.i]).into_owned())
    }
    fn value(&mut self, depth: usize) -> Result<V, String> {
        if depth > 128 {
            return Err(self.err("table nesting limit exceeded"));
        }
        self.space()?;
        if let Some(v) = self.long()? {
            return Ok(V::String(v));
        }
        let c = *self
            .b
            .get(self.i)
            .ok_or_else(|| self.err("expected value"))?;
        if c == b'\'' || c == b'"' {
            self.i += 1;
            let mut out = Vec::new();
            while let Some(&ch) = self.b.get(self.i) {
                self.i += 1;
                if ch == c {
                    return Ok(V::String(out));
                }
                if ch == b'\n' || ch == b'\r' {
                    return Err(self.err("newline in quoted string"));
                }
                if ch != b'\\' {
                    out.push(ch);
                    continue;
                }
                let e = *self
                    .b
                    .get(self.i)
                    .ok_or_else(|| self.err("unfinished escape"))?;
                self.i += 1;
                match e {
                    b'a' => out.push(7),
                    b'b' => out.push(8),
                    b'f' => out.push(12),
                    b'n' => out.push(10),
                    b'r' => out.push(13),
                    b't' => out.push(9),
                    b'v' => out.push(11),
                    b'\\' | b'\'' | b'"' => out.push(e),
                    b'\n' => out.push(10),
                    b'\r' => {
                        if self.b.get(self.i) == Some(&b'\n') {
                            self.i += 1;
                        }
                        out.push(10);
                    }
                    b'z' => {
                        while self.b.get(self.i).is_some_and(u8::is_ascii_whitespace) {
                            self.i += 1;
                        }
                    }
                    b'x' => {
                        let end = self.i + 2;
                        let s = self
                            .b
                            .get(self.i..end)
                            .ok_or_else(|| self.err("short hex escape"))?;
                        out.push(
                            u8::from_str_radix(std::str::from_utf8(s).unwrap_or(""), 16)
                                .map_err(|_| self.err("invalid hex escape"))?,
                        );
                        self.i = end;
                    }
                    b'0'..=b'9' => {
                        let mut n = (e - b'0') as u16;
                        for _ in 0..2 {
                            if let Some(d @ b'0'..=b'9') = self.b.get(self.i) {
                                n = n * 10 + (d - b'0') as u16;
                                self.i += 1;
                            } else {
                                break;
                            }
                        }
                        out.push(
                            u8::try_from(n).map_err(|_| self.err("decimal escape exceeds 255"))?,
                        );
                    }
                    _ => return Err(self.err("invalid escape")),
                }
            }
            return Err(self.err("unterminated string"));
        }
        if self.eat(b'{')? {
            let mut pairs = Vec::new();
            let mut index = 1;
            while !self.eat(b'}')? {
                self.space()?;
                let start = self.i;
                let is_long = self.b.get(self.i) == Some(&b'[')
                    && self
                        .b
                        .get(self.i + 1)
                        .is_some_and(|c| *c == b'[' || *c == b'=');
                let key = if !is_long && self.eat(b'[')? {
                    let k = self.value(depth + 1)?;
                    self.expect(b']')?;
                    self.expect(b'=')?;
                    Some(k)
                } else if self
                    .b
                    .get(self.i)
                    .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
                {
                    let n = self.name()?;
                    if self.eat(b'=')? {
                        Some(V::String(n.into_bytes()))
                    } else {
                        self.i = start;
                        None
                    }
                } else {
                    None
                };
                let k = key.unwrap_or_else(|| {
                    let k = V::Number(index as f64);
                    index += 1;
                    k
                });
                if !matches!(&k, V::String(_) | V::Boolean(_) | V::Number(_))
                    || matches!(&k, V::Number(n) if n.is_nan())
                {
                    return Err(self.err("invalid table key"));
                }
                pairs.push((k, self.value(depth + 1)?));
                if !self.eat(b',')? && !self.eat(b';')? {
                    self.expect(b'}')?;
                    break;
                }
            }
            return Ok(V::Table(pairs));
        }
        let start = self.i;
        while self
            .b
            .get(self.i)
            .is_some_and(|c| c.is_ascii_alphanumeric() || b".+-".contains(c))
        {
            if self.b[self.i..].starts_with(b"--") {
                break;
            }
            self.i += 1;
        }
        let token = std::str::from_utf8(&self.b[start..self.i]).unwrap_or("");
        match token {
            "nil" => Ok(V::Nil),
            "true" => Ok(V::Boolean(true)),
            "false" => Ok(V::Boolean(false)),
            _ => {
                let (negative, n) = if let Some(n) = token.strip_prefix('-') {
                    (true, n)
                } else {
                    (false, token.trim_start_matches('+'))
                };
                let number = if let Some(h) = n.strip_prefix("0x").or_else(|| n.strip_prefix("0X"))
                {
                    hex_number(h).map(|v| if negative { -v } else { v })
                } else {
                    token.parse::<f64>().ok()
                };
                number
                    .map(V::Number)
                    .ok_or_else(|| self.err("expected data value (expressions are forbidden)"))
            }
        }
    }
}

fn hex_number(text: &str) -> Option<f64> {
    let (mantissa, exponent) = text.split_once(['p', 'P']).unwrap_or((text, "0"));
    let exponent: i32 = exponent.parse().ok()?;
    let mut value = 0.0;
    let mut fraction = false;
    let mut scale = 1.0;
    let mut digits = 0;
    for c in mantissa.chars() {
        if c == '.' && !fraction {
            fraction = true;
            continue;
        }
        let digit = c.to_digit(16)? as f64;
        digits += 1;
        if fraction {
            scale /= 16.0;
            value += digit * scale;
        } else {
            value = value * 16.0 + digit;
        }
    }
    (digits > 0).then_some(value * 2.0_f64.powi(exponent))
}

#[derive(Default)]
pub struct Import {
    pub values: BTreeMap<String, V>,
    pub warnings: Vec<String>,
}
/// Load only this addon's declared names, keeping account/character scopes separate.
pub fn load(
    root: &Path,
    account: &str,
    realm: &str,
    character: &str,
    toc: &crate::loader::Toc,
) -> Import {
    let mut out = Import::default();
    for part in [account, realm, character] {
        if part.contains(['/', '\\', ':']) || part == "." || part == ".." {
            out.warnings.push("invalid selection".into());
            return out;
        }
    }
    let account_dir = root.join("Account").join(account);
    for (scope, (dir, names)) in [
        (account_dir.clone(), toc.list("SavedVariables")),
        (
            account_dir.join(realm).join(character),
            toc.list("SavedVariablesPerCharacter"),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        if names.is_empty() {
            continue;
        }
        if scope == 1 && (realm.is_empty() || character.is_empty()) {
            out.warnings.push(format!(
                "character not selected; not found: {}",
                names.join(", ")
            ));
            continue;
        }
        let path = dir.join("SavedVariables").join(format!("{}.lua", toc.name));
        let parsed = std::fs::read(&path)
            .map_err(|e| format!("{}: {e}", path.display()))
            .and_then(|b| parse(&path.display().to_string(), &b));
        match parsed {
            Ok(mut values) => {
                for name in names {
                    if let Some(v) = values.remove(&name) {
                        out.values.insert(name, v);
                    } else {
                        out.warnings.push(format!("not found: {name}"));
                    }
                }
                for name in values.keys() {
                    out.warnings
                        .push(format!("undeclared name skipped: {name}"));
                }
            }
            Err(e) => out.warnings.push(e),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_parser() {
        let v = parse(
            "fixture.lua",
            br#"-- comment
DB = {name="a\n\065\x42", [2]={true;false}, [=[long]=], --[[comment]] x=-0x10, y=1.2e-3}
"#,
        )
        .unwrap();
        assert!(matches!(v["DB"], V::Table(_)));
        let V::Table(pairs) = &v["DB"] else { panic!() };
        assert_eq!(pairs[0].1, V::String(b"a\nAB".to_vec()));
        assert_eq!(
            parse("f", b"X=-0x1.fp+2 --comment").unwrap()["X"],
            V::Number(-7.75)
        );
        assert_eq!(parse("f", b"X={nil; true; {x=false}}").unwrap().len(), 1);
        assert!(matches!(parse("f", b"X=inf").unwrap()["X"], V::Number(n) if n.is_infinite()));
        assert!(matches!(parse("f", b"X=nan").unwrap()["X"], V::Number(n) if n.is_nan()));
        assert_eq!(
            parse("f", b"X=[[\nhello]]").unwrap()["X"],
            V::String(b"hello".to_vec())
        );
        for s in [
            "X=1\nY=os.execute('x')",
            "X={\n[{}]=1}",
            "X='\\999'",
            "X=[[unfinished",
        ] {
            let e = parse("fixture.lua", s.as_bytes()).unwrap_err();
            assert!(e.starts_with("fixture.lua:"), "{e}");
        }
        assert!(
            parse("f", b"X=1\nY=function() end")
                .unwrap_err()
                .starts_with("f:2:")
        );
    }
}

#[cfg(test)]
mod integration_tests {
    #[test]
    fn saved_wtf_seeds_before_addon_loaded() {
        let root = std::env::temp_dir().join(format!("selu-sl29-{}", std::process::id()));
        let addon = root.join("Fixture");
        std::fs::create_dir_all(&addon).unwrap();
        std::fs::write(
            addon.join("Fixture.toc"),
            "## SavedVariables: AccountDB\n## SavedVariablesPerCharacter: CharacterDB\nmain.lua\n",
        )
        .unwrap();
        std::fs::write(addon.join("main.lua"), "local f=CreateFrame('Frame'); f:RegisterEvent('ADDON_LOADED'); f:SetScript('OnEvent',function() assert(AccountDB.value==42); assert(CharacterDB.name=='Fixture'); ImportedAtLoad=true end)").unwrap();
        for (part, text) in [
            (
                "Account/TEST/SavedVariables",
                "AccountDB={value=42} Extra=1",
            ),
            (
                "Account/TEST/Realm/Hero/SavedVariables",
                "CharacterDB={name='Fixture'}",
            ),
        ] {
            let dir = root.join("WTF").join(part);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("Fixture.lua"), text).unwrap();
        }
        let toc = crate::loader::read_toc(&addon).unwrap();
        let imported = super::load(&root.join("WTF"), "TEST", "Realm", "Hero", &toc);
        assert_eq!(imported.values.len(), 2);
        assert_eq!(imported.warnings, ["undeclared name skipped: Extra"]);
        let mut session = crate::Session::new(Default::default()).unwrap();
        session.seed_saved(&addon, &imported.values).unwrap();
        session.load_addon_dir(&addon).unwrap();
        session.eval("assert(ImportedAtLoad)").unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
