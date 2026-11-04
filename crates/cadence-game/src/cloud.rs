//! Cloud save and load: the Account page's Save and Load (`AccountLayer::doBackup` /
//! `doSync`, `GJAccountManager::backupAccount` / `syncAccount`).
//!
//! The game asks `getAccountURL.php` (`accountID=%i&type=%i&secret=…`, type 1 to save, 2 to
//! load) for the account server, then POSTs to `<that>/database/accounts/backupGJAccountNew.php`
//! the base string, `&saveData=` and its two saves (`CCGameManager` and `CCLocalLevels` as plain
//! plist XML, each gzipped and URL-safe base64'd, joined by `;`), and `&secret=Wmfv3899gc9`;
//! loading POSTs the base string and the secret to `syncGJAccountNew.php`, which answers the two
//! saves the same way, then `;gameVersion;binaryVersion;…`. (Recorded from the real game with
//! the harness's request recorder.)
//!
//! Cadence keeps its own, smaller progress (`ui::save::Progress`), so:
//! - Load reads the cloud saves into it: each main, tower and online level's best, practice best,
//!   attempts, jumps and coins, the vaults' coins, the game variables and volumes.
//! - Save downloads the cloud saves first and writes Cadence's progress into them, never lowering
//!   anything (best %, attempts, coins only grow), so everything Cadence doesn't keep (icons,
//!   stats it doesn't track, the player's own levels) stays exactly as the game left it.

use cadence_core::save::{Dict, Value};

use crate::ui::save::Progress;

const SECRET: &str = "Wmfd2893gb7";
const ACCOUNT_SECRET: &str = "Wmfv3899gc9";

/// The two saves, as plist XML.
#[derive(Clone, Debug, Default)]
pub struct CloudSave {
    pub game: String,
    pub levels: String,
}

/// One save as the requests carry it: gzip, then URL-safe base64.
pub fn encode_part(xml: &str) -> String {
    cadence_core::codec::base64_encode_url(&cadence_core::codec::gzip(xml.as_bytes()))
}

pub fn decode_part(part: &str) -> Result<String, String> {
    let raw = cadence_core::codec::base64_decode(part.trim()).map_err(|e| e.to_string())?;
    let xml = cadence_core::codec::inflate(&raw).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&xml).into_owned())
}

/// `getAccountURL.php`: the account server for saving (1) or loading (2).
async fn account_url(kind: i32) -> Result<String, i32> {
    let id = crate::gjapi::identity();
    let account = id.account.as_ref().ok_or(-1)?;
    let body = crate::net::post("getAccountURL.php", &format!("accountID={}&type={kind}&secret={SECRET}", account.account_id)).await.map_err(|_| -1)?;
    let url = body.trim();
    if url.is_empty() || url.starts_with('-') || !url.contains("://") {
        return Err(-1);
    }
    Ok(url.trim_end_matches('/').to_string())
}

/// The backup request's body.
pub fn backup_form(base: &str, save: &CloudSave) -> String {
    format!("{base}&saveData={};{}&secret={ACCOUNT_SECRET}", encode_part(&save.game), encode_part(&save.levels))
}

/// Save to the cloud. Errors are the server's codes (−1 anything else).
pub async fn backup(save: &CloudSave) -> Result<(), i32> {
    let url = account_url(1).await?;
    let base = crate::gjapi::identity().base();
    let body = crate::net::post_url(&format!("{url}/database/accounts/backupGJAccountNew.php"), &backup_form(&base, save)).await.map_err(|_| -1)?;
    match body.trim() {
        "1" => Ok(()),
        other => Err(other.parse().unwrap_or(-1)),
    }
}

/// Load from the cloud.
pub async fn sync() -> Result<CloudSave, i32> {
    let url = account_url(2).await?;
    let base = crate::gjapi::identity().base();
    let body = crate::net::post_url(&format!("{url}/database/accounts/syncGJAccountNew.php"), &format!("{base}&secret={ACCOUNT_SECRET}")).await.map_err(|_| -1)?;
    parse_sync(&body)
}

pub fn parse_sync(body: &str) -> Result<CloudSave, i32> {
    let body = body.trim();
    if let Ok(code) = body.parse::<i32>() {
        return Err(code.min(-1));
    }
    let mut parts = body.split(';');
    let game = parts.next().map(decode_part).ok_or(-1)?.map_err(|_| -1)?;
    let levels = parts.next().map(decode_part).unwrap_or(Ok(String::new())).map_err(|_| -1)?;
    if !game.contains("<dict>") {
        return Err(-1);
    }
    Ok(CloudSave { game, levels })
}

fn get<'a>(d: &'a Dict, key: &str) -> Option<&'a Value> {
    d.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn num(d: &Dict, key: &str) -> u32 {
    get(d, key).and_then(Value::as_str).and_then(|s| s.parse::<f64>().ok()).map_or(0, |v| v.max(0.0) as u32)
}

