//! Loading the player's own game files: sprite sheets (cocos2d/TexturePacker plists + PNGs).

use std::collections::HashMap;

use crate::files::Files;

/// Texture quality, i.e. which file variant to load. Layout is identical; only pixels differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Low,
    Medium,
    High,
}

impl Quality {
    pub fn suffix(self) -> &'static str {
        match self {
            Quality::Low => "",
            Quality::Medium => "-hd",
            Quality::High => "-uhd",
        }
    }

    /// Pixels per game point.
    pub fn scale(self) -> f32 {
        match self {
            Quality::Low => 1.0,
            Quality::Medium => 2.0,
            Quality::High => 4.0,
        }
    }
}

/// Decoded RGBA image.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn load_png(files: &dyn Files, path: &str) -> Result<Image, String> {
    let bytes = files.read(path).ok_or_else(|| format!("{path}: missing"))?;
    decode_png(bytes, path)
}

/// A PNG from memory (premultiplied like cocos2d's).
pub fn decode_png(bytes: Vec<u8>, path: &str) -> Result<Image, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(|e| format!("{path}: {e}"))?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("png too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("{path}: {e}"))?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::GrayscaleAlpha => buf.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        other => return Err(format!("{path}: unsupported color type {other:?}")),
    };
    Ok(Image { width: info.width, height: info.height, rgba: premultiply(rgba) })
}

/// cocos2d premultiplies alpha when it loads a PNG (`CC_RGB_PREMULTIPLY_ALPHA`), and the game's
/// blend modes assume it. Same rounding: c * (a + 1) >> 8.
fn premultiply(mut rgba: Vec<u8>) -> Vec<u8> {
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3] as u32 + 1;
        for c in &mut px[..3] {
            *c = ((*c as u32 * a) >> 8) as u8;
        }
    }
    rgba
}

/// One frame in a sheet, in points (pixels / quality scale).
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub sheet: usize,
    /// Atlas rect in pixels (as stored; if `rotated`, width/height are swapped in the atlas).
    pub tex_x: f32,
    pub tex_y: f32,
    pub tex_w: f32,
    pub tex_h: f32,
    pub rotated: bool,
    /// Trimmed image size, offset of its center from the untrimmed center, and untrimmed size.
    pub size: (f32, f32),
    pub offset: (f32, f32),
    pub source_size: (f32, f32),
}

pub struct Sheets {
    pub images: Vec<Image>,
    pub frames: HashMap<String, Frame>,
    pub quality: Quality,
    /// Sheets that failed to load (not retried).
    pub failed: std::collections::HashSet<String>,
}

impl Sheets {
    pub fn new(quality: Quality) -> Sheets {
        Sheets { images: Vec::new(), frames: HashMap::new(), quality, failed: Default::default() }
    }

