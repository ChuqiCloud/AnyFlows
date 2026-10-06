use std::{
    cell::Cell,
    fmt,
    io::{self, Write},
};

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

/// JSON 解析预算。根值深度为 0，对象键不计入节点数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct JsonLimits {
    pub(crate) max_depth: usize,
    pub(crate) max_nodes: usize,
    pub(crate) max_object_entries: usize,
    pub(crate) max_array_items: usize,
    pub(crate) max_string_bytes: usize,
    pub(crate) max_key_bytes: usize,
}

/// 受限 JSON 解析仅暴露固定错误类别，避免携带请求内容。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BoundedJsonError {
    InvalidJson,
    DuplicateKey,
    LimitExceeded,
}

pub(crate) fn parse_value(input: &[u8], limits: JsonLimits) -> Result<Value, BoundedJsonError> {
    let state = ParseState::new(limits);
    let mut deserializer = serde_json::Deserializer::from_slice(input);
    let value = ValueSeed {
        state: &state,
        depth: 0,
    }
    .deserialize(&mut deserializer)
    .map_err(|_| state.failure.get().unwrap_or(BoundedJsonError::InvalidJson))?;

    deserializer
        .end()
        .map_err(|_| BoundedJsonError::InvalidJson)?;
    Ok(value)
}

/// 校验已构造 JSON 对象的结构与序列化字节预算，不复制完整正文。
pub(crate) fn validate_object(
    object: &Map<String, Value>,
    limits: JsonLimits,
    max_serialized_bytes: usize,
) -> Result<(), BoundedJsonError> {
    let mut state = ValueBudgetState::new(limits);
    state.visit_object(object, 0)?;

    let mut writer = BoundedWriter::new(max_serialized_bytes);
    serde_json::to_writer(&mut writer, object).map_err(|_| BoundedJsonError::LimitExceeded)
}

struct ValueBudgetState {
    limits: JsonLimits,
    nodes: usize,
    string_bytes: usize,
}

impl ValueBudgetState {
    const fn new(limits: JsonLimits) -> Self {
        Self {
            limits,
            nodes: 0,
            string_bytes: 0,
        }
    }

    fn visit_value(&mut self, value: &Value, depth: usize) -> Result<(), BoundedJsonError> {
        self.add_node(depth)?;
        match value {
            Value::Object(object) => self.visit_object_entries(object, depth),
            Value::Array(array) => self.visit_array_items(array, depth),
            Value::String(value) => self.add_string(value.len()),
            Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
        }
    }

    fn visit_object(
        &mut self,
        object: &Map<String, Value>,
        depth: usize,
    ) -> Result<(), BoundedJsonError> {
        self.add_node(depth)?;
        self.visit_object_entries(object, depth)
    }

    fn visit_object_entries(
        &mut self,
        object: &Map<String, Value>,
        depth: usize,
    ) -> Result<(), BoundedJsonError> {
        if object.len() > self.limits.max_object_entries {
            return Err(BoundedJsonError::LimitExceeded);
        }
        let child_depth = depth
            .checked_add(1)
            .ok_or(BoundedJsonError::LimitExceeded)?;
        for (key, value) in object {
            if key.len() > self.limits.max_key_bytes {
                return Err(BoundedJsonError::LimitExceeded);
            }
            self.add_string(key.len())?;
            self.visit_value(value, child_depth)?;
        }
        Ok(())
    }

    fn visit_array_items(&mut self, array: &[Value], depth: usize) -> Result<(), BoundedJsonError> {
        if array.len() > self.limits.max_array_items {
            return Err(BoundedJsonError::LimitExceeded);
        }
        let child_depth = depth
            .checked_add(1)
            .ok_or(BoundedJsonError::LimitExceeded)?;
        for value in array {
            self.visit_value(value, child_depth)?;
        }
        Ok(())
    }

    fn add_node(&mut self, depth: usize) -> Result<(), BoundedJsonError> {
        if depth > self.limits.max_depth {
            return Err(BoundedJsonError::LimitExceeded);
        }
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or(BoundedJsonError::LimitExceeded)?;
        if self.nodes > self.limits.max_nodes {
            return Err(BoundedJsonError::LimitExceeded);
        }
        Ok(())
    }

    fn add_string(&mut self, bytes: usize) -> Result<(), BoundedJsonError> {
        self.string_bytes = self
            .string_bytes
            .checked_add(bytes)
            .ok_or(BoundedJsonError::LimitExceeded)?;
        if self.string_bytes > self.limits.max_string_bytes {
            return Err(BoundedJsonError::LimitExceeded);
        }
        Ok(())
    }
}

struct BoundedWriter {
    bytes: usize,
    limit: usize,
}

