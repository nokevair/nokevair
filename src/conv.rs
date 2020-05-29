//! Implement conversions between various kinds of data defined in crates.

use rmpv::Value as MPV;
use rlua::Value as LV;
use tera::Value as TV;

use rlua::ToLua;

use std::fmt::Write as _;
use std::io::{self, Read, Write};
use std::str;

/// Load a MessagePack value from a reader.
pub fn bytes_to_msgpack<R: Read>(bytes: &mut R) -> io::Result<rmpv::Value> {
    rmpv::decode::read_value(bytes).map_err(Into::into)
}

/// Write a MessagePack value to a writer.
pub fn msgpack_to_bytes<W: Write>(w: &mut W, mp: &MPV) -> io::Result<()> {
    rmpv::encode::write_value(w, mp).map_err(Into::into)
}

/// Determine whether a Lua table represents a sequence.
fn is_seq(t: rlua::Table) -> bool {
    let seq_len = t.raw_len();
    let real_len = t.pairs::<LV, LV>().count();
    seq_len == real_len as i64
}

/// If the provided Lua value is a string that constitutes a valid identifier,
/// return it.
fn as_ident<'a>(v: &'a LV) -> Option<&'a str> {
    /// Determine whether a character is a valid start to an identifier.
    fn is_ident_start(b: u8) -> bool {
        b.is_ascii_alphabetic() || b == b'_'
    }
    let bs = match v {
        LV::String(s) => s.as_bytes(),
        _ => return None,
    };
    if !is_ident_start(*bs.get(0)?) {
        return None;
    }
    if !bs.iter().skip(1).all(|&b| b.is_ascii_digit() || is_ident_start(b)) {
        return None;
    }
    Some(str::from_utf8(bs).unwrap())
}