fn dict_mut<'a>(d: &'a mut Dict, key: &str) -> &'a mut Dict {
    if !d.iter().any(|(k, v)| k == key && matches!(v, Value::Dict(_))) {
        d.retain(|(k, _)| k != key);
        d.push((key.to_string(), Value::Dict(Vec::new())));
    }
    match d.iter_mut().find(|(k, _)| k == key) {
        Some((_, Value::Dict(inner))) => inner,
        _ => unreachable!(),
    }
}

fn set(d: &mut Dict, key: &str, v: Value) {
    match d.iter_mut().find(|(k, _)| k == key) {
        Some((_, old)) => *old = v,
        None => d.push((key.to_string(), v)),
    }
}

/// The level dicts Cadence keeps progress for: the main and tower levels (`GLM_01`) and the
/// online ones (`GLM_03`).
const LEVEL_DICTS: [&str; 2] = ["GLM_01", "GLM_03"];

fn is_main(id: i32) -> bool {
    (1..=22).contains(&id) || id == 3001 || (5001..=5004).contains(&id)
}

/// Read the cloud's game save into Cadence's progress (the larger of the two everywhere).
pub fn import(game_xml: &str, p: &mut Progress) -> Result<(), String> {
    let root = cadence_core::save::parse_plist(game_xml).map_err(|e| e.to_string())?;
    for list in LEVEL_DICTS {
        let Some(levels) = get(&root, list).and_then(Value::as_dict) else { continue };
        for (key, v) in levels {
            let (Ok(id), Some(l)) = (key.parse::<i32>(), v.as_dict()) else { continue };
            let (best, practice, attempts, jumps) = (num(l, "k19").min(100), num(l, "k20").min(100), num(l, "k18"), num(l, "k36"));
            if best + practice + attempts + jumps == 0 {
                continue;
            }
            let e = p.entry(id);
            e.best = e.best.max(best);
            e.practice = e.practice.max(practice);
            e.attempts = e.attempts.max(attempts);
            e.jumps = e.jumps.max(jumps);
        }
    }
    // coins: main levels' `unique_<id>_<n>` and the vaults' `unique_<name>` in GS_value, online
    // levels' `<id>_<n>` in GS_3 (verified) and GS_4 (pending)
    if let Some(values) = get(&root, "GS_value").and_then(Value::as_dict) {
        for (key, _) in values {
            let Some(rest) = key.strip_prefix("unique_") else { continue };
            match rest.split_once('_').and_then(|(a, b)| Some((a.parse::<i32>().ok()?, b.parse::<u32>().ok()?))) {
                Some((id, n)) if (1..=3).contains(&n) => p.entry(id).coins |= 1 << (n - 1),
                _ if rest.starts_with("secret") => p.set_flag(&format!("coin {rest}")),
                _ => {}
            }
        }
    }
    for list in ["GS_3", "GS_4"] {
        if let Some(coins) = get(&root, list).and_then(Value::as_dict) {
            for (key, _) in coins {
                if let Some((id, n)) = key.split_once('_').and_then(|(a, b)| Some((a.parse::<i32>().ok()?, b.parse::<u32>().ok()?))) {
                    if (1..=3).contains(&n) {
                        p.entry(id).coins |= 1 << (n - 1);
                    }
                }
            }
        }
    }
    // the options' game variables (gv_NNNN) and the volumes
    if let Some(vars) = get(&root, "valueKeeper").and_then(Value::as_dict) {
        for (key, v) in vars {
            if let (Some(n), Some(s)) = (key.strip_prefix("gv_"), v.as_str()) {
                p.settings.insert(n.to_string(), if s == "1" { "1" } else { "0" }.to_string());
            }
        }
    }
    for (key, name) in [("bgVolume", "music"), ("sfxVolume", "sfx")] {
        if let Some(v) = get(&root, key).and_then(Value::as_str).and_then(|s| s.parse::<f32>().ok()) {
            p.set_number(name, v);
        }
    }
    Ok(())
}

