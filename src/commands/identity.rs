//! User and group names for `ls -l`, read from `/etc/passwd` and `/etc/group`.

use std::collections::HashMap;
use std::fs;

#[derive(Default)]
pub struct Names {
    users: HashMap<u32, String>,
    groups: HashMap<u32, String>,
}

impl Names {
    pub fn load() -> Names {
        Names {
            users: parse_table("/etc/passwd", 2),
            groups: parse_table("/etc/group", 2),
        }
    }

    /// The user name for `uid`, or the number itself if it has no name.
    pub fn user(&self, uid: u32) -> String {
        lookup(&self.users, uid)
    }

    /// The group name for `gid`, or the number itself if it has no name.
    pub fn group(&self, gid: u32) -> String {
        lookup(&self.groups, gid)
    }
}

fn lookup(table: &HashMap<u32, String>, id: u32) -> String {
    table.get(&id).cloned().unwrap_or_else(|| id.to_string())
}

/// Parses colon-separated lines of the form `name:x:ID:...`.
/// The first entry wins when an ID appears twice, as with `getpwuid`.
fn parse_table(path: &str, id_field: usize) -> HashMap<u32, String> {
    let mut table = HashMap::new();
    let content = fs::read_to_string(path).unwrap_or_default();
    for line in content.lines() {
        let mut fields = line.split(':');
        let name = fields.next();
        let id = fields
            .nth(id_field - 1)
            .and_then(|field| field.parse::<u32>().ok());
        if let (Some(name), Some(id)) = (name, id) {
            table.entry(id).or_insert_with(|| name.to_string());
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_ids_fall_back_to_numbers() {
        let names = Names::default();
        assert_eq!(names.user(4242), "4242");
        assert_eq!(names.group(4242), "4242");
    }
}
