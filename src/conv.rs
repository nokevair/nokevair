//! Implement conversions between various kinds of data defined in crates.

use rmpv::Value as MPV;
use rlua::Value as LV;
use tera::Value as TV;

use rlua::ToLua;

use std::io::{self, Read, Write};

/// Load a MessagePack value from a reader.
pub fn bytes_to_msgpack<R: Read>(bytes: &mut R) -> io::Result<rmpv::Value> {
    rmpv::decode::read_value(bytes).map_err(Into::into)
}

/// Write a MessagePack value to a writer.
pub fn msgpack_to_bytes<W: Write>(w: &mut W, mp: &MPV) -> io::Result<()> {
    rmpv::encode::write_value(w, mp).map_err(Into::into)
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

        // TODO: maybe make some effort to determine whether the table
        // is a sequence, and to represent it as an array if so?
        // This isn't critical, as they get deserialized the same either way.
        LV::Table(t) => {
            let mut pairs = Vec::new();
            for pair in t.pairs() {
                let (k, v) = pair?;
                pairs.push((lua_to_msgpack(k)?, lua_to_msgpack(v)?));
            }
            Ok(pairs.into())
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
            let mut array_values = Vec::new();
            for val in t.clone().sequence_values() {
                array_values.push(lua_to_json(val?)?);
            }
            // If all the keys in the table were indices, treat the table
            // as an array. Note that this means we treat empty tables as
            // arrays even though it is technically ambiguous. This means
            // trying to iterate over a map in Tera code will produce an
            // error if the map is empty. This decision was made because
            // we don't expect Tera code to need to iterate over maps very
            // frequently.
            if array_values.len() == t.raw_len() as usize {
                Ok(array_values.into())
            } else {
                let mut map_values = serde_json::Map::new();
                // Don't attempt to support tables that contiain a mixture
                // of indices and numeric keys.
                for pair in t.pairs::<String, LV>() {
                    let (k, v) = pair?;
                    map_values.insert(k, lua_to_json(v)?);
                }
                Ok(map_values.into())
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