/// Cadence's progress written into the cloud's game save: values only ever go up, and new main
/// level completions and coins count in the stats like the game's own (`c_<id>` / `star_<id>`,
/// stats 3 levels, 6 stars, 8 secret coins).
pub fn merge(game_xml: &str, p: &Progress) -> Result<String, String> {
    let mut root = cadence_core::save::parse_plist(game_xml).map_err(|e| e.to_string())?;
    let mut new_levels = 0;
    let mut new_stars = 0;
    let mut new_coins = 0;
    let mut new_completed: Vec<i32> = Vec::new();
    let mut unique: Vec<String> = Vec::new();
    for (&id, lp) in &p.levels {
        let list = if is_main(id) { "GLM_01" } else { "GLM_03" };
        let levels = dict_mut(&mut root, list);
        let l = dict_mut(levels, &id.to_string());
        if get(l, "kCEK").is_none() {
            set(l, "kCEK", Value::Int("4".into()));
            set(l, "k1", Value::Int(id.to_string()));
        }
        let was_complete = num(l, "k19") >= 100;
        for (key, v) in [("k19", lp.best), ("k20", lp.practice), ("k18", lp.attempts), ("k36", lp.jumps)] {
            if v > num(l, key) {
                set(l, key, Value::Int(v.to_string()));
            }
        }
        if is_main(id) {
            if lp.best >= 100 && !was_complete {
                new_completed.push(id);
            }
            for n in 0..3 {
                if lp.coin(n) {
                    unique.push(format!("unique_{id}_{}", n + 1));
                }
            }
        }
    }
    for f in p.flags.iter().filter_map(|f| f.strip_prefix("coin ")) {
        unique.push(format!("unique_{f}"));
    }
    {
        let completed = dict_mut(&mut root, "GS_completed");
        for id in &new_completed {
            if get(completed, &format!("c_{id}")).is_none() {
                set(completed, &format!("c_{id}"), Value::Str("1".into()));
                set(completed, &format!("star_{id}"), Value::Str("1".into()));
                new_levels += 1;
                new_stars += crate::ui::level_info(*id).stars;
            }
        }
    }
    let values = dict_mut(&mut root, "GS_value");
    for key in unique {
        if get(values, &key).is_none() {
            set(values, &key, Value::Str("1".into()));
            if key.split('_').count() == 3 {
                new_coins += 1;
            }
        }
    }
    for (stat, add) in [("3", new_levels), ("6", new_stars), ("8", new_coins)] {
        if add > 0 {
            let v = num(values, stat) + add;
            set(values, stat, Value::Str(v.to_string()));
        }
    }
    let vars = dict_mut(&mut root, "valueKeeper");
    for (key, v) in &p.settings {
        if key.len() == 4 && key.bytes().all(|b| b.is_ascii_digit()) {
            set(vars, &format!("gv_{key}"), Value::Str(v.clone()));
        }
    }
    for (key, name) in [("bgVolume", "music"), ("sfxVolume", "sfx")] {
        if p.settings.contains_key(name) {
            set(&mut root, key, Value::Real(format!("{}", p.number(name, 1.0))));
        }
    }
    Ok(cadence_core::save::write_plist(&root))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAME: &str = r#"<?xml version="1.0"?><plist version="1.0" gjver="2.0"><dict><k>valueKeeper</k><d><k>gv_0026</k><s>1</s></d><k>bgVolume</k><r>0.5</r><k>playerFrame</k><i>12</i><k>GLM_01</k><d><k>1</k><d><k>kCEK</k><i>4</i><k>k1</k><i>1</i><k>k18</k><i>12</i><k>k36</k><i>55</i><k>k19</k><i>26</i><k>k2</k><s>Stereo Madness</s></d></d><k>GS_value</k><d><k>6</k><s>3</s><k>8</k><s>1</s><k>unique_2_1</k><s>1</s><k>unique_secret06</k><s>1</s></d><k>GS_completed</k><d /><k>GS_3</k><d><k>6508283_2</k><s>1</s></d></dict></plist>"#;

    #[test]
    fn imports_levels_coins_and_settings() {
        let mut p = Progress::default();
        import(GAME, &mut p).unwrap();
        assert_eq!((p.get(1).best, p.get(1).attempts, p.get(1).jumps), (26, 12, 55));
        assert!(p.get(2).coin(0) && !p.get(2).coin(1));
        assert!(p.get(6508283).coin(1));
        assert!(p.flag("coin secret06"));
        assert!(p.var("0026", false));
        assert_eq!(p.number("music", 1.0), 0.5);
    }

    #[test]
    fn merging_only_raises_and_keeps_the_rest() {
        let mut p = Progress::default();
        p.entry(1).best = 100;
        p.entry(1).attempts = 3;
        p.entry(1).coins = 0b001;
        let xml = merge(GAME, &p).unwrap();
        let root = cadence_core::save::parse_plist(&xml).unwrap();
        let l1 = get(get(&root, "GLM_01").unwrap().as_dict().unwrap(), "1").unwrap().as_dict().unwrap();
        // best goes up to 100, attempts stay at the cloud's 12, the name is untouched
        assert_eq!((num(l1, "k19"), num(l1, "k18")), (100, 12));
        assert_eq!(get(l1, "k2").and_then(Value::as_str), Some("Stereo Madness"));
        let values = get(&root, "GS_value").unwrap().as_dict().unwrap();
        assert!(get(values, "unique_1_1").is_some());
        // Stereo Madness's 1 star and its coin join the stats; the icon stays
        assert_eq!((num(values, "6"), num(values, "8")), (4, 2));
        assert_eq!(get(&root, "playerFrame").and_then(Value::as_str), Some("12"));
        let completed = get(&root, "GS_completed").unwrap().as_dict().unwrap();
        assert!(get(completed, "c_1").is_some());
    }

    #[test]
    fn parts_round_trip_and_sync_answers_parse() {
        let body = format!("{};{};22;51;a;b", encode_part(GAME), encode_part("<?xml?><plist><dict></dict></plist>"));
        let s = parse_sync(&body).unwrap();
        assert_eq!(s.game, GAME);
        assert_eq!(parse_sync("-2").unwrap_err(), -2);
        let form = backup_form("gameVersion=22", &s);
        assert!(form.starts_with("gameVersion=22&saveData=H4sI") && form.ends_with("&secret=Wmfv3899gc9"));
    }
}
