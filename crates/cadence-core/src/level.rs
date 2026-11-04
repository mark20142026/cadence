//! The decoded level string: a settings section followed by one section per object.
//!
//! Raw key/value pairs are kept in file order so a level can be written back unchanged,
//! including keys this code doesn't interpret yet.

/// An object property key. Almost all are integers; start positions (object 31) also embed
/// level settings under the header's string keys (`kA2`, `kA3`, ...).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Int(u32),
    Str(String),
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Key::Int(k) => write!(f, "{k}"),
            Key::Str(k) => f.write_str(k),
        }
    }
}

/// One object: keys with their raw string values, in file order.
/// One object of a level string, kept compact: its `key,value,…` text once and where each field
/// is in it (levels have hundreds of thousands of objects; a `String` per field was most of the
/// memory a big level took).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RawObject {
    text: Box<str>,
    fields: Box<[Field]>,
}

/// A field of a [`RawObject`]: its integer key (`STR_KEY` for a non-numeric key) and where its
/// key and value are in the object's text.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Field {
    key: u32,
    key_at: u32,
    value_at: u32,
    value_end: u32,
}

const STR_KEY: u32 = u32::MAX;

impl RawObject {
    /// An object from its `key,value,…` text (a trailing key without a value is dropped).
    pub fn parse(section: &str) -> RawObject {
        let mut fields = Vec::new();
        let mut at = 0usize;
        let mut parts = section.split(',');
        while let (Some(k), Some(v)) = (parts.next(), parts.next()) {
            let key_at = at;
            let value_at = key_at + k.len() + 1;
            at = value_at + v.len() + 1;
            fields.push(Field { key: k.parse().unwrap_or(STR_KEY), key_at: key_at as u32, value_at: value_at as u32, value_end: (value_at + v.len()) as u32 });
        }
        RawObject { text: section.into(), fields: fields.into_boxed_slice() }
    }

    fn key_text(&self, f: &Field) -> &str {
        &self.text[f.key_at as usize..f.value_at as usize - 1]
    }

    fn value(&self, f: &Field) -> &str {
        &self.text[f.value_at as usize..f.value_end as usize]
    }

    /// The fields in order: (key, value).
    pub fn props(&self) -> impl Iterator<Item = (Key, &str)> + '_ {
        self.fields.iter().map(move |f| {
            let key = if f.key == STR_KEY { Key::Str(self.key_text(f).to_owned()) } else { Key::Int(f.key) };
            (key, self.value(f))
        })
    }

    pub fn get(&self, key: u32) -> Option<&str> {
        self.fields.iter().find(|f| f.key == key && key != STR_KEY).map(|f| self.value(f))
    }

    pub fn get_str_key(&self, key: &str) -> Option<&str> {
        self.fields.iter().find(|f| f.key == STR_KEY && self.key_text(f) == key).map(|f| self.value(f))
    }

    pub fn get_f32(&self, key: u32) -> Option<f32> {
        self.get(key)?.parse().ok()
    }

    pub fn get_i32(&self, key: u32) -> Option<i32> {
        self.get(key)?.parse().ok()
    }

    /// Object ID (key 1).
    pub fn id(&self) -> i32 {
        self.get_i32(1).unwrap_or(0)
    }

    /// The id the game uses once the level is loaded: a few legacy ids are converted (the old
    /// color triggers 104 → 915 and 221/717/718 → 899, the old sawblades 675–677 → 1734–1736).
    /// Color triggers keep reading `id` (their old ids name the channel).
    pub fn canonical_id(&self) -> i32 {
        match self.id() {
            104 => 915,
            221 | 717 | 718 => 899,
            675 => 1734,
            676 => 1735,
            677 => 1736,
            id => id,
        }
    }

    pub fn x(&self) -> f32 {
        self.get_f32(2).unwrap_or(0.0)
    }

    pub fn y(&self) -> f32 {
        self.get_f32(3).unwrap_or(0.0)
    }

    /// Group ids (key 57, dot-separated).
    pub fn groups(&self) -> Vec<i32> {
        self.get(57).map(|g| g.split('.').filter_map(|v| v.parse().ok()).collect()).unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LevelString {
    /// Level settings (`kS38`, `kA13`, ...), raw values in file order.
    pub settings: Vec<(String, String)>,
    pub objects: Vec<RawObject>,
}

impl LevelString {
    pub fn parse(text: &str) -> Self {
        let mut sections = text.split(';');
        let settings = pairs(sections.next().unwrap_or_default())
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();

        let mut objects = Vec::new();
        for section in sections.filter(|s| !s.is_empty()) {
            objects.push(RawObject::parse(section));
        }
        LevelString { settings, objects }
    }

    pub fn setting(&self, key: &str) -> Option<&str> {
        self.settings.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// Write the level back in the game's format.
    pub fn to_string(&self) -> String {
        let mut out = String::new();
        join_pairs(&mut out, self.settings.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        out.push(';');
        for obj in &self.objects {
            let props: Vec<(String, &str)> = obj.props().map(|(k, v)| (k.to_string(), v)).collect();
            join_pairs(&mut out, props.iter().map(|(k, v)| (k.as_str(), *v)));
            out.push(';');
        }
        out
    }
}

/// Split `a,1,b,2` into `(a, 1), (b, 2)`. A trailing key without a value is dropped.
fn pairs(section: &str) -> impl Iterator<Item = (&str, &str)> {
    let mut parts = section.split(',');
    std::iter::from_fn(move || Some((parts.next()?, parts.next()?)))
}

fn join_pairs<'a>(out: &mut String, items: impl Iterator<Item = (&'a str, &'a str)>) {
    for (i, (k, v)) in items.enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(k);
        out.push(',');
        out.push_str(v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_settings_and_objects() {
        let level = LevelString::parse("kA13,0,kS38,1_40;1,8,2,45,3,15;1,1,2,75,3,15,57,2.3;");
        assert_eq!(level.setting("kA13"), Some("0"));
        assert_eq!(level.objects.len(), 2);
        assert_eq!(level.objects[0].id(), 8);
        assert_eq!(level.objects[1].x(), 75.0);
        assert_eq!(level.objects[1].get(57), Some("2.3"));
    }

    #[test]
    fn round_trips_unchanged() {
        let text = "kA13,0,kS38,1_40;1,8,2,45,3,15;1,1,2,75,3,15,57,2.3;";
        assert_eq!(LevelString::parse(text).to_string(), text);
    }

    #[test]
    fn start_positions_keep_string_keys() {
        let text = "kA13,0;1,31,2,2745,3,75,kA2,0,kA4,1;";
        let level = LevelString::parse(text);
        assert_eq!(level.objects[0].id(), 31);
        assert_eq!(level.objects[0].get_str_key("kA4"), Some("1"));
        assert_eq!(level.to_string(), text);
    }
}
