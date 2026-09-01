//! Encode Brendan-Gregg folded stacks into Google pprof protobuf.
//!
//! This is the AI-agent *analysis-power* projection of a Lisp CPU capture:
//! `go tool pprof -top/-tree/-peek` (and speedscope) read pprof, giving the one
//! profiling format with a scriptable CLI. We embed the Lisp function names
//! directly in the `Function` table, so the profile is self-contained — no
//! neomacs binary or symbols are needed to analyze it.
//!
//! We hand-write the wire format (the schema is tiny and fixed) to avoid a
//! protobuf codegen dependency. Field numbers are from Google's `profile.proto`:
//! Profile{sample_type=1, sample=2, location=4, function=5, string_table=6};
//! ValueType{type=1, unit=2}; Sample{location_id=1, value=2};
//! Location{id=1, line=4}; Line{function_id=1}; Function{id=1, name=2}.

use std::collections::HashMap;

/// A minimal protobuf message writer.
struct ProtoBuf {
    buf: Vec<u8>,
}

impl ProtoBuf {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn varint(&mut self, mut v: u64) {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v != 0 {
                self.buf.push(byte | 0x80);
            } else {
                self.buf.push(byte);
                break;
            }
        }
    }

    fn tag(&mut self, field: u32, wire: u32) {
        self.varint(((field << 3) | wire) as u64);
    }

    fn field_varint(&mut self, field: u32, value: u64) {
        self.tag(field, 0);
        self.varint(value);
    }

    fn field_bytes(&mut self, field: u32, bytes: &[u8]) {
        self.tag(field, 2);
        self.varint(bytes.len() as u64);
        self.buf.extend_from_slice(bytes);
    }

    fn field_message(&mut self, field: u32, msg: &ProtoBuf) {
        self.field_bytes(field, &msg.buf);
    }

    /// Packed repeated uint64 (used for Sample.location_id).
    fn field_packed_u64(&mut self, field: u32, values: &[u64]) {
        let mut inner = ProtoBuf::new();
        for &v in values {
            inner.varint(v);
        }
        self.field_bytes(field, &inner.buf);
    }
}

/// pprof string table (index 0 must be the empty string).
struct StringTable {
    strings: Vec<String>,
    index: HashMap<String, i64>,
}

impl StringTable {
    fn new() -> Self {
        let mut table = Self {
            strings: vec![String::new()],
            index: HashMap::new(),
        };
        table.index.insert(String::new(), 0);
        table
    }

    fn intern(&mut self, s: &str) -> i64 {
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let i = self.strings.len() as i64;
        self.strings.push(s.to_string());
        self.index.insert(s.to_string(), i);
        i
    }
}

/// Convert folded stacks into a self-contained pprof profile (raw protobuf).
///
/// One sample type, `samples`/`count`. Each distinct function name becomes a
/// `Function` + `Location`; each folded line becomes a `Sample` whose
/// `location_id` list is leaf-first (pprof convention), the reverse of the
/// root-first folded order.
pub fn folded_to_pprof(folded: &str) -> Vec<u8> {
    let mut st = StringTable::new();
    let samples_type = st.intern("samples");
    let count_unit = st.intern("count");

    // Assign ids (1-based) to distinct function names, in first-seen order.
    let mut fn_ids: HashMap<String, u64> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut parsed: Vec<(Vec<String>, i64)> = Vec::new();

    for line in folded.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((stack, cnt)) = line.rsplit_once(' ') else {
            continue;
        };
        let Ok(count) = cnt.trim().parse::<i64>() else {
            continue;
        };
        let frames: Vec<String> = stack
            .split(';')
            .filter(|f| !f.is_empty())
            .map(|s| s.to_string())
            .collect();
        if frames.is_empty() {
            continue;
        }
        for frame in &frames {
            if !fn_ids.contains_key(frame) {
                let id = order.len() as u64 + 1;
                fn_ids.insert(frame.clone(), id);
                order.push(frame.clone());
            }
        }
        parsed.push((frames, count));
    }

    // Intern all function names before emitting the string table.
    let name_str_idx: Vec<i64> = order.iter().map(|n| st.intern(n)).collect();

    let mut profile = ProtoBuf::new();

    // 1: sample_type ValueType { type, unit }
    let mut value_type = ProtoBuf::new();
    value_type.field_varint(1, samples_type as u64);
    value_type.field_varint(2, count_unit as u64);
    profile.field_message(1, &value_type);

    // 2: samples
    for (frames, count) in &parsed {
        let mut sample = ProtoBuf::new();
        // Folded is root;...;leaf; pprof wants leaf-first location ids.
        let loc_ids: Vec<u64> = frames.iter().rev().map(|f| fn_ids[f]).collect();
        sample.field_packed_u64(1, &loc_ids);
        sample.field_packed_u64(2, &[*count as u64]);
        profile.field_message(2, &sample);
    }

    // 4: locations (one per function name; a single line -> the function)
    for i in 0..order.len() {
        let id = i as u64 + 1;
        let mut location = ProtoBuf::new();
        location.field_varint(1, id);
        let mut line = ProtoBuf::new();
        line.field_varint(1, id); // Line.function_id
        location.field_message(4, &line); // Location.line
        profile.field_message(4, &location);
    }

    // 5: functions
    for (i, &name_idx) in name_str_idx.iter().enumerate() {
        let id = i as u64 + 1;
        let mut function = ProtoBuf::new();
        function.field_varint(1, id);
        function.field_varint(2, name_idx as u64);
        profile.field_message(5, &function);
    }

    // 6: string_table (repeated, index-ordered) — emit after all interning.
    for s in &st.strings {
        profile.field_bytes(6, s.as_bytes());
    }

    profile.buf
}
