use crate::manager::DisplayName;

use strsim::jaro;

/// In case of violations, this is the max amount of names to be shown to the
/// end user.
pub const VIOLATIONS_CAP: usize = 5;

pub struct DisplayNameHandler<'a> {
    display_names: &'a [&'a DisplayName],
    limit: f64,
}

impl<'a> DisplayNameHandler<'a> {
    pub fn with_state(state: &'a [&'a DisplayName]) -> Self {
        DisplayNameHandler {
            display_names: state,
            limit: 0.5,
        }
    }
    pub fn verify_display_name(&self, display_name: &DisplayName) -> Vec<DisplayName> {
        let mut violations = vec![];

        for &persisted in self.display_names {
            if Self::is_too_similar(persisted, display_name, self.limit) {
                // Clone the display name, since this will get inserted into an
                // event which requires ownership.
                violations.push(persisted.clone());
            }

            // Cap the violation list, prevent sending oversized buffers.
            if violations.len() == VIOLATIONS_CAP {
                break;
            }
        }

        violations
    }
    fn is_too_similar(display_name: &DisplayName, account: &DisplayName, limit: f64) -> bool {
        let name_str = display_name.as_str().to_lowercase();
        let account_str = account.as_str().to_lowercase();

        let similarities = [
            jaro(&name_str, &account_str),
            jaro_words(&name_str, &account_str, &[" ", "-", "_"]),
        ];

        similarities.iter().any(|&s| s > limit)
    }
}

fn jaro_words(left: &str, right: &str, delimiter: &[&str]) -> f64 {
    fn splitter<'a>(string: &'a str, delimiter: &[&str]) -> Vec<&'a str> {
        let mut all = vec![];

        for del in delimiter {
            let mut words: Vec<&str> = string
                .split(del)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            all.append(&mut words);
        }

        all
    }

    let left_words = splitter(left, delimiter);
    let right_words = splitter(right, delimiter);

    let mut total = 0.0;

    for left_word in &left_words {
        let mut temp = 0.0;

        for right_word in &right_words {
            let sim = jaro(left_word, right_word);

            if sim > temp {
                temp = sim;
            }
        }

        total += temp;
    }

    total as f64 / left_words.len().max(right_words.len()) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::DisplayName;

    const LIMIT: f64 = 0.85;

    impl<'a> From<&'a str> for DisplayName {
        fn from(val: &'a str) -> Self {
            DisplayName::from(val.to_string())
        }
    }

    #[test]
    fn is_too_similar() {
        let current = [
            DisplayName::from("dave"),
            DisplayName::from("Dave"),
            DisplayName::from("daev"),
            DisplayName::from("Daev"),
        ];

        let new = DisplayName::from("dave");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(res);
        }

        let current = [
            DisplayName::from("David"),
            DisplayName::from("alice"),
            DisplayName::from("Alice"),
            DisplayName::from("bob"),
            DisplayName::from("Bob"),
            DisplayName::from("eve"),
            DisplayName::from("Eve"),
            DisplayName::from("David"),
        ];

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(!res);
        }
    }

    #[test]
    fn is_too_similar_words() {
        let current = [
            DisplayName::from("adam & eve"),
            DisplayName::from("Adam & Eve"),
            DisplayName::from("aadm & Eve"),
            DisplayName::from("Aadm & Eve"),
            DisplayName::from("adam & ev"),
            DisplayName::from("Adam & Ev"),
            DisplayName::from("eve & adam"),
            DisplayName::from("Eve & Adam"),
        ];

        let new = DisplayName::from("Adam & Eve");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(res);
        }

        let current = [
            DisplayName::from("alice & bob"),
            DisplayName::from("Alice & Bob"),
            DisplayName::from("jeff & john"),
            DisplayName::from("Jeff & John"),
        ];

        let new = DisplayName::from("Adam & Eve");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(!res);
        }
    }

    #[test]
    fn is_too_similar_words_special_delimiter() {
        let current = [
            DisplayName::from("adam & eve"),
            DisplayName::from("Adam & Eve"),
            DisplayName::from("aadm & Eve"),
            DisplayName::from("Aadm & Eve"),
            DisplayName::from("adam & ev"),
            DisplayName::from("Adam & Ev"),
            DisplayName::from("eve & adam"),
            DisplayName::from("Eve & Adam"),
            //
            DisplayName::from("adam-&-eve"),
            DisplayName::from("Adam-&-Eve"),
            DisplayName::from("aadm-&-Eve"),
            DisplayName::from("Aadm-&-Eve"),
            DisplayName::from("adam-&-ev"),
            DisplayName::from("Adam-&-Ev"),
            DisplayName::from("eve-&-adam"),
            DisplayName::from("Eve-&-Adam"),
            //
            DisplayName::from("adam_&_eve"),
            DisplayName::from("Adam_&_Eve"),
            DisplayName::from("aadm_&_Eve"),
            DisplayName::from("Aadm_&_Eve"),
            DisplayName::from("adam_&_ev"),
            DisplayName::from("Adam_&_Ev"),
            DisplayName::from("eve_&_adam"),
            DisplayName::from("Eve_&_Adam"),
        ];

        let new = DisplayName::from("Adam & Eve");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(res);
        }

        let current = [
            DisplayName::from("alice & bob"),
            DisplayName::from("Alice & Bob"),
            DisplayName::from("jeff & john"),
            DisplayName::from("Jeff & John"),
            //
            DisplayName::from("alice_&_bob"),
            DisplayName::from("Alice_&_Bob"),
            DisplayName::from("jeff_&_john"),
            DisplayName::from("Jeff_&_John"),
            //
            DisplayName::from("alice-&-bob"),
            DisplayName::from("Alice-&-Bob"),
            DisplayName::from("jeff-&-john"),
            DisplayName::from("Jeff-&-John"),
        ];

        let new = DisplayName::from("Adam & Eve");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(!res);
        }
    }

    #[test]
    fn is_too_similar_unicode() {
        let current = [DisplayName::from("👻🥺👌 Alice")];

        let new = DisplayName::from("👻🥺👌 Alice");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(res);
        }

        let current = [
            DisplayName::from("Alice"),
            DisplayName::from("👻🥺👌 Johnny 💀"),
            DisplayName::from("🤖👈👈 Alice"),
            DisplayName::from("👻🥺👌 Bob"),
            DisplayName::from("👻🥺👌 Eve"),
        ];

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(!res);
        }
    }
}