    /// Load `<dir>/<name><suffix>.plist` and its texture (`dir` is "" or e.g. "icons/"). Later
    /// sheets don't override earlier frames with the same name.
    pub fn load(&mut self, files: &dyn Files, dir: &str, name: &str) -> Result<(), String> {
        let suffix = self.quality.suffix();
        let plist_path = format!("{dir}{name}{suffix}.plist");
        let bytes = files.read(&plist_path).ok_or_else(|| format!("{plist_path}: missing"))?;
        let root = plist::Value::from_reader(std::io::Cursor::new(bytes)).map_err(|e| format!("{plist_path}: {e}"))?;
        let root = root.as_dictionary().ok_or("plist root is not a dict")?;
        let texture = root
            .get("metadata")
            .and_then(|m| m.as_dictionary())
            .and_then(|m| m.get("textureFileName").or_else(|| m.get("realTextureFileName")))
            .and_then(|v| v.as_string())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{name}{suffix}.png"));
        // Icon plists name their texture relative to Resources ("icons/player_01-uhd.png").
        let texture_path = if texture.contains('/') { texture } else { format!("{dir}{texture}") };
        let image = load_png(files, &texture_path)?;
        let sheet = self.images.len();
        let scale = self.quality.scale();
        let frames = root.get("frames").and_then(|f| f.as_dictionary()).ok_or("no frames")?;
        for (frame_name, value) in frames {
            let Some(d) = value.as_dictionary() else { continue };
            let get = |k: &str| d.get(k).and_then(|v| v.as_string()).unwrap_or("");
            let rect = parse_floats(get("textureRect"));
            let size = parse_floats(get("spriteSize"));
            let offset = parse_floats(get("spriteOffset"));
            let source = parse_floats(get("spriteSourceSize"));
            if rect.len() < 4 || size.len() < 2 {
                continue;
            }
            let rotated = d.get("textureRotated").and_then(|v| v.as_boolean()).unwrap_or(false);
            let pt = |v: f32| v / scale;
            self.frames.entry(frame_name.clone()).or_insert(Frame {
                sheet,
                tex_x: rect[0],
                tex_y: rect[1],
                tex_w: rect[2],
                tex_h: rect[3],
                rotated,
                size: (pt(size[0]), pt(size[1])),
                offset: (pt(*offset.first().unwrap_or(&0.0)), pt(*offset.get(1).unwrap_or(&0.0))),
                source_size: (pt(*source.first().unwrap_or(&size[0])), pt(*source.get(1).unwrap_or(&size[1]))),
            });
        }
        self.images.push(image);
        Ok(())
    }

    /// Load a single image file as a one-frame sheet named after the file.
    pub fn load_single(&mut self, files: &dyn Files, name: &str) -> Result<(), String> {
        let suffix = self.quality.suffix();
        self.load_image(files, &format!("{name}{suffix}.png"), &format!("{name}.png"), self.quality.scale())
    }

    /// Load a single image at this quality, or the best lower resolution the game ships it in
    /// (cocos2d falls back -uhd → -hd → plain; `GJ_gradientBG` has no -uhd file).
    pub fn load_single_best(&mut self, files: &dyn Files, name: &str) -> Result<(), String> {
        let options = [(Quality::High, "-uhd", 4.0), (Quality::Medium, "-hd", 2.0), (Quality::Low, "", 1.0)];
        let start = options.iter().position(|o| o.0 == self.quality).unwrap_or(0);
        for &(_, suffix, scale) in &options[start..] {
            let file = format!("{name}{suffix}.png");
            if files.read(&file).is_some() {
                return self.load_image(files, &file, &format!("{name}.png"), scale);
            }
        }
        Err(format!("{name}.png: missing"))
    }

    /// One image the game ships in a single resolution (`square.png`, the particle texture).
    pub fn load_unscaled(&mut self, files: &dyn Files, file: &str) -> Result<(), String> {
        self.load_image(files, file, file, 1.0)
    }

    fn load_image(&mut self, files: &dyn Files, file: &str, frame_name: &str, scale: f32) -> Result<(), String> {
        let image = load_png(files, file)?;
        self.add_image(frame_name, image, scale);
        Ok(())
    }

    /// An image made or downloaded at run time (the news pictures) as a one-frame sheet.
    pub fn add_image(&mut self, frame_name: &str, image: Image, scale: f32) {
        let (w, h) = (image.width as f32, image.height as f32);
        self.frames.insert(
            frame_name.to_string(),
            Frame {
                sheet: self.images.len(),
                tex_x: 0.0,
                tex_y: 0.0,
                tex_w: w,
                tex_h: h,
                rotated: false,
                size: (w / scale, h / scale),
                offset: (0.0, 0.0),
                source_size: (w / scale, h / scale),
            },
        );
        self.images.push(image);
    }
}

/// "{{1,2},{3,4}}" or "{5,6}" -> [1,2,3,4] / [5,6]
fn parse_floats(s: &str) -> Vec<f32> {
    s.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e'))
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_floats;

    #[test]
    fn parses_cocos_rects() {
        assert_eq!(parse_floats("{{3709,991},{162,162}}"), vec![3709.0, 991.0, 162.0, 162.0]);
        assert_eq!(parse_floats("{-79,79}"), vec![-79.0, 79.0]);
    }
}