/// Create a Lua value from a MessagePack value.
pub fn msgpack_to_lua(mp: MPV, ctx: rlua::Context) -> rlua::Result<LV> {
    /// Newtype that lets us implement `ToLua` for MessagePack values.
    struct MPVWrapper(MPV);

    impl<'lua> ToLua<'lua> for MPVWrapper {
        fn to_lua(self, ctx: rlua::Context<'lua>) -> rlua::Result<LV<'lua>> {
            match self.0 {
                MPV::Nil => Ok(LV::Nil),

                MPV::Boolean(b) => b.to_lua(ctx),

                MPV::Integer(i) =>
                    if let Some(i) = i.as_i64() {
                        i.to_lua(ctx)
                    } else {
                        Err(rlua::Error::ToLuaConversionError {
                            from: "rmpv::Value",
                            to: "rlua::Value",
                            message: Some(String::from("int is too big to fit in i64"))
                        })
                    }

                MPV::F32(x) => x.to_lua(ctx),

                MPV::F64(x) => x.to_lua(ctx),

                MPV::String(s) => ctx.create_string(s.as_bytes()).map(LV::String),

                MPV::Binary(b) => ctx.create_string(&b).map(LV::String),

                MPV::Array(a) =>
                    ctx.create_sequence_from(
                        a.into_iter().map(MPVWrapper))
                        .map(LV::Table),

                MPV::Map(m) =>
                    ctx.create_table_from(
                        m.into_iter()
                            .map(|(k, v)| (MPVWrapper(k), MPVWrapper(v))))
                        .map(LV::Table),

                MPV::Ext(_, _) => Err(rlua::Error::ToLuaConversionError {
                    from: "rmpv::Value",
                    to: "rlua::Value",
                    message: Some(String::from("extension data cannot be converted"))
                }),
            }
        }
    }

    MPVWrapper(mp).to_lua(ctx)
}

/// Used by the two functions below to generate an error message describing why
/// the conversion is impossible.
macro_rules! cannot_convert {
    ($name:expr => ($to_type:expr) $to:expr) => {{
        Err(rlua::Error::FromLuaConversionError {
            from: "rlua::Value",
            to: $to_type,
            message: Some(String::from(
                concat!(
                    $name,
                    " cannot be represented as a ",
                    $to,
                    " value"
                )))
        })
    }}
}

/// Create a MessagePack value from a Lua value.
pub fn lua_to_msgpack(lua: LV) -> rlua::Result<MPV> {
    match lua {
        LV::Nil => Ok(MPV::Nil),

        LV::Boolean(b) => Ok(b.into()),

        LV::Integer(i) => Ok(i.into()),

        LV::Number(x) => Ok(x.into()),

        LV::String(s) => Ok(s.as_bytes().into()),

        LV::Table(t) => {
            // It isn't strictly necessary that we serialize sequences
            // as arrays, since they get deserialized the same regardless.
            // However, arrays can be represented more compactly.
            if is_seq(t.clone()) {
                let mut vals = Vec::new();
                for val in t.sequence_values() {
                    vals.push(lua_to_msgpack(val?)?);
                }
                Ok(vals.into())
            } else {
                let mut pairs = Vec::new();
                for pair in t.pairs() {
                    let (k, v) = pair?;
                    pairs.push((lua_to_msgpack(k)?, lua_to_msgpack(v)?));
                }
                Ok(pairs.into())
            }
        }

        LV::Function(_) =>
            cannot_convert!("functions" => ("rmpv::Value") "MessagePack"),

        LV::Thread(_) =>
            cannot_convert!("threads" => ("rmpv::Value") "MessagePack"),

        LV::LightUserData(_) | LV::UserData(_) =>
            cannot_convert!("userdata" => ("rmpv::Value") "MessagePack"),

        LV::Error(e) => Err(e),
    }
}

/// Create a JSON value from a Lua value (usable with Tera).
pub fn lua_to_json(lua: LV) -> rlua::Result<TV> {
    match lua {
        LV::Nil => Ok(().into()),

        LV::Boolean(b) => Ok(b.into()),

        LV::Integer(i) => Ok(i.into()),

        LV::Number(x) => Ok(x.into()),

        LV::String(s) => s.to_str().map(TV::from),

        LV::Table(t) => {
            // We treat empty tables as arrays even though it is technically
            // ambiguous. This means trying to iterate over a map in Tera
            // code will produce an error if the map is empty. This decision
            // was made because we don't expect Tera code to need to iterate
            // over maps very frequently.
            if is_seq(t.clone()) {
                let mut vals = Vec::new();
                for val in t.sequence_values() {
                    vals.push(lua_to_json(val?)?);
                }
                Ok(vals.into())
            } else {
                let mut string_keys = serde_json::Map::new();
                for pair in t.pairs::<String, LV>() {
                    if let Ok((k, v)) = pair {
                        string_keys.insert(k, lua_to_json(v)?);
                    }
                }
                Ok(string_keys.into())
            }
        }

        LV::Function(_) =>
            cannot_convert!("functions" => ("serde_json::Value") "JSON"),

        LV::Thread(_) =>
            cannot_convert!("threads" => ("serde_json::Value") "JSON"),

        LV::LightUserData(_) | LV::UserData(_) =>
            cannot_convert!("userdata" => ("serde_json::Value") "JSON"),
        
        LV::Error(e) => Err(e),
    }
}

/// Pretty-print a Lua object.
pub fn lua_to_string(lua: LV) -> String {
    /// Append the pretty-printed version of the Lua object
    fn fmt(lua: LV, s: &mut String) {
        match lua {
            LV::Nil => s.push_str("nil"),

            LV::Boolean(b) => write!(s, "{}", b).unwrap(),

            LV::Integer(i) => write!(s, "{}", i).unwrap(),

            LV::Number(x) => write!(s, "{}", x).unwrap(),

            LV::String(bs) => {
                s.push('"');
                for &b in bs.as_bytes() {
                    // this escapes all non-ascii characters, unfortunately
                    write!(s, "{}", std::ascii::escape_default(b)).unwrap();
                }
                s.push('"');
            }

            LV::Table(t) => {
                s.push('{');
                if is_seq(t.clone()) {
                    t.sequence_values()
                        .filter_map(|val| val.ok())
                        .enumerate()
                        .for_each(|(i, val)| {
                            if i != 0 {
                                s.push_str(", ");
                            }
                            fmt(val, s);
                        });
                } else {
                    t.pairs()
                        .filter_map(|val| val.ok())
                        .enumerate()
                        .for_each(|(i, (k, v)): (usize, (LV, LV))| {
                            if i != 0 {
                                s.push_str(", ");
                            }
                            if let Some(id) = as_ident(&k) {
                                s.push_str(id)
                            } else {
                                s.push('[');
                                fmt(k, s);
                                s.push(']');
                            }
                            s.push_str(" = ");
                            fmt(v, s);
                        });
                }
                s.push('}')
            }

            LV::Function(_) => s.push_str("[function]"),

            LV::Thread(_) => s.push_str("[thread]"),

            LV::LightUserData(_) | LV::UserData(_) => s.push_str("[userdata]"),

            LV::Error(e) => write!(s, "[error: {}]", e).unwrap(),
        }
    }
    let mut result = String::new();
    fmt(lua, &mut result);
    result
}
