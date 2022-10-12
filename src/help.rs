use lua_patterns::{LuaPattern, errors::PatternError as LuaPatternError};
use std::collections::HashMap;

struct Replacement {
    pattern: &'static str,
    replacement: &'static str,
    should_escape_pattern: bool,
}

impl Replacement {
    const fn new(
        pattern: &'static str,
        replacement: &'static str,
        should_escape_pattern: bool,
    ) -> Self {
        Self {
            pattern,
            replacement,
            should_escape_pattern,
        }
    }
}

lazy_static::lazy_static! {
    static ref FULL_REPLACEMENTS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("*", "star");
        m.insert("g*", "gstar");
        m.insert("[*", "[star");
        m.insert("]*", "]star");
        m.insert("/*", "/star");
        m.insert("/\\*", "/\\star");
        m.insert("\"*", "quotestar");
        m.insert("**", "starstar");
        m
    };

    static ref REPLACEMENTS: Vec<Replacement> = vec![
        Replacement::new("\"", "quote", true),
        Replacement::new("|", "bar", true),
        // NOTE(smolck)/TODO(smolck): Hmm so in the Lua implementation it was CTRL%-%1, which makes sense,
        // but for some reason that results in the escaped pattern for ^N ultimately becoming
        // CTRL%%%-N, which is not CTRL%-N, the desired result. Removing the % before the - here fixesthat
        // that. So idk if that's a bug in the lua patterns implementation or if I'm misunderstanding
        // something or what.
        //
        // Worth noting for now I guess, but ideally I'll just switch to actual regex or something
        // in the future and none of this should/will matter.
        Replacement::new("%^(.)", "CTRL-%1", false), // ^N to CTRL-N
        Replacement::new("(CTRL%-.)([^_])", "%1_%2", false), // Insert _ between CTRL-XCTRL-N
    ];
}

#[derive(Debug)]
pub struct Patterns {
    escaped: String,
    icase: String,
    wildcard: String,
}

#[derive(Debug, PartialEq, Eq, PartialOrd)]
pub struct Tag<'a> {
    pub name: &'a str,
    pub file: &'a str,
}

#[derive(Debug)]
pub struct Match<'a> {
    tag: Tag<'a>,
    score: i32,
}

fn escape_pattern(text: &str) -> Result<String, LuaPatternError> {
    // gsub_checked in case emojis or something are used which would make this
    // create invalid unicode and crash the program if we used the normal gsub
    LuaPattern::new("([^%w])").gsub_checked(text, "%%%1")
}

fn ignorecase_pattern(text: &str) -> String {
    LuaPattern::new("(%a)").gsub_with(text, |cc| {
        // TODO(smolck): umm . . . what
        format!("[{}{}]", cc.get(1).to_lowercase(), cc.get(1).to_uppercase())
    })
}

fn generate_search_patterns(name: &str) -> Option<Patterns> {
    let name = if let Some(replacement) = FULL_REPLACEMENTS.get(&name) {
        replacement.to_string()
    } else {
        let mut name = name.to_string();
        for r in REPLACEMENTS.iter() {
            let patt = if r.should_escape_pattern {
                let Ok(escaped) = escape_pattern(&r.pattern) else {
                    continue;
                };
                escaped
            } else {
                r.pattern.to_string()
            };
            name = LuaPattern::new(&patt).gsub(&name, r.replacement);
        }
        name
    };

    let Ok(escaped) = escape_pattern(&name) else {
        return None;
    };
    let wildcard = LuaPattern::new("%%%*").gsub(&escaped, ".*");
    let wildcard = LuaPattern::new("%%%?").gsub(&wildcard, ".");
    Some(Patterns {
        icase: ignorecase_pattern(&escaped),
        // TODO(smolck): Umm . . . what
        wildcard: if escaped == wildcard {
            "^$".to_string()
        } else {
            wildcard
        },
        escaped,
    })
}

fn find_in_tagfile_and_score<'a>(tagfile: &'a str, patterns: Patterns) -> Vec<Match<'a>> {
    let mut escaped = LuaPattern::new(&patterns.escaped);
    let mut icase = LuaPattern::new(&patterns.icase);
    let mut wildcard = LuaPattern::new(&patterns.wildcard);
    let mut matches = Vec::new();
    let mut score = 0;
    let mut add = false;
    let mut matchpos = None;

    for line in tagfile.lines() {
        if line.is_empty() {
            break;
        }

        let entry = line.split('\t').collect::<Vec<_>>();
        let tag = entry[0];

        // TODO(smolck): Is this `matches` function the right thing/doing what I
        // think it's doing?
        if escaped.matches(tag) {
            add = true;
            // TODO(smolck): This right?
            matchpos = Some(escaped.first_capture().start);
        } else {
            // Case insensitive match
            if icase.matches(tag) {
                add = true;
                score += 5000;
                matchpos = Some(icase.first_capture().start);
            } else {
                // Wildcard match
                if wildcard.matches(tag) {
                    add = true;
                    score += 20_000;
                    matchpos = Some(wildcard.first_capture().start);
                }
            }
        }

        if add {
            score += tag.len() as i32;
            for char in tag.chars() {
                if char.is_ascii_alphabetic() {
                    score += 100;
                }
            }

            // This should always be true maybe? idk
            if let Some(pos) = matchpos {
                if pos > 1 && LuaPattern::new("^%w%w").matches(&tag[(pos - 1)..]) {
                    score += 10_000;
                } else if pos > 3 {
                    score *= 200;
                }
            }

            matches.push(Match {
                tag: Tag {
                    name: entry[0],
                    file: entry[1],
                },
                score,
            });
        }
        add = false;
        score = 0;
    }

    matches
}

pub fn help<'a>(thing: &str) -> Option<Tag<'a>> {
    let Some(patterns) = generate_search_patterns(thing) else {
        return None;
    };

    if let Some(m) =
        find_in_tagfile_and_score(include_str!("tags"), patterns)
            .into_iter()
            .min_by_key(|m| m.score)
    {
        Some(m.tag)
    } else {
        None
    }
}

impl<'a> Tag<'a> {
    pub fn to_url(&self) -> String {
        let file_without_ext = {
            let s = self.file.split('.').next().unwrap();
            // index.txt maps to vimindex.html on the website because reasons
            if s == "index" {
                "vimindex"
            } else {
                s
            }
        };

        format!(
            "https://neovim.io/doc/user/{}.html#{}",
            file_without_ext, self.name
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;

    macro_rules! t {
        ($input:expr, $result_name:expr, $result_file:expr) => {
            assert_eq!(
                help($input).map(|t| (t.name, t.file)),
                Some(($result_name, $result_file))
            );
        };
    }

    #[test]
    fn help_works() {
        t!("^N", "CTRL-N", "motion.txt");
        t!("^n", "CTRL-N", "motion.txt"); // case insensitive
        t!("^X^N", "i_CTRL-X_CTRL-N", "insert.txt");
        t!("^x^n", "i_CTRL-X_CTRL-N", "insert.txt"); // case insensitive
        t!("nvim_cmd", "nvim_cmd()", "api.txt");
        t!("cd", ":cd", "editing.txt");
        t!("'cd", "'cd'", "options.txt");
        t!("\\c", "/\\c", "pattern.txt");
        t!("let-&", ":let-&", "eval.txt");
        t!("wildmenu", "'wildmenu'", "options.txt");
        t!("'wildmenu'", "'wildmenu'", "options.txt");
    }
}