impl BoundedWriter {
    const fn new(limit: usize) -> Self {
        Self { bytes: 0, limit }
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let bytes = self
            .bytes
            .checked_add(buffer.len())
            .ok_or_else(limit_writer_error)?;
        if bytes > self.limit {
            return Err(limit_writer_error());
        }
        self.bytes = bytes;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn limit_writer_error() -> io::Error {
    io::Error::other("JSON 输出超过序列化大小限制")
}

struct ParseState {
    limits: JsonLimits,
    nodes: Cell<usize>,
    string_bytes: Cell<usize>,
    failure: Cell<Option<BoundedJsonError>>,
}

impl ParseState {
    const fn new(limits: JsonLimits) -> Self {
        Self {
            limits,
            nodes: Cell::new(0),
            string_bytes: Cell::new(0),
            failure: Cell::new(None),
        }
    }

    fn begin_value<E>(&self, depth: usize) -> Result<(), E>
    where
        E: de::Error,
    {
        if depth > self.limits.max_depth {
            return Err(self.reject(BoundedJsonError::LimitExceeded));
        }

        let nodes = self
            .nodes
            .get()
            .checked_add(1)
            .ok_or_else(|| self.reject(BoundedJsonError::LimitExceeded))?;
        if nodes > self.limits.max_nodes {
            return Err(self.reject(BoundedJsonError::LimitExceeded));
        }
        self.nodes.set(nodes);
        Ok(())
    }

    fn record_string<E>(&self, bytes: usize) -> Result<(), E>
    where
        E: de::Error,
    {
        let string_bytes = self
            .string_bytes
            .get()
            .checked_add(bytes)
            .ok_or_else(|| self.reject(BoundedJsonError::LimitExceeded))?;
        if string_bytes > self.limits.max_string_bytes {
            return Err(self.reject(BoundedJsonError::LimitExceeded));
        }
        self.string_bytes.set(string_bytes);
        Ok(())
    }

    fn reject<E>(&self, failure: BoundedJsonError) -> E
    where
        E: de::Error,
    {
        if self.failure.get().is_none() {
            self.failure.set(Some(failure));
        }
        E::custom("JSON 输入未通过受限解析")
    }
}

struct ValueSeed<'a> {
    state: &'a ParseState,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for ValueSeed<'_> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        self.state.begin_value(self.depth)?;
        deserializer.deserialize_any(ValueVisitor {
            state: self.state,
            depth: self.depth,
        })
    }
}

struct ChildValueSeed<'a> {
    state: &'a ParseState,
    parent_depth: usize,
}

impl<'de> DeserializeSeed<'de> for ChildValueSeed<'_> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let depth = self
            .parent_depth
            .checked_add(1)
            .ok_or_else(|| self.state.reject(BoundedJsonError::LimitExceeded))?;
        ValueSeed {
            state: self.state,
            depth,
        }
        .deserialize(deserializer)
    }
}

struct ValueVisitor<'a> {
    state: &'a ParseState,
    depth: usize,
}

impl<'de> Visitor<'de> for ValueVisitor<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("合法且未超过预算的 JSON 值")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("JSON 数字无效"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.check_string::<E>(value)?;
        Ok(Value::String(value.to_owned()))
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_str(value)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.check_string::<E>(&value)?;
        Ok(Value::String(value))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        loop {
            if values.len() >= self.state.limits.max_array_items {
                let extra = sequence.next_element_seed(RejectLimitSeed { state: self.state })?;
                if extra.is_none() {
                    break;
                }
                return Err(self.state.reject(BoundedJsonError::LimitExceeded));
            }

            let Some(value) = sequence.next_element_seed(ChildValueSeed {
                state: self.state,
                parent_depth: self.depth,
            })?
            else {
                break;
            };
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = Map::new();
        let mut entries = 0_usize;
        loop {
            if entries >= self.state.limits.max_object_entries {
                let extra = object.next_key_seed(RejectLimitSeed { state: self.state })?;
                if extra.is_none() {
                    break;
                }
                return Err(self.state.reject(BoundedJsonError::LimitExceeded));
            }

            let Some(key) = object.next_key_seed(KeySeed { state: self.state })? else {
                break;
            };
            if values.contains_key(&key) {
                return Err(self.state.reject(BoundedJsonError::DuplicateKey));
            }

            let value = object.next_value_seed(ChildValueSeed {
                state: self.state,
                parent_depth: self.depth,
            })?;
            values.insert(key, value);
            entries += 1;
        }
        Ok(Value::Object(values))
    }
}

impl ValueVisitor<'_> {
    fn check_string<E>(&self, value: &str) -> Result<(), E>
    where
        E: de::Error,
    {
        self.state.record_string(value.len())
    }
}

struct KeySeed<'a> {
    state: &'a ParseState,
}

impl<'de> DeserializeSeed<'de> for KeySeed<'_> {
    type Value = String;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_string(KeyVisitor { state: self.state })
    }
}

struct KeyVisitor<'a> {
    state: &'a ParseState,
}

impl<'de> Visitor<'de> for KeyVisitor<'_> {
    type Value = String;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("未超过预算的 JSON 对象键")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.check_key::<E>(value)?;
        Ok(value.to_owned())
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_str(value)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.check_key::<E>(&value)?;
        Ok(value)
    }
}

impl KeyVisitor<'_> {
    fn check_key<E>(&self, value: &str) -> Result<(), E>
    where
        E: de::Error,
    {
        if value.len() > self.state.limits.max_key_bytes {
            return Err(self.state.reject(BoundedJsonError::LimitExceeded));
        }
        self.state.record_string(value.len())
    }
}

/// 达到集合预算后，仅探测是否存在下一项，不继续解析其内容。
struct RejectLimitSeed<'a> {
    state: &'a ParseState,
}

impl<'de> DeserializeSeed<'de> for RejectLimitSeed<'_> {
    type Value = ();

    fn deserialize<D>(self, _deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        Err(self.state.reject(BoundedJsonError::LimitExceeded))
    }
}
