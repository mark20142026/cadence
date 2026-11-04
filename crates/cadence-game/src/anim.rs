//! Part-animated icons (the robot and the spider): the game's `*_AnimDesc.plist` files, which
//! `CCPartAnimSprite` plays. Each frame lists the icon's parts (a texture from the icon sheet,
//! with position, scale, rotation, flips and z order). Parts tagged `back01`..`back03` are the
//! far limbs, which `GJRobotSprite::updateColors` tints at 0.7 of the player's colors.

use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    /// Icon frame without the suffix, e.g. "robot_01_03" (the sheet has `_001`, `_2_001`, `_glow_001`).
    pub texture: String,
    pub pos: (f32, f32),
    pub scale: (f32, f32),
    /// Degrees, clockwise like cocos2d.
    pub rotation: f32,
    pub flip: (bool, bool),
    pub z: i32,
    /// A far limb (customID "back…"): drawn darker.
    pub back: bool,
    /// The part's tag: which `CCSpritePart` this is (`CCPartAnimSprite::transformSprite`
    /// 0x2ec9dc picks the part by it).
    pub tag: u32,
}

#[derive(Clone, Debug, Default)]
pub struct AnimDesc {
    /// Animation name ("run", "jump_loop", ...) → frames → parts sorted by z.
    anims: HashMap<String, Vec<Vec<Part>>>,
}

/// "{a, b}" → (a, b).
fn pair(s: &str) -> (f32, f32) {
    let s = s.trim().trim_start_matches('{').trim_end_matches('}');
    let mut it = s.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(0.0));
    (it.next().unwrap_or(0.0), it.next().unwrap_or(0.0))
}

impl AnimDesc {
    pub fn parse(bytes: &[u8]) -> Result<AnimDesc, String> {
        // The robot's file starts with a blank line before `<?xml`.
        let start = bytes.iter().position(|&b| b == b'<').unwrap_or(0);
        let root = plist::Value::from_reader(std::io::Cursor::new(&bytes[start..])).map_err(|e| e.to_string())?;
        let root = root.as_dictionary().ok_or("anim desc root is not a dict")?;
        // customIDs by tag ("back01" for the far limbs).
        let mut back_tags = Vec::new();
        if let Some(used) = root.get("usedTextures").and_then(|v| v.as_dictionary()) {
            for t in used.values().filter_map(|v| v.as_dictionary()) {
                let tag = t.get("tag").and_then(|v| v.as_string()).unwrap_or("");
                if t.get("customID").and_then(|v| v.as_string()).is_some_and(|c| c.starts_with("back")) {
                    back_tags.push(tag.to_string());
                }
            }
        }
        let container = root.get("animationContainer").and_then(|v| v.as_dictionary()).ok_or("no animationContainer")?;
        let mut frames: HashMap<String, Vec<(u32, Vec<Part>)>> = HashMap::new();
        for (name, frame) in container {
            // "Robot_run_001.png" → ("run", 1)
            let stem = name.trim_end_matches(".png");
            let Some((rest, index)) = stem.rsplit_once('_') else { continue };
            let Some((_, anim)) = rest.split_once('_') else { continue };
            let Ok(index) = index.parse::<u32>() else { continue };
            let Some(frame) = frame.as_dictionary() else { continue };
            let mut parts: Vec<Part> = frame
                .values()
                .filter_map(|v| v.as_dictionary())
                .map(|p| {
                    let get = |k: &str| p.get(k).and_then(|v| v.as_string()).unwrap_or("");
                    let (fx, fy) = pair(get("flipped"));
                    Part {
                        texture: get("texture").trim_end_matches("_001.png").to_string(),
                        pos: pair(get("position")),
                        scale: pair(get("scale")),
                        rotation: get("rotation").parse().unwrap_or(0.0),
                        flip: (fx != 0.0, fy != 0.0),
                        z: get("zValue").parse().unwrap_or(0),
                        back: back_tags.iter().any(|t| t == get("tag")),
                        tag: get("tag").parse().unwrap_or(0),
                    }
                })
                .collect();
            parts.sort_by_key(|p| p.z);
            frames.entry(anim.to_string()).or_default().push((index, parts));
        }
        let anims = frames
            .into_iter()
            .map(|(name, mut list)| {
                list.sort_by_key(|f| f.0);
                (name, list.into_iter().map(|f| f.1).collect())
            })
            .collect();
        Ok(AnimDesc { anims })
    }

    /// Frame `index` (wrapping) of an animation.
    pub fn frame(&self, anim: &str, index: usize) -> Option<&[Part]> {
        let frames = self.anims.get(anim)?;
        frames.get(index % frames.len().max(1)).map(Vec::as_slice)
    }

    pub fn len(&self, anim: &str) -> usize {
        self.anims.get(anim).map_or(0, Vec::len)
    }
}

/// Which animation an icon plays and for how long it has played it.
#[derive(Clone, Debug, Default)]
pub struct AnimState {
    pub anim: &'static str,
    pub time: f32,
}

impl AnimState {
    /// Switch to `anim` (restarting it) or keep playing; `dt` seconds.
    pub fn play(&mut self, anim: &'static str, dt: f32) {
        if self.anim != anim {
            self.anim = anim;
            self.time = 0.0;
        } else {
            self.time += dt;
        }
    }

    /// The frame index at `delay` seconds per frame.
    pub fn index(&self, delay: f32) -> usize {
        (self.time / delay.max(1e-3)) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs() {
        assert_eq!(pair("{-7.5, -5.175}"), (-7.5, -5.175));
        assert_eq!(pair("{0, 1}"), (0.0, 1.0));
    }
}
