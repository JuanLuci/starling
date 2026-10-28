//! Ely.by's skin system (skinsystem.ely.by): the skins players without a Mojang account keep there
//! under their name, which offline launchers and servers show. Asked by name without signing in,
//! each name once a session on a thread of its own; Skin Customization's Ely.by Skins turns it off
//! (`elyby:off` in horizon-player.txt).
use crate::{offline_name, skins::Textures};
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};

/// `GET /textures/{nickname}`: the skin's URL and model, 204 for a name without a skin.
const TEXTURES: &str = "https://skinsystem.ely.by/textures/";
const KEY: &str = "elyby";

/// Whether Ely.by's skins show: on unless Skin Customization turned them off.
pub fn enabled(dir: &Path) -> bool {
    offline_name::value(dir, KEY).as_deref() != Some("off")
}

/// Keep Skin Customization's Ely.by Skins in `dir`'s horizon-player.txt.
pub fn set_enabled(dir: &Path, on: bool) -> std::io::Result<()> {
    offline_name::set_values(dir, &[(KEY, Some(if on { "on" } else { "off" }))])
}

/// Whether `name` can be a player's (1 to 16 letters, digits or `_`): an NPC's styled name is
/// never asked for.
pub fn is_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The skin of `/textures/{nickname}`'s answer, `{"SKIN": {"url": ..., "metadata": {"model":
/// "slim"}}}`; None when it names none. A cape is left out: one that failed to download would
/// fail the skin with it (`skins::lookup`).
pub fn parse(json: &str) -> Option<Textures> {
    let answer: serde_json::Value = serde_json::from_str(json).ok()?;
    let skin = &answer["SKIN"];
    let url = skin["url"].as_str()?;
    Some(Textures {
        skin: Some(url.to_owned()),
        slim: skin["metadata"]["model"].as_str() == Some("slim"),
        ..Textures::default()
    })
}

/// Names asked of Ely.by this session and what it answered.
pub struct Lookup {
    /// By the name in lower case (Ely.by's are told apart without case): None while asked.
    answers: HashMap<String, Option<Option<Textures>>>,
    send: Sender<(String, Option<Textures>)>,
    done: Receiver<(String, Option<Textures>)>,
}

impl Default for Lookup {
    fn default() -> Self {
        let (send, done) = channel();
        Self {
            answers: HashMap::new(),
            send,
            done,
        }
    }
}

impl Lookup {
    /// `name`'s skin on Ely.by once it answered (None before, and for a name without one); asked
    /// the first time.
    pub fn textures(&mut self, name: &str) -> Option<Textures> {
        while let Ok((key, answer)) = self.done.try_recv() {
            self.answers.insert(key, Some(answer));
        }
        if !is_name(name) {
            return None;
        }
        let key = name.to_ascii_lowercase();
        if let Some(answer) = self.answers.get(&key) {
            return answer.clone().flatten();
        }
        self.answers.insert(key.clone(), None);
        let send = self.send.clone();
        let asked = std::thread::Builder::new()
            .name("ely.by".into())
            .spawn(move || {
                let answer = fetch(&key);
                let _ = send.send((key, answer));
            });
        if let Err(e) = asked {
            log::warn!("Horizon: Ely.by skin: {e}");
        }
        None
    }
}

/// Ely.by's answer for `name`: its skin, or None (no skin, or no answer).
fn fetch(name: &str) -> Option<Textures> {
    use std::io::Read;
    let response = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .get(&format!("{TEXTURES}{name}"))
        .call()
        .ok()?;
    if response.status() != 200 {
        return None;
    }
    let mut body = String::new();
    response
        .into_reader()
        .take(64 << 10)
        .read_to_string(&mut body)
        .ok()?;
    parse(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skin_and_its_model_from_the_answer() {
        let wide = r#"{"SKIN":{"url":"http://ely.by/storage/skins/69c6740d2993e5d6f6a7fc92420efc29.png"}}"#;
        let t = parse(wide).unwrap();
        assert_eq!(
            t.skin.as_deref(),
            Some("http://ely.by/storage/skins/69c6740d2993e5d6f6a7fc92420efc29.png")
        );
        assert!(!t.slim && !t.signed && t.cape.is_none());
        let slim = r#"{"SKIN":{"url":"http://ely.by/storage/skins/a.png","metadata":{"model":"slim"}},"CAPE":{"url":"http://ely.by/storage/cloaks/b.png"}}"#;
        let t = parse(slim).unwrap();
        assert!(t.slim);
        assert!(t.cape.is_none());
        assert!(parse("").is_none());
        assert!(parse(r#"{"CAPE":{"url":"http://ely.by/storage/cloaks/b.png"}}"#).is_none());
    }

    #[test]
    fn only_player_names_are_asked() {
        assert!(is_name("ErickSkrauch"));
        assert!(is_name("a_1"));
        assert!(!is_name(""));
        assert!(!is_name("§6Shopkeeper"));
        assert!(!is_name("seventeen_letters"));
        assert!(!is_name("two words"));
        let mut lookup = Lookup::default();
        assert!(lookup.textures("§cNPC").is_none());
        assert!(lookup.answers.is_empty());
    }

    #[test]
    fn turned_off_and_on_in_the_player_file() {
        let dir = std::env::temp_dir().join(format!("horizon-elyby-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert!(enabled(&dir));
        set_enabled(&dir, false).unwrap();
        assert!(!enabled(&dir));
        set_enabled(&dir, true).unwrap();
        assert!(enabled(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore = "asks skinsystem.ely.by"]
    fn ely_by_answers_a_known_name() {
        let t = fetch("ErickSkrauch").expect("a skin");
        assert!(t.skin.unwrap().contains("ely.by"));
        assert!(fetch("zzzznosuchname123").is_none());
    }
}
