//! The binary building blocks of the snapshot protocol: LEB128 varints, zigzag-signed
//! integers, field tables and records of quantized slots.
//!
//! A record is one entity (or the match, the map, an event) as a fixed list of slots, one
//! per field of its kind's table. Numbers keep the JSON protocol's quantization as whole
//! units: positions in millimetres, rotations in ten-thousandths, meters in hundredths
//! (see [`super::json`]), so a value reads back as the same double the JSON text did;
//! countdown timers travel in simulation steps. The host compares slots to find changed
//! fields; the client keeps the same records, so a fixed-point field the client already
//! holds travels as the difference from its value, which is usually one byte.
//!
//! Entity records also keep their [`Motion`]: what each fixed-point field changed by in the
//! previous frame. Every frame moves such a field on by that much before applying changes,
//! so a tank driving straight, a falling piece of debris or a timer counting down sends
//! nothing until its motion changes, and then only the difference from the continuation.
//!
//! A record's changes are a varint field mask (bit 0 flags deleted fields, bit `i + 1`
//! field `i`), a varint mask of deleted fields when flagged, then each present field's
//! value in table order.

use super::json::POSITION_SCALE;
use super::schema::ReadResult;
use crate::sim::math::js_round;

/// Appends `value` as an unsigned LEB128 varint.
pub fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// Appends `value` zigzag-encoded, so small magnitudes of either sign stay short.
pub fn put_signed(out: &mut Vec<u8>, value: i64) {
    put_varint(out, ((value << 1) ^ (value >> 63)) as u64);
}

pub fn put_text(out: &mut Vec<u8>, text: &str) {
    put_varint(out, text.len() as u64);
    out.extend_from_slice(text.as_bytes());
}

/// `value` in whole units of `1 / scale`, rounded like the JSON protocol's `wire_round`.
///
/// # Panics
/// On a non-finite value: the simulation state is corrupt.
pub fn units(value: f64, scale: f64) -> i64 {
    assert!(value.is_finite(), "Non-finite wire number");
    js_round(value * scale) as i64
}

/// `value` in whole thousandths, as positions, elapsed times and ticks travel.
pub fn thousandths(value: f64) -> i64 {
    units(value, POSITION_SCALE)
}

/// Reads a message front to back; every read fails cleanly on truncated or invalid data.
#[derive(Clone, Debug)]
pub struct WireReader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> WireReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.at >= self.bytes.len()
    }

    pub fn byte(&mut self) -> ReadResult<u8> {
        let byte = *self
            .bytes
            .get(self.at)
            .ok_or_else(|| "Truncated message".to_string())?;
        self.at += 1;
        Ok(byte)
    }

    pub fn varint(&mut self) -> ReadResult<u64> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = self.byte()?;
            let bits = u64::from(byte & 0x7f);
            if shift == 63 && bits > 1 {
                break;
            }
            value |= bits << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("Invalid number".into())
    }

    pub fn signed(&mut self) -> ReadResult<i64> {
        let value = self.varint()?;
        Ok((value >> 1) as i64 ^ -((value & 1) as i64))
    }

    /// A varint that must fit 32 bits.
    pub fn varint32(&mut self) -> ReadResult<u32> {
        u32::try_from(self.varint()?).map_err(|_| "Invalid number".to_string())
    }

    pub fn bytes(&mut self, length: usize) -> ReadResult<&'a [u8]> {
        let end = self
            .at
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| "Truncated message".to_string())?;
        let bytes = &self.bytes[self.at..end];
        self.at = end;
        Ok(bytes)
    }

    /// A length-prefixed UTF-8 string of at most `max` bytes.
    pub fn text(&mut self, max: usize) -> ReadResult<&'a str> {
        let length = self.varint()?;
        if length > max as u64 {
            return Err("Invalid text".into());
        }
        std::str::from_utf8(self.bytes(length as usize)?).map_err(|_| "Invalid text".into())
    }
}

