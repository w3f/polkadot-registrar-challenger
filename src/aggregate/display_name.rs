use crate::manager::DisplayName;

use strsim::jaro;

/// In case of violations, this is the max amount of names to be shown to the
/// end user.
pub const VIOLATIONS_CAP: usize = 5;

pub struct DisplayNameHandler<'a> {
    display_names: &'a [DisplayName],
    limit: f64,
}

impl<'a> DisplayNameHandler<'a> {
    pub fn with_state(state: &'a [DisplayName]) -> Self {
        DisplayNameHandler {
            display_names: state,
            limit: 0.5,
        }
    }
    pub fn verify_display_name(&self, _display_name: &DisplayName) -> Vec<DisplayName> {
        let mut violations = vec![];

        for display_name in self.display_names {
            if Self::is_too_similar(display_name, display_name, self.limit) {
                // Clone the display name, since this will get inserted into an
                // event which requires ownership.
                violations.push(display_name.clone());
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

/*
#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::Account;

    const LIMIT: f64 = 0.85;

    #[test]
    fn is_too_similar() {
        let current = [
            Account::from("dave"),
            Account::from("Dave"),
            Account::from("daev"),
            Account::from("Daev"),
        ];

        let new = Account::from("dave");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(res);
        }

        let current = [
            Account::from("David"),
            Account::from("alice"),
            Account::from("Alice"),
            Account::from("bob"),
            Account::from("Bob"),
            Account::from("eve"),
            Account::from("Eve"),
            Account::from("David"),
        ];

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(!res);
        }
    }

    #[test]
    fn is_too_similar_words() {
        let current = [
            Account::from("adam & eve"),
            Account::from("Adam & Eve"),
            Account::from("aadm & Eve"),
            Account::from("Aadm & Eve"),
            Account::from("adam & ev"),
            Account::from("Adam & Ev"),
            Account::from("eve & adam"),
            Account::from("Eve & Adam"),
        ];

        let new = Account::from("Adam & Eve");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(res);
        }

        let current = [
            Account::from("alice & bob"),
            Account::from("Alice & Bob"),
            Account::from("jeff & john"),
            Account::from("Jeff & John"),
        ];

        let new = Account::from("Adam & Eve");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(!res);
        }
    }

    #[test]
    fn is_too_similar_words_special_delimiter() {
        let current = [
            Account::from("adam & eve"),
            Account::from("Adam & Eve"),
            Account::from("aadm & Eve"),
            Account::from("Aadm & Eve"),
            Account::from("adam & ev"),
            Account::from("Adam & Ev"),
            Account::from("eve & adam"),
            Account::from("Eve & Adam"),
            //
            Account::from("adam-&-eve"),
            Account::from("Adam-&-Eve"),
            Account::from("aadm-&-Eve"),
            Account::from("Aadm-&-Eve"),
            Account::from("adam-&-ev"),
            Account::from("Adam-&-Ev"),
            Account::from("eve-&-adam"),
            Account::from("Eve-&-Adam"),
            //
            Account::from("adam_&_eve"),
            Account::from("Adam_&_Eve"),
            Account::from("aadm_&_Eve"),
            Account::from("Aadm_&_Eve"),
            Account::from("adam_&_ev"),
            Account::from("Adam_&_Ev"),
            Account::from("eve_&_adam"),
            Account::from("Eve_&_Adam"),
        ];

        let new = Account::from("Adam & Eve");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(res);
        }

        let current = [
            Account::from("alice & bob"),
            Account::from("Alice & Bob"),
            Account::from("jeff & john"),
            Account::from("Jeff & John"),
            //
            Account::from("alice_&_bob"),
            Account::from("Alice_&_Bob"),
            Account::from("jeff_&_john"),
            Account::from("Jeff_&_John"),
            //
            Account::from("alice-&-bob"),
            Account::from("Alice-&-Bob"),
            Account::from("jeff-&-john"),
            Account::from("Jeff-&-John"),
        ];

        let new = Account::from("Adam & Eve");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(!res);
        }
    }

    #[test]
    fn is_too_similar_unicode() {
        let current = [Account::from("👻🥺👌 Alice")];

        let new = Account::from("👻🥺👌 Alice");

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(res);
        }

        let current = [
            Account::from("Alice"),
            Account::from("👻🥺👌 Johnny 💀"),
            Account::from("🤖👈👈 Alice"),
            Account::from("👻🥺👌 Bob"),
            Account::from("👻🥺👌 Eve"),
        ];

        for account in &current {
            let res = DisplayNameHandler::is_too_similar(account, &new, LIMIT);
            assert!(!res);
        }
    }
}
*/
