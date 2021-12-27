#[derive(Debug, PartialEq, Eq, PartialOrd)]
pub struct Tag<'a> {
    pub name: &'a str,
    pub file: &'a str,
    // pub score: i32,
    // pub r#type: &'a str,
    // pub _tag: &'a str, // TODO(smolck)
}

impl Ord for Tag<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}

impl<'a> Tag<'a> {
    /// Assumes `s` is not empty
    pub fn from_str(s: &'a str) -> Self {
        let mut parts = s.split('\t');

        let name = parts.next().unwrap();
        let file = parts.next().unwrap();
        // let _tag = parts.next().unwrap(); // TODO(smolck)

        // Tag { name, file, score: 0, r#type: "" }
        Tag { name, file }
    }

    /// Assumes s is not empty
    /*pub fn from_str_and_score(s: &'a str) -> Self {
        let mut tag = Tag::from_str(s);

        tag
    }*/

    pub fn from_name(name: &'a str) -> Self {
        Tag {
            name,
            file: "",
            // score: 0,
            // r#type: "",
        }
    }

    pub fn to_url(&self) -> String {
        let file_without_ext = self.file.split('.').next().unwrap();

        format!(
            "https://neovim.io/doc/user/{}.html#{}",
            file_without_ext, self.name
        )
    }
}

/*pub struct ScoredTag<'a> {
    pub tag_name: &'a str,
    pub fname_no_ext: &'a str,
    pub score: i32,
    pub t: &'a str,
}*/

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_tag_from_str() {
        let tag = Tag::from_str("!	change.txt	/*!*");
        assert_eq!(
            tag,
            Tag {
                name: "!",
                file: "change.txt",
                // _tag: "/*!*"
            }
        );
    }

    #[test]
    fn test_tag_ord() {
        let tag = Tag::from_str("!	change.txt	/*!*");
        let tag2 = Tag::from_str("$NVIM_LISTEN_ADDRESS	deprecated.txt	/*$NVIM_LISTEN_ADDRESS*");

        assert_eq!(tag.cmp(&tag2), std::cmp::Ordering::Less);
    }
}