/// How one field's value is stored and sent.
#[derive(Clone, Copy, Debug)]
pub enum FieldKind {
    /// A non-negative integer: ids, counters, colours, teams.
    Count,
    /// A signed number in whole units of `1 / scale`. Sent as the difference from the
    /// client's value when it holds one.
    Fixed(f64),
    Flag,
    /// An index into these wire names.
    Choice(&'static [&'static str]),
    /// UTF-8 text of at most this many bytes.
    Text(usize),
    /// A nested structure in its own encoding, at most this many bytes, compared and sent
    /// whole. The function reads it for the JSON view.
    Blob(usize, BlobView),
}

pub type BlobView = fn(&[u8]) -> ReadResult<serde_json::Value>;

/// One field of a record table. Dotted names nest in the JSON view (`position.x`).
#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub name: &'static str,
    pub kind: FieldKind,
    /// An absent value means `null` (an indestructible cover's hp, a match without a
    /// winner) rather than a missing field.
    pub nullable: bool,
}

impl Field {
    pub const fn new(name: &'static str, kind: FieldKind) -> Self {
        Self {
            name,
            kind,
            nullable: false,
        }
    }

    pub const fn nullable(name: &'static str, kind: FieldKind) -> Self {
        Self {
            name,
            kind,
            nullable: true,
        }
    }
}

