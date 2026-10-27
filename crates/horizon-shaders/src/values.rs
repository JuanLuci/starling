//! Option values a player picked (spec 8.1, 8.3): the value of a `#define NAME VALUE` or
//! `const TYPE NAME = VALUE;` line, a switch `#define NAME` on or commented out, and the sets of
//! them a `profile.NAME` of `shaders.properties` names. They take effect where the pack declares
//! them: [`Pack::set_values`](crate::Pack::set_values) rewrites those lines as the sources are
//! read, so the preprocessor, the constants and the switches all see the picked value.

use std::collections::BTreeMap;

use crate::Properties;

/// Values by option name: a number or word for a valued option, `true` or `false` for a switch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Values {
    values: BTreeMap<String, String>,
}

impl Values {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn set(&mut self, name: &str, value: &str) {
        self.values.insert(name.to_owned(), value.to_owned());
    }

    /// `NAME=VALUE`, `NAME` (a switch on) and `!NAME` (off), separated by spaces or commas, as a
    /// profile lists them; later ones win.
    pub fn parse_list(&mut self, list: &str) {
        for word in list.split([' ', ',', '\t']).filter(|w| !w.is_empty()) {
            if let Some(name) = word.strip_prefix('!') {
                self.set(name, "false");
            } else if let Some((name, value)) = word.split_once('=') {
                self.set(name, value);
            } else {
                self.set(word, "true");
            }
        }
    }

    /// The values `profile.NAME` sets (spec 8.3): the profiles it names first, then its own.
    /// None when the pack has no such profile.
    pub fn profile(properties: &Properties, name: &str) -> Option<Self> {
        let mut out = Self::default();
        out.add_profile(properties, name, 0).then_some(out)
    }

    fn add_profile(&mut self, properties: &Properties, name: &str, depth: u8) -> bool {
        let Some(list) = properties.get(&format!("profile.{name}")) else {
            return false;
        };
        let (inherited, own): (Vec<&str>, Vec<&str>) = list
            .split_whitespace()
            .partition(|w| w.starts_with("profile."));
        if depth < 8 {
            for other in inherited {
                self.add_profile(properties, &other["profile.".len()..], depth + 1);
            }
        }
        self.parse_list(&own.join(" "));
        true
    }

    /// `text` with every declaration of an option set here carrying its value.
    pub fn apply(&self, text: &str) -> String {
        if self.values.is_empty() {
            return text.to_owned();
        }
        let mut out = String::with_capacity(text.len());
        for line in text.split_inclusive('\n') {
            let (body, end) = match line.strip_suffix('\n') {
                Some(b) => b.strip_suffix('\r').map_or((b, "\n"), |b| (b, "\r\n")),
                None => (line, ""),
            };
            match self.rewrite(body) {
                Some(new) => out.push_str(&new),
                None => out.push_str(body),
            }
            out.push_str(end);
        }
        out
    }

    /// The line declaring an option set here, rewritten; None for any other line.
    fn rewrite(&self, line: &str) -> Option<String> {
        let indent = &line[..line.len() - line.trim_start().len()];
        let rest = line.trim_start();
        let (commented, rest) = match rest.strip_prefix("//") {
            Some(r) => (true, r.trim_start()),
            None => (false, rest),
        };
        if let Some(after) = rest.strip_prefix("#define") {
            let after = after.strip_prefix([' ', '\t'])?.trim_start();
            let name_len = identifier(after);
            let (name, tail) = after.split_at(name_len);
            let value = self.values.get(name)?;
            // What follows the name: a value, then perhaps a comment.
            let (old, comment) = split_comment(tail);
            if old.trim().is_empty() {
                // A switch.
                let on = match value.as_str() {
                    "true" => true,
                    "false" => false,
                    _ => return None,
                };
                let prefix = if on { "" } else { "//" };
                return Some(format!("{indent}{prefix}#define {name}{tail}"));
            }
            if commented {
                return None;
            }
            return Some(format!("{indent}#define {name} {value}{}", spaced(comment)));
        }
        if commented {
            return None;
        }
        let after = rest
            .strip_prefix("const")?
            .strip_prefix([' ', '\t'])?
            .trim_start();
        let type_len = identifier(after);
        let after_type = after[type_len..].trim_start();
        let name_len = identifier(after_type);
        let name = &after_type[..name_len];
        let value = self.values.get(name)?;
        let after_name = after_type[name_len..].trim_start().strip_prefix('=')?;
        let (_, tail) = after_name.split_once(';')?;
        Some(format!(
            "{indent}const {} {name} = {value};{tail}",
            &after[..type_len]
        ))
    }
}

/// The length of the identifier `text` starts with.
fn identifier(text: &str) -> usize {
    text.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(text.len())
}

/// A define's tail as its value and its `//` comment (with the `//`).
fn split_comment(tail: &str) -> (&str, &str) {
    match tail.find("//") {
        Some(i) => (&tail[..i], &tail[i..]),
        None => (tail, ""),
    }
}

fn spaced(comment: &str) -> String {
    if comment.is_empty() {
        String::new()
    } else {
        format!(" {comment}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Macros;

    #[test]
    fn a_picked_value_replaces_the_declared_one() {
        let mut values = Values::default();
        values.parse_list("SHADOW_QUALITY=0 shadowDistance=64.0 BLOOM !FXAA");
        let source = "    #define SHADOW_QUALITY 2 //[-1 0 1 2]\n\
                      \tconst float shadowDistance = 192.0; //[64.0 192.0]\r\n\
                      //#define BLOOM // bloom\n\
                      #define FXAA\n\
                      #define OTHER 3\n\
                      // #define SHADOW_QUALITY 5\n\
                      #if SHADOW_QUALITY == 2\n";
        assert_eq!(
            values.apply(source),
            "    #define SHADOW_QUALITY 0 //[-1 0 1 2]\n\
             \tconst float shadowDistance = 64.0; //[64.0 192.0]\r\n\
             #define BLOOM // bloom\n\
             //#define FXAA\n\
             #define OTHER 3\n\
             // #define SHADOW_QUALITY 5\n\
             #if SHADOW_QUALITY == 2\n"
        );
    }

    #[test]
    fn a_profile_takes_the_profiles_it_names_then_its_own() {
        let text = "profile.LOW = SHADOW_QUALITY=0 shadowDistance=96.0 !SSAO\n\
                    profile.POTATO = profile.LOW shadowDistance=64.0\n";
        let properties = Properties::parse(text, &Macros::standard());
        let potato = Values::profile(&properties, "POTATO").unwrap();
        let mut expected = Values::default();
        expected.parse_list("SHADOW_QUALITY=0 shadowDistance=64.0 !SSAO");
        assert_eq!(potato, expected);
        assert!(Values::profile(&properties, "ULTRA").is_none());
    }
}