/// The wire names of a name table, for [`FieldKind::Choice`].
pub const fn names<T, const N: usize>(table: &[(&'static str, T); N]) -> [&'static str; N] {
    let mut names = [""; N];
    let mut index = 0;
    while index < N {
        names[index] = table[index].0;
        index += 1;
    }
    names
}

/// Declares a field table and a module of its field indices.
macro_rules! wire_fields {
    ($(#[$doc:meta])* $table:ident, $module:ident { $($index:ident = $field:expr),+ $(,)? }) => {
        $(#[$doc])*
        pub const $table: &[$crate::net::wire::Field] = &[$($field),+];
        #[doc = concat!("Indices into [`", stringify!($table), "`].")]
        pub mod $module {
            $crate::net::wire::wire_fields!(@indices 0usize; $($index),+);
        }
    };
    (@indices $at:expr; $first:ident $(, $rest:ident)*) => {
        pub const $first: usize = $at;
        $crate::net::wire::wire_fields!(@indices $at + 1; $($rest),*);
    };
    (@indices $at:expr;) => {};
}
pub(crate) use wire_fields;

/// One field's value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Slot {
    #[default]
    Absent,
    /// Counts, fixed-point units, flags (0 or 1) and choice indices.
    Number(i64),
    Text(String),
    Blob(Vec<u8>),
}

impl Slot {
    pub fn is_absent(&self) -> bool {
        matches!(self, Slot::Absent)
    }

    pub fn number(&self) -> Option<i64> {
        match self {
            Slot::Number(value) => Some(*value),
            _ => None,
        }
    }
}

/// One record: an entity id (zero for the match, the map and events) and a slot per field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WireRecord {
    pub id: u32,
    pub slots: Vec<Slot>,
}

impl WireRecord {
    /// Starts overwriting this record for a table of `fields` fields. Numbers become
    /// absent; text and blob slots keep their buffers for the writer to overwrite, so a
    /// writer must set or [`clear`](Self::clear) every text and blob field.
    pub fn reset(&mut self, id: u32, fields: usize) {
        self.id = id;
        self.slots.truncate(fields);
        for slot in &mut self.slots {
            if !matches!(slot, Slot::Text(_) | Slot::Blob(_)) {
                *slot = Slot::Absent;
            }
        }
        self.slots.resize(fields, Slot::Absent);
    }

    pub fn set_number(&mut self, field: usize, value: i64) {
        self.slots[field] = Slot::Number(value);
    }

    pub fn set_count(&mut self, field: usize, value: u64) {
        self.set_number(field, value as i64);
    }

    pub fn set_fixed(&mut self, field: usize, value: f64, scale: f64) {
        self.set_number(field, units(value, scale));
    }

    pub fn set_flag(&mut self, field: usize, value: bool) {
        self.set_number(field, i64::from(value));
    }

    pub fn set_text(&mut self, field: usize, text: &str) {
        match &mut self.slots[field] {
            Slot::Text(existing) => {
                if existing != text {
                    existing.clear();
                    existing.push_str(text);
                }
            }
            slot => *slot = Slot::Text(text.to_string()),
        }
    }

    /// Fills a blob in place: `write` appends to an emptied buffer.
    pub fn set_blob(&mut self, field: usize, write: impl FnOnce(&mut Vec<u8>)) {
        let slot = &mut self.slots[field];
        if !matches!(slot, Slot::Blob(_)) {
            *slot = Slot::Blob(Vec::new());
        }
        if let Slot::Blob(bytes) = slot {
            bytes.clear();
            write(bytes);
        }
    }

    pub fn clear(&mut self, field: usize) {
        self.slots[field] = Slot::Absent;
    }
}

/// What each slot of a record changed by in the previous frame: a fixed-point field's
/// difference in units, zero for every other field, and no entries for a record that
/// stands still.
pub type Motion = Vec<i64>;

/// Whether `motion` moves nothing.
pub fn is_still(motion: &[i64]) -> bool {
    motion.iter().all(|change| *change == 0)
}

/// The value the client expects field `index` to hold before reading a frame's changes:
/// a fixed-point number moved on by its motion.
fn expected(fields: &[Field], index: usize, held: Option<&Slot>, motion: &[i64]) -> Option<i64> {
    match (fields[index].kind, held) {
        (FieldKind::Fixed(_), Some(Slot::Number(value))) => {
            Some(value.wrapping_add(motion.get(index).copied().unwrap_or(0)))
        }
        _ => None,
    }
}

/// A field's motion from `held` to `next`.
fn change(field: &Field, held: Option<&Slot>, next: &Slot) -> i64 {
    match (field.kind, held, next) {
        (FieldKind::Fixed(_), Some(Slot::Number(old)), Slot::Number(new)) => new.wrapping_sub(*old),
        _ => 0,
    }
}

/// Sets `motion` to the changes from `held` to `next`, empty when nothing moved.
fn set_motion(fields: &[Field], held: Option<&WireRecord>, next: &[Slot], motion: &mut Motion) {
    motion.clear();
    for (index, (field, slot)) in fields.iter().zip(next).enumerate() {
        let change = change(field, held.map(|record| &record.slots[index]), slot);
        if change != 0 {
            motion.resize(fields.len(), 0);
            motion[index] = change;
        }
    }
}

fn write_value(field: &Field, expected: Option<i64>, slot: &Slot, out: &mut Vec<u8>) {
    match (field.kind, slot) {
        (FieldKind::Fixed(_), Slot::Number(value)) => {
            put_signed(out, value.wrapping_sub(expected.unwrap_or(0)));
        }
        (_, Slot::Number(value)) => put_varint(out, *value as u64),
        (_, Slot::Text(text)) => put_text(out, text),
        (_, Slot::Blob(bytes)) => {
            put_varint(out, bytes.len() as u64);
            out.extend_from_slice(bytes);
        }
        (_, Slot::Absent) => unreachable!("absent fields are not written"),
    }
}

/// Writes `next`'s changes against `previous`, the record the client holds (all present
/// fields when it holds none). Returns false, writing nothing, when nothing changed.
pub fn write_changes(
    fields: &[Field],
    previous: Option<&WireRecord>,
    next: &WireRecord,
    out: &mut Vec<u8>,
) -> bool {
    write_moving_changes(fields, previous, next, None, out)
}

/// [`write_changes`] for a record with `motion`: the client first moves `previous` on by
/// it, so only fields that then differ are sent, as the difference from the moved value.
/// `motion` becomes `next`'s, which the following frame moves it on by.
pub fn write_moving_changes(
    fields: &[Field],
    previous: Option<&WireRecord>,
    next: &WireRecord,
    motion: Option<&mut Motion>,
    out: &mut Vec<u8>,
) -> bool {
    debug_assert!(fields.len() < 64 && next.slots.len() == fields.len());
    let held_motion: &[i64] = motion.as_deref().map_or(&[], |motion| motion.as_slice());
    let mut mask = 0u64;
    let mut deleted = 0u64;
    for (index, slot) in next.slots.iter().enumerate() {
        let old = previous.map(|record| &record.slots[index]);
        let unchanged = match (expected(fields, index, old, held_motion), slot) {
            (Some(expected), Slot::Number(value)) => expected == *value,
            _ => old == Some(slot),
        };
        if unchanged {
            continue;
        }
        if slot.is_absent() {
            if old.is_some_and(|old| !old.is_absent()) {
                deleted |= 1 << index;
            }
        } else {
            mask |= 1 << (index + 1);
        }
    }
    if deleted != 0 {
        mask |= 1;
    }
    if mask != 0 {
        put_varint(out, mask);
        if deleted != 0 {
            put_varint(out, deleted);
        }
        for (index, (field, slot)) in fields.iter().zip(&next.slots).enumerate() {
            if mask & (1 << (index + 1)) != 0 {
                let old = previous.map(|record| &record.slots[index]);
                write_value(field, expected(fields, index, old, held_motion), slot, out);
            }
        }
    }
    if let Some(motion) = motion {
        set_motion(fields, previous, &next.slots, motion);
    }
    mask != 0
}

/// Which fields a frame changed in a record: bit `i` for field `i`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChangedFields {
    pub set: u64,
    pub deleted: u64,
}

fn read_value(
    field: &Field,
    expected: Option<i64>,
    reader: &mut WireReader<'_>,
) -> ReadResult<Slot> {
    Ok(match field.kind {
        FieldKind::Fixed(_) => Slot::Number(expected.unwrap_or(0).wrapping_add(reader.signed()?)),
        FieldKind::Count => {
            Slot::Number(i64::try_from(reader.varint()?).map_err(|_| "Invalid number".to_string())?)
        }
        FieldKind::Flag => match reader.varint()? {
            value @ (0 | 1) => Slot::Number(value as i64),
            _ => return Err("Invalid boolean".into()),
        },
        FieldKind::Choice(names) => match reader.varint()? {
            index if (index as usize) < names.len() => Slot::Number(index as i64),
            _ => return Err("Invalid choice".into()),
        },
        FieldKind::Text(max) => Slot::Text(reader.text(max)?.to_string()),
        FieldKind::Blob(max, _) => {
            let length = reader.varint()?;
            if length > max as u64 {
                return Err("Invalid list".into());
            }
            Slot::Blob(reader.bytes(length as usize)?.to_vec())
        }
    })
}

/// Reads one record's changes onto `record`, which starts as the client's copy (or empty
/// for a new record). Errors name the field.
pub fn read_changes(
    fields: &[Field],
    record: &mut WireRecord,
    reader: &mut WireReader<'_>,
) -> ReadResult<ChangedFields> {
    read_moving_changes(fields, record, None, reader)
}

/// [`read_changes`] for a record with `motion` ([`write_moving_changes`]): fixed-point
/// fields move on by it first, and `motion` becomes this frame's. The changed fields are
/// those whose value differs afterwards, sent or moved on.
pub fn read_moving_changes(
    fields: &[Field],
    record: &mut WireRecord,
    motion: Option<&mut Motion>,
    reader: &mut WireReader<'_>,
) -> ReadResult<ChangedFields> {
    record.slots.resize(fields.len(), Slot::Absent);
    let mask = reader.varint()?;
    if mask >> (fields.len() + 1) != 0 {
        return Err("Invalid field mask".into());
    }
    let deleted = if mask & 1 != 0 { reader.varint()? } else { 0 };
    let sent = mask >> 1;
    if deleted >> fields.len() != 0 || deleted & sent != 0 {
        return Err("Invalid field mask".into());
    }
    let held_motion: &[i64] = motion.as_deref().map_or(&[], |motion| motion.as_slice());
    let mut changed = ChangedFields::default();
    let mut next_motion = Motion::new();
    for (index, field) in fields.iter().enumerate() {
        let old = &record.slots[index];
        let expected = expected(fields, index, Some(old), held_motion);
        let next = if sent & (1 << index) != 0 {
            read_value(field, expected, reader)
                .map_err(|error| format!("{}: {error}", field.name))?
        } else if deleted & (1 << index) != 0 {
            Slot::Absent
        } else if let Some(expected) = expected {
            Slot::Number(expected)
        } else {
            continue;
        };
        if next != *old {
            if next.is_absent() {
                changed.deleted |= 1 << index;
            } else {
                changed.set |= 1 << index;
            }
        }
        let change = change(field, Some(old), &next);
        if change != 0 {
            next_motion.resize(fields.len(), 0);
            next_motion[index] = change;
        }
        record.slots[index] = next;
    }
    if let Some(motion) = motion {
        *motion = next_motion;
    }
    Ok(changed)
}

/// Moves a record the frame did not send on by its `motion`, which stays as it is; returns
/// the fields that changed.
pub fn move_on(fields: &[Field], record: &mut WireRecord, motion: &[i64]) -> ChangedFields {
    let mut changed = ChangedFields::default();
    for (index, slot) in record.slots.iter_mut().enumerate() {
        let change = motion.get(index).copied().unwrap_or(0);
        if let (FieldKind::Fixed(_), Slot::Number(value)) = (fields[index].kind, &mut *slot)
            && change != 0
        {
            *value = value.wrapping_add(change);
            changed.set |= 1 << index;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    wire_fields!(SAMPLE_FIELDS, sample {
        X = Field::new("position.x", FieldKind::Fixed(1000.0)),
        LIFE = Field::new("life", FieldKind::Count),
        NAME = Field::new("name", FieldKind::Text(64)),
        ALIVE = Field::new("alive", FieldKind::Flag),
        KIND = Field::new("kind", FieldKind::Choice(&["a", "b"])),
        HP = Field::nullable("hp", FieldKind::Fixed(100.0)),
    });

    fn record(x: f64, hp: Option<f64>) -> WireRecord {
        let mut record = WireRecord::default();
        record.reset(7, SAMPLE_FIELDS.len());
        record.set_fixed(sample::X, x, 1000.0);
        record.set_count(sample::LIFE, 2);
        record.set_text(sample::NAME, "Bob");
        record.set_flag(sample::ALIVE, true);
        record.set_number(sample::KIND, 1);
        if let Some(hp) = hp {
            record.set_fixed(sample::HP, hp, 100.0);
        }
        record
    }

    #[test]
    fn varints_round_trip_at_the_edges() {
        let mut out = Vec::new();
        let values = [
            0,
            1,
            127,
            128,
            16_383,
            16_384,
            u64::from(u32::MAX),
            u64::MAX,
        ];
        for value in values {
            put_varint(&mut out, value);
        }
        let signed = [0, -1, 1, -64, 64, i64::MIN, i64::MAX];
        for value in signed {
            put_signed(&mut out, value);
        }
        let mut reader = WireReader::new(&out);
        for value in values {
            assert_eq!(reader.varint(), Ok(value));
        }
        for value in signed {
            assert_eq!(reader.signed(), Ok(value));
        }
        assert!(reader.is_empty());
        assert!(reader.byte().is_err());
        assert!(WireReader::new(&[0xff; 11]).varint().is_err());
        assert!(WireReader::new(&[0x80]).varint().is_err());
    }

    #[test]
    fn changes_send_differences_and_deletions_and_read_back_exactly() {
        let before = record(12.5, Some(40.0));
        let after = record(12.75, None);
        let mut out = Vec::new();
        assert!(write_changes(
            SAMPLE_FIELDS,
            Some(&before),
            &after,
            &mut out
        ));
        // Mask, deletion mask, then x as the +250 mm difference (two bytes).
        assert_eq!(out.len(), 1 + 1 + 2);
        let mut copy = before.clone();
        let changed = read_changes(SAMPLE_FIELDS, &mut copy, &mut WireReader::new(&out)).unwrap();
        assert_eq!(copy, after);
        assert_eq!(changed.set, 1 << sample::X);
        assert_eq!(changed.deleted, 1 << sample::HP);
        out.clear();
        assert!(!write_changes(
            SAMPLE_FIELDS,
            Some(&after),
            &after,
            &mut out
        ));
        assert!(out.is_empty());
        // A record the client lacks travels whole, with absolute values.
        assert!(write_changes(SAMPLE_FIELDS, None, &before, &mut out));
        let mut fresh = WireRecord {
            id: 7,
            ..WireRecord::default()
        };
        read_changes(SAMPLE_FIELDS, &mut fresh, &mut WireReader::new(&out)).unwrap();
        assert_eq!(fresh, before);
    }

    #[test]
    fn moving_fields_carry_on_until_their_change_changes() {
        // x advances 250 mm a frame then stops; hp drops once, then is deleted.
        let frames = [
            (record(12.25, Some(40.0)), true),
            (record(12.5, Some(40.0)), false),
            (record(12.75, Some(39.0)), true),
            (record(12.75, Some(39.0)), true),
            (record(12.75, Some(39.0)), false),
            (record(12.75, None), true),
        ];
        let mut held = record(12.0, Some(40.0));
        let (mut motion, mut copy, mut copy_motion) = (Motion::new(), held.clone(), Motion::new());
        for (index, (next, sends)) in frames.into_iter().enumerate() {
            let mut out = Vec::new();
            let sent = write_moving_changes(
                SAMPLE_FIELDS,
                Some(&held),
                &next,
                Some(&mut motion),
                &mut out,
            );
            assert_eq!(sent, sends, "frame {index}");
            let before = copy.clone();
            let changed = if sent {
                let mut reader = WireReader::new(&out);
                let changed = read_moving_changes(
                    SAMPLE_FIELDS,
                    &mut copy,
                    Some(&mut copy_motion),
                    &mut reader,
                )
                .unwrap();
                assert!(reader.is_empty());
                changed
            } else {
                move_on(SAMPLE_FIELDS, &mut copy, &copy_motion)
            };
            assert_eq!(copy, next, "frame {index}");
            assert_eq!(copy_motion, motion, "frame {index}");
            // Changed fields are the ones whose value differs, sent or carried on.
            let differs = (0..SAMPLE_FIELDS.len())
                .filter(|&field| {
                    copy.slots[field] != before.slots[field] && !copy.slots[field].is_absent()
                })
                .fold(0, |mask, field| mask | 1 << field);
            assert_eq!(changed.set, differs, "frame {index}");
            held = next;
        }
        assert!(motion.is_empty(), "a record at rest keeps no motion");
    }

    #[test]
    fn invalid_masks_choices_and_flags_are_rejected() {
        let mut target = WireRecord::default();
        let mut read =
            |bytes: &[u8]| read_changes(SAMPLE_FIELDS, &mut target, &mut WireReader::new(bytes));
        assert!(read(&[0x80, 0x01]).is_err(), "mask beyond the table");
        assert!(
            read(&[1 << (sample::KIND + 1), 2]).is_err(),
            "choice out of range"
        );
        assert!(
            read(&[1 << (sample::ALIVE + 1), 2]).is_err(),
            "flag out of range"
        );
        assert!(
            read(&[1 << (sample::NAME + 1), 5, b'a']).is_err(),
            "truncated text"
        );
    }
}
